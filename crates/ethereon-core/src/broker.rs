//! El broker: tiene el vault abierto, abre las conexiones, aplica las reglas, pide
//! aprobaciones y anota todo. La interfaz y el servidor MCP solo hablan con esto.
//!
//! Lo que puede pedir un agente esta en `agent_*`: `agent_list_hosts`, `agent_run` y, si
//! el host lo tiene prendido en sus reglas (`Policy::files`), `agent_download` y
//! `agent_upload` por SFTP (los chequeos propios de un archivo estan en `transfer.rs`).
//! Todo lo demas (desbloquear, editar, exportar) lo llama solo la interfaz, que corre en
//! el mismo proceso.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

use crate::audit::{self, Audit, Entry};
use crate::mask::Masker;
use crate::model::*;
use crate::policy::{self, Verdict};
use crate::serial;
use crate::ssh::{self, Conn, Hop, SshError};
use crate::transfer::{self, Direction, DownloadResult, FileRequest, UploadResult};
use crate::{files, vault, Error, Result};

/// Como avisa el broker a quien lo aloja (la app de escritorio o el servidor).
pub trait Notifier: Send + Sync {
    /// Evento para la interfaz: "vault", "approvals", "alerts", "activity", "data", "connlog".
    fn event(&self, name: &str, payload: serde_json::Value);
    /// Notificacion del sistema (escritorio).
    fn notify(&self, title: &str, body: &str);
    /// Un sonido ("approval"), con volumen de 0 a 100. Quien no tiene parlantes no hace nada.
    fn sound(&self, _name: &str, _volume: u8) {}
}

/// Quien pide: un dispositivo (token) y, si se sabe, la sesion MCP.
#[derive(Clone, Debug)]
pub struct AgentCtx {
    pub device_id: String,
    pub device_name: String,
    pub client: String,
    pub session: String,
}

#[derive(Debug, Serialize)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub timed_out: bool,
    /// Cuantos secretos se taparon con ***.
    pub masked: usize,
    /// Si una persona cambio el comando antes de aprobarlo, el que corrio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_command: Option<String>,
    /// En una consola serial no hay codigo de salida: en su lugar, el prompt con el que
    /// termino el equipo (`Switch#`). None si vencio el tiempo sin verlo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApprovalView {
    pub id: u64,
    pub host: String,
    pub env: Env,
    pub command: String,
    pub sudo: bool,
    pub reason: String,
    pub device: String,
    pub warnings: Vec<String>,
    pub created: String,
    /// Un pedido de archivo (`download` / `upload`): que archivo, en que direccion y de que
    /// tamano. En ese caso `command` es solo un resumen y no se puede editar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Reject { note: Option<String> },
    /// Aprobar y dejar pasar lo que coincida con `pattern` por el resto de la sesion.
    ApprovePattern { pattern: String },
    /// Aprobar con el comando cambiado.
    ApproveEdited { command: String },
}

struct Pending {
    view: ApprovalView,
    session: String,
    tx: oneshot::Sender<Decision>,
}

struct Open {
    vault: vault::Unlocked,
    data: VaultData,
    masker: Arc<Masker>,
}

/// Una huella pendiente o un cambio de clave, para mostrar en la interfaz.
#[derive(Clone, Debug, Serialize)]
pub struct Alert {
    pub id: u64,
    pub ts: String,
    pub error: SshError,
}

pub struct Broker {
    dir: PathBuf,
    open: Mutex<Option<Open>>,
    audit: Audit,
    pool: tokio::sync::Mutex<HashMap<String, Arc<Conn>>>,
    /// Puertos serie abiertos, por host. Uno solo por puerto: lo comparten la terminal y
    /// el agente.
    serial_pool: Mutex<HashMap<String, Arc<serial::Conn>>>,
    approvals: Mutex<HashMap<u64, Pending>>,
    session_patterns: Mutex<HashMap<String, Vec<String>>>,
    rate: Mutex<HashMap<String, VecDeque<Instant>>>,
    alerts: Mutex<Vec<Alert>>,
    last_ui: Mutex<Instant>,
    seq: AtomicU64,
    /// Comandos del agente corriendo ahora (para no actualizar en el medio).
    running: AtomicUsize,
    /// Cuantas veces se bloqueo el vault. Lo que se abrio antes de un bloqueo (por ejemplo
    /// una sesion SFTP) no vuelve a servir despues de desbloquear.
    locks: AtomicUsize,
    notifier: Arc<dyn Notifier>,
}

/// Cuenta un comando en curso mientras vive.
struct RunGuard<'a>(&'a AtomicUsize);
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Un pedido rechazado por las reglas: queda como "denied" en la auditoria.
fn denied(entry: &mut Entry, msg: String) -> Error {
    entry.decision = "denied".into();
    Error::Invalid(msg)
}

fn too_big(size: u64, pol: &Policy, host: &str) -> String {
    format!(
        "rejected: the file is {}, more than the {} limit for `{host}` (the user can raise it in the host's agent rules)",
        transfer::human_size(size),
        transfer::human_size(pol.max_file_kb.max(1) as u64 * 1024)
    )
}

/// Corta una operacion de archivos al llegar el limite de tiempo del host.
async fn timed<T>(deadline: tokio::time::Instant, f: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout_at(deadline, f).await.unwrap_or_else(|_| Err(Error::Invalid("the file transfer timed out".into())))
}

fn token_hash(token: &str) -> String {
    audit::sha256_hex(token.as_bytes())
}

