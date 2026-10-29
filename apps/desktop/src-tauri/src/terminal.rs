//! Terminal interactiva para la persona (no para el agente: no pasa por las reglas).
//! Puede ser una shell en un host por SSH o una shell local de esta PC (ConPTY).
//! La salida va al WebView por un canal IPC en base64; si la ventana se cierra, el
//! canal falla y la sesion se corta sola. Una terminal se puede pasar a otra ventana sin
//! cortarse: la vieja suelta el canal (`term_detach`) y la nueva lo toma (`term_attach`).

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use russh::client::Msg;
use russh::{ChannelMsg, ChannelWriteHalf};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::State;
use ethereon_core::audit::Entry;
use ethereon_core::Error;

use crate::AppState;

/// Una shell local: el lado "master" del PTY, por donde se escribe y se cambia el tamaño.
struct Local {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Clone)]
enum Session {
    Ssh(Arc<ChannelWriteHalf<Msg>>),
    Local(Arc<Mutex<Local>>),
}

/// Cuanto de la salida reciente se guarda para la ventana que toma la terminal.
const BACKLOG: usize = 512 * 1024;

/// A donde va la salida de una sesion. Entre que una ventana la suelta y otra la toma,
/// la salida se sigue guardando; la ventana nueva arranca con eso y la pantalla queda igual.
struct Sink {
    ch: Option<Channel<TermEvent>>,
    backlog: VecDeque<u8>,
    /// Soltada a proposito, esperando otra ventana.
    held: bool,
}

impl Sink {
    fn new(ch: Channel<TermEvent>) -> Arc<Mutex<Sink>> {
        Arc::new(Mutex::new(Sink { ch: Some(ch), backlog: VecDeque::new(), held: false }))
    }

    /// Devuelve false si la ventana se cerro (y nadie la esta esperando): hay que cortar.
    fn push(&mut self, data: &[u8]) -> bool {
        self.backlog.extend(data);
        let extra = self.backlog.len().saturating_sub(BACKLOG);
        self.backlog.drain(..extra);
        match &self.ch {
            Some(ch) => {
                if ch.send(TermEvent::Data { b64: B64.encode(data) }).is_ok() {
                    true
                } else {
                    self.ch = None;
                    self.held
                }
            }
            None => self.held,
        }
    }

    fn exit(&mut self, code: Option<u32>) {
        if let Some(ch) = &self.ch {
            let _ = ch.send(TermEvent::Exit { code });
        }
    }
}

struct Live {
    session: Session,
    sink: Arc<Mutex<Sink>>,
}

#[derive(Default)]
pub struct Terms {
    map: Arc<Mutex<HashMap<u32, Live>>>,
    next: AtomicU32,
}

impl Terms {
    fn add(&self, session: Session, sink: Arc<Mutex<Sink>>) -> u32 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.map.lock().unwrap().insert(id, Live { session, sink });
        id
    }
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TermEvent {
    Data { b64: String },
    Exit { code: Option<u32> },
}

#[tauri::command]
pub async fn term_open(s: State<'_, AppState>, host: String, cols: u32, rows: u32, on_event: Channel<TermEvent>) -> Result<u32, Error> {
    let b = s.broker.clone();
    b.touch_ui();
    let h = b.read(|d| d.hosts.iter().find(|x| x.name == host).cloned())?.ok_or(Error::NotFound(host.clone()))?;
    let conn = b.connection(&h).await?;
    let ch = ethereon_core::ssh::open_shell(&conn, cols, rows).await.map_err(|e| Error::Invalid(format!("ssh error: {e}")))?;
    let (mut read, write) = ch.split();
    let sink = Sink::new(on_event);
    let id = s.terms.add(Session::Ssh(Arc::new(write)), sink.clone());
    b.audit_log(Entry {
        device: "ui".into(),
        host: host.clone(),
        command: "[interactive terminal]".into(),
        decision: "allowed".into(),
        ..Default::default()
    });

    let map = s.terms.map.clone();
    tauri::async_runtime::spawn(async move {
        let mut code = None;
        while let Some(msg) = read.wait().await {
            conn.touch();
            let data = match msg {
                ChannelMsg::Data { data } => data,
                ChannelMsg::ExtendedData { data, .. } => data,
                ChannelMsg::ExitStatus { exit_status } => {
                    code = Some(exit_status);
                    continue;
                }
                ChannelMsg::Close | ChannelMsg::Eof => break,
                _ => continue,
            };
            if !sink.lock().unwrap().push(&data[..]) {
                break; // la ventana ya no esta
            }
        }
        sink.lock().unwrap().exit(code);
        map.lock().unwrap().remove(&id);
    });
    Ok(id)
}

