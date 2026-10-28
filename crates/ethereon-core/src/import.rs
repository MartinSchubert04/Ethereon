//! Importar: `~/.ssh/config` (lo que tambien exporta Termius), archivos de clave y el
//! JSON sin cifrar de Bitwarden. Devuelven lo que hay que agregar; el broker decide.

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::model::{Credential, CredentialKind, Host};

#[derive(Debug, Default, serde::Serialize)]
pub struct Imported {
    #[serde(skip)]
    pub hosts: Vec<Host>,
    #[serde(skip)]
    pub credentials: Vec<Credential>,
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
            let p = expand(&path);
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