impl Broker {
    pub fn new(dir: PathBuf, notifier: Arc<dyn Notifier>) -> Arc<Broker> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Broker {
            audit: Audit::open(dir.join("audit.jsonl")),
            dir,
            open: Mutex::new(None),
            pool: Default::default(),
            serial_pool: Default::default(),
            approvals: Default::default(),
            session_patterns: Default::default(),
            rate: Default::default(),
            alerts: Default::default(),
            last_ui: Mutex::new(Instant::now()),
            seq: AtomicU64::new(1),
            running: AtomicUsize::new(0),
            locks: AtomicUsize::new(0),
            notifier,
        })
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join("vault.json")
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    fn next_id(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    // ------------------------------------------------------------------ vault

    pub fn vault_exists(&self) -> bool {
        vault::exists(&self.vault_path())
    }

    pub fn is_unlocked(&self) -> bool {
        self.open.lock().unwrap().is_some()
    }

    /// Guarda como estaba la interfaz (antes de actualizar). Solo para la interfaz.
    pub fn set_ui_session(&self, v: serde_json::Value) -> Result<()> {
        self.write(|d| {
            d.ui_session = Some(v);
            Ok(())
        })
    }

    /// Lo que guardo la interfaz antes de actualizar, una sola vez (despues se borra).
    pub fn take_ui_session(&self) -> Result<Option<serde_json::Value>> {
        if self.read(|d| d.ui_session.is_none())? {
            return Ok(None);
        }
        self.write(|d| Ok(d.ui_session.take()))
    }

    pub fn create(&self, password: &str) -> Result<()> {
        if self.vault_exists() {
            return Err(Error::Invalid("a vault already exists".into()));
        }
        let data = VaultData { denylist: policy::default_denylist(), ..Default::default() };
        let v = vault::create(&self.vault_path(), password, data.clone())?;
        let (_, data) = vault::open(&self.vault_path(), password)?;
        self.set_open(v, data);
        Ok(())
    }

    pub fn unlock(&self, password: &str) -> Result<()> {
        let (v, data) = vault::open(&self.vault_path(), password)?;
        self.set_open(v, data);
        Ok(())
    }

    fn set_open(&self, vault: vault::Unlocked, data: VaultData) {
        let masker = Arc::new(Masker::from_vault(&data));
        *self.open.lock().unwrap() = Some(Open { vault, data, masker });
        self.touch_ui();
        self.notifier.event("vault", serde_json::json!({ "unlocked": true }));
    }

    /// Bloquea: suelta la clave y los secretos, corta las conexiones y rechaza lo que
    /// estaba esperando aprobacion.
    pub async fn lock(&self) {
        let was = self.open.lock().unwrap().take().is_some();
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.pool.lock().await.clear();
        self.close_serial(|_| true);
        for (_, p) in self.approvals.lock().unwrap().drain() {
            let _ = p.tx.send(Decision::Reject { note: Some("the vault was locked".into()) });
        }
        self.session_patterns.lock().unwrap().clear();
        if was {
            self.notifier.event("vault", serde_json::json!({ "unlocked": false }));
            self.notifier.event("approvals", serde_json::json!([]));
        }
    }

    pub fn verify_password(&self, password: &str) -> Result<()> {
        vault::open(&self.vault_path(), password).map(|_| ())
    }

    pub fn change_password(&self, old: &str, new: &str) -> Result<()> {
        self.verify_password(old)?;
        let mut g = self.open.lock().unwrap();
        let o = g.as_mut().ok_or(Error::Locked)?;
        let data = o.data.clone();
        o.vault.rekey(&self.vault_path(), new, &data)
    }

    /// Marca uso de la interfaz (posterga el autobloqueo). El agente no lo posterga.
    pub fn touch_ui(&self) {
        *self.last_ui.lock().unwrap() = Instant::now();
    }

    /// Lee el vault abierto.
    pub fn read<T>(&self, f: impl FnOnce(&VaultData) -> T) -> Result<T> {
        let g = self.open.lock().unwrap();
        g.as_ref().map(|o| f(&o.data)).ok_or(Error::Locked)
    }

    /// Cambia el vault abierto y lo guarda. Si algo falla, no queda nada a medias.
    pub fn write<T>(&self, f: impl FnOnce(&mut VaultData) -> Result<T>) -> Result<T> {
        let out = {
            let mut g = self.open.lock().unwrap();
            let o = g.as_mut().ok_or(Error::Locked)?;
            let mut data = o.data.clone();
            let out = f(&mut data)?;
            o.vault.save(&self.vault_path(), &data)?;
            o.masker = Arc::new(Masker::from_vault(&data));
            o.data = data;
            out
        };
        self.touch_ui();
        self.notifier.event("data", serde_json::json!({}));
        Ok(out)
    }

    /// Despues de cambiar hosts, credenciales o huellas: las conexiones abiertas ya no valen.
    pub fn lock_count(&self) -> usize {
        self.locks.load(Ordering::SeqCst)
    }

    pub async fn drop_connections(&self) {
        self.pool.lock().await.clear();
        // Un puerto serie es exclusivo: si una terminal lo esta usando, se queda; si no,
        // se suelta para que la proxima conexion lo abra con los datos nuevos.
        self.close_serial(|c| !c.in_use());
    }

    /// Cierra el puerto serie de un host, aunque una terminal lo este usando (el host se
    /// edito o se borro).
    pub fn drop_serial(&self, name: &str) {
        if let Some(c) = self.serial_pool.lock().unwrap().remove(name) {
            c.close();
        }
    }

    /// Cierra los puertos serie que cumplen `which` y los saca del pool.
    fn close_serial(&self, which: impl Fn(&serial::Conn) -> bool) {
        let mut pool = self.serial_pool.lock().unwrap();
        pool.retain(|_, c| {
            let go = which(c);
            if go {
                c.close();
            }
            !go
        });
    }

    /// Respaldo cifrado: el archivo del vault tal cual.
    pub fn backup_to(&self, to: &std::path::Path) -> Result<()> {
        std::fs::copy(self.vault_path(), to)?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Settings> {
        self.read(|d| d.settings.clone())
    }

    pub fn agent_enabled(&self) -> bool {
        self.read(|d| d.settings.agent_enabled).unwrap_or(false)
    }

    pub fn set_agent_enabled(&self, on: bool) -> Result<()> {
        self.write(|d| {
            d.settings.agent_enabled = on;
            Ok(())
        })?;
        if !on {
            for (_, p) in self.approvals.lock().unwrap().drain() {
                let _ = p.tx.send(Decision::Reject { note: Some("agent access was turned off".into()) });
            }
            self.notifier.event("approvals", serde_json::json!([]));
        }
        Ok(())
    }

    // ------------------------------------------------------------- dispositivos

    /// Crea un token nuevo. Es la unica vez que se ve entero.
    pub fn create_device(&self, name: &str, expires_days: Option<u32>, hosts: Vec<String>) -> Result<(Device, String)> {
        let token = format!("eth_{}", hex::encode(vault::random::<24>()));
        let dev = Device {
            id: hex::encode(vault::random::<6>()),
            name: name.trim().to_string(),
            token_hash: token_hash(&token),
            created: now(),
            expires: expires_days.map(|d| {
                (chrono::Utc::now() + chrono::Duration::days(d as i64)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            }),
            hosts,
            revoked: false,
        };
        if dev.name.is_empty() {
            return Err(Error::Invalid("the device needs a name".into()));
        }
        let d2 = dev.clone();
        self.write(move |d| {
            d.devices.push(d2);
            Ok(())
        })?;
        Ok((dev, token))
    }

    /// Valida un token del agente. Con el vault bloqueado no se puede ni saber quien es.
    pub fn authenticate(&self, token: &str, client: &str, session: &str) -> Result<AgentCtx> {
        let h = token_hash(token);
        self.read(|d| {
            let dev = d.devices.iter().find(|x| x.token_hash == h).ok_or(Error::Invalid("unknown device token".into()))?;
            if dev.revoked {
                return Err(Error::Invalid("this device token was revoked".into()));
            }
            if let Some(exp) = &dev.expires {
                if chrono::DateTime::parse_from_rfc3339(exp).map(|e| e < chrono::Utc::now()).unwrap_or(true) {
                    return Err(Error::Invalid("this device token expired".into()));
                }
            }
            Ok(AgentCtx { device_id: dev.id.clone(), device_name: dev.name.clone(), client: client.into(), session: session.into() })
        })?
    }

    // ------------------------------------------------------------------ agente

    fn device_allows(&self, ctx: &AgentCtx, host: &str) -> bool {
        self.read(|d| {
            d.devices.iter().find(|x| x.id == ctx.device_id).is_some_and(|x| x.hosts.is_empty() || x.hosts.iter().any(|h| h == host))
        })
        .unwrap_or(false)
    }

    pub fn effective_policy(d: &VaultData, h: &Host) -> Policy {
        if let Some(p) = &h.policy {
            return p.clone();
        }
        h.group.as_ref().and_then(|g| d.groups.iter().find(|x| &x.name == g)).map(|g| g.policy.clone()).unwrap_or_default()
    }

    /// Las reglas se aplican igual por SSH que por serial; lo unico que cambia es la lista
    /// base del modo solo lectura cuando el host no tiene la suya: en un equipo de red los
    /// comandos que no cambian nada son `show`, `display`, `get`...
    pub fn evaluate(h: &Host, command: &str, pol: &Policy, deny: &[String], patterns: &[String]) -> Verdict {
        let base = if h.is_serial() { policy::default_allowlist_serial() } else { policy::default_allowlist() };
        policy::evaluate_with(command, pol, deny, patterns, &base)
    }

    pub fn agent_list_hosts(&self, ctx: &AgentCtx) -> Result<Vec<AgentHostView>> {
        self.check_agent()?;
        let list = self.read(|d| {
            d.hosts
                .iter()
                .map(|h| {
                    let p = Self::effective_policy(d, h);
                    AgentHostView {
                        name: h.name.clone(),
                        transport: h.transport,
                        group: h.group.clone(),
                        env: h.env,
                        tags: h.tags.clone(),
                        description: h.description.clone(),
                        mode: p.mode,
                        sudo: p.allow_sudo && h.sudo_credential.is_some() && !h.is_serial(),
                        files: p.files && !h.is_serial(),
                    }
                })
                .filter(|v| v.mode != Mode::Off)
                .collect::<Vec<_>>()
        })?;
        Ok(list.into_iter().filter(|v| self.device_allows(ctx, &v.name)).collect())
    }

    fn check_agent(&self) -> Result<()> {
        if !self.is_unlocked() {
            return Err(Error::Locked);
        }
        if !self.agent_enabled() {
            return Err(Error::Invalid("agent access is turned off in Ethereon".into()));
        }
        Ok(())
    }

    fn rate_ok(&self, key: &str, max: u32) -> bool {
        let mut r = self.rate.lock().unwrap();
        let q = r.entry(key.to_string()).or_default();
        let cutoff = Instant::now() - Duration::from_secs(60);
        while q.front().is_some_and(|t| *t < cutoff) {
            q.pop_front();
        }
        if q.len() as u32 >= max {
            return false;
        }
        q.push_back(Instant::now());
        true
    }

    /// Hay comandos del agente corriendo o esperando aprobacion.
    pub fn busy(&self) -> bool {
        self.running.load(Ordering::Relaxed) > 0 || !self.approvals.lock().unwrap().is_empty()
    }

    pub async fn agent_run(&self, ctx: &AgentCtx, host: &str, command: &str, sudo: bool, reason: &str) -> Result<RunResult> {
        self.running.fetch_add(1, Ordering::Relaxed);
        let _guard = RunGuard(&self.running);
        let mut entry = Entry {
            device: ctx.device_name.clone(),
            client: ctx.client.clone(),
            host: host.into(),
            command: command.into(),
            sudo,
            reason: reason.into(),
            ..Default::default()
        };
        match self.agent_run_inner(ctx, host, command, sudo, reason, &mut entry).await {
            Ok(r) => Ok(r),
            Err(e) => {
                if entry.decision.is_empty() {
                    entry.decision = if matches!(e, Error::Locked) { "locked".into() } else { "error".into() };
                }
                if entry.detail.is_empty() {
                    entry.detail = e.to_string();
                }
                self.log(entry);
                Err(e)
            }
        }
    }

    fn log(&self, e: Entry) {
        let key = self.read(|d| hex::decode(&d.audit_key).ok()).ok().flatten();
        self.audit.append(e.clone(), key.as_deref());
        self.notifier.event("activity", serde_json::to_value(&e).unwrap());
    }

    async fn agent_run_inner(
        &self,
        ctx: &AgentCtx,
        host: &str,
        command: &str,
        sudo: bool,
        reason: &str,
        entry: &mut Entry,
    ) -> Result<RunResult> {
        self.check_agent()?;
        if !self.device_allows(ctx, host) {
            entry.decision = "denied".into();
            return Err(Error::Invalid(format!("this device has no access to `{host}`")));
        }
        let (h, pol, deny, settings) = self.read(|d| {
            let h = d.hosts.iter().find(|x| x.name == host).cloned();
            let pol = h.as_ref().map(|h| Self::effective_policy(d, h));
            (h, pol, d.denylist.clone(), d.settings.clone())
        })?;
        let (Some(h), Some(pol)) = (h, pol) else {
            return Err(Error::NotFound(format!("host `{host}` (use list_hosts)")));
        };
        if sudo && h.is_serial() {
            entry.decision = "denied".into();
            return Err(Error::Invalid(format!("`{host}` is a serial console: there is no sudo, use the device's own commands")));
        }
        if sudo && (!pol.allow_sudo || h.sudo_credential.is_none()) {
            entry.decision = "denied".into();
            return Err(Error::Invalid(format!("sudo is not enabled for `{host}`")));
        }
        if !self.rate_ok(&format!("{}/{}", ctx.device_id, host), pol.max_per_minute) {
            entry.decision = "denied".into();
            return Err(Error::Invalid(format!("rate limit: more than {} commands per minute on `{host}`", pol.max_per_minute)));
        }

        let patterns = self.session_patterns.lock().unwrap().get(&ctx.session).cloned().unwrap_or_default();
        let mut command = command.to_string();
        let mut edited = None;
        match Self::evaluate(&h, &command, &pol, &deny, &patterns) {
            Verdict::Deny(why) => {
                entry.decision = "denied".into();
                return Err(Error::Invalid(format!("rejected: {why}")));
            }
            Verdict::Allow => entry.decision = "allowed".into(),
            Verdict::Ask(warnings) => {
                let view = ApprovalView {
                    id: self.next_id(),
                    host: host.into(),
                    env: h.env,
                    command: command.clone(),
                    sudo,
                    reason: reason.into(),
                    device: ctx.device_name.clone(),
                    warnings,
                    created: now(),
                    file: None,
                };
                let decision = self.wait_approval(view, &ctx.session, &settings).await;
                match decision {
                    Some(Decision::Approve) => {}
                    Some(Decision::ApprovePattern { pattern }) => {
                        self.session_patterns.lock().unwrap().entry(ctx.session.clone()).or_default().push(pattern);
                    }
                    Some(Decision::ApproveEdited { command: c }) => {
                        // Lo prohibido sigue prohibido aunque lo escriba una persona.
                        let free = Policy { mode: Mode::Guarded, ..pol.clone() };
                        if let Verdict::Deny(why) = Self::evaluate(&h, &c, &free, &deny, &[]) {
                            entry.decision = "rejected".into();
                            return Err(Error::Invalid(format!("the edited command was rejected: {why}")));
                        }
                        entry.detail = format!("edited from: {command}");
                        command = c.clone();
                        entry.command = c.clone();
                        edited = Some(c);
                    }
                    Some(Decision::Reject { note }) => {
                        entry.decision = "rejected".into();
                        let note = note.filter(|n| !n.trim().is_empty()).map(|n| format!(": {n}")).unwrap_or_default();
                        return Err(Error::Invalid(format!("a person rejected this command{note}")));
                    }
                    None => {
                        entry.decision = "timeout".into();
                        return Err(Error::Invalid("nobody approved the command in time".into()));
                    }
                }
                entry.decision = "approved".into();
                entry.approved_by = "ui".into();
            }
        }

        let res = self.exec(&h, &command, sudo, &pol, &settings).await;
        let (r, raw_hash, masked_text) = match res {
            Ok(x) => x,
            Err(e) => return Err(e),
        };
        entry.exit_code = r.exit_code;
        entry.duration_ms = Some(r.duration_ms);
        entry.output_sha256 = raw_hash;
        entry.masked = r.masked;
        if settings.audit_output {
            entry.output = Some(masked_text);
        }
        self.log(entry.clone());
        Ok(RunResult { edited_command: edited, ..r })
    }

    async fn wait_approval(&self, view: ApprovalView, session: &str, settings: &Settings) -> Option<Decision> {
        let timeout_secs = settings.approval_timeout_secs;
        let (tx, rx) = oneshot::channel();
        let id = view.id;
        let title = match view.env {
            Env::Unset => format!("Approve on {}", view.host),
            e => format!("Approve on {} ({})", view.host, e.as_str()),
        };
        let body = match &view.file {
            Some(f) => format!("{} {} ({})", if f.direction == Direction::Download { "Download" } else { "Upload" }, f.path, transfer::human_size(f.size)),
            None => view.command.clone(),
        };
        let body = body.chars().take(180).collect::<String>();
        self.approvals.lock().unwrap().insert(id, Pending { view, session: session.into(), tx });
        self.notifier.notify(&title, &body);
        self.notifier.sound("approval", settings.sound_volume);
        self.emit_approvals();
        let out = tokio::time::timeout(Duration::from_secs(timeout_secs.max(10)), rx).await.ok().and_then(|r| r.ok());
        if self.approvals.lock().unwrap().remove(&id).is_some() {
            self.emit_approvals();
        }
        out
    }

    fn emit_approvals(&self) {
        self.notifier.event("approvals", serde_json::to_value(self.approvals()).unwrap());
    }

    pub fn approvals(&self) -> Vec<ApprovalView> {
        let mut v: Vec<ApprovalView> = self.approvals.lock().unwrap().values().map(|p| p.view.clone()).collect();
        v.sort_by_key(|a| a.id);
        v
    }

    pub fn resolve(&self, id: u64, d: Decision) -> Result<()> {
        self.touch_ui();
        let p = self.approvals.lock().unwrap().remove(&id).ok_or(Error::NotFound("that request is no longer waiting".into()))?;
        let _ = p.session;
        let _ = p.tx.send(d);
        self.emit_approvals();
        Ok(())
    }

    /// Arma la cadena de saltos con credenciales y huellas.
    fn hops(&self, h: &Host) -> Result<Vec<Hop>> {
        self.read(|d| {
            let mut chain: Vec<&Host> = Vec::new();
            for j in &h.jump {
                let jh = d.hosts.iter().find(|x| &x.name == j).ok_or(Error::NotFound(format!("jump host `{j}`")))?;
                chain.push(jh);
            }
            chain.push(h);
            chain
                .into_iter()
                .map(|x| {
                    let cred = d
                        .credentials
                        .iter()
                        .find(|c| c.id == x.credential)
                        .cloned()
                        .ok_or(Error::NotFound(format!("credential `{}` of host `{}`", x.credential, x.name)))?;
                    let addr = format!("{}:{}", x.hostname, x.port);
                    let expected = d.known_hosts.iter().find(|k| k.addr == addr).map(|k| k.fingerprint.clone());
                    let user = d.user_of(x);
                    if user.is_empty() {
                        return Err(Error::Invalid(format!("host `{}` has no user, and its credential has none either", x.name)));
                    }
                    Ok(Hop { name: x.name.clone(), hostname: x.hostname.clone(), port: x.port, user, cred, expected })
                })
                .collect()
        })?
    }

    pub async fn connection(&self, h: &Host) -> Result<Arc<Conn>> {
        if h.is_serial() {
            return Err(Error::Invalid(format!("`{}` is a serial console: it has no SSH connection", h.name)));
        }
        let mut pool = self.pool.lock().await;
        if let Some(c) = pool.get(&h.name) {
            if !c.is_closed() {
                return Ok(c.clone());
            }
            pool.remove(&h.name);
        }
        let hops = self.hops(h)?;
        let log = self.conn_log(&h.name);
        let conn = match ssh::connect(&hops, &log).await {
            Ok(c) => Arc::new(c),
            Err(e) => {
                log(e.to_string());
                if matches!(e, SshError::HostKeyUnknown { .. } | SshError::HostKeyChanged { .. }) {
                    self.raise_alert(e.clone());
                }
                return Err(e.into());
            }
        };
        pool.insert(h.name.clone(), conn.clone());
        Ok(conn)
    }

    /// El puerto serie de un host, abierto si hacia falta. El nombre del puerto y los
    /// parametros van al registro de conexion (solo la persona lo ve).
    pub fn serial_connection(&self, h: &Host) -> Result<Arc<serial::Conn>> {
        if !h.is_serial() {
            return Err(Error::Invalid(format!("`{}` is not a serial console", h.name)));
        }
        let mut pool = self.serial_pool.lock().unwrap();
        if let Some(c) = pool.get(&h.name) {
            if !c.is_closed() {
                return Ok(c.clone());
            }
            pool.remove(&h.name);
        }
        let log = self.conn_log(&h.name);
        let s = &h.serial;
        let parity = match s.parity {
            Parity::None => 'N',
            Parity::Even => 'E',
            Parity::Odd => 'O',
        };
        log(format!("Opening {} at {} {}{}{}", s.port, s.baud, s.data_bits, parity, s.stop_bits));
        let conn = serial::open(s).and_then(serial::Conn::new).map_err(|e| {
            log(format!("Could not open {}: {e}", s.port));
            Error::Invalid(format!("could not open the serial port of `{}`: {e}", h.name))
        })?;
        log("Port open".into());
        let conn = Arc::new(conn);
        pool.insert(h.name.clone(), conn.clone());
        Ok(conn)
    }

    /// Usa `conn` como el puerto de `name` en vez de abrirlo: para probar el flujo completo
    /// con un puerto falso, sin hardware.
    pub fn add_serial_connection(&self, name: &str, conn: serial::Conn) {
        self.serial_pool.lock().unwrap().insert(name.into(), Arc::new(conn));
    }

    /// Las credenciales de la consola serial de `h`: usuario, contrasena de login y de `enable`.
    fn serial_login(&self, h: &Host) -> Result<serial::Login> {
        self.read(|d| {
            let secret = |id: &str| d.credentials.iter().find(|c| c.id == id).map(|c| Zeroizing::new(c.secret.clone()));
            let user = d.user_of(h);
            serial::Login {
                user: (!user.is_empty()).then_some(user),
                password: (!h.credential.is_empty()).then(|| secret(&h.credential)).flatten(),
                enable: h.serial.enable_credential.as_deref().and_then(secret),
            }
        })
    }

    /// Despierta la consola serial y la deja en el prompt (login y `enable` incluidos). En
    /// otro hilo: el puerto es bloqueante.
    async fn serial_ready(&self, h: &Host, conn: Arc<serial::Conn>) -> std::io::Result<String> {
        let prompt = Regex::new(&h.serial.prompt).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        let login = self.serial_login(h).map_err(|e| std::io::Error::other(e.to_string()))?;
        let timeout = Duration::from_secs(h.serial.timeout_secs.clamp(1, 60));
        tokio::task::spawn_blocking(move || serial::ready(&conn, &prompt, &login, timeout))
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?
    }

    fn raise_alert(&self, e: SshError) {
        let mut a = self.alerts.lock().unwrap();
        let key = |x: &SshError| match x {
            SshError::HostKeyUnknown { addr, .. } | SshError::HostKeyChanged { addr, .. } => addr.clone(),
            _ => String::new(),
        };
        a.retain(|x| key(&x.error) != key(&e));
        if let SshError::HostKeyChanged { host, .. } = &e {
            self.notifier.notify(&format!("Host key changed: {host}"), "The connection was blocked. Check it in Ethereon.");
        }
        a.push(Alert { id: self.next_id(), ts: now(), error: e });
        self.notifier.event("alerts", serde_json::to_value(&*a).unwrap());
    }

    pub fn alerts(&self) -> Vec<Alert> {
        self.alerts.lock().unwrap().clone()
    }

    pub fn dismiss_alert(&self, id: u64) {
        let mut a = self.alerts.lock().unwrap();
        a.retain(|x| x.id != id);
        self.notifier.event("alerts", serde_json::to_value(&*a).unwrap());
    }

    /// Confirma la huella de una direccion (desde la interfaz).
    pub async fn trust_host_key(&self, addr: &str, fingerprint: &str, algorithm: &str) -> Result<()> {
        self.write(|d| {
            d.known_hosts.retain(|k| k.addr != addr);
            d.known_hosts.push(KnownHost { addr: addr.into(), fingerprint: fingerprint.into(), algorithm: algorithm.into() });
            Ok(())
        })?;
        self.alerts.lock().unwrap().retain(|x| match &x.error {
            SshError::HostKeyUnknown { addr: a, .. } | SshError::HostKeyChanged { addr: a, .. } => a != addr,
            _ => true,
        });
        self.notifier.event("alerts", serde_json::to_value(self.alerts()).unwrap());
        self.drop_connections().await;
        Ok(())
    }

    /// Prueba la conexion a un host. Si hay que confirmar una huella, la devuelve.
    pub async fn test_host(&self, name: &str) -> Result<Option<SshError>> {
        self.touch_ui();
        let h = self.read(|d| d.hosts.iter().find(|x| x.name == name).cloned())?.ok_or(Error::NotFound(name.into()))?;
        let log = self.conn_log(&h.name);
        if h.is_serial() {
            // Lo que falla es no poder abrir el puerto. Que el equipo no conteste (apagado,
            // arrancando) o no acepte el login se anota, pero la terminal se abre igual: la
            // persona puede querer ver el arranque o loguearse a mano.
            let conn = self.serial_connection(&h)?;
            match self.serial_ready(&h, conn).await {
                Ok(p) => log(format!("Console ready at prompt {p}")),
                Err(e) => log(format!("No prompt yet: {e}. The terminal opens anyway; press Enter to wake the device")),
            }
            return Ok(None);
        }
        let hops = self.hops(&h)?;
        let res = ssh::probe(&hops, &log).await;
        match &res {
            Ok(Some(e)) | Err(e) => log(e.to_string()),
            Ok(None) => {}
        }
        Ok(res?)
    }

    /// Los pasos de una conexion a `host` van a la interfaz (evento "connlog"), que los muestra
    /// en la terminal que esta conectando. Solo a la interfaz: el agente nunca los ve.
    fn conn_log(&self, host: &str) -> ssh::Log {
        let notifier = self.notifier.clone();
        let host = host.to_string();
        Arc::new(move |line: String| {
            notifier.event("connlog", serde_json::json!({ "host": host, "line": line, "ts": now() }));
        })
    }

    /// Corre el comando por el transporte del host. Devuelve el resultado enmascarado, el
    /// hash de la salida cruda y el texto enmascarado para la auditoria.
    async fn exec(&self, h: &Host, command: &str, sudo: bool, pol: &Policy, s: &Settings) -> Result<(RunResult, String, String)> {
        if h.is_serial() {
            self.exec_serial(h, command, pol).await
        } else {
            self.exec_ssh(h, command, sudo, pol, s).await
        }
    }

    /// Por el cable de consola: despierta la linea (login si hace falta), manda el comando
    /// y lee hasta el prompt. Lo que falle al abrir el puerto va al registro de conexion;
    /// al agente le llega un error sin el nombre del puerto.
    async fn exec_serial(&self, h: &Host, command: &str, pol: &Policy) -> Result<(RunResult, String, String)> {
        let started = Instant::now();
        let max = pol.max_output_kb.max(1) * 1024;
        let timeout = Duration::from_secs(pol.timeout_secs.max(1));
        let prompt = Regex::new(&h.serial.prompt).map_err(|e| Error::Invalid(format!("bad prompt pattern on `{}`: {e}", h.name)))?;
        let generic = |what: &str| Error::Invalid(format!("serial console of `{}`: {what} (details in the connection log in Ethereon)", h.name));
        let mut attempt = 0;
        let out = loop {
            let conn = self.serial_connection(h).map_err(|_| generic("could not open the port"))?;
            let res: std::io::Result<serial::ExecOut> = async {
                self.serial_ready(h, conn.clone()).await?;
                let (c, cmd, p) = (conn.clone(), command.to_string(), prompt.clone());
                tokio::task::spawn_blocking(move || serial::exec(&c, &cmd, &p, timeout, max)).await.map_err(|e| std::io::Error::other(e.to_string()))?
            }
            .await;
            match res {
                Ok(o) => break o,
                // El puerto se cerro debajo (se desenchufo el cable): una vez mas con uno nuevo.
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe && attempt == 0 => {
                    self.drop_serial(&h.name);
                    attempt += 1;
                }
                Err(e) => {
                    self.conn_log(&h.name)(format!("Command failed: {e}"));
                    return Err(match e.kind() {
                        std::io::ErrorKind::PermissionDenied => generic("the console asked for a login that the host does not have"),
                        std::io::ErrorKind::TimedOut => generic("the device did not show a prompt"),
                        _ => generic("the port failed"),
                    });
                }
            }
        };
        let masker = self.masker()?;
        let raw_hash = audit::sha256_hex(out.output.as_bytes());
        let (stdout, masked) = masker.mask(&out.output);
        let joined = stdout.clone();
        Ok((
            RunResult {
                stdout,
                stderr: String::new(),
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                truncated: out.truncated,
                timed_out: out.timed_out,
                masked,
                edited_command: None,
                prompt: out.prompt.map(|p| masker.mask(&p).0),
            },
            raw_hash,
            joined,
        ))
    }

    async fn exec_ssh(&self, h: &Host, command: &str, sudo: bool, pol: &Policy, _s: &Settings) -> Result<(RunResult, String, String)> {
        let stdin = if sudo {
            let id = h.sudo_credential.clone().unwrap_or_default();
            let pw = self
                .read(|d| d.credentials.iter().find(|c| c.id == id).map(|c| Zeroizing::new(c.secret.clone())))?
                .ok_or(Error::NotFound(format!("sudo credential `{id}`")))?;
            Some(pw)
        } else {
            None
        };
        let real = if sudo { ssh::sudo_wrap(command) } else { command.to_string() };
        let started = Instant::now();
        let max = pol.max_output_kb.max(1) * 1024;
        let timeout = Duration::from_secs(pol.timeout_secs.max(1));
        let mut attempt = 0;
        let out = loop {
            let conn = self.connection(h).await?;
            match ssh::exec(&conn, &real, stdin.clone(), timeout, max).await {
                Ok(o) => break o,
                // La conexion guardada se cayo: una vez mas con una nueva.
                Err(_) if attempt == 0 => {
                    self.pool.lock().await.remove(&h.name);
                    attempt += 1;
                }
                Err(e) => return Err(Error::Invalid(format!("ssh error: {e}"))),
            }
        };
        let masker = self.masker()?;
        let raw_hash = {
            let mut all = out.stdout.clone();
            all.extend_from_slice(&out.stderr);
            audit::sha256_hex(&all)
        };
        let (stdout, n1) = masker.mask(&String::from_utf8_lossy(&out.stdout));
        let (stderr, n2) = masker.mask(&String::from_utf8_lossy(&out.stderr));
        let joined = format!("{stdout}{}{stderr}", if stderr.is_empty() { "" } else { "\n--- stderr ---\n" });
        Ok((
            RunResult {
                stdout,
                stderr,
                exit_code: out.exit_code,
                duration_ms: started.elapsed().as_millis() as u64,
                truncated: out.truncated,
                timed_out: out.timed_out,
                masked: n1 + n2,
                edited_command: None,
                prompt: None,
            },
            raw_hash,
            joined,
        ))
    }

    // ------------------------------------------------------- archivos (SFTP)

    /// Baja un archivo de texto y lo devuelve enmascarado. Mismas reglas que `run`: modo del
    /// host, reglas `path:`, aprobacion, frecuencia y auditoria (con tamano y hash).
    pub async fn agent_download(&self, ctx: &AgentCtx, host: &str, path: &str, reason: &str) -> Result<DownloadResult> {
        self.running.fetch_add(1, Ordering::Relaxed);
        let _guard = RunGuard(&self.running);
        let mut entry = self.file_entry(ctx, host, format!("download {path}"), reason);
        let res = self.agent_download_inner(ctx, host, path, reason, &mut entry).await;
        self.finish_file(entry, res)
    }

    /// Sube `content` (texto del pedido) o el archivo `local_path` de esta PC a `path`.
    #[allow(clippy::too_many_arguments)]
    pub async fn agent_upload(
        &self,
        ctx: &AgentCtx,
        host: &str,
        path: &str,
        content: Option<&str>,
        local_path: Option<&str>,
        overwrite: bool,
        reason: &str,
    ) -> Result<UploadResult> {
        self.running.fetch_add(1, Ordering::Relaxed);
        let _guard = RunGuard(&self.running);
        let mut entry = self.file_entry(ctx, host, format!("upload {path}"), reason);
        let res = self.agent_upload_inner(ctx, host, path, content, local_path, overwrite, reason, &mut entry).await;
        self.finish_file(entry, res)
    }

    fn file_entry(&self, ctx: &AgentCtx, host: &str, command: String, reason: &str) -> Entry {
        Entry {
            device: ctx.device_name.clone(),
            client: ctx.client.clone(),
            host: host.into(),
            command,
            reason: reason.into(),
            ..Default::default()
        }
    }

    /// Anota el pedido (salio bien o no) y devuelve el resultado.
    fn finish_file<T>(&self, mut entry: Entry, res: Result<T>) -> Result<T> {
        if let Err(e) = &res {
            if entry.decision.is_empty() || entry.decision == "allowed" || entry.decision == "approved" {
                entry.decision = if matches!(e, Error::Locked) { "locked".into() } else { "error".into() };
            }
            entry.detail = [entry.detail.as_str(), &e.to_string()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("; ");
        }
        self.log(entry);
        res
    }

    /// Lo comun a bajar y subir: el host, sus reglas y si el agente puede pasar archivos ahi.
    fn file_prelude(&self, ctx: &AgentCtx, host: &str, dir: Direction, entry: &mut Entry) -> Result<(Host, Policy, Vec<String>, Settings)> {
        self.check_agent()?;
        if !self.device_allows(ctx, host) {
            return Err(denied(entry, format!("this device has no access to `{host}`")));
        }
        let (h, pol, deny, settings) = self.read(|d| {
            let h = d.hosts.iter().find(|x| x.name == host).cloned();
            let pol = h.as_ref().map(|h| Self::effective_policy(d, h));
            (h, pol, d.denylist.clone(), d.settings.clone())
        })?;
        let (Some(h), Some(pol)) = (h, pol) else {
            return Err(Error::NotFound(format!("host `{host}` (use list_hosts)")));
        };
        if h.is_serial() {
            return Err(denied(entry, format!("`{host}` is a serial console: file transfer needs an SSH host")));
        }
        if pol.mode == Mode::Off {
            return Err(denied(entry, "rejected: the agent has no access to this host".into()));
        }
        if !pol.files {
            return Err(denied(
                entry,
                format!("file transfer is not enabled for `{host}`; the user can turn it on in the host's agent rules in Ethereon"),
            ));
        }
        if dir == Direction::Upload && pol.mode == Mode::ReadOnly {
            return Err(denied(entry, format!("rejected: `{host}` is read-only, so you can download but not upload")));
        }
        if !self.rate_ok(&format!("{}/{}", ctx.device_id, host), pol.max_per_minute) {
            return Err(denied(entry, format!("rate limit: more than {} requests per minute on `{host}`", pol.max_per_minute)));
        }
        Ok((h, pol, deny, settings))
    }

    /// Una sesion SFTP con el host para el agente. Si la conexion guardada se cayo, una vez
    /// mas con una nueva. Los errores no dicen la direccion ni el usuario: el detalle va al
    /// registro de conexion, que solo ve la persona.
    async fn agent_sftp(&self, h: &Host) -> Result<files::Sftp> {
        let mut attempt = 0;
        loop {
            let conn = self.connection(h).await.map_err(|e| self.agent_conn_err(h, e))?;
            match files::Sftp::open(conn).await {
                Ok(s) => return Ok(s),
                Err(_) if attempt == 0 => {
                    self.pool.lock().await.remove(&h.name);
                    attempt += 1;
                }
                Err(e) => {
                    self.conn_log(&h.name)(format!("SFTP failed: {e}"));
                    return Err(Error::Invalid(format!("could not open SFTP on `{}`: the server may not have the SFTP subsystem enabled", h.name)));
                }
            }
        }
    }

    /// Un error al conectar, dicho sin direcciones ni usuarios (los de `SshError` los traen).
    fn agent_conn_err(&self, h: &Host, e: Error) -> Error {
        let host = &h.name;
        let msg = match e {
            Error::Locked => return Error::Locked,
            Error::Ssh(SshError::HostKeyUnknown { .. }) => format!("the host key of `{host}` is not confirmed yet: ask the user to confirm it in Ethereon"),
            Error::Ssh(SshError::HostKeyChanged { .. }) => format!("the host key of `{host}` CHANGED and the connection was blocked: tell the user"),
            Error::Ssh(SshError::Auth { .. }) => format!("could not log in to `{host}` (details in the connection log in Ethereon)"),
            Error::Ssh(SshError::Connect { .. }) => format!("could not connect to `{host}` (details in the connection log in Ethereon)"),
            other => {
                self.conn_log(host)(other.to_string());
                format!("`{host}` is not set up correctly in Ethereon (details in the connection log)")
            }
        };
        Error::Invalid(msg)
    }

    /// Espera a la persona para un pedido de archivo.
    #[allow(clippy::too_many_arguments)]
    async fn approve_file(
        &self,
        ctx: &AgentCtx,
        h: &Host,
        req: FileRequest,
        reason: &str,
        warnings: Vec<String>,
        settings: &Settings,
        entry: &mut Entry,
    ) -> Result<()> {
        let view = ApprovalView {
            id: self.next_id(),
            host: h.name.clone(),
            env: h.env,
            command: format!("{} {}", req.direction.as_str(), req.path),
            sudo: false,
            reason: reason.into(),
            device: ctx.device_name.clone(),
            warnings,
            created: now(),
            file: Some(req),
        };
        let (decision, msg) = match self.wait_approval(view, &ctx.session, settings).await {
            // Un patron de sesion es para comandos: en un archivo vale como aprobar este.
            Some(Decision::Approve) | Some(Decision::ApprovePattern { .. }) => {
                entry.decision = "approved".into();
                entry.approved_by = "ui".into();
                return Ok(());
            }
            Some(Decision::ApproveEdited { .. }) => ("rejected", "a file request cannot be edited, so it was not approved".to_string()),
            Some(Decision::Reject { note }) => {
                let note = note.filter(|n| !n.trim().is_empty()).map(|n| format!(": {n}")).unwrap_or_default();
                ("rejected", format!("a person rejected this transfer{note}"))
            }
            None => ("timeout", "nobody approved the transfer in time".to_string()),
        };
        entry.decision = decision.into();
        Err(Error::Invalid(msg))
    }

    async fn agent_download_inner(&self, ctx: &AgentCtx, host: &str, path: &str, reason: &str, entry: &mut Entry) -> Result<DownloadResult> {
        let (h, pol, deny, settings) = self.file_prelude(ctx, host, Direction::Download, entry)?;
        let max = pol.max_file_kb.max(1) * 1024;
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(pol.timeout_secs.max(1));

        let sftp = self.agent_sftp(&h).await?;
        let s = sftp.session();
        let home = timed(deadline, transfer::home(s)).await?;
        let abs = transfer::remote_abs(path, &home).map_err(|e| denied(entry, e))?;
        if let Some(why) = transfer::remote_blocked(&transfer::rule_forms(&abs, &home), &pol, &deny) {
            return Err(denied(entry, format!("rejected: {why}")));
        }
        let plan = timed(deadline, transfer::plan_download(s, &abs)).await?;
        if plan.real != abs {
            if let Some(why) = transfer::remote_blocked(&transfer::rule_forms(&plan.real, &home), &pol, &deny) {
                return Err(denied(entry, format!("rejected: the path leads through a link to a forbidden place ({why})")));
            }
            entry.detail = "followed a link".into();
        }
        if plan.size > max as u64 {
            return Err(denied(entry, too_big(plan.size, &pol, host)));
        }
        // Se lee antes de pedir aprobacion: la persona aprueba estos bytes, y si cambian
        // mientras espera, no importa.
        let data = timed(deadline, transfer::read_capped(s, &plan.real, max)).await?;
        drop(sftp);
        if data.len() > max {
            return Err(denied(entry, too_big(data.len() as u64, &pol, host)));
        }
        entry.bytes = Some(data.len() as u64);
        entry.output_sha256 = audit::sha256_hex(&data);
        let Some(text) = transfer::as_text(&data) else {
            return Err(denied(
                entry,
                "rejected: binary file. Ethereon only passes text, because it cannot hide secrets inside a binary; ask the user to download it from the SFTP tab".into(),
            ));
        };
        let (content, masked) = self.masker()?.mask(text);

        if pol.mode == Mode::Approval {
            let req = FileRequest { direction: Direction::Download, path: path.into(), size: data.len() as u64, local: None, replaces: false };
            self.approve_file(ctx, &h, req, reason, vec![], &settings, entry).await?;
        } else {
            entry.decision = "allowed".into();
        }
        entry.masked = masked;
        entry.duration_ms = Some(started.elapsed().as_millis() as u64);
        if settings.audit_output {
            entry.output = Some(content.clone());
        }
        Ok(DownloadResult { path: path.into(), content, masked, duration_ms: started.elapsed().as_millis() as u64 })
    }

    #[allow(clippy::too_many_arguments)]
    async fn agent_upload_inner(
        &self,
        ctx: &AgentCtx,
        host: &str,
        path: &str,
        content: Option<&str>,
        local_path: Option<&str>,
        overwrite: bool,
        reason: &str,
        entry: &mut Entry,
    ) -> Result<UploadResult> {
        let (h, pol, deny, settings) = self.file_prelude(ctx, host, Direction::Upload, entry)?;
        let max = pol.max_file_kb.max(1) * 1024;
        let started = Instant::now();

        // De donde sale: el texto del pedido o un archivo de esta PC que se puede mostrar.
        let (data, local) = match (content, local_path.map(str::trim).filter(|p| !p.is_empty())) {
            (Some(_), Some(_)) => return Err(Error::Invalid("give either `content` or `local_path`, not both".into())),
            (None, None) => return Err(Error::Invalid("give the file's text in `content`, or a file on this PC in `local_path`".into())),
            (Some(c), None) => (c.as_bytes().to_vec(), None),
            (None, Some(lp)) => {
                let guard = transfer::LocalGuard::new(&self.dir, &files::home_local(), &deny);
                let real = guard.check(lp).map_err(|e| denied(entry, format!("rejected: {e}")))?;
                let len = std::fs::metadata(&real).map(|m| m.len()).unwrap_or(0);
                if len > max as u64 {
                    return Err(denied(entry, too_big(len, &pol, host)));
                }
                let (_, data) = guard.read(lp, max).map_err(|e| denied(entry, format!("rejected: {e}")))?;
                (data, Some(lp.to_string()))
            }
        };
        if let Some(l) = &local {
            entry.detail = format!("from {l}");
        }
        entry.bytes = Some(data.len() as u64);
        entry.output_sha256 = audit::sha256_hex(&data);
        if data.len() > max {
            return Err(denied(entry, too_big(data.len() as u64, &pol, host)));
        }
        let scan = transfer::scan_upload(&data, &*self.masker()?);
        if let Some(why) = scan.hard {
            return Err(denied(entry, format!("rejected: {why}. It is not uploaded in any mode; the user can do it from the SFTP tab in Ethereon")));
        }

        let timeout = Duration::from_secs(pol.timeout_secs.max(1));
        let sftp = self.agent_sftp(&h).await?;
        let (home, abs, plan) = timed(tokio::time::Instant::now() + timeout, transfer::locate_upload(sftp.session(), path)).await?;
        if let Some(why) = transfer::remote_blocked(&transfer::rule_forms(&abs, &home), &pol, &deny) {
            return Err(denied(entry, format!("rejected: {why}")));
        }
        let plan = plan?;
        if plan.real != abs {
            if let Some(why) = transfer::remote_blocked(&transfer::rule_forms(&plan.real, &home), &pol, &deny) {
                return Err(denied(entry, format!("rejected: the folder leads through a link to a forbidden place ({why})")));
            }
        }
        if plan.exists && !overwrite {
            return Err(denied(entry, format!("`{path}` already exists on `{host}`: pass overwrite: true to replace it")));
        }

        let sftp = match pol.mode {
            Mode::Approval => {
                drop(sftp); // no tener un canal abierto mientras la persona decide
                let req = FileRequest { direction: Direction::Upload, path: path.into(), size: data.len() as u64, local: local.clone(), replaces: plan.exists };
                self.approve_file(ctx, &h, req, reason, scan.warnings.clone(), &settings, entry).await?;
                // Lo que se aprobo tiene que seguir siendo lo mismo en el servidor.
                let sftp = self.agent_sftp(&h).await?;
                let (_, _, again) = timed(tokio::time::Instant::now() + timeout, transfer::locate_upload(sftp.session(), path)).await?;
                if again.ok().is_none_or(|p| p != plan) {
                    return Err(Error::Invalid("the target changed on the host while waiting for approval; nothing was written".into()));
                }
                sftp
            }
            Mode::Guarded if scan.masked_marker && plan.exists => {
                return Err(denied(
                    entry,
                    "rejected: the text has ***, which is how Ethereon hides secrets in downloads. Replacing the existing file would put *** where its real secrets are; ask the user".into(),
                ));
            }
            _ => {
                entry.decision = "allowed".into();
                sftp
            }
        };
        timed(tokio::time::Instant::now() + timeout, transfer::write_replace(sftp.session(), &plan, &data)).await?;
        if plan.exists {
            entry.detail = [entry.detail.as_str(), "replaced the existing file"].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("; ");
        }
        if !scan.warnings.is_empty() {
            entry.detail = [entry.detail.clone(), scan.warnings.join("; ")].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("; ");
        }
        entry.duration_ms = Some(started.elapsed().as_millis() as u64);
        Ok(UploadResult { path: path.into(), size: data.len() as u64, replaced: plan.exists, duration_ms: started.elapsed().as_millis() as u64 })
    }

    /// Que sistema corre el host, para su icono en la interfaz. Se pregunta una sola vez, con
    /// la conexion que ya abrio la terminal, y queda en el vault. El agente no lo ve.
    pub async fn detect_os(&self, name: &str, conn: &Conn) -> Result<()> {
        let rev = self.read(|d| d.hosts.iter().find(|h| h.name == name).map(|h| h.os_rev))?;
        if rev.is_none_or(|r| r >= ssh::OS_PROBE_REV) {
            return Ok(());
        }
        let r = ssh::exec(conn, ssh::OS_PROBE, None, Duration::from_secs(5), 16 * 1024)
            .await
            .map_err(|e| Error::Invalid(format!("ssh error: {e}")))?;
        self.log(Entry {
            device: "ui".into(),
            host: name.into(),
            command: ssh::OS_PROBE.into(),
            reason: "detect the OS for its icon".into(),
            decision: "allowed".into(),
            exit_code: r.exit_code,
            ..Default::default()
        });
        let os = ssh::parse_os(&String::from_utf8_lossy(&r.stdout));
        self.write(|d| {
            if let Some(h) = d.hosts.iter_mut().find(|h| h.name == name) {
                h.os = os.or(h.os.take());
                h.os_rev = ssh::OS_PROBE_REV;
            }
            Ok(())
        })
    }

    pub fn masker(&self) -> Result<Arc<Masker>> {
        self.open.lock().unwrap().as_ref().map(|o| o.masker.clone()).ok_or(Error::Locked)
    }

    // --------------------------------------------------------------- auditoria

    pub fn audit(&self) -> &Audit {
        &self.audit
    }

    pub fn audit_log(&self, e: Entry) {
        self.log(e)
    }

    pub fn audit_verify(&self) -> Result<std::result::Result<usize, String>> {
        let key = self.read(|d| hex::decode(&d.audit_key).unwrap_or_default())?;
        Ok(self.audit.verify(&key))
    }

    // ----------------------------------------------------------- mantenimiento

    /// Cada 15 s: autobloqueo y cierre de conexiones sin uso. Lo lanza quien aloja el broker.
    pub async fn maintenance_loop(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        loop {
            tick.tick().await;
            let Ok(s) = self.settings() else { continue };
            if s.autolock_minutes > 0 && self.last_ui.lock().unwrap().elapsed() > Duration::from_secs(s.autolock_minutes * 60) {
                // Si hay algo esperando aprobacion, alguien esta por mirar: se espera.
                if self.approvals.lock().unwrap().is_empty() {
                    self.lock().await;
                    continue;
                }
            }
            let idle = Duration::from_secs(s.keep_alive_minutes.max(1) * 60);
            self.pool.lock().await.retain(|_, c| !c.is_closed() && c.last_used.lock().unwrap().elapsed() < idle);
            // Un puerto serie con una terminal abierta no esta sin uso.
            self.close_serial(|c| c.is_closed() || (!c.in_use() && c.last_used.lock().unwrap().elapsed() >= idle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::fake::FakeLink;

    struct Quiet;
    impl Notifier for Quiet {
        fn event(&self, _: &str, _: serde_json::Value) {}
        fn notify(&self, _: &str, _: &str) {}
    }

    /// Un switch por consola serial, con login y `enable`: el agente corre un comando de solo
    /// lectura, recibe la salida con los secretos tapados y el prompt; lo que configura se
    /// rechaza; y ni el puerto ni las credenciales salen por `list_hosts` ni por los errores.
    #[tokio::test]
    async fn serial_host_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let b = Broker::new(dir.path().to_path_buf(), Arc::new(Quiet));
        b.create("tester-pass-123").unwrap();
        b.write(|d| {
            let cred = |id: &str, secret: &str| Credential {
                id: id.into(),
                kind: CredentialKind::Password,
                username: String::new(),
                secret: secret.into(),
                passphrase: String::new(),
                certificate: String::new(),
                note: String::new(),
            };
            d.credentials.push(cred("sw-pw", "s3cret-pass"));
            d.credentials.push(cred("sw-en", "en4ble-pass"));
            d.hosts.push(Host {
                name: "sw1".into(),
                transport: Transport::Serial,
                serial: SerialConfig { port: "COM99".into(), enable_credential: Some("sw-en".into()), ..Default::default() },
                user: "admin".into(),
                credential: "sw-pw".into(),
                description: "access switch".into(),
                policy: Some(Policy { mode: Mode::ReadOnly, ..Default::default() }),
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
        let link = FakeLink::new(&[
            ("\r", "\r\nUser Access Verification\r\n\r\nUsername: "),
            ("admin\r", "admin\r\nPassword: "),
            ("s3cret-pass\r", "\r\nsw1>"),
            ("enable\r", "enable\r\nPassword: "),
            ("en4ble-pass\r", "\r\nsw1#"),
            ("show run\r", "show run\r\nenable secret en4ble-pass\r\nsnmp-server community s3cret-pass RO\r\n --More-- "),
            (" ", "\r        \rend\r\nsw1#"),
        ]);
        b.add_serial_connection("sw1", serial::Conn::new(Box::new(link.clone())).unwrap());
        let (_, token) = b.create_device("pc", None, vec![]).unwrap();
        let ctx = b.authenticate(&token, "127.0.0.1", "s1").unwrap();

        let hosts = b.agent_list_hosts(&ctx).unwrap();
        let json = serde_json::to_string(&hosts).unwrap();
        assert!(json.contains("\"transport\":\"serial\""));
        assert!(!json.contains("COM99") && !json.contains("admin") && !json.contains("sw-pw"), "{json}");

        let r = b.agent_run(&ctx, "sw1", "show run", false, "check config").await.unwrap();
        assert_eq!(r.stdout, "enable secret ***\nsnmp-server community *** RO\nend");
        assert_eq!(r.prompt.as_deref(), Some("sw1#"));
        assert_eq!(r.exit_code, None);
        assert!(!r.timed_out);
        assert_eq!(r.masked, 2);
        assert!(!link.written().contains("configure"));

        // Solo lectura: lo que configura se rechaza antes de tocar el puerto.
        let e = b.agent_run(&ctx, "sw1", "configure terminal", false, "x").await.unwrap_err().to_string();
        assert!(e.contains("not in the allowed list"), "{e}");
        // Sudo no existe en una consola.
        let e = b.agent_run(&ctx, "sw1", "show version", true, "x").await.unwrap_err().to_string();
        assert!(e.contains("serial console"), "{e}");
        assert!(!link.written().contains("show version"));

        // La auditoria tiene el comando, el host y el hash de la salida sin enmascarar.
        let entries = b.audit().search("show run", 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].decision, "allowed");
        assert_eq!(entries[0].output_sha256, audit::sha256_hex(b"enable secret en4ble-pass\nsnmp-server community s3cret-pass RO\nend"));

        // Al bloquear, el puerto se cierra.
        b.lock().await;
        assert!(link.written().len() > 0);
        let e = b.agent_run(&ctx, "sw1", "show run", false, "x").await.unwrap_err();
        assert!(matches!(e, Error::Locked));
    }

    /// Archivos del agente: lo que se decide antes de conectar (sin servidor SSH; lo que
    /// pasa por SFTP esta en tests/acceptance.rs). Apagado por defecto y heredado del grupo,
    /// solo SSH, solo lectura solo baja, archivos locales y secretos del vault no salen, y
    /// cada rechazo queda en la auditoria.
    #[tokio::test]
    async fn file_transfer_rules_before_connecting() {
        let dir = tempfile::tempdir().unwrap();
        let b = Broker::new(dir.path().join("data"), Arc::new(Quiet));
        b.create("tester-pass-123").unwrap();
        let files_on = |mode| Some(Policy { mode, files: true, ..Default::default() });
        b.write(|d| {
            d.credentials.push(Credential {
                id: "pw".into(),
                kind: CredentialKind::Password,
                username: "deploy".into(),
                secret: "Sup3r-S3cret-Value".into(),
                passphrase: String::new(),
                certificate: String::new(),
                note: String::new(),
            });
            // Nada escucha en el puerto 1: si algo llegara a conectar, fallaria distinto.
            let host = |name: &str, policy, group: Option<&str>| Host {
                name: name.into(),
                hostname: "127.0.0.1".into(),
                port: 1,
                credential: "pw".into(),
                group: group.map(String::from),
                policy,
                ..Default::default()
            };
            d.groups.push(Group { name: "lab".into(), policy: Policy { mode: Mode::Guarded, files: true, ..Default::default() } });
            d.hosts.push(host("plain", Some(Policy { mode: Mode::Guarded, ..Default::default() }), None));
            d.hosts.push(host("ro", files_on(Mode::ReadOnly), None));
            d.hosts.push(host("guarded", files_on(Mode::Guarded), None));
            d.hosts.push(host("in-lab", None, Some("lab")));
            d.hosts.push(Host { transport: Transport::Serial, serial: SerialConfig { port: "COM99".into(), ..Default::default() }, ..host("sw", files_on(Mode::Guarded), None) });
            Ok(())
        })
        .unwrap();
        let (_, token) = b.create_device("pc", None, vec![]).unwrap();
        let ctx = b.authenticate(&token, "127.0.0.1", "s1").unwrap();

        // list_hosts dice cuales tienen archivos (heredado del grupo tambien) y nada mas.
        let hosts = serde_json::to_value(b.agent_list_hosts(&ctx).unwrap()).unwrap();
        let files = |n: &str| hosts.as_array().unwrap().iter().find(|h| h["name"] == n).unwrap().get("files").cloned();
        assert_eq!(files("plain"), None, "off by default, and the field is not even there");
        assert_eq!(files("guarded"), Some(serde_json::json!(true)));
        assert_eq!(files("in-lab"), Some(serde_json::json!(true)), "inherited from the group");
        assert_eq!(files("sw"), None, "serial hosts never have files");
        let json = hosts.to_string();
        assert!(!json.contains("127.0.0.1") && !json.contains("deploy") && !json.contains("COM99"));

        let err = |r: Result<DownloadResult>| r.unwrap_err().to_string();
        let up_err = |r: Result<UploadResult>| r.unwrap_err().to_string();
        assert!(err(b.agent_download(&ctx, "plain", "/etc/hosts", "x").await).contains("not enabled"));
        assert!(err(b.agent_download(&ctx, "sw", "/etc/hosts", "x").await).contains("serial console"));
        assert!(up_err(b.agent_upload(&ctx, "ro", "/tmp/x", Some("hi"), None, false, "x").await).contains("read-only"));
        assert!(err(b.agent_download(&ctx, "nope", "/etc/hosts", "x").await).contains("not found"));

        // Subir: un secreto del vault o una clave no salen en ningun modo, aunque el
        // texto venga en el pedido.
        let e = up_err(b.agent_upload(&ctx, "guarded", "/tmp/x", Some("pass=Sup3r-S3cret-Value"), None, false, "x").await);
        assert!(e.contains("secret stored in Ethereon"), "{e}");
        assert!(!e.contains("Sup3r"));
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbg==\n-----END OPENSSH PRIVATE KEY-----\n";
        assert!(up_err(b.agent_upload(&ctx, "guarded", "/tmp/k", Some(key), None, false, "x").await).contains("private key"));
        // Ni el vault ni la auditoria de Ethereon, ni rutas de red ni relativas.
        let vault = dir.path().join("data").join("vault.json");
        let e = up_err(b.agent_upload(&ctx, "guarded", "/tmp/v", None, Some(&vault.to_string_lossy()), false, "x").await);
        assert!(e.contains("Ethereon's own data"), "{e}");
        assert!(up_err(b.agent_upload(&ctx, "guarded", "/tmp/v", None, Some(r"\\evil\share\f"), false, "x").await).contains("network"));
        assert!(up_err(b.agent_upload(&ctx, "guarded", "/tmp/v", None, Some("notes.txt"), false, "x").await).contains("absolute"));
        assert!(up_err(b.agent_upload(&ctx, "guarded", "/tmp/v", Some("a"), Some("/x"), false, "x").await).contains("not both"));
        // Muy grande, antes de conectar.
        let big = "x".repeat(1024 * 1024 + 1);
        assert!(up_err(b.agent_upload(&ctx, "guarded", "/tmp/big", Some(&big), None, false, "x").await).contains("limit"));

        // La auditoria tiene cada intento, rechazado, con tamano y hash cuando hay contenido.
        let log = b.audit().search("upload /tmp/", 20);
        let secret = log.iter().find(|e| e.host == "guarded" && e.command == "upload /tmp/x").unwrap();
        assert_eq!(secret.decision, "denied");
        assert_eq!(secret.bytes, Some(23));
        assert_eq!(secret.output_sha256, audit::sha256_hex(b"pass=Sup3r-S3cret-Value"));
        let ro = log.iter().find(|e| e.host == "ro").unwrap();
        assert!(ro.decision == "denied" && ro.detail.contains("read-only"));
        let raw = std::fs::read_to_string(dir.path().join("data").join("audit.jsonl")).unwrap();
        assert!(!raw.contains("Sup3r-S3cret-Value"));
        assert!(b.audit_verify().unwrap().is_ok());

        // Frecuencia: comparte el limite de `run` del host.
        b.write(|d| {
            d.hosts.iter_mut().find(|h| h.name == "ro").unwrap().policy = Some(Policy { mode: Mode::ReadOnly, files: true, max_per_minute: 1, ..Default::default() });
            Ok(())
        })
        .unwrap();
        // El primero llega a conectar (y falla: no hay nada en el puerto 1) sin decir la direccion.
        let e = err(b.agent_download(&ctx, "ro", "/etc/hosts", "x").await);
        assert!(e.contains("could not connect to `ro`") && !e.contains("127.0.0.1"), "{e}");
        assert!(err(b.agent_download(&ctx, "ro", "/etc/hosts", "x").await).contains("rate limit"));

        // Un vault de antes (sin los campos) se lee con los archivos apagados.
        let old: Policy = serde_json::from_str(r#"{"mode":"guarded","allow":[],"deny":[]}"#).unwrap();
        assert!(!old.files);
        assert_eq!(old.max_file_kb, 1024);
    }
}
