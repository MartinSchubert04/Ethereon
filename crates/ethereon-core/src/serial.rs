//! Consola serial (el cable de consola de switches, APs y routers): el otro transporte
//! ademas de SSH. Abre el puerto, deja que la terminal de la interfaz hable con el equipo
//! en crudo, y le da al broker un `exec` por prompt: en serial no hay codigo de salida, asi
//! que se manda `comando\r`, se lee hasta que la salida termina en el prompt del equipo (o
//! vence el tiempo), se descarta el eco del comando y se contesta a los paginadores
//! (`--More--`, `Press any key`) con un espacio. Antes de cada comando, `ready` despierta la
//! linea con un Enter y, si la consola pide login o `enable`, contesta con las credenciales
//! del vault; nada de eso vuelve al agente.
//!
//! El puerto esta detras del trait `Link`, asi la deteccion de prompt, el paginado y el
//! login se prueban con un stream falso en memoria, sin hardware (ver los tests).
//!
//! Una conexion por host (`Conn`), compartida entre la terminal y el agente: un hilo lee el
//! puerto y le reparte los bytes a todos los que escuchan (`tap`), y los comandos del agente
//! se turnan con un candado. Si la persona tiene la terminal abierta, ve pasar lo que hace el
//! agente. El nombre del puerto (`COM3`) es como la IP: solo lo ve la persona.
//!
//! Ademas cada conexion guarda lo ultimo que llego (`History`): la terminal que se abre
//! despues arranca mostrando eso, y el agente lo lee por partes con un cursor
//! (`Conn::read_since`) para seguir la consola en vivo sin depender del prompt (un equipo
//! arrancando, un login que no tiene, ROMMON).
//!
//! El puerto puede quedar en "automatico" (vacio): se usa el adaptador USB que este
//! enchufado, porque Windows le puede dar otro COM al mismo cable. Ver `open`.

use regex::Regex;
use serde::Serialize;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

use crate::model::{Parity, SerialConfig};

/// Un puerto serie (o algo que se le parece, en los tests).
pub trait Link: Send {
    /// Lee lo que haya, esperando como mucho un instante. Ok(0) = no llego nada por ahora;
    /// Err = el puerto se cerro o fallo.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    /// Otro manejador del mismo puerto, para leer desde un hilo y escribir desde otro.
    fn try_clone(&self) -> io::Result<Box<dyn Link>>;
    /// Prende o apaga la senal de Break (la linea en 0 mas que un caracter entero). Es lo
    /// que manda la tecla Break de PuTTY: en muchos equipos corta el arranque y entra al
    /// monitor (ROMMON).
    fn set_break(&mut self, _on: bool) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "this port can't send a break"))
    }
}

/// Cuanto espera cada lectura del puerto. Tambien es el tope de la demora de una escritura:
/// en Windows el puerto atiende de a una operacion por vez, asi que cada tecla espera a que
/// termine la lectura en curso. Con 50 ms se notaba al escribir rapido (las teclas se
/// encolaban); con 5 ms no se nota y el hilo lector igual duerme en el puerto (la lectura
/// vuelve apenas llega un byte, no espera los 5 ms).
const READ_WAIT: Duration = Duration::from_millis(5);

struct Port(Box<dyn serialport::SerialPort>);

impl Link for Port {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => Ok(0),
            Err(e) => Err(e),
        }
    }
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.write_all(data)?;
        self.0.flush()
    }
    fn try_clone(&self) -> io::Result<Box<dyn Link>> {
        Ok(Box::new(Port(self.0.try_clone()?)))
    }
    fn set_break(&mut self, on: bool) -> io::Result<()> {
        if on { self.0.set_break() } else { self.0.clear_break() }.map_err(io::Error::from)
    }
}

/// Un puerto abierto y como se eligio (para el registro de conexion de la persona).
pub struct Opened {
    pub link: Box<dyn Link>,
    /// El puerto que se abrio (`COM10`).
    pub port: String,
    /// Por que ese: vacio si es el del host; si no, que se busco y que se encontro.
    pub note: String,
}

/// Abre el puerto del host. Vacio = automatico: el adaptador USB que este enchufado (si hay
/// varios, el primero que contesta a un Enter). Si el host tiene un puerto fijo que ya no
/// esta (Windows le dio otro COM al mismo cable) y hay un solo adaptador USB, se usa ese.
pub fn open(cfg: &SerialConfig) -> io::Result<Opened> {
    let wanted = cfg.port.trim();
    let ports = list_ports();
    if !wanted.is_empty() && (ports.is_empty() || ports.iter().any(|p| p.name.eq_ignore_ascii_case(wanted))) {
        return Ok(Opened { link: open_port(wanted, cfg)?, port: wanted.to_string(), note: String::new() });
    }
    let usb: Vec<&PortInfo> = ports.iter().filter(|p| p.console).collect();
    let names = || usb.iter().map(|p| describe(p)).collect::<Vec<_>>().join(", ");
    if !wanted.is_empty() {
        // El puerto fijo no esta. Con un solo adaptador es el mismo cable con otro numero;
        // con varios no se adivina (podria ser la consola de otro equipo).
        return match usb.as_slice() {
            [one] => Ok(Opened {
                link: open_port(&one.name, cfg)?,
                port: one.name.clone(),
                note: format!("{wanted} is not plugged in; using {} instead", describe(one)),
            }),
            [] => Err(io::Error::new(io::ErrorKind::NotFound, format!("{wanted} is not plugged in, and there is no USB serial adapter"))),
            _ => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{wanted} is not plugged in; there are several USB serial adapters ({}): pick one, or Automatic", names()),
            )),
        };
    }
    match usb.as_slice() {
        [] => Err(io::Error::new(io::ErrorKind::NotFound, "no USB serial adapter is plugged in (Bluetooth and built-in ports are skipped)")),
        [one] => Ok(Opened { link: open_port(&one.name, cfg)?, port: one.name.clone(), note: format!("Automatic: {}", describe(one)) }),
        _ => {
            // Varios: el primero donde un equipo contesta. Si ninguno contesta (apagado,
            // arrancando), el primero que abre.
            let mut first: Option<(Box<dyn Link>, &PortInfo)> = None;
            for p in &usb {
                let Ok(mut link) = open_port(&p.name, cfg) else { continue };
                if answers(link.as_mut(), PROBE) {
                    return Ok(Opened { link, port: p.name.clone(), note: format!("Automatic: {} answered (out of {})", describe(p), names()) });
                }
                if first.is_none() {
                    first = Some((link, p));
                }
            }
            match first {
                Some((link, p)) => Ok(Opened { link, port: p.name.clone(), note: format!("Automatic: nothing answered on {}; using {}", names(), describe(p)) }),
                None => Err(io::Error::other(format!("could not open any USB serial adapter ({})", names()))),
            }
        }
    }
}

