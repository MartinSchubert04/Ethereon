//! Exportar. Lo llama solo la interfaz, y la que lleva secretos pide la contrasena
//! maestra otra vez. El agente no tiene forma de llegar aca.
//!
//! - `Bitwarden`: JSON sin cifrar que Bitwarden importa (logins y claves SSH).
//! - `OpenSsh`: carpeta con `config` y los archivos de clave. Es lo que importan Termius,
//!   VS Code, OpenSSH y casi todo; Termius lo toma desde "Import > SSH config".
//! - `Csv`: hosts sin secretos, para una planilla.
//! - `Json`: todo el vault en claro (para migrar o revisar).
//! - El respaldo cifrado (copiar el vault tal cual) esta en `Broker::backup_to`.

use serde::Deserialize;
use serde_json::json;
use std::path::Path;

use crate::model::{CredentialKind, VaultData};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Bitwarden,
    OpenSsh,
    Csv,
    Json,
}

pub fn has_secrets(f: Format) -> bool {
    f != Format::Csv
}

/// Escribe el export en `to` (un archivo; para OpenSSH, una carpeta). Devuelve un resumen.
pub fn write(d: &VaultData, f: Format, to: &Path) -> std::io::Result<String> {
    match f {
        Format::Json => {
            let mut v = d.clone();
            v.audit_key = String::new();
            v.ui_session = None;
            std::fs::write(to, serde_json::to_vec_pretty(&v)?)?;
            Ok(format!("{} hosts and {} credentials", d.hosts.len(), d.credentials.len()))
        }
        Format::Csv => {
            let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
            let mut s = String::from("name,hostname,port,user,credential,group,env,tags,jump,description\n");
            for h in &d.hosts {
                s += &format!(
                    "{},{},{},{},{},{},{},{},{},{}\n",
                    q(&h.name),
                    q(&h.hostname),
                    h.port,
                    q(&h.user),
                    q(&h.credential),
                    q(h.group.as_deref().unwrap_or("")),
                    h.env.as_str(),
                    q(&h.tags.join(" ")),
                    q(&h.jump.join(",")),
                    q(&h.description)
                );
            }
            std::fs::write(to, s)?;
            Ok(format!("{} hosts", d.hosts.len()))
        }
        Format::Bitwarden => {
            let mut items = Vec::new();
            for c in &d.credentials {
                let users: Vec<String> = if c.username.is_empty() { d.hosts.iter().filter(|h| h.credential == c.id).map(|h| d.user_of(h)).collect() } else { vec![c.username.clone()] };
                let uris: Vec<_> = d
                    .hosts
                    .iter()
                    .filter(|h| h.credential == c.id || h.sudo_credential.as_deref() == Some(&c.id))
                    .map(|h| json!({ "match": null, "uri": format!("ssh://{}:{}", h.hostname, h.port) }))
                    .collect();
                let item = match c.kind {
                    CredentialKind::PrivateKey | CredentialKind::Certificate => json!({
                        "type": 5, "name": c.id, "notes": note(c.note.as_str(), &c.passphrase, &c.certificate),
                        "favorite": false, "folderId": null,
                        "sshKey": { "privateKey": c.secret, "publicKey": "", "keyFingerprint": "" }
                    }),
                    _ => json!({
                        "type": 1, "name": c.id, "notes": c.note, "favorite": false, "folderId": null,
                        "login": { "username": users.first().cloned().unwrap_or_default(), "password": c.secret, "totp": null, "uris": uris }
                    }),
                };
                items.push(item);
            }
            let n = items.len();
            std::fs::write(to, serde_json::to_vec_pretty(&json!({ "encrypted": false, "folders": [], "items": items }))?)?;
            Ok(format!("{n} items"))
        }
        Format::OpenSsh => {
            std::fs::create_dir_all(to)?;
            let mut cfg = String::from("# Exportado de Ethereon\n");
            for c in &d.credentials {
                if matches!(c.kind, CredentialKind::PrivateKey | CredentialKind::Certificate) {
                    std::fs::write(to.join(&c.id), &c.secret)?;
                    if !c.certificate.is_empty() {
                        std::fs::write(to.join(format!("{}-cert.pub", c.id)), &c.certificate)?;
                    }
                }
            }
            for h in &d.hosts {
                cfg += &format!("\nHost {}\n  HostName {}\n  Port {}\n  User {}\n", h.name, h.hostname, h.port, d.user_of(h));
                let is_key = d.credentials.iter().any(|c| c.id == h.credential && matches!(c.kind, CredentialKind::PrivateKey | CredentialKind::Certificate));
                if is_key {
                    cfg += &format!("  IdentityFile ~/.ssh/{}\n", h.credential);
                }
                if !h.jump.is_empty() {
                    cfg += &format!("  ProxyJump {}\n", h.jump.join(","));
                }
            }
            std::fs::write(to.join("config"), cfg)?;
            Ok(format!("{} hosts; copy the folder contents to ~/.ssh", d.hosts.len()))
        }
    }
}

fn note(n: &str, passphrase: &str, cert: &str) -> String {
    let mut s = n.to_string();
    if !passphrase.is_empty() {
        s += &format!("\nPassphrase: {passphrase}");
    }
    if !cert.is_empty() {
        s += &format!("\nCertificate: {cert}");
    }
    s.trim().to_string()
}
