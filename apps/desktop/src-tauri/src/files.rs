//! Comandos del explorador SFTP: una sesion por pestaña, transferencias con progreso y
//! cancelacion. Todo lo remoto exige el vault abierto y queda en la auditoria.

use ethereon_core::audit::Entry;
use ethereon_core::files::{self, FileEntry, Sftp};
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
    sessions: Mutex<HashMap<u32, (String, Arc<Sftp>, usize)>>,
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
}

fn session(s: &State<AppState>, id: u32) -> Result<(String, Arc<Sftp>), Error> {
    s.broker.read(|_| ())?; // con el vault cerrado no se toca nada remoto
    let (host, sftp, locks) = s.sftps.sessions.lock().unwrap().get(&id).cloned().ok_or(Error::NotFound("SFTP session".into()))?;
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

fn audit(s: &State<AppState>, host: &str, what: String, ok: &Result<(), Error>) {
    s.broker.audit_log(Entry {
        device: "ui".into(),
        host: host.into(),
        command: format!("[sftp] {what}"),
        decision: if ok.is_ok() { "allowed" } else { "error" }.into(),
        detail: ok.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
        ..Default::default()
    });
}

#[tauri::command]
pub async fn sftp_open(s: State<'_, AppState>, host: String) -> Result<Opened, Error> {
    let b = s.broker.clone();
    b.touch_ui();
    let h = b.read(|d| d.hosts.iter().find(|x| x.name == host).cloned())?.ok_or(Error::NotFound(host.clone()))?;
    let conn = b.connection(&h).await?;
    let locks = b.lock_count();
    let sftp = Sftp::open(conn).await?;
    let home = sftp.home().await.unwrap_or_else(|_| "/".into());
    let id = s.sftps.next.fetch_add(1, Ordering::Relaxed) + 1;
    s.sftps.sessions.lock().unwrap().insert(id, (host.clone(), Arc::new(sftp), locks));
    audit(&s, &host, "open".into(), &Ok(()));
    Ok(Opened { id, home })
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
    audit(&s, &host, format!("mkdir {path}"), &r);
    r
}

#[tauri::command]
pub async fn sftp_rename(s: State<'_, AppState>, id: u32, from: String, to: String) -> Result<(), Error> {
    let (host, sftp) = session(&s, id)?;
    let r = sftp.rename(&from, &to).await;
    audit(&s, &host, format!("rename {from} -> {to}"), &r);
    r
}

#[tauri::command]
pub async fn sftp_remove(s: State<'_, AppState>, id: u32, paths: Vec<String>) -> Result<(), Error> {
    let (host, sftp) = session(&s, id)?;
    for p in paths {
        let r = sftp.remove(&p).await;
        audit(&s, &host, format!("delete {p}"), &r);
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

/// `upload`: de rutas locales a la carpeta remota `into`. Si no, de rutas remotas a la
/// carpeta local `into`. Devuelve el id de la transferencia (para cancelarla) y sigue en
/// segundo plano.
#[tauri::command]
pub async fn sftp_transfer(
    s: State<'_, AppState>,
    id: u32,
    upload: bool,
    paths: Vec<String>,
    into: String,
    on_event: Channel<XferEvent>,
) -> Result<u32, Error> {
    let (host, sftp) = session(&s, id)?;
    let total = if upload {
        paths.iter().map(|p| files::size_local(&PathBuf::from(p))).sum()
    } else {
        let mut t = 0;
        for p in &paths {
            t += sftp.size_of(p).await.unwrap_or(0);
        }
        t
    };
    let tid = s.sftps.next.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    s.sftps.transfers.lock().unwrap().insert(tid, cancel.clone());
    let broker = s.broker.clone();
    let what = format!("{} {} item(s) {} {into}", if upload { "upload" } else { "download" }, paths.len(), if upload { "to" } else { "to local" });
    let meter = Meter { done: AtomicU64::new(0), total, last: Mutex::new(Instant::now() - Duration::from_secs(1)), ch: on_event.clone() };
    let _ = on_event.send(XferEvent::Progress { done: 0, total, file: String::new() });

    let transfers = s.sftps.transfers.clone();
    tauri::async_runtime::spawn(async move {
        let progress = |n: u64, f: &str| meter.add(n, f);
        let mut result = Ok(());
        for p in &paths {
            result = if upload {
                sftp.upload(&PathBuf::from(p), &into, &progress, &cancel).await
            } else {
                sftp.download(p, &PathBuf::from(&into), &progress, &cancel).await
            };
            if result.is_err() {
                break;
            }
        }
        broker.audit_log(Entry {
            device: "ui".into(),
            host,
            command: format!("[sftp] {what}: {}", paths.join(", ")),
            decision: if result.is_ok() { "allowed" } else { "error" }.into(),
            detail: result.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
            ..Default::default()
        });
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
