//! Comandos de la interfaz. Son finitos a proposito: la logica esta en `ethereon-core`.
//! Ninguno devuelve un secreto: las credenciales se ven como "guardada / vacia".

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use ethereon_core::broker::{Alert, ApprovalView, Decision};
use ethereon_core::export::{self, Format};
use ethereon_core::model::*;
use ethereon_core::{audit::Entry, import, policy, Error};

use crate::AppState;

type R<T> = Result<T, Error>;

fn invalid(s: impl Into<String>) -> Error {
    Error::Invalid(s.into())
}

#[derive(Serialize)]
pub struct Status {
    exists: bool,
    unlocked: bool,
    agent_enabled: bool,
    mcp_url: String,
    mcp_error: Option<String>,
    data_dir: String,
}

#[tauri::command]
pub fn status(s: State<AppState>) -> Status {
    let b = &s.broker;
    Status {
        exists: b.vault_exists(),
        unlocked: b.is_unlocked(),
        agent_enabled: b.is_unlocked() && b.agent_enabled(),
        mcp_url: format!("http://127.0.0.1:{}/mcp", s.config.port),
        mcp_error: s.mcp_error.lock().unwrap().clone(),
        data_dir: b.dir().display().to_string(),
    }
}

#[tauri::command]
pub async fn vault_create(s: State<'_, AppState>, password: String) -> R<()> {
    let b = s.broker.clone();
    tauri::async_runtime::spawn_blocking(move || b.create(&password)).await.map_err(|e| invalid(e.to_string()))?
}

#[tauri::command]
pub async fn vault_unlock(s: State<'_, AppState>, password: String) -> R<()> {
    // Argon2 tarda ~0.3 s: fuera del hilo de la interfaz.
    let b = s.broker.clone();
    tauri::async_runtime::spawn_blocking(move || b.unlock(&password)).await.map_err(|e| invalid(e.to_string()))?
}

#[tauri::command]
pub async fn vault_lock(s: State<'_, AppState>) -> R<()> {
    s.broker.lock().await;
    Ok(())
}

#[tauri::command]
pub async fn vault_change_password(s: State<'_, AppState>, old: String, new: String) -> R<()> {
    let b = s.broker.clone();
    tauri::async_runtime::spawn_blocking(move || b.change_password(&old, &new)).await.map_err(|e| invalid(e.to_string()))?
}

#[tauri::command]
pub fn touch(s: State<AppState>) {
    s.broker.touch_ui();
}

#[derive(Serialize)]
pub struct CredView {
    id: String,
    kind: CredentialKind,
    username: String,
    note: String,
    has_secret: bool,
    has_passphrase: bool,
    has_certificate: bool,
    used_by: Vec<String>,
}

#[derive(Serialize)]
pub struct DeviceView {
    id: String,
    name: String,
    created: String,
    expires: Option<String>,
    hosts: Vec<String>,
    revoked: bool,
}

#[derive(Serialize)]
pub struct Snapshot {
    hosts: Vec<Host>,
    groups: Vec<Group>,
    credentials: Vec<CredView>,
    known_hosts: Vec<KnownHost>,
    devices: Vec<DeviceView>,
    denylist: Vec<String>,
    settings: Settings,
    default_allow: Vec<String>,
    default_deny: Vec<String>,
}

#[tauri::command]
pub fn snapshot(s: State<AppState>) -> R<Snapshot> {
    s.broker.read(|d| Snapshot {
        hosts: d.hosts.clone(),
        groups: d.groups.clone(),
        credentials: d
            .credentials
            .iter()
            .map(|c| CredView {
                id: c.id.clone(),
                kind: c.kind,
                username: c.username.clone(),
                note: c.note.clone(),
                has_secret: !c.secret.is_empty(),
                has_passphrase: !c.passphrase.is_empty(),
                has_certificate: !c.certificate.is_empty(),
                used_by: d
                    .hosts
                    .iter()
                    .filter(|h| h.credential == c.id || h.sudo_credential.as_deref() == Some(&c.id))
                    .map(|h| h.name.clone())
                    .collect(),
            })
            .collect(),
        known_hosts: d.known_hosts.clone(),
        devices: d
            .devices
            .iter()
            .map(|x| DeviceView {
                id: x.id.clone(),
                name: x.name.clone(),
                created: x.created.clone(),
                expires: x.expires.clone(),
                hosts: x.hosts.clone(),
                revoked: x.revoked,
            })
            .collect(),
        denylist: d.denylist.clone(),
        settings: d.settings.clone(),
        default_allow: policy::default_allowlist(),
        default_deny: policy::default_denylist(),
    })
}