/// Cuanto espera una respuesta cada puerto al buscar entre varios adaptadores.
const PROBE: Duration = Duration::from_millis(1200);

/// Manda un Enter y espera a que llegue algo.
fn answers(link: &mut dyn Link, wait: Duration) -> bool {
    if link.write(b"\r").is_err() {
        return false;
    }
    let end = Instant::now() + wait;
    let mut buf = [0u8; 256];
    while Instant::now() < end {
        match link.read(&mut buf) {
            Ok(n) if n > 0 => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

fn describe(p: &PortInfo) -> String {
    if p.label.is_empty() { p.name.clone() } else { format!("{} ({})", p.name, p.label) }
}

/// Abre un puerto por nombre. Si otro programa (o una conexion que se esta cerrando)
/// todavia lo tiene, reintenta un momento.
fn open_port(name: &str, cfg: &SerialConfig) -> io::Result<Box<dyn Link>> {
    use serialport::{DataBits, FlowControl, StopBits};
    let builder = serialport::new(name, cfg.baud.max(1))
        .data_bits(match cfg.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        })
        .parity(match cfg.parity {
            Parity::None => serialport::Parity::None,
            Parity::Even => serialport::Parity::Even,
            Parity::Odd => serialport::Parity::Odd,
        })
        .stop_bits(if cfg.stop_bits == 2 { StopBits::Two } else { StopBits::One })
        .flow_control(FlowControl::None)
        .timeout(READ_WAIT);
    let mut last = None;
    for _ in 0..5 {
        match builder.clone().open() {
            Ok(p) => return Ok(Box::new(Port(p))),
            Err(e) => {
                let busy = matches!(e.kind, serialport::ErrorKind::Io(io::ErrorKind::PermissionDenied))
                    || e.description.to_lowercase().contains("denied")
                    || e.description.to_lowercase().contains("busy");
                last = Some(io::Error::from(e));
                if !busy {
                    break;
                }
                std::thread::sleep(Duration::from_millis(120));
            }
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("could not open the serial port")))
}

/// Un puerto de esta PC, para elegirlo en el formulario.
#[derive(Clone, Debug, Serialize)]
pub struct PortInfo {
    pub name: String,
    /// Que es (el nombre del adaptador USB, "Bluetooth"...), si se sabe.
    pub label: String,
    /// Tiene sentido como cable de consola: un adaptador USB-serie. Los puertos Bluetooth,
    /// los de la placa (PCI, modems) y los que no se sabe que son, no: el modo automatico
    /// no los prueba.
    pub console: bool,
}

pub fn available_ports() -> Vec<PortInfo> {
    list_ports()
}

fn list_ports() -> Vec<PortInfo> {
    use serialport::SerialPortType;
    let mut out: Vec<PortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let (label, usb) = match p.port_type {
                SerialPortType::UsbPort(u) => (u.product.or(u.manufacturer).unwrap_or_else(|| "USB".into()), true),
                SerialPortType::BluetoothPort => ("Bluetooth".into(), false),
                SerialPortType::PciPort => ("PCI".into(), false),
                SerialPortType::Unknown => (String::new(), false),
            };
            let console = is_console(&p.port_name, &label, usb);
            PortInfo { name: p.port_name, label, console }
        })
        .collect();
    out.sort_by(|a, b| natural_key(&a.name).cmp(&natural_key(&b.name)));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Si un puerto parece un cable de consola. En Linux, sin libudev, los adaptadores USB
/// llegan como "desconocidos": se reconocen por el nombre (`ttyUSB0`, `ttyACM0`).
fn is_console(name: &str, label: &str, usb: bool) -> bool {
    if label.to_lowercase().contains("bluetooth") || name.to_lowercase().contains("rfcomm") {
        return false;
    }
    let n = name.to_lowercase();
    usb || n.contains("ttyusb") || n.contains("ttyacm") || n.contains("cu.usb") || n.contains("tty.usb")
}

/// "COM10" despues de "COM3".
fn natural_key(s: &str) -> (String, u64) {
    let digits: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    (s[..s.len() - digits.len()].to_string(), digits.parse().unwrap_or(0))
}

/// Revisa lo que se puede antes de guardar un host serial.
pub fn validate(cfg: &SerialConfig) -> Result<(), String> {
    // El puerto vacio es "automatico" (ver `open`).
    if cfg.baud == 0 {
        return Err("the baud rate must be more than 0".into());
    }
    if !(5..=8).contains(&cfg.data_bits) {
        return Err("data bits must be between 5 and 8".into());
    }
    if !(1..=2).contains(&cfg.stop_bits) {
        return Err("stop bits must be 1 or 2".into());
    }
    Regex::new(&cfg.prompt).map_err(|e| format!("the prompt pattern is not a valid regex: {e}"))?;
    if cfg.timeout_secs == 0 {
        return Err("the command timeout must be at least 1 second".into());
    }
    Ok(())
}

// --------------------------------------------------------------------------- conexion

type Taps = Arc<Mutex<Vec<(u64, Sender<Vec<u8>>)>>>;

/// Cuanto de lo que llego del puerto se guarda (para la terminal que se abre despues y para
/// el agente que lee con cursor).
pub const HISTORY: usize = 256 * 1024;

/// Lo ultimo que llego del puerto. `start` es la posicion (desde que se abrio el puerto)
/// del primer byte guardado: un cursor es una posicion, y sigue valiendo aunque lo viejo
/// se descarte.
#[derive(Default)]
struct History {
    buf: std::collections::VecDeque<u8>,
    start: u64,
}

impl History {
    fn end(&self) -> u64 {
        self.start + self.buf.len() as u64
    }
    fn push(&mut self, data: &[u8]) {
        self.buf.extend(data);
        let extra = self.buf.len().saturating_sub(HISTORY);
        if extra > 0 {
            self.buf.drain(..extra);
            self.start += extra as u64;
        }
    }
}

/// Lo que devuelve `Conn::read_since`.
#[derive(Debug, PartialEq)]
pub struct Since {
    pub data: Vec<u8>,
    /// Desde donde seguir la proxima vez.
    pub cursor: u64,
    /// Se perdio algo entre el cursor pedido y lo que se devuelve (era mas viejo que lo
    /// guardado, o habia mas que `max`).
    pub skipped: bool,
}

/// Un puerto abierto, compartido por la terminal y el agente.
pub struct Conn {
    writer: Mutex<Box<dyn Link>>,
    taps: Taps,
    next_tap: AtomicU64,
    closed: Arc<AtomicBool>,
    history: Arc<Mutex<History>>,
    /// Los comandos del agente van de a uno; `ready` y `exec` lo toman.
    turn: Mutex<()>,
    pub last_used: Mutex<Instant>,
}

/// Lo que llega del puerto desde que se pidio. Al soltarlo deja de recibir.
pub struct Tap {
    id: u64,
    rx: Receiver<Vec<u8>>,
    taps: Taps,
}

impl Tap {
    /// Ok(Some) datos, Ok(None) nada en `wait`, Err el puerto se cerro.
    pub fn recv(&self, wait: Duration) -> Result<Option<Vec<u8>>, Closed> {
        match self.rx.recv_timeout(wait) {
            Ok(d) => Ok(Some(d)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(Closed),
        }
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.taps.lock().unwrap().retain(|(id, _)| *id != self.id);
    }
}

#[derive(Debug)]
pub struct Closed;

impl Conn {
    pub fn new(link: Box<dyn Link>) -> io::Result<Conn> {
        let mut reader = link.try_clone()?;
        let taps: Taps = Default::default();
        let closed = Arc::new(AtomicBool::new(false));
        let history: Arc<Mutex<History>> = Default::default();
        let (t, c, hist) = (taps.clone(), closed.clone(), history.clone());
        std::thread::Builder::new().name("serial-reader".into()).spawn(move || {
            let mut buf = [0u8; 4096];
            while !c.load(Ordering::SeqCst) {
                match reader.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        let data = &buf[..n];
                        // Primero al historial: quien lee con cursor y despues escucha no
                        // se pierde nada en el medio.
                        hist.lock().unwrap().push(data);
                        t.lock().unwrap().retain(|(_, s)| s.send(data.to_vec()).is_ok());
                    }
                    Err(_) => break,
                }
            }
            c.store(true, Ordering::SeqCst);
            // Soltar los emisores: los que escuchan ven "cerrado".
            t.lock().unwrap().clear();
        })?;
        Ok(Conn {
            writer: Mutex::new(link),
            taps,
            next_tap: AtomicU64::new(1),
            closed,
            history,
            turn: Mutex::new(()),
            last_used: Mutex::new(Instant::now()),
        })
    }

    /// Hasta donde llego el puerto (la posicion del proximo byte).
    pub fn cursor(&self) -> u64 {
        self.history.lock().unwrap().end()
    }

    /// Lo que llego desde `cursor`, como mucho `max` bytes (los ultimos). Sin cursor: lo
    /// ultimo que hay guardado (lo que hay "en pantalla").
    pub fn read_since(&self, cursor: Option<u64>, max: usize) -> Since {
        let h = self.history.lock().unwrap();
        let end = h.end();
        let from = cursor.unwrap_or(end.saturating_sub(max as u64)).min(end);
        let mut skipped = from < h.start && cursor.is_some();
        let mut from = from.max(h.start);
        if end - from > max as u64 {
            from = end - max as u64;
            skipped = true;
        }
        let data = h.buf.iter().skip((from - h.start) as usize).copied().collect();
        Since { data, cursor: end, skipped }
    }

    /// Manda un Break (la linea en 0 durante `dur`). Se turna con las escrituras.
    pub fn send_break(&self, dur: Duration) -> io::Result<()> {
        if self.is_closed() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the serial port is closed"));
        }
        self.touch();
        let mut w = self.writer.lock().unwrap();
        w.set_break(true)?;
        std::thread::sleep(dur);
        w.set_break(false)
    }

    /// Lo ultimo que llego (hasta `max`) y un `Tap` que sigue justo desde ahi, sin repetir
    /// ni perder nada en el medio: el hilo lector espera el historial antes de repartir.
    pub fn tap_with_history(&self, max: usize) -> (Vec<u8>, Tap) {
        let h = self.history.lock().unwrap();
        let skip = h.buf.len().saturating_sub(max);
        let past = h.buf.iter().skip(skip).copied().collect();
        let tap = self.tap();
        drop(h);
        (past, tap)
    }

    pub fn tap(&self) -> Tap {
        let (tx, rx) = mpsc::channel();
        let id = self.next_tap.fetch_add(1, Ordering::Relaxed);
        self.taps.lock().unwrap().push((id, tx));
        Tap { id, rx, taps: self.taps.clone() }
    }

    pub fn write(&self, data: &[u8]) -> io::Result<()> {
        if self.is_closed() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the serial port is closed"));
        }
        self.touch();
        self.writer.lock().unwrap().write(data)
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Alguien (una terminal) sigue escuchando: no es una conexion sin uso.
    pub fn in_use(&self) -> bool {
        !self.taps.lock().unwrap().is_empty()
    }

    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    /// Corta el puerto para todos los que escuchan. El hilo lector suelta su manejador en
    /// menos de `READ_WAIT`; el de escritura, cuando nadie mas tenga la conexion.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.close();
    }
}

