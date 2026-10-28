//! Importar: `~/.ssh/config` (lo que tambien exporta Termius), archivos de clave, el
//! JSON sin cifrar de Bitwarden y lo que exporta Ethereon (JSON, CSV, carpeta OpenSSH y
//! el respaldo cifrado). `detect` reconoce el formato mirando el contenido. Devuelven lo
//! que hay que agregar; el broker decide.

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::model::{Credential, CredentialKind, Group, Host, KnownHost, VaultData};

#[derive(Debug, Default, serde::Serialize)]
pub struct Imported {
    #[serde(skip)]
    pub hosts: Vec<Host>,
    #[serde(skip)]
    pub credentials: Vec<Credential>,
    #[serde(skip)]
    pub groups: Vec<Group>,
    #[serde(skip)]
    pub known_hosts: Vec<KnownHost>,
    pub notes: Vec<String>,
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default()
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

/// Un ID de credencial legible a partir de un nombre de archivo: `id_ed25519`.
pub fn cred_id(s: &str) -> String {
    let base = Path::new(s).file_stem().and_then(|x| x.to_str()).unwrap_or(s);
    let id: String = base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    id.trim_matches('-').to_lowercase()
}

pub fn key_file(path: &Path) -> Result<Credential, String> {
    let secret = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !secret.contains("PRIVATE KEY") && !secret.starts_with("PuTTY-User-Key-File") {
        return Err(format!("{} is not a private key", path.display()));
    }
    // Si hay un certificado al lado (id_ed25519-cert.pub), se suma.
    let cert_path = PathBuf::from(format!("{}-cert.pub", path.display()));
    let certificate = std::fs::read_to_string(&cert_path).unwrap_or_default().trim().to_string();
    Ok(Credential {
        id: cred_id(&path.to_string_lossy()),
        kind: if certificate.is_empty() { CredentialKind::PrivateKey } else { CredentialKind::Certificate },
        username: String::new(),
        secret,
        passphrase: String::new(),
        certificate,
        note: format!("imported from {}", path.display()),
    })
}

/// Lee un `~/.ssh/config`. Los bloques con comodines (`Host *`) se saltean.
pub fn ssh_config(text: &str) -> Imported {
    ssh_config_in(text, None)
}

/// Con `dir`, los IdentityFile se buscan primero en esa carpeta: la exportacion OpenSSH de
/// Ethereon dice `~/.ssh/<clave>`, pero las claves estan al lado del config.
pub fn ssh_config_in(text: &str, dir: Option<&Path>) -> Imported {
    let mut out = Imported::default();
    let mut cur: Option<Host> = None;
    let mut blocks: Vec<(Host, Option<String>, Vec<String>)> = Vec::new();
    let mut identity: Option<String> = None;
    let mut jumps: Vec<String> = Vec::new();
    let flush = |cur: &mut Option<Host>, identity: &mut Option<String>, jumps: &mut Vec<String>, blocks: &mut Vec<(Host, Option<String>, Vec<String>)>| {
        if let Some(h) = cur.take() {
            blocks.push((h, identity.take(), std::mem::take(jumps)));
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (k.to_lowercase(), v.trim().trim_start_matches('=').trim().trim_matches('"').to_string()),
            None => continue,
        };
        match k.as_str() {
            "host" => {
                flush(&mut cur, &mut identity, &mut jumps, &mut blocks);
                let name = v.split_whitespace().next().unwrap_or("").to_string();
                if name.contains(['*', '?', '!']) {
                    cur = None;
                } else {
                    cur = Some(Host { name: name.clone(), hostname: name, port: 22, ..Default::default() });
                }
            }
            "match" => flush(&mut cur, &mut identity, &mut jumps, &mut blocks),
            _ => {
                let Some(h) = cur.as_mut() else { continue };
                match k.as_str() {
                    "hostname" => h.hostname = v,
                    "user" => h.user = v,
                    "port" => h.port = v.parse().unwrap_or(22),
                    "identityfile" => identity = identity.or(Some(v)),
                    "proxyjump" => jumps = v.split(',').map(|s| s.trim().to_string()).filter(|s| s != "none").collect(),
                    _ => {}
                }
            }
        }
    }
    flush(&mut cur, &mut identity, &mut jumps, &mut blocks);

    let names: Vec<String> = blocks.iter().map(|b| b.0.name.clone()).collect();
    for (mut h, ident, jumps) in blocks {
        if let Some(path) = ident {
            let local = dir.and_then(|d| Path::new(&path).file_name().map(|n| d.join(n))).filter(|p| p.is_file());
            let p = local.unwrap_or_else(|| expand(&path));
            match key_file(&p) {
                Ok(c) => {
                    h.credential = c.id.clone();
                    if !out.credentials.iter().any(|x| x.id == c.id) {
                        out.credentials.push(c);
                    }
                }
                Err(e) => out.notes.push(format!("{}: {e}", h.name)),
            }
        } else {
            out.notes.push(format!("{}: no IdentityFile, pick a credential", h.name));
        }
        for j in jumps {
            // ProxyJump puede ser otro alias o "usuario@host:puerto".
            if names.contains(&j) {
                h.jump.push(j);
            } else {
                let (user, rest) = j.split_once('@').map(|(u, r)| (u.to_string(), r.to_string())).unwrap_or((String::new(), j.clone()));
                let (host, port) = rest.rsplit_once(':').map(|(h, p)| (h.to_string(), p.parse().unwrap_or(22))).unwrap_or((rest.clone(), 22));
                let name = format!("jump-{host}");
                if !out.hosts.iter().any(|x| x.name == name) {
                    out.hosts.push(Host { name: name.clone(), hostname: host, port, user, credential: h.credential.clone(), ..Default::default() });
                }
                h.jump.push(name);
            }
        }
        if h.user.is_empty() {
            h.user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "root".into());
        }
        out.hosts.push(h);
    }
    out
}