fn valid_name(n: &str) -> R<()> {
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err(invalid("names use letters, numbers, - _ and . only"));
    }
    Ok(())
}

/// Mueve un host a un grupo (o a ninguno) y a una posicion: delante de `before`, o al final
/// del grupo. El orden de la lista es el que se ve en Hosts.
#[tauri::command]
pub fn host_move(s: State<AppState>, name: String, group: Option<String>, before: Option<String>) -> R<()> {
    s.broker.write(|d| {
        if let Some(g) = &group {
            if !d.groups.iter().any(|x| &x.name == g) {
                return Err(invalid(format!("there is no group `{g}`")));
            }
        }
        let i = d.hosts.iter().position(|h| h.name == name).ok_or_else(|| Error::NotFound(name.clone()))?;
        let mut h = d.hosts.remove(i);
        h.group = group.clone();
        let at = before
            .as_deref()
            .filter(|b| *b != name)
            .and_then(|b| d.hosts.iter().position(|x| x.name == b))
            .unwrap_or_else(|| d.hosts.iter().rposition(|x| x.group == group).map(|j| j + 1).unwrap_or(d.hosts.len()));
        d.hosts.insert(at, h);
        Ok(())
    })
}

#[tauri::command]
pub async fn host_save(s: State<'_, AppState>, host: Host, original: Option<String>) -> R<()> {
    let mut host = host;
    host.name = host.name.trim().to_string();
    host.hostname = host.hostname.trim().to_string();
    valid_name(&host.name)?;
    host.user = host.user.trim().to_string();
    if host.hostname.is_empty() {
        return Err(invalid("hostname is required"));
    }
    s.broker.write(|d| {
        if d.user_of(&host).is_empty() {
            return Err(invalid("type a user, or pick a credential that has one"));
        }
        let orig = original.unwrap_or_else(|| host.name.clone());
        if host.name != orig && d.hosts.iter().any(|h| h.name == host.name) {
            return Err(invalid(format!("a host named `{}` already exists", host.name)));
        }
        if !host.credential.is_empty() && !d.credentials.iter().any(|c| c.id == host.credential) {
            return Err(invalid(format!("credential `{}` does not exist", host.credential)));
        }
        for j in &host.jump {
            if j == &host.name || !d.hosts.iter().any(|h| &h.name == j) {
                return Err(invalid(format!("jump host `{j}` does not exist")));
            }
        }
        // Renombrar: los saltos que apuntaban al nombre viejo siguen funcionando.
        if host.name != orig {
            for h in d.hosts.iter_mut() {
                for j in h.jump.iter_mut() {
                    if *j == orig {
                        *j = host.name.clone();
                    }
                }
            }
        }
        match d.hosts.iter_mut().find(|h| h.name == orig) {
            Some(h) => {
                // El sistema detectado sigue valiendo si es la misma maquina; si cambio la
                // direccion se vuelve a preguntar.
                host.os = if h.hostname == host.hostname && h.port == host.port { h.os.take() } else { None };
                *h = host
            }
            None => d.hosts.push(host),
        }
        Ok(())
    })?;
    s.broker.drop_connections().await;
    Ok(())
}

#[tauri::command]
pub async fn host_delete(s: State<'_, AppState>, name: String) -> R<()> {
    s.broker.write(|d| {
        if let Some(h) = d.hosts.iter().find(|h| h.jump.contains(&name)) {
            return Err(invalid(format!("`{}` uses this host as a jump", h.name)));
        }
        d.hosts.retain(|h| h.name != name);
        Ok(())
    })?;
    s.broker.drop_connections().await;
    Ok(())
}

/// Prueba la conexion. Devuelve null si todo esta bien, o la huella a confirmar.
#[tauri::command]
pub async fn host_test(s: State<'_, AppState>, name: String) -> R<Option<ethereon_core::ssh::SshError>> {
    s.broker.test_host(&name).await
}

/// Abre un link en el navegador (por ejemplo, el que manda Tailscale SSH para autorizar un
/// login). Solo https, para que no sirva para lanzar otra cosa.
#[tauri::command]
pub fn open_link(url: String) -> R<()> {
    let ok = url.starts_with("https://") && !url.chars().any(|c| c.is_whitespace() || c == '"' || c.is_control());
    if !ok {
        return Err(Error::Invalid("only https links can be opened".into()));
    }
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("rundll32");
    #[cfg(windows)]
    cmd.args(["url.dll,FileProtocolHandler", &url]);
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "macos")]
    cmd.arg(&url);
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(not(any(windows, target_os = "macos")))]
    cmd.arg(&url);
    cmd.spawn().map(|_| ()).map_err(|e| Error::Invalid(format!("could not open the browser: {e}")))
}

