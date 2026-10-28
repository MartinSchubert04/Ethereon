//! Lo que se guarda dentro del vault. Todo vive cifrado en un solo archivo: hosts, reglas y
//! huellas tambien, para que un proceso de la misma PC (el agente, por ejemplo) no pueda
//! editarlos sin la contrasena maestra.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VaultData {
    #[serde(default)]
    pub credentials: Vec<Credential>,
    #[serde(default)]
    pub hosts: Vec<Host>,
    #[serde(default)]
    pub groups: Vec<Group>,
    /// Huellas confirmadas: "hostname:puerto" -> "SHA256:..."
    #[serde(default)]
    pub known_hosts: Vec<KnownHost>,
    #[serde(default)]
    pub devices: Vec<Device>,
    #[serde(default)]
    pub denylist: Vec<String>,
    /// Clave del HMAC que encadena la auditoria (para notar si alguien la edita).
    #[serde(default)]
    pub audit_key: String,
    #[serde(default)]
    pub settings: Settings,
}

/// Ajustes que afectan la seguridad: viven en el vault para que el agente no pueda
/// cambiarlos editando un archivo.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Interruptor general del acceso del agente.
    pub agent_enabled: bool,
    /// Minutos sin uso de la interfaz hasta bloquear el vault (0 = nunca).
    pub autolock_minutes: u64,
    /// Cuanto espera un comando a que lo apruebes antes de rechazarse solo.
    pub approval_timeout_secs: u64,
    /// Guardar la salida (enmascarada) en la auditoria, ademas del hash.
    pub audit_output: bool,
    /// Minutos que una conexion SSH sin uso queda abierta para el proximo comando.
    pub keep_alive_minutes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { agent_enabled: true, autolock_minutes: 30, approval_timeout_secs: 300, audit_output: false, keep_alive_minutes: 5 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Credential {
    /// El ID que ven los hosts y el agente, por ejemplo `bastion-key`.
    pub id: String,
    #[zeroize(skip)]
    pub kind: CredentialKind,
    /// Contrasena, clave privada (PEM/OpenSSH), contrasena de sudo o token.
    pub secret: String,
    /// Passphrase de la clave privada, si tiene.
    #[serde(default)]
    pub passphrase: String,
    /// Certificado SSH (`ssh-ed25519-cert-v01@openssh.com ...`), si la clave lo usa.
    #[serde(default)]
    pub certificate: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    Password,
    PrivateKey,
    Certificate,
    SudoPassword,
    ApiToken,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Env {
    Prod,
    Test,
    #[default]
    Dev,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Host {
    /// Nombre unico; es lo que usa el agente en `run(host, ...)`.
    pub name: String,
    pub hostname: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    /// ID de la credencial de login.
    pub credential: String,
    /// ID de la contrasena de sudo (opcional).
    #[serde(default)]
    pub sudo_credential: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub env: Env,
    /// Saltos en orden: el primero es al que se conecta la PC.
    #[serde(default)]
    pub jump: Vec<String>,
    /// Contexto para el agente: para que es, que corre.
    #[serde(default)]
    pub description: String,
    /// Reglas propias; si es None usa las del grupo.
    #[serde(default)]
    pub policy: Option<Policy>,
}

fn default_port() -> u16 {
    22
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Solo lo que esta en `allow`.
    ReadOnly,
    /// Cada comando espera aprobacion en la interfaz.
    #[default]
    Approval,
    /// Todo menos lo prohibido.
    Free,
    /// El agente no puede usar este host.
    Off,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub mode: Mode,
    /// Comandos permitidos en modo solo lectura (vacio = la lista por defecto).
    #[serde(default)]
    pub allow: Vec<String>,
    /// Prohibidos ademas de la lista global.
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default = "d_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "d_output")]
    pub max_output_kb: usize,
    #[serde(default = "d_rate")]
    pub max_per_minute: u32,
    #[serde(default = "d_true")]
    pub allow_sudo: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            mode: Mode::Approval,
            allow: vec![],
            deny: vec![],
            timeout_secs: d_timeout(),
            max_output_kb: d_output(),
            max_per_minute: d_rate(),
            allow_sudo: true,
        }
    }
}

fn d_timeout() -> u64 {
    60
}
fn d_output() -> usize {
    256
}
fn d_rate() -> u32 {
    30
}
fn d_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnownHost {
    /// "hostname:puerto"
    pub addr: String,
    pub fingerprint: String,
    pub algorithm: String,
}

/// Una PC (o un agente) con acceso. Se guarda solo el hash del token.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub created: String,
    #[serde(default)]
    pub expires: Option<String>,
    /// Vacio = todos los hosts.
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub revoked: bool,
}

/// Lo que ve el agente de un host: nunca IPs, usuarios ni credenciales.
#[derive(Clone, Debug, Serialize)]
pub struct AgentHostView {
    pub name: String,
    pub group: Option<String>,
    pub env: Env,
    pub tags: Vec<String>,
    pub description: String,
    pub mode: Mode,
    pub sudo: bool,
}
