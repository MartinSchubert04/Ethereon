//! Terminal interactiva para la persona (no para el agente: no pasa por las reglas).
//! La salida va al WebView por un canal IPC en base64; si la ventana se cierra, el
//! canal falla y la sesion se corta sola.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use russh::client::Msg;
use russh::{ChannelMsg, ChannelWriteHalf};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::State;
use ethereon_core::audit::Entry;
use ethereon_core::Error;

use crate::AppState;

#[derive(Default)]
pub struct Terms {
    map: Arc<Mutex<HashMap<u32, Arc<ChannelWriteHalf<Msg>>>>>,
    next: AtomicU32,
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
    let id = s.terms.next.fetch_add(1, Ordering::Relaxed) + 1;
    s.terms.map.lock().unwrap().insert(id, Arc::new(write));
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
            if on_event.send(TermEvent::Data { b64: B64.encode(&data[..]) }).is_err() {
                break; // la ventana ya no esta
            }
        }
        let _ = on_event.send(TermEvent::Exit { code });
        map.lock().unwrap().remove(&id);
    });
    Ok(id)
}

fn get(s: &State<AppState>, id: u32) -> Result<Arc<ChannelWriteHalf<Msg>>, Error> {
    s.terms.map.lock().unwrap().get(&id).cloned().ok_or(Error::NotFound("terminal session".into()))
}

#[tauri::command]
pub async fn term_write(s: State<'_, AppState>, id: u32, data: String) -> Result<(), Error> {
    let w = get(&s, id)?;
    w.data(data.as_bytes()).await.map_err(|e| Error::Invalid(e.to_string()))
}

#[tauri::command]
pub async fn term_resize(s: State<'_, AppState>, id: u32, cols: u32, rows: u32) -> Result<(), Error> {
    let w = get(&s, id)?;
    w.window_change(cols, rows, 0, 0).await.map_err(|e| Error::Invalid(e.to_string()))
}

#[tauri::command]
pub async fn term_close(s: State<'_, AppState>, id: u32) -> Result<(), Error> {
    if let Some(w) = s.terms.map.lock().unwrap().remove(&id) {
        tauri::async_runtime::spawn(async move {
            let _ = w.close().await;
        });
    }
    Ok(())
}