pub fn default_ssh_config() -> Option<String> {
    std::fs::read_to_string(home().join(".ssh").join("config")).ok()
}

/// JSON sin cifrar de Bitwarden: logins (usuario/contrasena) y claves SSH.
pub fn bitwarden(text: &str) -> Result<Imported, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not a JSON file: {e}"))?;
    if v.get("encrypted").and_then(|e| e.as_bool()) == Some(true) {
        return Err("this is an encrypted Bitwarden export; export it as unencrypted JSON".into());
    }
    let mut out = Imported::default();
    for it in v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default() {
        let name = it.get("name").and_then(|n| n.as_str()).unwrap_or("item");
        let id = cred_id(name);
        let note = it.get("notes").and_then(|n| n.as_str()).unwrap_or("").to_string();
        match it.get("type").and_then(|t| t.as_i64()) {
            Some(1) => {
                let pw = it.pointer("/login/password").and_then(|p| p.as_str()).unwrap_or("");
                if pw.is_empty() {
                    continue;
                }
                let username = it.pointer("/login/username").and_then(|p| p.as_str()).unwrap_or("").to_string();
                out.credentials.push(Credential { id, kind: CredentialKind::Password, username, secret: pw.into(), passphrase: String::new(), certificate: String::new(), note });
            }
            Some(5) => {
                let key = it.pointer("/sshKey/privateKey").and_then(|p| p.as_str()).unwrap_or("");
                if key.is_empty() {
                    continue;
                }
                out.credentials.push(Credential { id, kind: CredentialKind::PrivateKey, username: String::new(), secret: key.into(), passphrase: String::new(), certificate: String::new(), note });
            }
            _ => {}
        }
    }
    out.notes.push(format!("{} credentials found", out.credentials.len()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_config() {
        let cfg = "Host *\n  User nobody\n\nHost bastion\n  HostName 1.2.3.4\n  User ops\n\nHost web\n  HostName 10.0.0.5\n  Port 2222\n  User deploy\n  ProxyJump bastion\n\nHost db\n  HostName 10.0.0.9\n  ProxyJump admin@gw.example.com:2200\n";
        let r = ssh_config(cfg);
        let web = r.hosts.iter().find(|h| h.name == "web").unwrap();
        assert_eq!((web.hostname.as_str(), web.port, web.user.as_str()), ("10.0.0.5", 2222, "deploy"));
        assert_eq!(web.jump, vec!["bastion"]);
        let db = r.hosts.iter().find(|h| h.name == "db").unwrap();
        assert_eq!(db.jump, vec!["jump-gw.example.com"]);
        let gw = r.hosts.iter().find(|h| h.name == "jump-gw.example.com").unwrap();
        assert_eq!((gw.user.as_str(), gw.port), ("admin", 2200));
        assert!(!r.hosts.iter().any(|h| h.name == "*"));
    }

    /// Lo que genera `termius export-ssh-config` (CLI 1.2.15): claves en minuscula, sin claves privadas.
    #[test]
    fn parses_termius_cli_export() {
        let cfg = "Host web
    hostname 10.0.0.5
    user deploy
    port 2222

Host bastion
    hostname 1.2.3.4
    user ops
    port 22
";
        let r = ssh_config(cfg);
        assert_eq!(r.hosts.len(), 2);
        let web = r.hosts.iter().find(|h| h.name == "web").unwrap();
        assert_eq!((web.hostname.as_str(), web.port, web.user.as_str()), ("10.0.0.5", 2222, "deploy"));
        assert!(r.credentials.is_empty());
    }

    #[test]
    fn parses_bitwarden() {
        let j = r#"{"encrypted":false,"items":[{"type":1,"name":"Prod DB","login":{"username":"a","password":"pw123456"}},{"type":5,"name":"deploy key","sshKey":{"privateKey":"-----BEGIN OPENSSH PRIVATE KEY-----\nx\n-----END OPENSSH PRIVATE KEY-----"}}]}"#;
        let r = bitwarden(j).unwrap();
        assert_eq!(r.credentials.len(), 2);
        assert_eq!(r.credentials[0].id, "prod-db");
        assert_eq!(r.credentials[1].kind, CredentialKind::PrivateKey);
    }
}

// ---- deteccion automatica

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    SshConfig,
    SshFolder,
    KeyFile,
    Bitwarden,
    EthereonJson,
    EthereonBackup,
    EthereonCsv,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::SshConfig => "SSH config",
            Kind::SshFolder => "SSH folder",
            Kind::KeyFile => "private key",
            Kind::Bitwarden => "Bitwarden export",
            Kind::EthereonJson => "Ethereon JSON export",
            Kind::EthereonBackup => "Ethereon encrypted backup",
            Kind::EthereonCsv => "Ethereon CSV export",
        }
    }
}