#[tauri::command]
pub async fn host_trust(s: State<'_, AppState>, addr: String, fingerprint: String, algorithm: String) -> R<()> {
    s.broker.trust_host_key(&addr, &fingerprint, &algorithm).await
}

#[derive(Deserialize)]
pub struct CredInput {
    id: String,
    original: Option<String>,
    kind: CredentialKind,
    #[serde(default)]
    username: String,
    /// None = dejar el que esta.
    secret: Option<String>,
    passphrase: Option<String>,
    certificate: Option<String>,
    #[serde(default)]
    note: String,
}

#[tauri::command]
pub async fn cred_save(s: State<'_, AppState>, cred: CredInput) -> R<()> {
    let id = cred.id.trim().to_string();
    valid_name(&id)?;
    s.broker.write(|d| {
        let orig = cred.original.clone().unwrap_or_else(|| id.clone());
        if id != orig && d.credentials.iter().any(|c| c.id == id) {
            return Err(invalid(format!("a credential `{id}` already exists")));
        }
        let existing = d.credentials.iter().position(|c| c.id == orig);
        let mut c = match existing {
            Some(i) => d.credentials[i].clone(),
            None => Credential {
                id: id.clone(),
                kind: cred.kind,
                username: String::new(),
                secret: String::new(),
                passphrase: String::new(),
                certificate: String::new(),
                note: String::new(),
            },
        };
        c.id = id.clone();
        c.kind = cred.kind;
        c.note = cred.note.clone();
        c.username = cred.username.trim().to_string();
        if let Some(v) = &cred.secret {
            c.secret = v.clone();
        }
        if let Some(v) = &cred.passphrase {
            c.passphrase = v.clone();
        }
        if let Some(v) = &cred.certificate {
            c.certificate = v.trim().to_string();
        }
        if c.secret.is_empty() {
            return Err(invalid("the secret is empty"));
        }
        if matches!(c.kind, CredentialKind::PrivateKey | CredentialKind::Certificate) {
            let pass = (!c.passphrase.is_empty()).then_some(c.passphrase.as_str());
            if let Err(e) = russh::keys::decode_secret_key(&c.secret, pass) {
                return Err(invalid(format!("could not read the private key: {e}")));
            }
        }
        match existing {
            Some(i) => d.credentials[i] = c,
            None => d.credentials.push(c),
        }
        if id != orig {
            for h in d.hosts.iter_mut() {
                if h.credential == orig {
                    h.credential = id.clone();
                }
                if h.sudo_credential.as_deref() == Some(&orig) {
                    h.sudo_credential = Some(id.clone());
                }
            }
        }
        Ok(())
    })?;
    s.broker.drop_connections().await;
    Ok(())
}

#[tauri::command]
pub async fn cred_delete(s: State<'_, AppState>, id: String) -> R<()> {
    s.broker.write(|d| {
        if let Some(h) = d.hosts.iter().find(|h| h.credential == id || h.sudo_credential.as_deref() == Some(&id)) {
            return Err(invalid(format!("`{}` uses this credential", h.name)));
        }
        d.credentials.retain(|c| c.id != id);
        Ok(())
    })?;
    s.broker.drop_connections().await;
    Ok(())
}

#[tauri::command]
pub fn group_save(s: State<AppState>, group: Group, original: Option<String>) -> R<()> {
    valid_name(group.name.trim())?;
    s.broker.write(|d| {
        let orig = original.unwrap_or_else(|| group.name.clone());
        if group.name != orig {
            if d.groups.iter().any(|g| g.name == group.name) {
                return Err(invalid("that group already exists"));
            }
            for h in d.hosts.iter_mut().filter(|h| h.group.as_deref() == Some(&orig)) {
                h.group = Some(group.name.clone());
            }
        }
        match d.groups.iter_mut().find(|g| g.name == orig) {
            Some(g) => *g = group,
            None => d.groups.push(group),
        }
        Ok(())
    })
}