// ---- shells locales

#[derive(Clone, Serialize)]
pub struct LocalShell {
    pub id: String,
    pub label: String,
}

/// Las shells que hay instaladas, en orden de preferencia.
fn shells() -> Vec<(LocalShell, PathBuf, Vec<String>)> {
    let mut out = Vec::new();
    let mut add = |id: &str, label: &str, path: Option<PathBuf>, args: &[&str]| {
        if let Some(p) = path.filter(|p| p.is_file()) {
            out.push((LocalShell { id: id.into(), label: label.into() }, p, args.iter().map(|s| s.to_string()).collect()));
        }
    };
    #[cfg(windows)]
    {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let sys = env("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
        add("pwsh", "PowerShell 7", which("pwsh.exe"), &["-NoLogo"]);
        add("powershell", "Windows PowerShell", Some(sys.join(r"WindowsPowerShell\v1.0\powershell.exe")), &["-NoLogo"]);
        add("cmd", "Command Prompt", Some(sys.join("cmd.exe")), &[]);
        add("wsl", "WSL", Some(sys.join("wsl.exe")), &["--cd", "~"]);
        let git = env("ProgramFiles").map(|p| p.join(r"Git\bin\bash.exe"));
        add("gitbash", "Git Bash", git, &["--login", "-i"]);
    }
    #[cfg(not(windows))]
    {
        let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        add("shell", "Shell", Some(PathBuf::from(sh)), &["-l"]);
    }
    out
}

#[cfg(windows)]
fn which(exe: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(exe)).find(|p| p.is_file())
}

#[tauri::command]
pub fn local_shells() -> Vec<LocalShell> {
    shells().into_iter().map(|(s, ..)| s).collect()
}

#[tauri::command]
pub fn term_open_local(
    s: State<'_, AppState>,
    shell: String,
    cols: u32,
    rows: u32,
    cwd: Option<String>,
    on_event: Channel<TermEvent>,
) -> Result<u32, Error> {
    let (_, path, args) = shells().into_iter().find(|(x, ..)| x.id == shell).ok_or(Error::NotFound(shell))?;
    let pty = native_pty_system()
        .openpty(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 })
        .map_err(pty_err)?;
    let mut cmd = CommandBuilder::new(&path);
    cmd.args(&args);
    // La carpeta pedida (por ejemplo la del Explorador) o la del usuario.
    let start = cwd.map(PathBuf::from).filter(|p| p.is_dir()).or_else(|| {
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
    });
    if let Some(dir) = start {
        cmd.cwd(dir);
    }
    // portable-pty arma el PATH leyendo el registro, pero no expande entradas como
    // %USERPROFILE%\AppData\Local\Microsoft\WindowsApps: sin esto no aparecen oh-my-posh,
    // winget y compania.
    if let Some(p) = cmd.get_env("PATH").and_then(|p| p.to_str()).map(expand_vars) {
        cmd.env("PATH", p);
    }
    scrub_env(&mut cmd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let child = pty.slave.spawn_command(cmd).map_err(pty_err)?;
    drop(pty.slave); // si no, el lector nunca ve el fin cuando la shell sale
    let mut reader = pty.master.try_clone_reader().map_err(pty_err)?;
    let writer = pty.master.take_writer().map_err(pty_err)?;
    let local = Arc::new(Mutex::new(Local { master: pty.master, writer, child }));
    let sink = Sink::new(on_event);
    let id = s.terms.add(Session::Local(local.clone()), sink.clone());

    // Leer del PTY bloquea: va en un hilo propio.
    let map = s.terms.map.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if !sink.lock().unwrap().push(&buf[..n]) {
                        let _ = local.lock().unwrap().child.kill();
                        break;
                    }
                }
            }
        }
        let code = local.lock().unwrap().child.try_wait().ok().flatten().map(|st| st.exit_code());
        sink.lock().unwrap().exit(code);
        map.lock().unwrap().remove(&id);
    });
    Ok(id)
}

/// Saca las variables que hablan de la terminal desde la que se lanzo Ethereon y no de
/// esta: si Ethereon arranco desde Windows Terminal o desde Claude Code, la shell heredaba
/// WT_SESSION (el perfil activa cosas de Windows Terminal, como imagenes Sixel), NO_COLOR
/// (PowerShell saca los colores) y las de Claude Code. NO_COLOR y FORCE_COLOR se respetan
/// si la persona las puso en las variables de Windows. La shell sabe que esta en Ethereon
/// por TERM_PROGRAM.
fn scrub_env(cmd: &mut CommandBuilder) {
    let keys: Vec<String> = cmd.iter_full_env_as_str().map(|(k, _)| k.to_string()).collect();
    for k in keys {
        let up = k.to_ascii_uppercase();
        let foreign = up.starts_with("WT_")
            || up.starts_with("CLAUDE")
            || up.starts_with("TERM_PROGRAM")
            || ((up == "NO_COLOR" || up == "FORCE_COLOR") && !set_in_windows(&k));
        if foreign {
            cmd.env_remove(&k);
        }
    }
    cmd.env("TERM_PROGRAM", "Ethereon");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
}

