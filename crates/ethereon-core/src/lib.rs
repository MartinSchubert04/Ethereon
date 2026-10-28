//! Ethereon: el broker que guarda las credenciales y abre las conexiones SSH.
//! El agente nunca ve un secreto: pide `run(host, comando)` y recibe la salida.
//!
//! - `vault`: archivo cifrado (AES-256-GCM + Argon2id).
//! - `model`: lo que hay adentro (credenciales, hosts, reglas, dispositivos, ajustes).
//! - `shell` + `policy`: analisis de comandos y reglas.
//! - `mask`: enmascarado de la salida.
//! - `audit`: registro encadenado.
//! - `ssh`: conexiones, saltos y huellas.
//! - `broker`: une todo; lo usan la interfaz y el servidor MCP.
//! - `mcp`: el servidor MCP por HTTP para los agentes.
//! - `import`: ~/.ssh/config y archivos de clave.
//! - `files`: SFTP y disco local para la persona (no para el agente).

pub mod audit;
pub mod broker;
pub mod export;
pub mod files;
pub mod import;
pub mod mask;
pub mod mcp;
pub mod model;
pub mod policy;
pub mod shell;
pub mod ssh;
pub mod vault;

pub use broker::{Broker, Notifier};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the vault is locked")]
    Locked,
    #[error("wrong master password")]
    WrongPassword,
    #[error("the vault file is damaged or was modified")]
    Corrupt,
    #[error("{0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Ssh(#[from] ssh::SshError),
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
