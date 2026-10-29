//! El broker: tiene el vault abierto, abre las conexiones, aplica las reglas, pide
//! aprobaciones y anota todo. La interfaz y el servidor MCP solo hablan con esto.
//!
//! Lo que puede pedir un agente esta en `agent_*`. Todo lo demas (desbloquear, editar,
//! exportar) lo llama solo la interfaz, que corre en el mismo proceso.

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
use crate::ssh::{self, Conn, Hop, SshError};
use crate::{vault, Error, Result};

/// Como avisa el broker a quien lo aloja (la app de escritorio o el servidor).
pub trait Notifier: Send + Sync {
    /// Evento para la interfaz: "vault", "approvals", "alerts", "activity", "data", "connlog".
    fn event(&self, name: &str, payload: serde_json::Value);
    /// Notificacion del sistema (escritorio).
    fn notify(&self, title: &str, body: &str);
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

    pub fn agent_list_hosts(&self, ctx: &AgentCtx) -> Result<Vec<AgentHostView>> {
        self.check_agent()?;
        let list = self.read(|d| {
            d.hosts
                .iter()
                .map(|h| {
                    let p = Self::effective_policy(d, h);
                    AgentHostView {
                        name: h.name.clone(),
                        group: h.group.clone(),
                        env: h.env,
                        tags: h.tags.clone(),
                        description: h.description.clone(),
                        mode: p.mode,
                        sudo: p.allow_sudo && h.sudo_credential.is_some(),
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
        match policy::evaluate(&command, &pol, &deny, &patterns) {
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
                };
                let decision = self.wait_approval(view, &ctx.session, settings.approval_timeout_secs).await;
                match decision {
                    Some(Decision::Approve) => {}
                    Some(Decision::ApprovePattern { pattern }) => {
                        self.session_patterns.lock().unwrap().entry(ctx.session.clone()).or_default().push(pattern);
                    }
                    Some(Decision::ApproveEdited { command: c }) => {
                        // Lo prohibido sigue prohibido aunque lo escriba una persona.
                        let free = Policy { mode: Mode::Guarded, ..pol.clone() };
                        if let Verdict::Deny(why) = policy::evaluate(&c, &free, &deny, &[]) {
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

    async fn wait_approval(&self, view: ApprovalView, session: &str, timeout_secs: u64) -> Option<Decision> {
        let (tx, rx) = oneshot::channel();
        let id = view.id;
        let title = match view.env {
            Env::Unset => format!("Approve on {}", view.host),
            e => format!("Approve on {} ({})", view.host, e.as_str()),
        };
        let body = view.command.chars().take(180).collect::<String>();
        self.approvals.lock().unwrap().insert(id, Pending { view, session: session.into(), tx });
        self.notifier.notify(&title, &body);
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
        let hops = self.hops(&h)?;
        let log = self.conn_log(&h.name);
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

    async fn exec(&self, h: &Host, command: &str, sudo: bool, pol: &Policy, _s: &Settings) -> Result<(RunResult, String, String)> {
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
            },
            raw_hash,
            joined,
        ))
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
        }
    }
}