/// Si la variable esta puesta en las variables de entorno de Windows (usuario o sistema).
#[cfg(windows)]
fn set_in_windows(name: &str) -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let user = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment");
    let system = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"System\CurrentControlSet\Control\Session Manager\Environment");
    [user, system].into_iter().flatten().any(|k| k.get_raw_value(name).is_ok())
}

#[cfg(not(windows))]
fn set_in_windows(_: &str) -> bool {
    false
}

/// Reemplaza %VAR% por su valor; las que no existen quedan como estaban.
fn expand_vars(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 && !after[..j].contains(';') => {
                match std::env::var(&after[..j]) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&rest[i..i + j + 2]),
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[test]
fn expands_path_vars() {
    std::env::set_var("ETH_T_HOME", r"C:\Users\x");
    assert_eq!(expand_vars(r"%ETH_T_HOME%\bin;C:\a;%NOPE_X%;50%"), r"C:\Users\x\bin;C:\a;%NOPE_X%;50%");
}

fn pty_err(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("local shell: {e}"))
}

// ---- comunes

fn get(s: &State<AppState>, id: u32) -> Result<Session, Error> {
    s.terms.map.lock().unwrap().get(&id).map(|e| e.session.clone()).ok_or(Error::NotFound("terminal session".into()))
}

fn close_session(session: Session) {
    match session {
        Session::Ssh(w) => {
            tauri::async_runtime::spawn(async move {
                let _ = w.close().await;
            });
        }
        Session::Local(l) => {
            let _ = l.lock().unwrap().child.kill();
        }
    }
}

/// La ventana que la tenia la suelta para pasarla a otra. Si en 30 s nadie la toma, se
/// cierra (por ejemplo si la ventana nueva no llego a abrirse).
#[tauri::command]
pub fn term_detach(s: State<'_, AppState>, id: u32) -> Result<(), Error> {
    let sink = s.terms.map.lock().unwrap().get(&id).map(|e| e.sink.clone()).ok_or(Error::NotFound("terminal session".into()))?;
    {
        let mut k = sink.lock().unwrap();
        k.held = true;
        k.ch = None;
    }
    let map = s.terms.map.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        if sink.lock().unwrap().held {
            if let Some(e) = map.lock().unwrap().remove(&id) {
                close_session(e.session);
            }
        }
    });
    Ok(())
}

/// Otra ventana toma la terminal: primero recibe lo ultimo que paso y despues sigue en vivo.
#[tauri::command]
pub fn term_attach(s: State<'_, AppState>, id: u32, on_event: Channel<TermEvent>) -> Result<(), Error> {
    let sink = s.terms.map.lock().unwrap().get(&id).map(|e| e.sink.clone()).ok_or(Error::NotFound("terminal session".into()))?;
    let mut k = sink.lock().unwrap();
    let back: Vec<u8> = k.backlog.iter().copied().collect();
    for chunk in back.chunks(64 * 1024) {
        let _ = on_event.send(TermEvent::Data { b64: B64.encode(chunk) });
    }
    k.ch = Some(on_event);
    k.held = false;
    Ok(())
}

#[tauri::command]
pub async fn term_write(s: State<'_, AppState>, id: u32, data: String) -> Result<(), Error> {
    match get(&s, id)? {
        Session::Ssh(w) => w.data(data.as_bytes()).await.map_err(|e| Error::Invalid(e.to_string())),
        Session::Local(l) => {
            let mut l = l.lock().unwrap();
            l.writer.write_all(data.as_bytes()).and_then(|_| l.writer.flush()).map_err(|e| Error::Invalid(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn term_resize(s: State<'_, AppState>, id: u32, cols: u32, rows: u32) -> Result<(), Error> {
    match get(&s, id)? {
        Session::Ssh(w) => w.window_change(cols, rows, 0, 0).await.map_err(|e| Error::Invalid(e.to_string())),
        Session::Local(l) => l
            .lock()
            .unwrap()
            .master
            .resize(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| Error::Invalid(e.to_string())),
    }
}

#[tauri::command]
pub async fn term_close(s: State<'_, AppState>, id: u32) -> Result<(), Error> {
    if let Some(e) = s.terms.map.lock().unwrap().remove(&id) {
        close_session(e.session);
    }
    Ok(())
}
