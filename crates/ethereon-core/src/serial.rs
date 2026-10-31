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
}

/// Cuanto espera cada lectura del puerto. Tambien es el tope de la demora de una escritura
/// (en Windows el puerto atiende de a una operacion por vez).
const READ_WAIT: Duration = Duration::from_millis(50);

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
}

/// Abre el puerto con los parametros del host. Si otro programa (o una conexion que se esta
/// cerrando) todavia lo tiene, reintenta un momento.
pub fn open(cfg: &SerialConfig) -> io::Result<Box<dyn Link>> {
    use serialport::{DataBits, FlowControl, StopBits};
    if cfg.port.trim().is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no serial port configured"));
    }
    let builder = serialport::new(cfg.port.trim(), cfg.baud.max(1))
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
}

pub fn available_ports() -> Vec<PortInfo> {
    use serialport::SerialPortType;
    let mut out: Vec<PortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let label = match p.port_type {
                SerialPortType::UsbPort(u) => u.product.or(u.manufacturer).unwrap_or_else(|| "USB".into()),
                SerialPortType::BluetoothPort => "Bluetooth".into(),
                SerialPortType::PciPort => "PCI".into(),
                SerialPortType::Unknown => String::new(),
            };
            PortInfo { name: p.port_name, label }
        })
        .collect();
    out.sort_by(|a, b| natural_key(&a.name).cmp(&natural_key(&b.name)));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// "COM10" despues de "COM3".
fn natural_key(s: &str) -> (String, u64) {
    let digits: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    (s[..s.len() - digits.len()].to_string(), digits.parse().unwrap_or(0))
}

/// Revisa lo que se puede antes de guardar un host serial.
pub fn validate(cfg: &SerialConfig) -> Result<(), String> {
    if cfg.port.trim().is_empty() {
        return Err("pick a serial port".into());
    }
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

/// Un puerto abierto, compartido por la terminal y el agente.
pub struct Conn {
    writer: Mutex<Box<dyn Link>>,
    taps: Taps,
    next_tap: AtomicU64,
    closed: Arc<AtomicBool>,
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
        let (t, c) = (taps.clone(), closed.clone());
        std::thread::Builder::new().name("serial-reader".into()).spawn(move || {
            let mut buf = [0u8; 4096];
            while !c.load(Ordering::SeqCst) {
                match reader.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        let data = &buf[..n];
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
            turn: Mutex::new(()),
            last_used: Mutex::new(Instant::now()),
        })
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
pub fn ready(conn: &Conn, prompt: &Regex, login: &Login, timeout: Duration) -> io::Result<String> {
    let _turn = conn.turn.lock().unwrap();
    let tap = conn.tap();
    let mut col = Collector::new(prompt.clone(), 64 * 1024);
    conn.write(b"\r")?;
    let deadline = Instant::now() + timeout;
    let mut sent_user = 0;
    let mut sent_pw = 0;
    let mut enabled = false;
    let mut settling: Option<Instant> = None;
    let fail = |m: &str| io::Error::new(io::ErrorKind::PermissionDenied, m.to_string());
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "the console did not show a prompt"));
        }
        let wait = match settling {
            Some(s) => SETTLE.saturating_sub(now.duration_since(s)),
            None => (deadline - now).min(Duration::from_millis(100)),
        };
        let step = match tap.recv(wait) {
            Ok(Some(d)) => Some(col.feed(&d)),
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
        let p = ready(&conn, &re(), &login, Duration::from_secs(3)).unwrap();
        assert_eq!(p, "Switch#");
        assert_eq!(link.written(), "\radmin\rs3cret\renable\ren4ble\r");
        // Ya en el prompt: un Enter alcanza.
        link.0.lock().unwrap().script.push(("\r\r".into(), b"\r\nSwitch#".to_vec(), false));
        let p = ready(&conn, &re(), &login, Duration::from_secs(3)).unwrap();
        assert_eq!(p, "Switch#");
    }

    #[test]
    fn ready_fails_without_credentials() {
        let link = FakeLink::new(&[("\r", "\r\nPassword: ")]);
        let conn = Conn::new(Box::new(link)).unwrap();
        let e = ready(&conn, &re(), &Login::default(), Duration::from_secs(2)).unwrap_err();
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
        assert!(validate(&SerialConfig::default()).is_err());
        assert!(validate(&SerialConfig { prompt: "[".into(), ..ok.clone() }).is_err());
        assert!(validate(&SerialConfig { data_bits: 9, ..ok.clone() }).is_err());
        assert!(validate(&SerialConfig { baud: 0, ..ok }).is_err());
    }

    #[test]
    fn ports_sort_naturally() {
        assert!(natural_key("COM3") < natural_key("COM10"));
        assert!(natural_key("/dev/ttyUSB0") < natural_key("/dev/ttyUSB1"));
    }
}
