//! Enmascarado de la salida antes de que llegue al agente o a la auditoria.
//!
//! Dos pasadas: primero cada secreto del vault (tal cual y en base64), despues patrones
//! conocidos (claves privadas, JWT, `password=`, cabeceras Authorization, cadenas de
//! conexion con contrasena y prefijos de tokens comunes).

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use regex::Regex;
use std::sync::OnceLock;
use zeroize::Zeroizing;

use crate::model::VaultData;

pub const MASK: &str = "***";

pub struct Masker {
    /// Ordenados de mas largo a mas corto: un secreto que contiene a otro se tapa entero.
    secrets: Vec<Zeroizing<String>>,
}

impl Masker {
    pub fn from_vault(v: &VaultData) -> Masker {
        let mut secrets: Vec<Zeroizing<String>> = Vec::new();
        let mut add = |s: &str| {
            let s = s.trim();
            // Algo de 3 letras taparia media salida; esos los cubren los patrones.
            if s.chars().count() >= 4 && !secrets.iter().any(|x| x.as_str() == s) {
                secrets.push(Zeroizing::new(s.to_string()));
            }
        };
        for c in &v.credentials {
            add(&c.passphrase);
            if c.secret.contains("-----BEGIN") || c.secret.contains("PuTTY-User-Key") {
                // Una clave: cada linea del cuerpo, por si se ve un pedazo.
                for line in c.secret.lines().filter(|l| l.len() >= 16 && !l.starts_with("-----")) {
                    add(line);
                }
            } else {
                add(&c.secret);
                if c.secret.len() >= 6 {
                    add(&B64.encode(c.secret.as_bytes()));
                }
            }
        }
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        Masker { secrets }
    }

    pub fn mask(&self, text: &str) -> (String, usize) {
        let mut out = text.to_string();
        let mut hits = 0;
        for s in &self.secrets {
            let n = out.matches(s.as_str()).count();
            if n > 0 {
                hits += n;
                out = out.replace(s.as_str(), MASK);
            }
        }
        let (out, n) = mask_patterns(&out);
        (out, hits + n)
    }
}

struct Pat {
    re: Regex,
    rep: &'static str,
}

fn patterns() -> &'static [Pat] {
    static P: OnceLock<Vec<Pat>> = OnceLock::new();
    P.get_or_init(|| {
        let p = |re: &str, rep: &'static str| Pat { re: Regex::new(re).unwrap(), rep };
        vec![
            p(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?(?:-----END [A-Z0-9 ]*PRIVATE KEY-----|\z)", "***PRIVATE KEY***"),
            p(r"PuTTY-User-Key-File-\d[^\n]*(?:\n[^\n]*){0,40}?Private-MAC:[^\n]*", "***PRIVATE KEY***"),
            p(r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}", "***"),
            p(r"(?i)(authorization\s*:\s*)(?:(?:bearer|basic|token|digest)\s+)?[^\s]+", "${1}***"),
            p(r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/=-]{8,}", "${1}***"),
            p(r"([A-Za-z][A-Za-z0-9+.-]*://[^:/\s@]+:)[^@\s/]+@", "${1}***@"),
            p(
                r#"(?i)\b([a-z0-9_.-]*(?:password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key)[a-z0-9_.-]*["']?\s*[=:]\s*)("[^"\n]*"|'[^'\n]*'|[^\s,;&"']+)"#,
                "${1}***",
            ),
            p(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}", "***"),
            p(r"\bgithub_pat_[A-Za-z0-9_]{20,}", "***"),
            p(r"\bsk-[A-Za-z0-9_-]{20,}", "***"),
            p(r"\bxox[abprs]-[A-Za-z0-9-]{10,}", "***"),
            p(r"\bAKIA[0-9A-Z]{16}\b", "***"),
            p(r"\bglpat-[A-Za-z0-9_-]{20,}", "***"),
            p(r"\bAIza[0-9A-Za-z_-]{35}", "***"),
        ]
    })
}

pub fn mask_patterns(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut hits = 0;
    for p in patterns() {
        let n = p.re.find_iter(&out).count();
        if n > 0 {
            hits += n;
            out = p.re.replace_all(&out, p.rep).into_owned();
        }
    }
    (out, hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Credential, CredentialKind};

    fn cred(secret: &str) -> Credential {
        Credential {
            id: "x".into(),
            kind: CredentialKind::Password,
            secret: secret.into(),
            passphrase: String::new(),
            certificate: String::new(),
            note: String::new(),
        }
    }

    #[test]
    fn masks_vault_secrets() {
        let mut v = VaultData::default();
        v.credentials.push(cred("hunter2-very-secret"));
        let m = Masker::from_vault(&v);
        let (out, n) = m.mask("DB_PASS line: hunter2-very-secret\nother");
        assert_eq!(out, "DB_PASS line: ***\nother");
        assert_eq!(n, 1);
        let b64 = B64.encode("hunter2-very-secret");
        assert!(!m.mask(&b64).0.contains(&b64));
    }

    #[test]
    fn masks_patterns() {
        for (input, leak) in [
            ("password=abc123xyz", "abc123xyz"),
            ("DB_PASSWORD: 'p@ss w0rd'", "p@ss w0rd"),
            ("Authorization: Bearer abcdefghijklmnop", "abcdefghijklmnop"),
            ("postgres://app:s3cr3tpass@db:5432/x", "s3cr3tpass"),
            ("token eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abcdefghijk", "eyJzdWIiOiIxMjM0In0"),
            ("-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----", "b3BlbnNzaC1rZXk"),
            ("key ghp_abcdefghijklmnopqrstuvwxyz0123", "ghp_abcdefghijklmnopqrstuvwxyz0123"),
        ] {
            let (out, n) = mask_patterns(input);
            assert!(!out.contains(leak), "leaked {leak} in {out}");
            assert!(n > 0);
        }
        assert_eq!(mask_patterns("total 48\ndrwxr-xr-x 2 root").0, "total 48\ndrwxr-xr-x 2 root");
    }
}
