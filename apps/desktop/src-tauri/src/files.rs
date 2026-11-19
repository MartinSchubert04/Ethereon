//! Comandos del explorador de archivos: una sesion por pestaña (SFTP, SCP o FTP/FTPS segun
//! el host, ver `ethereon_core::remote`), transferencias con progreso y cancelacion. Todo
//! lo remoto exige el vault abierto y queda en la auditoria.

use ethereon_core::audit::Entry;
use ethereon_core::files::{self, FileEntry};
use ethereon_core::remote::Remote;
use ethereon_core::Error;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::State;

use crate::AppState;

#[derive(Default)]
pub struct Sftps {
    /// id -> (host, sesion, bloqueos del vault cuando se abrio)
    sessions: Mutex<HashMap<u32, (String, Arc<Remote>, usize)>>,
    transfers: Arc<Mutex<HashMap<u32, Arc<AtomicBool>>>>,
    next: AtomicU32,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum XferEvent {
    Progress { done: u64, total: u64, file: String },
    Done,
    Error { message: String },
}

#[derive(Serialize)]
pub struct Opened {
    id: u32,
    home: String,
    /// "SFTP", "SCP", "FTP" o "FTPS".
    proto: &'static str,
}

fn session(s: &State<AppState>, id: u32) -> Result<(String, Arc<Remote>), Error> {
    s.broker.read(|_| ())?; // con el vault cerrado no se toca nada remoto
    let (host, sftp, locks) = s.sftps.sessions.lock().unwrap().get(&id).cloned().ok_or(Error::NotFound("file session".into()))?;
    if locks != s.broker.lock_count() {
        s.sftps.sessions.lock().unwrap().remove(&id);
        return Err(Error::Invalid("the vault was locked since this tab opened. Reopen the tab.".into()));
    }
    if sftp.is_closed() {
        return Err(Error::Invalid("the connection was closed. Reopen the tab.".into()));
    }
    s.broker.touch_ui();
    Ok((host, sftp))
}

fn audit(s: &State<AppState>, host: &str, proto: &str, what: String, ok: &Result<(), Error>) {
    s.broker.audit_log(Entry {
        device: "ui".into(),
        host: host.into(),
        command: format!("[{}] {what}", proto.to_lowercase()),
        decision: if ok.is_ok() { "allowed" } else { "error" }.into(),
        detail: ok.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
        ..Default::default()
    });
}

#[tauri::command]
pub async fn sftp_open(s: State<'_, AppState>, host: String) -> Result<Opened, Error> {
    let b = s.broker.clone();
    let locks = b.lock_count();
    let remote = b.open_remote(&host).await?;
    let proto = remote.proto();
    let home = remote.home().await.unwrap_or_else(|_| "/".into());
    let id = s.sftps.next.fetch_add(1, Ordering::Relaxed) + 1;
    s.sftps.sessions.lock().unwrap().insert(id, (host.clone(), Arc::new(remote), locks));
    audit(&s, &host, proto, "open".into(), &Ok(()));
    Ok(Opened { id, home, proto })
}

#[tauri::command]
pub async fn sftp_close(s: State<'_, AppState>, id: u32) -> Result<(), Error> {
    s.sftps.sessions.lock().unwrap().remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn sftp_list(s: State<'_, AppState>, id: u32, path: String) -> Result<Vec<FileEntry>, Error> {
    let (_, sftp) = session(&s, id)?;
    sftp.list(&path).await
}

#[tauri::command]
pub async fn sftp_realpath(s: State<'_, AppState>, id: u32, path: String) -> Result<String, Error> {
    let (_, sftp) = session(&s, id)?;
    sftp.canonical(&path).await
}

#[tauri::command]
pub async fn sftp_mkdir(s: State<'_, AppState>, id: u32, path: String) -> Result<(), Error> {
    let (host, sftp) = session(&s, id)?;
    let r = sftp.mkdir(&path).await;
    audit(&s, &host, sftp.proto(), format!("mkdir {path}"), &r);
    r
}

#[tauri::command]
pub async fn sftp_rename(s: State<'_, AppState>, id: u32, from: String, to: String) -> Result<(), Error> {
    let (host, sftp) = session(&s, id)?;
    let r = sftp.rename(&from, &to).await;
    audit(&s, &host, sftp.proto(), format!("rename {from} -> {to}"), &r);
    r
}

#[tauri::command]
pub async fn sftp_remove(s: State<'_, AppState>, id: u32, paths: Vec<String>) -> Result<(), Error> {
    let (host, sftp) = session(&s, id)?;
    for p in paths {
        let r = sftp.remove(&p).await;
        audit(&s, &host, sftp.proto(), format!("delete {p}"), &r);
        r?;
    }
    Ok(())
}

/// Avisa el progreso como mucho cada 100 ms para no inundar la ventana.
struct Meter {
    done: AtomicU64,
    total: u64,
    last: Mutex<Instant>,
    ch: Channel<XferEvent>,
}

impl Meter {
    fn add(&self, n: u64, file: &str) {
        let done = self.done.fetch_add(n, Ordering::Relaxed) + n;
        let mut last = self.last.lock().unwrap();
        if last.elapsed() >= Duration::from_millis(100) || done >= self.total {
            *last = Instant::now();
            let _ = self.ch.send(XferEvent::Progress { done, total: self.total, file: file.into() });
        }
    }
}

/// Pasa `paths` de un lado al otro. Cada lado es una sesion de archivos (su id) o esta PC
/// (`None`): subir, bajar, de un servidor a otro (sin tocar el disco local) o copiar
/// dentro de esta PC. Devuelve el id de la transferencia (para cancelarla) y sigue en
/// segundo plano.
#[tauri::command]
pub async fn files_transfer(
    s: State<'_, AppState>,
    from: Option<u32>,
    to: Option<u32>,
    paths: Vec<String>,
    into: String,
    on_event: Channel<XferEvent>,
) -> Result<u32, Error> {
    let src = from.map(|id| session(&s, id)).transpose()?;
    let dst = to.map(|id| session(&s, id)).transpose()?;
    let total = match &src {
        None => paths.iter().map(|p| files::size_local(&PathBuf::from(p))).sum(),
        Some((_, sftp)) => {
            let mut t = 0;
            for p in &paths {
                t += sftp.size_of(p).await.unwrap_or(0);
            }
            t
        }
    };
    let tid = s.sftps.next.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    s.sftps.transfers.lock().unwrap().insert(tid, cancel.clone());
    let broker = s.broker.clone();
    let name = |e: &Option<(String, Arc<Remote>)>| e.as_ref().map(|(h, _)| h.clone()).unwrap_or_else(|| "this PC".into());
    let proto = src.as_ref().or(dst.as_ref()).map(|(_, r)| r.proto().to_lowercase()).unwrap_or_else(|| "files".into());
    let what = format!("copy {} item(s) from {} to {}:{into}", paths.len(), name(&src), name(&dst));
    // Queda en la auditoria de cada servidor que toca.
    let hosts: Vec<String> = [&src, &dst].iter().filter_map(|e| e.as_ref().map(|(h, _)| h.clone())).collect();
    let meter = Meter { done: AtomicU64::new(0), total, last: Mutex::new(Instant::now() - Duration::from_secs(1)), ch: on_event.clone() };
    let _ = on_event.send(XferEvent::Progress { done: 0, total, file: String::new() });

    let transfers = s.sftps.transfers.clone();
    tauri::async_runtime::spawn(async move {
        let progress = |n: u64, f: &str| meter.add(n, f);
        let mut result = Ok(());
        for p in &paths {
            result = match (&src, &dst) {
                (None, Some((_, b))) => b.upload(&PathBuf::from(p), &into, &progress, &cancel).await,
                (Some((_, a)), None) => a.download(p, &PathBuf::from(&into), &progress, &cancel).await,
                (Some((_, a)), Some((_, b))) => a.copy_to(b, p, &into, &progress, &cancel).await,
                (None, None) => files::copy_local(&PathBuf::from(p), &PathBuf::from(&into), &progress, &cancel).await,
            };
            if result.is_err() {
                break;
            }
        }
        for host in hosts {
            broker.audit_log(Entry {
                device: "ui".into(),
                host,
                command: format!("[{proto}] {what}: {}", paths.join(", ")),
                decision: if result.is_ok() { "allowed" } else { "error" }.into(),
                detail: result.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
                ..Default::default()
            });
        }
        let _ = on_event.send(match result {
            Ok(()) => XferEvent::Done,
            Err(e) => XferEvent::Error { message: e.to_string() },
        });
        transfers.lock().unwrap().remove(&tid);
    });
    Ok(tid)
}

#[tauri::command]
pub fn sftp_cancel(s: State<'_, AppState>, transfer: u32) {
    if let Some(c) = s.sftps.transfers.lock().unwrap().get(&transfer) {
        c.store(true, Ordering::Relaxed);
    }
}

// ---- disco local (el panel izquierdo)

#[tauri::command]
pub fn local_home() -> String {
    files::home_local().to_string_lossy().to_string()
}

#[tauri::command]
pub fn local_list(path: String) -> Result<Vec<FileEntry>, Error> {
    files::list_local(&path)
}

#[tauri::command]
pub fn local_mkdir(path: String) -> Result<(), Error> {
    std::fs::create_dir(&path).map_err(|e| Error::Invalid(e.to_string()))
}

#[tauri::command]
pub fn local_rename(from: String, to: String) -> Result<(), Error> {
    std::fs::rename(&from, &to).map_err(|e| Error::Invalid(e.to_string()))
}

#[tauri::command]
pub fn local_remove(paths: Vec<String>) -> Result<(), Error> {
    for p in paths {
        files::remove_local(&PathBuf::from(p))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- servidor TFTP

#[derive(Serialize)]
pub struct TftpStatus {
    running: bool,
    dir: String,
    port: u16,
    write: bool,
    /// Las IPv4 de esta PC, para el `copy tftp://<ip>/...` del equipo.
    addrs: Vec<String>,
}

fn local_ipv4s() -> Vec<String> {
    // Sin mandar nada: un socket UDP "conectado" dice por que interfaz saldria.
    let mut out = Vec::new();
    for probe in ["192.168.0.1:9", "10.0.0.1:9", "172.16.0.1:9", "8.8.8.8:9"] {
        if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if s.connect(probe).is_ok() {
                if let Ok(a) = s.local_addr() {
                    let ip = a.ip().to_string();
                    if !ip.starts_with("0.") && !out.contains(&ip) {
                        out.push(ip);
                    }
                }
            }
        }
    }
    out
}

async fn tftp_now(s: &State<'_, AppState>) -> TftpStatus {
    let g = s.tftp.lock().await;
    match g.as_ref() {
        Some(t) => TftpStatus { running: true, dir: t.dir.display().to_string(), port: t.port, write: t.write, addrs: local_ipv4s() },
        None => TftpStatus { running: false, dir: String::new(), port: 69, write: false, addrs: local_ipv4s() },
    }
}

/// Prende el servidor TFTP sirviendo `dir`. Los pedidos llegan como evento "tftp".
#[tauri::command]
pub async fn tftp_start(app: tauri::AppHandle, s: State<'_, AppState>, dir: String, port: Option<u16>, write: bool) -> Result<TftpStatus, Error> {
    use tauri::Emitter;
    let mut g = s.tftp.lock().await;
    if let Some(old) = g.take() {
        old.stop();
    }
    let log: ethereon_core::tftp::Log = std::sync::Arc::new(move |e| {
        let _ = app.emit("tftp", e);
    });
    let srv = ethereon_core::tftp::TftpServer::start(PathBuf::from(&dir), port.unwrap_or(69), write, log).await.map_err(|e| Error::Invalid(e.to_string()))?;
    *g = Some(srv);
    drop(g);
    Ok(tftp_now(&s).await)
}

#[tauri::command]
pub async fn tftp_stop(s: State<'_, AppState>) -> Result<TftpStatus, Error> {
    if let Some(t) = s.tftp.lock().await.take() {
        t.stop();
    }
    Ok(tftp_now(&s).await)
}

#[tauri::command]
pub async fn tftp_status(s: State<'_, AppState>) -> Result<TftpStatus, Error> {
    Ok(tftp_now(&s).await)
}