#[tauri::command]
pub fn group_delete(s: State<AppState>, name: String) -> R<()> {
    s.broker.write(|d| {
        d.groups.retain(|g| g.name != name);
        for h in d.hosts.iter_mut().filter(|h| h.group.as_deref() == Some(&name)) {
            h.group = None;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn denylist_save(s: State<AppState>, rules: Vec<String>) -> R<()> {
    let rules: Vec<String> = rules.into_iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
    s.broker.write(|d| {
        d.denylist = rules;
        Ok(())
    })
}

#[tauri::command]
pub fn settings_save(s: State<AppState>, settings: Settings) -> R<()> {
    s.broker.write(|d| {
        d.settings = settings;
        Ok(())
    })
}

#[tauri::command]
pub fn agent_set(s: State<AppState>, on: bool) -> R<()> {
    s.broker.set_agent_enabled(on)
}

#[derive(Serialize)]
pub struct NewDevice {
    id: String,
    token: String,
    command: String,
}

#[tauri::command]
pub fn device_create(s: State<AppState>, name: String, expires_days: Option<u32>, hosts: Vec<String>) -> R<NewDevice> {
    let (dev, token) = s.broker.create_device(&name, expires_days, hosts)?;
    let url = format!("http://127.0.0.1:{}/mcp", s.config.port);
    let command = format!("claude mcp add --transport http --scope user ethereon {url} --header \"Authorization: Bearer {token}\"");
    Ok(NewDevice { id: dev.id, token, command })
}

#[tauri::command]
pub fn device_revoke(s: State<AppState>, id: String) -> R<()> {
    s.broker.write(|d| {
        d.devices.iter_mut().filter(|x| x.id == id).for_each(|x| x.revoked = true);
        Ok(())
    })
}

#[tauri::command]
pub fn device_delete(s: State<AppState>, id: String) -> R<()> {
    s.broker.write(|d| {
        d.devices.retain(|x| x.id != id);
        Ok(())
    })
}

#[tauri::command]
pub fn approvals_list(s: State<AppState>) -> Vec<ApprovalView> {
    s.broker.approvals()
}

#[tauri::command]
pub fn approval_resolve(s: State<AppState>, id: u64, decision: Decision) -> R<()> {
    s.broker.resolve(id, decision)
}

#[tauri::command]
pub fn alerts_list(s: State<AppState>) -> Vec<Alert> {
    s.broker.alerts()
}

#[tauri::command]
pub fn alert_dismiss(s: State<AppState>, id: u64) {
    s.broker.dismiss_alert(id)
}

#[derive(Serialize)]
pub struct TestResult {
    verdict: &'static str,
    detail: String,
}

/// Que pasaria si el agente pidiera `command` en `host` (para probar reglas).
#[tauri::command]
pub fn policy_test(s: State<AppState>, host: String, command: String) -> R<TestResult> {
    s.broker.read(|d| {
        let h = d.hosts.iter().find(|h| h.name == host).ok_or(Error::NotFound(host.clone()))?;
        let p = ethereon_core::Broker::effective_policy(d, h);
        Ok(match policy::evaluate(&command, &p, &d.denylist, &[]) {
            policy::Verdict::Allow => TestResult { verdict: "allow", detail: "runs without asking".into() },
            policy::Verdict::Ask(w) => TestResult {
                verdict: "ask",
                detail: if w.is_empty() { "waits for your approval".into() } else { w.join("; ") },
            },
            policy::Verdict::Deny(why) => TestResult { verdict: "deny", detail: why },
        })
    })?
}

#[tauri::command]
pub fn audit_search(s: State<AppState>, query: String, limit: Option<usize>) -> Vec<Entry> {
    s.broker.audit().search(&query, limit.unwrap_or(300))
}

#[tauri::command]
pub fn audit_verify(s: State<AppState>) -> R<String> {
    match s.broker.audit_verify()? {
        Ok(n) => Ok(format!("{n} signed entries, chain intact")),
        Err(e) => Err(invalid(e)),
    }
}

#[tauri::command]
pub fn audit_export(s: State<AppState>, path: String) -> R<usize> {
    let p = PathBuf::from(&path);
    if p.extension().is_some_and(|e| e == "jsonl") {
        std::fs::copy(s.broker.audit().path(), &p)?;
        return Ok(s.broker.audit().search("", usize::MAX).len());
    }
    Ok(s.broker.audit().export_csv(&p)?)
}

#[derive(Serialize, Default)]
pub struct ImportSummary {
    hosts: usize,
    credentials: usize,
    skipped: usize,
    notes: Vec<String>,
}

fn merge(s: &State<AppState>, imp: import::Imported) -> R<ImportSummary> {
    s.broker.write(|d| {
        let mut sum = ImportSummary { notes: imp.notes.clone(), ..Default::default() };
        for c in imp.credentials.iter() {
            if d.credentials.iter().any(|x| x.id == c.id) {
                sum.skipped += 1;
            } else {
                d.credentials.push(c.clone());
                sum.credentials += 1;
            }
        }
        for h in imp.hosts.iter() {
            if d.hosts.iter().any(|x| x.name == h.name) {
                sum.skipped += 1;
            } else {
                d.hosts.push(h.clone());
                sum.hosts += 1;
            }
        }
        for g in imp.groups.iter() {
            if !d.groups.iter().any(|x| x.name == g.name) {
                d.groups.push(g.clone());
            }
        }
        // Una huella que ya esta confirmada aca no se pisa con la del archivo.
        for k in imp.known_hosts.iter() {
            match d.known_hosts.iter().find(|x| x.addr == k.addr) {
                None => d.known_hosts.push(k.clone()),
                Some(x) if x.fingerprint != k.fingerprint => {
                    sum.notes.push(format!("{}: kept the host key already confirmed here", k.addr))
                }
                Some(_) => {}
            }
        }
        Ok(sum)
    })
}

/// Sin `path`: el `~/.ssh/config` de esta PC.
#[tauri::command]
pub fn import_ssh_config(s: State<AppState>, path: Option<String>) -> R<ImportSummary> {
    let text = match path {
        Some(p) => std::fs::read_to_string(p)?,
        None => import::default_ssh_config().ok_or(invalid("there is no ~/.ssh/config on this PC"))?,
    };
    merge(&s, import::ssh_config(&text))
}

#[tauri::command]
pub fn import_key_file(s: State<AppState>, path: String) -> R<ImportSummary> {
    let c = import::key_file(std::path::Path::new(&path)).map_err(invalid)?;
    merge(&s, import::Imported { credentials: vec![c], ..Default::default() })
}

#[tauri::command]
pub fn import_bitwarden(s: State<AppState>, path: String) -> R<ImportSummary> {
    let text = std::fs::read_to_string(path)?;
    merge(&s, import::bitwarden(&text).map_err(invalid)?)
}

#[derive(Serialize)]
pub struct Detected {
    kind: import::Kind,
    label: &'static str,
    needs_password: bool,
}

/// Que es este archivo o carpeta (para el boton Import y para soltar archivos).
#[tauri::command]
pub fn import_detect(path: String) -> R<Detected> {
    let kind = import::detect(std::path::Path::new(&path)).map_err(invalid)?;
    Ok(Detected { kind, label: kind.label(), needs_password: kind == import::Kind::EthereonBackup })
}

/// Importa sumando a lo que hay (nunca reemplaza). El respaldo cifrado pide su
/// contrasena; abrirlo tarda (Argon2), por eso va en otro hilo.
#[tauri::command]
pub async fn import_auto(s: State<'_, AppState>, path: String, password: Option<String>) -> R<ImportSummary> {
    s.broker.read(|_| ())?;
    let imp = tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::PathBuf::from(&path);
        let kind = import::detect(&p)?;
        import::load(&p, kind, password.as_deref())
    })
    .await
    .map_err(|e| invalid(e.to_string()))?
    .map_err(invalid)?;
    merge(&s, imp)
}

/// Exportar con secretos pide la contrasena maestra otra vez.
#[tauri::command]
pub async fn export_data(s: State<'_, AppState>, format: Format, path: String, password: Option<String>) -> R<String> {
    if export::has_secrets(format) {
        let b = s.broker.clone();
        let pw = password.unwrap_or_default();
        tauri::async_runtime::spawn_blocking(move || b.verify_password(&pw)).await.map_err(|e| invalid(e.to_string()))??;
    }
    let p = PathBuf::from(path);
    s.broker.read(|d| export::write(d, format, &p))?.map_err(Error::Io)
}

#[tauri::command]
pub fn backup_vault(s: State<AppState>, path: String) -> R<()> {
    s.broker.backup_to(std::path::Path::new(&path))
}

// ---------------------------------------------------------------- actualizaciones

#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub fn update_status(app: tauri::AppHandle) -> Option<crate::updater::UpdateInfo> {
    crate::updater::status(&app)
}

#[tauri::command]
pub async fn update_check(app: tauri::AppHandle) -> Result<Option<String>, String> {
    crate::updater::check_now(&app).await
}

#[tauri::command]
pub async fn update_now(app: tauri::AppHandle) {
    crate::updater::apply(&app).await;
}