// --------------------------------------------------------------------- deteccion

/// Que pide la consola en la ultima linea, antes de llegar al prompt.
#[derive(Debug, PartialEq)]
pub enum Asks {
    Username,
    Password,
}

fn ansi_strip(s: &str) -> String {
    let re = ansi_re();
    re.replace_all(s, "").into_owned()
}

fn ansi_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[()][0-9A-Za-z]|\x1b[=>]").unwrap())
}

/// La ultima linea del buffer (lo que hay despues del ultimo salto), sin secuencias ANSI ni
/// retornos de carro.
pub fn last_line(buf: &[u8]) -> String {
    let start = buf.iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
    let s = String::from_utf8_lossy(&buf[start..]);
    ansi_strip(&s).replace('\r', "")
}

/// Hasta donde hay texto util; lo de despues es un paginador que hay que contestar.
const PAGERS: &[&str] = &["--more--", "-- more --", "---(more", "<--- more --->", "press any key", "more: <space>", "---- more ----"];

fn pager_in(line: &str) -> bool {
    let l = line.trim().to_lowercase();
    !l.is_empty() && l.len() < 160 && PAGERS.iter().any(|p| l.contains(p))
}

pub fn asks(line: &str) -> Option<Asks> {
    let l = line.trim().to_lowercase();
    if l.ends_with(':') || l.ends_with(": ") {
        let l = l.trim_end_matches([':', ' ']);
        if l.ends_with("username") || l.ends_with("user name") || l.ends_with("login") || l.ends_with("user") {
            return Some(Asks::Username);
        }
        if l.ends_with("password") || l.ends_with("passcode") || l.ends_with("passwd") {
            return Some(Asks::Password);
        }
    }
    None
}

