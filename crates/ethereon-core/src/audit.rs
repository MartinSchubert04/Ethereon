//! Auditoria: una linea JSON por pedido del agente (y por sesion de terminal).
//!
//! Cada linea lleva un HMAC encadenado con la anterior, con una clave que vive dentro
//! del vault: si alguien borra o edita una linea, `verify` lo encuentra. Los pedidos
//! que llegan con el vault bloqueado se anotan sin firma (no hay clave a mano).

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Entry {
    pub ts: String,
    /// Dispositivo / token que hizo el pedido ("ui" para la terminal propia).
    pub device: String,
    #[serde(default)]
    pub client: String,
    pub host: String,
    pub command: String,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub reason: String,
    /// allowed, denied, approved, rejected, timeout, error, locked
    pub decision: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub approved_by: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub output_sha256: String,
    /// Salida ya enmascarada, solo si esta prendido guardarla.
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub masked: usize,
    #[serde(default)]
    pub mac: String,
}

pub struct Audit {
    path: PathBuf,
    last_mac: Mutex<String>,
}

type HmacSha256 = Hmac<Sha256>;

fn mac_of(key: &[u8], prev: &str, e: &Entry) -> String {
    let mut e = e.clone();
    e.mac = String::new();
    let mut m = HmacSha256::new_from_slice(key).unwrap();
    m.update(prev.as_bytes());
    m.update(serde_json::to_string(&e).unwrap().as_bytes());
    hex::encode(m.finalize().into_bytes())
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

impl Audit {
    pub fn open(path: PathBuf) -> Audit {
        let last = read_all(&path).into_iter().rev().find(|e| !e.mac.is_empty()).map(|e| e.mac).unwrap_or_default();
        Audit { path, last_mac: Mutex::new(last) }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn append(&self, mut e: Entry, key: Option<&[u8]>) {
        if e.ts.is_empty() {
            e.ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        }
        let mut last = self.last_mac.lock().unwrap();
        if let Some(k) = key {
            e.mac = mac_of(k, &last, &e);
            *last = e.mac.clone();
        }
        let line = serde_json::to_string(&e).unwrap();
        let res = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut f| writeln!(f, "{line}"));
        if let Err(err) = res {
            log::error!("no se pudo escribir la auditoria: {err}");
        }
    }

    /// Los ultimos `limit` que contienen `query` (en cualquier campo), del mas nuevo al
    /// mas viejo.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Entry> {
        let q = query.to_lowercase();
        let mut out: Vec<Entry> = read_all(&self.path)
            .into_iter()
            .rev()
            .filter(|e| {
                q.is_empty()
                    || [&e.host, &e.command, &e.device, &e.decision, &e.reason, &e.detail]
                        .iter()
                        .any(|f| f.to_lowercase().contains(&q))
            })
            .take(limit)
            .collect();
        out.shrink_to_fit();
        out
    }

    /// Revisa la cadena. Devuelve cuantas lineas firmadas hay, o la primera rota.
    pub fn verify(&self, key: &[u8]) -> Result<usize, String> {
        let mut prev = String::new();
        let mut n = 0;
        for (i, e) in read_all(&self.path).iter().enumerate() {
            if e.mac.is_empty() {
                continue;
            }
            if mac_of(key, &prev, e) != e.mac {
                return Err(format!("entry {} ({}) was modified or an entry before it was removed", i + 1, e.ts));
            }
            prev = e.mac.clone();
            n += 1;
        }
        Ok(n)
    }

    pub fn export_csv(&self, to: &std::path::Path) -> std::io::Result<usize> {
        let rows = read_all(&self.path);
        let mut f = std::fs::File::create(to)?;
        writeln!(f, "ts,device,host,command,sudo,decision,detail,approved_by,exit_code,duration_ms,output_sha256")?;
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        for e in &rows {
            writeln!(
                f,
                "{},{},{},{},{},{},{},{},{},{},{}",
                e.ts,
                q(&e.device),
                q(&e.host),
                q(&e.command),
                e.sudo,
                e.decision,
                q(&e.detail),
                q(&e.approved_by),
                e.exit_code.map(|c| c.to_string()).unwrap_or_default(),
                e.duration_ms.map(|c| c.to_string()).unwrap_or_default(),
                e.output_sha256
            )?;
        }
        Ok(rows.len())
    }
}

fn read_all(path: &PathBuf) -> Vec<Entry> {
    let Ok(f) = std::fs::File::open(path) else { return vec![] };
    std::io::BufReader::new(f).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str(&l).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_detects_edits() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("audit.jsonl");
        let a = Audit::open(p.clone());
        for i in 0..3 {
            a.append(Entry { host: "web".into(), command: format!("ls {i}"), decision: "allowed".into(), ..Default::default() }, Some(b"k"));
        }
        assert_eq!(a.verify(b"k"), Ok(3));
        let raw = std::fs::read_to_string(&p).unwrap().replace("ls 1", "ls X");
        std::fs::write(&p, raw).unwrap();
        assert!(a.verify(b"k").is_err());
        assert_eq!(a.search("ls 2", 10).len(), 1);
    }
}