/// Reconoce el formato por el contenido, no por la extension.
pub fn detect(path: &Path) -> Result<Kind, String> {
    if path.is_dir() {
        return if path.join("config").is_file() {
            Ok(Kind::SshFolder)
        } else {
            Err("that folder has no `config` file (pick an OpenSSH export or a .ssh folder)".into())
        };
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if (t.starts_with("-----BEGIN") && t.contains("PRIVATE KEY")) || t.starts_with("PuTTY-User-Key-File") {
        return Ok(Kind::KeyFile);
    }
    if t.starts_with("ssh-") || t.starts_with("ecdsa-") {
        return Err("that is a public key: pick the private one (the file without .pub)".into());
    }
    if t.starts_with('{') {
        let v: Value = serde_json::from_str(t).map_err(|_| "it looks like JSON, but it could not be read".to_string())?;
        if v.get("kdf").is_some() && v.get("nonce").is_some() && v.get("data").is_some() {
            return Ok(Kind::EthereonBackup);
        }
        if v.get("items").is_some_and(|i| i.is_array()) {
            if v.get("encrypted").and_then(|e| e.as_bool()) == Some(true) {
                return Err("that Bitwarden export is encrypted: export it again as plain JSON".into());
            }
            return Ok(Kind::Bitwarden);
        }
        if v.get("hosts").is_some() || v.get("credentials").is_some() {
            return Ok(Kind::EthereonJson);
        }
        return Err("unknown JSON file".into());
    }
    if t.lines().next().is_some_and(|l| l.trim().starts_with("name,hostname,port,user")) {
        return Ok(Kind::EthereonCsv);
    }
    let is_ssh = t.lines().any(|l| {
        let l = l.trim().to_lowercase();
        l.starts_with("host ") || l.starts_with("host\t") || l.starts_with("host=")
    });
    if is_ssh {
        return Ok(Kind::SshConfig);
    }
    Err("could not tell what this file is. It works with SSH configs, private keys, Bitwarden JSON and Ethereon exports.".into())
}

/// Lee el archivo segun su formato. El respaldo cifrado necesita su contrasena maestra.
pub fn load(path: &Path, kind: Kind, password: Option<&str>) -> Result<Imported, String> {
    let read = || std::fs::read_to_string(path).map_err(|e| e.to_string());
    match kind {
        Kind::SshConfig => Ok(ssh_config_in(&read()?, path.parent())),
        Kind::SshFolder => {
            let text = std::fs::read_to_string(path.join("config")).map_err(|e| e.to_string())?;
            Ok(ssh_config_in(&text, Some(path)))
        }
        Kind::KeyFile => Ok(Imported { credentials: vec![key_file(path)?], ..Default::default() }),
        Kind::Bitwarden => bitwarden(&read()?),
        Kind::EthereonJson => {
            let d: VaultData = serde_json::from_str(&read()?).map_err(|e| format!("could not read the export: {e}"))?;
            Ok(from_vault(d))
        }
        Kind::EthereonBackup => {
            let pw = password.ok_or("this backup needs its master password")?;
            let (_, d) = crate::vault::open(path, pw).map_err(|e| match e {
                crate::Error::WrongPassword => "wrong master password for that backup".to_string(),
                e => e.to_string(),
            })?;
            Ok(from_vault(d))
        }
        Kind::EthereonCsv => csv(&read()?),
    }
}

/// De un vault (respaldo o export JSON) se traen hosts, credenciales, grupos y huellas.
/// Agentes, ajustes y la clave de la auditoria son de cada PC y no se mezclan.
fn from_vault(d: VaultData) -> Imported {
    let mut out = Imported {
        hosts: d.hosts.clone(),
        credentials: d.credentials.clone(),
        groups: d.groups.clone(),
        known_hosts: d.known_hosts.clone(),
        notes: Vec::new(),
    };
    if !d.devices.is_empty() {
        out.notes.push("Agent tokens are not imported: create new ones in Agents".into());
    }
    out
}

/// El CSV que exporta Ethereon (solo hosts, sin secretos).
pub fn csv(text: &str) -> Result<Imported, String> {
    let mut out = Imported::default();
    for (i, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let f = split_csv(line);
        if f.len() < 10 {
            out.notes.push(format!("line {}: expected 10 columns", i + 1));
            continue;
        }
        let env = match f[6].as_str() {
            "prod" => crate::model::Env::Prod,
            "test" => crate::model::Env::Test,
            "dev" => crate::model::Env::Dev,
            _ => crate::model::Env::Unset,
        };
        out.hosts.push(Host {
            name: f[0].clone(),
            hostname: f[1].clone(),
            port: f[2].parse().unwrap_or(22),
            user: f[3].clone(),
            credential: f[4].clone(),
            group: (!f[5].is_empty()).then(|| f[5].clone()),
            env,
            tags: f[7].split_whitespace().map(String::from).collect(),
            jump: f[8].split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
            description: f[9].clone(),
            ..Default::default()
        });
    }
    if !out.hosts.is_empty() {
        out.notes.push("The CSV has no secrets: check that each host's credential exists in Keychain".into());
    }
    Ok(out)
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                out.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(String::new()),
            c => out.last_mut().unwrap().push(c),
        }
    }
    out
}