/// Lo que dijo el equipo despues de mandar un comando. Junta los bytes, reconoce el prompt
/// al final y los paginadores, y guarda como mucho `max` bytes de salida.
pub struct Collector {
    prompt: Regex,
    /// Lo que se devuelve (hasta `max`).
    kept: Vec<u8>,
    /// Lo ultimo que llego (para reconocer el prompt y los paginadores aunque `kept` este lleno).
    tail: Vec<u8>,
    max: usize,
    pub truncated: bool,
    /// Despues de un paginador el equipo borra su linea con espacios, `\r` y retrocesos:
    /// se saltan.
    eating: bool,
    /// Ya llego al menos un salto de linea (el eco del comando): recien ahi la ultima linea
    /// puede ser el prompt.
    seen_newline: bool,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// Seguir leyendo.
    More,
    /// Hay un paginador: mandar un espacio.
    Page,
    /// La salida termina en el prompt.
    Prompt,
}

const TAIL: usize = 1024;

impl Collector {
    pub fn new(prompt: Regex, max: usize) -> Collector {
        Collector { prompt, kept: Vec::new(), tail: Vec::new(), max: max.max(1), truncated: false, eating: false, seen_newline: false }
    }

    pub fn feed(&mut self, data: &[u8]) -> Step {
        for &b in data {
            if self.eating {
                if matches!(b, b' ' | 0x08 | b'\r') {
                    continue;
                }
                self.eating = false;
            }
            if b == b'\n' {
                self.seen_newline = true;
            }
            if self.kept.len() < self.max {
                self.kept.push(b);
            } else {
                self.truncated = true;
            }
            self.tail.push(b);
        }
        if self.tail.len() > TAIL {
            let cut = self.tail.len() - TAIL;
            self.tail.drain(..cut);
        }
        let line = last_line(&self.tail);
        if pager_in(&line) {
            self.cut_last_line();
            self.eating = true;
            return Step::Page;
        }
        if self.seen_newline && self.prompt.is_match(&line) {
            return Step::Prompt;
        }
        Step::More
    }

    /// Saca la ultima linea (un paginador) de lo guardado.
    fn cut_last_line(&mut self) {
        let start = self.tail.iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
        let n = self.tail.len() - start;
        self.tail.truncate(start);
        if !self.truncated {
            let keep = self.kept.len().saturating_sub(n);
            self.kept.truncate(keep);
        }
    }

    pub fn last_line(&self) -> String {
        last_line(&self.tail)
    }

    /// Lo juntado como texto: sin el eco de `command` en la primera linea, sin el prompt
    /// final, con saltos `\n`.
    pub fn finish(self, command: &str, at_prompt: bool) -> (String, Option<String>) {
        let text = String::from_utf8_lossy(&self.kept).into_owned();
        let text = ansi_strip(&text).replace("\r\n", "\n").replace('\r', "\n");
        let mut lines: Vec<&str> = text.split('\n').collect();
        // El eco: la primera linea es el comando (o un pedazo, si el equipo lo partio).
        let cmd = command.trim();
        if let Some(first) = lines.first() {
            let f = first.trim();
            if !f.is_empty() && (f == cmd || cmd.ends_with(f) || f.ends_with(cmd)) {
                lines.remove(0);
            }
        }
        let prompt = if at_prompt && !self.truncated { lines.pop().map(|l| l.trim().to_string()) } else { None };
        let prompt = prompt.or_else(|| at_prompt.then(|| self.last_line().trim().to_string()));
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        while lines.first().is_some_and(|l| l.trim().is_empty()) {
            lines.remove(0);
        }
        (lines.join("\n"), prompt)
    }
}

// --------------------------------------------------------------------------- exec

#[derive(Debug, Default)]
pub struct ExecOut {
    pub output: String,
    /// El prompt con el que termino (`Switch#`); None si vencio el tiempo.
    pub prompt: Option<String>,
    pub truncated: bool,
    pub timed_out: bool,
}

/// Despues de ver el prompt se espera un instante por si llega mas (una linea de salida
/// que termina en `#` o `>` no es el prompt).
const SETTLE: Duration = Duration::from_millis(200);
/// Si en este tiempo el equipo no manda ni un byte despues del Enter, no hay nadie (apagado,
/// otro puerto, otra velocidad): al abrir una terminal no tiene sentido esperar el timeout
/// entero del prompt. Los comandos del agente no lo usan: esperan el timeout del host.
pub const SILENT: Duration = Duration::from_millis(2500);