#[cfg(test)]
mod detect_tests {
    use super::*;

    fn tmp(name: &str, body: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eth-detect-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn detects_formats_by_content() {
        assert_eq!(detect(&tmp("a", "Host web\n  HostName 1.2.3.4\n")), Ok(Kind::SshConfig));
        assert_eq!(detect(&tmp("b.txt", "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n")), Ok(Kind::KeyFile));
        assert_eq!(detect(&tmp("c.json", r#"{"encrypted":false,"items":[]}"#)), Ok(Kind::Bitwarden));
        assert_eq!(detect(&tmp("d.json", r#"{"kdf":{},"nonce":"x","data":"y"}"#)), Ok(Kind::EthereonBackup));
        assert_eq!(detect(&tmp("e.json", r#"{"hosts":[],"credentials":[]}"#)), Ok(Kind::EthereonJson));
        assert_eq!(detect(&tmp("f.csv", "name,hostname,port,user,credential,group,env,tags,jump,description\n")), Ok(Kind::EthereonCsv));
        assert!(detect(&tmp("g.pub", "ssh-ed25519 AAAA x")).unwrap_err().contains("public key"));
        assert!(detect(&tmp("h", "hello")).is_err());
    }

    #[test]
    fn csv_round_trip() {
        let mut d = VaultData::default();
        d.hosts.push(Host {
            name: "web".into(),
            hostname: "10.0.0.5".into(),
            port: 2222,
            user: "deploy".into(),
            credential: "k".into(),
            description: r#"a, "quoted" one"#.into(),
            tags: vec!["x".into(), "y".into()],
            jump: vec!["bastion".into()],
            env: crate::model::Env::Prod,
            ..Default::default()
        });
        let p = std::env::temp_dir().join(format!("eth-csv-{}.csv", std::process::id()));
        crate::export::write(&d, crate::export::Format::Csv, &p).unwrap();
        let imp = load(&p, detect(&p).unwrap(), None).unwrap();
        let h = &imp.hosts[0];
        assert_eq!((h.name.as_str(), h.port, h.user.as_str(), h.description.as_str()), ("web", 2222, "deploy", r#"a, "quoted" one"#));
        assert_eq!((h.tags.len(), h.jump[0].as_str(), h.env), (2, "bastion", crate::model::Env::Prod));
    }

    #[test]
    fn backup_round_trip_and_wrong_password() {
        let mut d = VaultData::default();
        d.hosts.push(Host { name: "db".into(), hostname: "10.0.0.9".into(), ..Default::default() });
        let p = std::env::temp_dir().join(format!("eth-backup-{}.json", std::process::id()));
        crate::vault::create(&p, "backup-pass-1", d).unwrap();
        assert_eq!(detect(&p), Ok(Kind::EthereonBackup));
        assert!(load(&p, Kind::EthereonBackup, Some("nope")).unwrap_err().contains("wrong master password"));
        assert_eq!(load(&p, Kind::EthereonBackup, Some("backup-pass-1")).unwrap().hosts[0].name, "db");
    }

    #[test]
    fn folder_export_finds_keys_next_to_config() {
        let d = std::env::temp_dir().join(format!("eth-folder-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("config"), "Host web\n  HostName 1.2.3.4\n  User u\n  IdentityFile ~/.ssh/eth-only-here-key\n").unwrap();
        std::fs::write(d.join("eth-only-here-key"), "-----BEGIN OPENSSH PRIVATE KEY-----\nbroken\n-----END OPENSSH PRIVATE KEY-----\n").unwrap();
        assert_eq!(detect(&d), Ok(Kind::SshFolder));
        let imp = load(&d, Kind::SshFolder, None).unwrap();
        assert_eq!(imp.hosts.len(), 1);
        // La clave esta rota a proposito: lo importante es que la busco al lado del config
        // (si no, el error seria que no existe el archivo).
        let note = imp.notes.join(" | ");
        assert!(!note.to_lowercase().contains("cannot find") && !note.to_lowercase().contains("no such file"), "{note}");
    }
}