/// Manda `command` y junta la salida hasta el prompt o hasta `timeout`.
pub fn exec(conn: &Conn, command: &str, prompt: &Regex, timeout: Duration, max: usize) -> io::Result<ExecOut> {
    let _turn = conn.turn.lock().unwrap();
    let tap = conn.tap();
    let mut col = Collector::new(prompt.clone(), max);
    conn.write(format!("{}\r", command.trim_end_matches(['\r', '\n'])).as_bytes())?;
    let deadline = Instant::now() + timeout;
    let mut at_prompt = false;
    let mut settling: Option<Instant> = None;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let wait = match settling {
            Some(s) => SETTLE.saturating_sub(now.duration_since(s)),
            None => (deadline - now).min(Duration::from_millis(100)),
        };
        match tap.recv(wait) {
            Ok(Some(d)) => match col.feed(&d) {
                Step::Page => {
                    settling = None;
                    conn.write(b" ")?;
                }
                Step::Prompt => settling = Some(Instant::now()),
                Step::More => settling = None,
            },
            Ok(None) => {
                if settling.is_some_and(|s| s.elapsed() >= SETTLE) {
                    at_prompt = true;
                    break;
                }
            }
            Err(Closed) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the serial port was closed")),
        }
    }
    conn.touch();
    let truncated = col.truncated;
    let (output, prompt) = col.finish(command, at_prompt);
    Ok(ExecOut { output, prompt, truncated, timed_out: !at_prompt })
}

// ------------------------------------------------------------------ consola en vivo
//
// Para seguir la consola sin esperar un prompt (un equipo arrancando, ROMMON, un login que
// el host no tiene): se manda texto y teclas tal cual y se devuelve lo que aparecio.

/// Una tecla especial, por nombre (`enter`, `ctrl-c`, `break`...).
#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Bytes(Vec<u8>),
    Break,
}

/// Los nombres que se aceptan, para el mensaje de error y la descripcion de la herramienta.
pub const KEY_NAMES: &str = "enter, space, tab, esc, backspace, delete, up, down, left, right, break, \
    ctrl-a … ctrl-z, ctrl-shift-6 (Cisco's escape, same as ctrl-^), ctrl-]";

pub fn key(name: &str) -> Option<Key> {
    let n = name.trim().to_lowercase().replace(['_', '+', ' '], "-");
    let b = |s: &[u8]| Some(Key::Bytes(s.to_vec()));
    match n.as_str() {
        "enter" | "return" | "cr" => b(b"\r"),
        "space" => b(b" "),
        "tab" => b(b"\t"),
        "esc" | "escape" => b(b"\x1b"),
        "backspace" | "bs" => b(b"\x08"),
        "delete" | "del" => b(b"\x7f"),
        "up" => b(b"\x1b[A"),
        "down" => b(b"\x1b[B"),
        "right" => b(b"\x1b[C"),
        "left" => b(b"\x1b[D"),
        "break" | "brk" => Some(Key::Break),
        "ctrl-shift-6" | "ctrl-^" | "ctrl-6" => b(&[0x1e]),
        "ctrl-]" => b(&[0x1d]),
        _ => {
            let c = n.strip_prefix("ctrl-")?;
            let ch = c.chars().next().filter(|ch| c.len() == 1 && ch.is_ascii_alphabetic())?;
            b(&[(ch as u8) & 0x1f])
        }
    }
}

/// Cuanto dura un Break. Los equipos piden mas que un caracter; PuTTY y los Cisco andan
/// bien con medio segundo.
pub const BREAK: Duration = Duration::from_millis(500);

/// Espera lo que llegue despues de `from`: termina cuando, despues de que llego algo, pasa
/// `quiet` sin nada nuevo, o a los `wait` (lo que pase primero).
pub fn listen(conn: &Conn, from: u64, wait: Duration, quiet: Duration, max: usize) -> io::Result<Since> {
    let tap = conn.tap();
    let deadline = Instant::now() + wait;
    let mut last: Option<Instant> = (conn.cursor() > from).then(Instant::now);
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let until = match last {
            Some(l) => (l + quiet).min(deadline),
            None => deadline,
        };
        if now >= until {
            break;
        }
        match tap.recv((until - now).min(Duration::from_millis(100))) {
            Ok(Some(_)) => last = Some(Instant::now()),
            Ok(None) => {}
            Err(Closed) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the serial port was closed")),
        }
    }
    conn.touch();
    Ok(conn.read_since(Some(from), max))
}

/// Los bytes de la consola como texto legible: sin secuencias ANSI ni campanas, con los
/// retrocesos aplicados, y un `\r` solo vuelve al principio de la linea (lo que sigue la
/// pisa, como en la pantalla: asi desaparece un `--More--` que el equipo borro).
pub fn console_text(data: &[u8]) -> String {
    let s = ansi_strip(&String::from_utf8_lossy(data));
    let mut out = String::with_capacity(s.len());
    let mut line_start = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => {
                // Lo que venga pisa la linea: se la deja vacia y se escribe de nuevo.
                let rest: String = out[line_start..].to_string();
                out.truncate(line_start);
                let mut ahead = String::new();
                while let Some(&n) = chars.peek() {
                    if n == '\r' || n == '\n' {
                        break;
                    }
                    ahead.push(n);
                    chars.next();
                }
                let ahead = apply_backspaces(&ahead);
                let ahead_len = ahead.chars().count();
                out.push_str(&ahead);
                // Si lo nuevo es mas corto, queda la cola de lo viejo (como en la pantalla),
                // salvo que lo nuevo sean solo espacios de borrar.
                if !ahead.trim().is_empty() {
                    out.extend(rest.chars().skip(ahead_len));
                }
                let trimmed = out[line_start..].trim_end().len();
                out.truncate(line_start + trimmed);
            }
            '\n' => {
                let trimmed = out[line_start..].trim_end().len();
                out.truncate(line_start + trimmed);
                out.push('\n');
                line_start = out.len();
            }
            '\x08' => {
                if out.len() > line_start {
                    out.pop();
                }
            }
            '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn apply_backspaces(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if c == '\x08' {
            o.pop();
        } else if c == '\t' || !c.is_control() {
            o.push(c);
        }
    }
    o
}

/// Las credenciales de la consola, si las hay. Nunca vuelven en la salida.
#[derive(Default)]
pub struct Login {
    pub user: Option<String>,
    pub password: Option<Zeroizing<String>>,
    pub enable: Option<Zeroizing<String>>,
}

/// Despierta la linea con un Enter y la deja en el prompt: contesta el login y, si el prompt
/// es de usuario (`>`) y hay contrasena de `enable`, entra al modo privilegiado. Devuelve el
/// prompt. Lo que el equipo diga en el camino (banner, eco del usuario) se descarta.
/// `silent`: si en ese tiempo no llega ni un byte, se abandona antes del timeout.
pub fn ready(conn: &Conn, prompt: &Regex, login: &Login, timeout: Duration, silent: Option<Duration>) -> io::Result<String> {
    let _turn = conn.turn.lock().unwrap();
    let tap = conn.tap();
    let mut col = Collector::new(prompt.clone(), 64 * 1024);
    conn.write(b"\r")?;
    let deadline = Instant::now() + timeout;
    let mut sent_user = 0;
    let mut sent_pw = 0;
    let mut enabled = false;
    let mut settling: Option<Instant> = None;
    let started = Instant::now();
    let mut heard = false;
    let fail = |m: &str| io::Error::new(io::ErrorKind::PermissionDenied, m.to_string());
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "the console did not show a prompt"));
        }
        if !heard && silent.is_some_and(|q| now.duration_since(started) >= q) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "the device did not answer (is it on, and is the baud rate right?)"));
        }
        let wait = match settling {
            Some(s) => SETTLE.saturating_sub(now.duration_since(s)),
            None => (deadline - now).min(Duration::from_millis(100)),
        };
        let step = match tap.recv(wait) {
            Ok(Some(d)) => {
                heard = true;
                Some(col.feed(&d))
            }
            Ok(None) => None,
            Err(Closed) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the serial port was closed")),
        };
        match step {
            Some(Step::Page) => {
                settling = None;
                conn.write(b"q")?;
            }
            Some(Step::Prompt) => settling = Some(Instant::now()),
            Some(Step::More) => {
                settling = None;
                match asks(&col.last_line()) {
                    Some(Asks::Username) => {
                        let u = login.user.as_deref().filter(|u| !u.trim().is_empty()).ok_or_else(|| fail("the console asks for a username and the host has none"))?;
                        if sent_user >= 2 {
                            return Err(fail("the console did not accept the username"));
                        }
                        sent_user += 1;
                        col = Collector::new(prompt.clone(), 64 * 1024);
                        conn.write(format!("{u}\r").as_bytes())?;
                    }
                    Some(Asks::Password) => {
                        let pw = if enabled { login.enable.as_ref() } else { login.password.as_ref().or(login.enable.as_ref()) };
                        let pw = pw.ok_or_else(|| fail("the console asks for a password and the host has none"))?;
                        if sent_pw >= 3 {
                            return Err(fail("the console did not accept the password"));
                        }
                        sent_pw += 1;
                        col = Collector::new(prompt.clone(), 64 * 1024);
                        conn.write(format!("{}\r", pw.as_str()).as_bytes())?;
                    }
                    None => {}
                }
            }
            None => {
                if settling.is_some_and(|s| s.elapsed() >= SETTLE) {
                    let line = col.last_line().trim().to_string();
                    if line.ends_with('>') && login.enable.is_some() && !enabled {
                        enabled = true;
                        settling = None;
                        col = Collector::new(prompt.clone(), 64 * 1024);
                        conn.write(b"enable\r")?;
                        continue;
                    }
                    conn.touch();
                    return Ok(line);
                }
            }
        }
    }
}

// --------------------------------------------------------------------------- tests

#[cfg(test)]
pub mod fake {
    //! Un "equipo" en memoria: cuando le escriben algo que coincide con un guion, contesta.
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    pub struct State {
        pub incoming: VecDeque<u8>,
        pub written: Vec<u8>,
        /// (lo que tiene que terminar de llegar, la respuesta). Se dispara una vez cada una.
        pub script: Vec<(String, Vec<u8>, bool)>,
        pub closed: bool,
    }

    #[derive(Clone, Default)]
    pub struct FakeLink(pub Arc<Mutex<State>>);

    impl FakeLink {
        pub fn new(script: &[(&str, &str)]) -> FakeLink {
            let f = FakeLink::default();
            f.0.lock().unwrap().script = script.iter().map(|(t, r)| (t.to_string(), r.as_bytes().to_vec(), false)).collect();
            f
        }
        pub fn written(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap().written).into_owned()
        }
        pub fn push(&self, data: &[u8]) {
            self.0.lock().unwrap().incoming.extend(data);
        }
    }

    impl Link for FakeLink {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut s = self.0.lock().unwrap();
            if s.closed {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
            }
            let n = s.incoming.len().min(buf.len());
            for b in buf.iter_mut().take(n) {
                *b = s.incoming.pop_front().unwrap();
            }
            drop(s);
            if n == 0 {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(n)
        }
        fn write(&mut self, data: &[u8]) -> io::Result<()> {
            let mut s = self.0.lock().unwrap();
            s.written.extend_from_slice(data);
            let w = String::from_utf8_lossy(&s.written).into_owned();
            let mut reply = Vec::new();
            for (trigger, resp, fired) in s.script.iter_mut() {
                if !*fired && w.ends_with(trigger.as_str()) {
                    *fired = true;
                    reply.extend_from_slice(resp);
                    break;
                }
            }
            s.incoming.extend(reply);
            Ok(())
        }
        fn try_clone(&self) -> io::Result<Box<dyn Link>> {
            Ok(Box::new(self.clone()))
        }
        fn set_break(&mut self, on: bool) -> io::Result<()> {
            // Queda anotado entre lo escrito, y puede disparar una respuesta del guion.
            if on {
                self.write(b"<BRK>")?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeLink;
    use super::*;
    use crate::model::DEFAULT_PROMPT;

    fn re() -> Regex {
        Regex::new(DEFAULT_PROMPT).unwrap()
    }

    #[test]
    fn default_prompt_matches_common_devices() {
        let re = re();
        for p in ["Switch#", "Router>", "AP(config)#", "user@host:~$ ", "[edit]\nuser@fw# ", "SW-1.lab# ", "ubnt@ubnt:~$"] {
            assert!(re.is_match(last_line(p.as_bytes()).as_str()), "{p:?}");
        }
        for p in ["Username:", "Password: ", "interface GigabitEthernet0/1", " --More-- "] {
            assert!(!re.is_match(p), "{p:?}");
        }
    }

    #[test]
    fn collector_sees_prompt_only_after_echo() {
        let mut c = Collector::new(re(), 4096);
        // El eco del comando termina en `$`: no es el prompt.
        assert_eq!(c.feed(b"echo $"), Step::More);
        assert_eq!(c.feed(b"\r\n$\r\n"), Step::More);
        assert_eq!(c.feed(b"Switch#"), Step::Prompt);
        let (out, prompt) = c.finish("echo $", true);
        assert_eq!(out, "$");
        assert_eq!(prompt.as_deref(), Some("Switch#"));
    }

    #[test]
    fn collector_answers_pagers_and_drops_them() {
        let mut c = Collector::new(re(), 4096);
        assert_eq!(c.feed(b"show run\r\nline 1\r\nline 2\r\n --More-- "), Step::Page);
        // Cisco borra el `--More--` con retornos y espacios antes de seguir.
        assert_eq!(c.feed(b"\r        \rline 3\r\n"), Step::More);
        assert_eq!(c.feed(b"---(more 50%)---"), Step::Page);
        assert_eq!(c.feed(b"\r                \rline 4\r\nRouter> "), Step::Prompt);
        let (out, prompt) = c.finish("show run", true);
        assert_eq!(out, "line 1\nline 2\nline 3\nline 4");
        assert_eq!(prompt.as_deref(), Some("Router>"));
    }

    #[test]
    fn collector_truncates_but_still_finds_the_prompt() {
        let mut c = Collector::new(re(), 16);
        let mut step = c.feed(b"show x\r\n");
        for _ in 0..100 {
            step = c.feed(b"0123456789abcdef\r\n");
        }
        assert_eq!(step, Step::More);
        assert_eq!(c.feed(b"SW#"), Step::Prompt);
        assert!(c.truncated);
        let (out, prompt) = c.finish("show x", true);
        assert_eq!(out.len(), 16 - "show x\r\n".len());
        assert_eq!(prompt.as_deref(), Some("SW#"));
    }

    #[test]
    fn login_prompts_are_recognized() {
        assert_eq!(asks("Username: "), Some(Asks::Username));
        assert_eq!(asks("\r\nlogin:"), Some(Asks::Username));
        assert_eq!(asks("Password:"), Some(Asks::Password));
        assert_eq!(asks("Enter password: "), Some(Asks::Password));
        assert_eq!(asks("Switch#"), None);
        assert_eq!(asks("Description: uplink"), None);
    }

    #[test]
    fn exec_over_a_fake_device() {
        // El equipo frena en `--More--` hasta que llega una tecla, y recien ahi sigue.
        let link = FakeLink::new(&[
            ("show version\r", "show version\r\nCisco IOS Software, C2960\r\nuptime is 3 days\r\n --More-- "),
            (" ", "\r         \rconfig register 0xF\r\nSwitch#"),
        ]);
        let conn = Conn::new(Box::new(link.clone())).unwrap();
        let r = exec(&conn, "show version", &re(), Duration::from_secs(2), 64 * 1024).unwrap();
        assert!(!r.timed_out);
        assert_eq!(r.output, "Cisco IOS Software, C2960\nuptime is 3 days\nconfig register 0xF");
        assert_eq!(r.prompt.as_deref(), Some("Switch#"));
        // El comando fue con \r y el paginador se contesto con un espacio.
        assert_eq!(link.written(), "show version\r ");
    }

    #[test]
    fn exec_times_out_without_prompt() {
        let link = FakeLink::new(&[("reload\r", "reload\r\nProceed with reload? [confirm]")]);
        let conn = Conn::new(Box::new(link)).unwrap();
        let r = exec(&conn, "reload", &re(), Duration::from_millis(400), 4096).unwrap();
        assert!(r.timed_out);
        assert!(r.prompt.is_none());
        assert_eq!(r.output, "Proceed with reload? [confirm]");
    }

    #[test]
    fn ready_logs_in_and_enables() {
        let link = FakeLink::new(&[
            ("\r", "\r\nUser Access Verification\r\n\r\nUsername: "),
            ("admin\r", "admin\r\nPassword: "),
            ("s3cret\r", "\r\nSwitch>"),
            ("enable\r", "enable\r\nPassword: "),
            ("en4ble\r", "\r\nSwitch#"),
        ]);
        let conn = Conn::new(Box::new(link.clone())).unwrap();
        let login = Login {
            user: Some("admin".into()),
            password: Some(Zeroizing::new("s3cret".into())),
            enable: Some(Zeroizing::new("en4ble".into())),
        };
        let p = ready(&conn, &re(), &login, Duration::from_secs(3), None).unwrap();
        assert_eq!(p, "Switch#");
        assert_eq!(link.written(), "\radmin\rs3cret\renable\ren4ble\r");
        // Ya en el prompt: un Enter alcanza.
        link.0.lock().unwrap().script.push(("\r\r".into(), b"\r\nSwitch#".to_vec(), false));
        let p = ready(&conn, &re(), &login, Duration::from_secs(3), None).unwrap();
        assert_eq!(p, "Switch#");
    }

    #[test]
    fn ready_waits_the_whole_timeout_for_the_agent() {
        // Un equipo que no contesta: para el agente se espera el timeout del host entero.
        let conn = Conn::new(Box::new(FakeLink::new(&[]))).unwrap();
        let t = Instant::now();
        let e = ready(&conn, &re(), &Login::default(), Duration::from_secs(4), None).unwrap_err();
        assert_eq!(e.to_string(), "the console did not show a prompt");
        assert!(t.elapsed() >= Duration::from_millis(3900), "corto a los {:?}", t.elapsed());
    }

    #[test]
    fn ready_gives_up_early_on_a_silent_device_when_asked() {
        // Al abrir la terminal: si no llega ni un byte, se abandona a los `silent`.
        let conn = Conn::new(Box::new(FakeLink::new(&[]))).unwrap();
        let t = Instant::now();
        let e = ready(&conn, &re(), &Login::default(), Duration::from_secs(4), Some(Duration::from_millis(500))).unwrap_err();
        assert!(e.to_string().contains("did not answer"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(2), "tardo {:?}", t.elapsed());
    }

    #[test]
    fn ready_fails_without_credentials() {
        let link = FakeLink::new(&[("\r", "\r\nPassword: ")]);
        let conn = Conn::new(Box::new(link)).unwrap();
        let e = ready(&conn, &re(), &Login::default(), Duration::from_secs(2), None).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn taps_share_the_line_and_close_together() {
        let link = FakeLink::new(&[]);
        let conn = Conn::new(Box::new(link.clone())).unwrap();
        let a = conn.tap();
        let b = conn.tap();
        assert!(conn.in_use());
        link.push(b"hello");
        assert_eq!(a.recv(Duration::from_secs(1)).unwrap().as_deref(), Some(&b"hello"[..]));
        assert_eq!(b.recv(Duration::from_secs(1)).unwrap().as_deref(), Some(&b"hello"[..]));
        drop(b);
        conn.close();
        assert!(matches!(a.recv(Duration::from_secs(1)), Err(Closed)));
        assert!(conn.write(b"x").is_err());
        drop(a);
        assert!(!conn.in_use());
    }

    #[test]
    fn validate_catches_bad_config() {
        let ok = SerialConfig { port: "COM3".into(), ..Default::default() };
        assert!(validate(&ok).is_ok());
        // Sin puerto es "automatico".
        assert!(validate(&SerialConfig::default()).is_ok());
        assert!(validate(&SerialConfig { prompt: "[".into(), ..ok.clone() }).is_err());
        assert!(validate(&SerialConfig { data_bits: 9, ..ok.clone() }).is_err());
        assert!(validate(&SerialConfig { baud: 0, ..ok }).is_err());
    }

    #[test]
    fn ports_sort_naturally() {
        assert!(natural_key("COM3") < natural_key("COM10"));
        assert!(natural_key("/dev/ttyUSB0") < natural_key("/dev/ttyUSB1"));
    }

    #[test]
    fn only_usb_adapters_count_as_consoles() {
        assert!(is_console("COM10", "USB-Serial Controller", true));
        assert!(!is_console("COM7", "Bluetooth", false));
        // Algunos drivers de Bluetooth se anuncian como USB.
        assert!(!is_console("COM6", "Standard Serial over Bluetooth link", true));
        assert!(!is_console("COM1", "PCI", false));
        assert!(!is_console("COM1", "", false));
        // Linux sin libudev: por el nombre.
        assert!(is_console("/dev/ttyUSB0", "", false));
        assert!(is_console("/dev/ttyACM0", "", false));
        assert!(!is_console("/dev/ttyS0", "", false));
        assert!(!is_console("/dev/rfcomm0", "", false));
    }

    #[test]
    fn history_keeps_a_cursor_across_reads() {
        let link = FakeLink::new(&[]);
        let conn = Conn::new(Box::new(link.clone())).unwrap();
        let tap = conn.tap();
        link.push(b"System Bootstrap\r\n");
        tap.recv(Duration::from_secs(1)).unwrap();
        // Sin cursor: lo que hay.
        let a = conn.read_since(None, 1024);
        assert_eq!(a.data, b"System Bootstrap\r\n");
        assert!(!a.skipped);
        link.push(b"switch: ");
        tap.recv(Duration::from_secs(1)).unwrap();
        // Con el cursor anterior: solo lo nuevo.
        let b = conn.read_since(Some(a.cursor), 1024);
        assert_eq!(b.data, b"switch: ");
        assert_eq!(b.cursor, a.cursor + 8);
        // Nada nuevo.
        assert!(conn.read_since(Some(b.cursor), 1024).data.is_empty());
        // Con un tope: los ultimos bytes, avisando que falta algo.
        let c = conn.read_since(Some(0), 4);
        assert_eq!(c.data, b"ch: ");
        assert!(c.skipped);
    }

    #[test]
    fn history_drops_the_oldest_and_says_so() {
        let mut h = History::default();
        h.push(&vec![b'a'; HISTORY]);
        h.push(b"bbbb");
        assert_eq!(h.start, 4);
        assert_eq!(h.end(), HISTORY as u64 + 4);
        assert_eq!(h.buf.len(), HISTORY);
    }

    #[test]
    fn keys_by_name() {
        assert_eq!(key("Enter"), Some(Key::Bytes(b"\r".to_vec())));
        assert_eq!(key("ctrl-c"), Some(Key::Bytes(vec![3])));
        assert_eq!(key("Ctrl+Z"), Some(Key::Bytes(vec![26])));
        assert_eq!(key("ctrl-shift-6"), Some(Key::Bytes(vec![0x1e])));
        assert_eq!(key("break"), Some(Key::Break));
        assert_eq!(key("up"), Some(Key::Bytes(b"\x1b[A".to_vec())));
        assert_eq!(key("ctrl-cc"), None);
        assert_eq!(key("f13"), None);
    }

    #[test]
    fn console_text_reads_like_the_screen() {
        assert_eq!(console_text(b"\x1b[1mSwitch\x1b[0m#\r\n"), "Switch#\n");
        // El paginador que el equipo borra con \r y espacios no queda.
        assert_eq!(console_text(b"line 1\r\n --More-- \r          \rline 2\r\n"), "line 1\nline 2\n");
        // Retrocesos y campanas.
        assert_eq!(console_text(b"shwo\x08\x08ow\x07 ver\r\n"), "show ver\n");
        // Un \r que reescribe con algo mas corto deja la cola, como la pantalla.
        assert_eq!(console_text(b"Loading 10%\rLoading 100%"), "Loading 100%");
    }

    #[test]
    fn listen_returns_when_the_console_goes_quiet() {
        let link = FakeLink::new(&[("<BRK>", "\r\nswitch: ")]);
        let conn = Conn::new(Box::new(link.clone())).unwrap();
        let from = conn.cursor();
        conn.send_break(Duration::from_millis(10)).unwrap();
        let t = Instant::now();
        let got = listen(&conn, from, Duration::from_secs(5), Duration::from_millis(300), 4096).unwrap();
        assert_eq!(console_text(&got.data), "\nswitch: ");
        assert!(t.elapsed() < Duration::from_secs(2), "espero de mas: {:?}", t.elapsed());
        assert_eq!(link.written(), "<BRK>");
        // Si no llega nada, espera hasta el tope.
        let t = Instant::now();
        let none = listen(&conn, got.cursor, Duration::from_millis(400), Duration::from_millis(100), 4096).unwrap();
        assert!(none.data.is_empty());
        assert!(t.elapsed() >= Duration::from_millis(390));
    }
}
