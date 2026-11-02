//! Archivos del agente por SFTP: las herramientas `download` y `upload` del MCP.
//!
//! Lo que decide (modo del host, aprobacion, frecuencia, auditoria) esta en
//! `Broker::agent_download` y `Broker::agent_upload`, igual que `run`. Aca estan los
//! chequeos que son propios de un archivo y las operaciones SFTP:
//!
//! - **Rutas remotas.** Lo que pide el agente se normaliza (`~`, relativas, `.`, `..`) y
//!   despues se resuelve en el servidor (`realpath`, que sigue los enlaces). Las reglas
//!   `path:` de la lista global y del host (`path:.ssh`, `path:/etc/shadow`) se aplican a
//!   las dos formas y tambien escritas con `~/`. Subir no escribe a traves de un enlace:
//!   escribe un temporal en la carpeta ya resuelta y lo renombra (eso tambien corta un
//!   enlace duro en vez de escribir a traves de el).
//! - **Bajar** devuelve el texto ya enmascarado en la respuesta y no escribe nada en el
//!   disco de esta PC: si escribiera, el agente leeria el archivo crudo y se saltearia
//!   `mask.rs`. Un binario no se puede enmascarar, asi que no se baja.
//! - **Subir** toma el texto del pedido o un archivo de esta PC (`LocalGuard`): nunca la
//!   carpeta de Ethereon (vault, auditoria), ni carpetas o archivos de credenciales
//!   (`~/.ssh`, `~/.aws`, claves, `.env`...), ni rutas de red (`\\servidor\...`: Windows
//!   mandaria las credenciales de la sesion). Y venga de donde venga, nada que tenga un
//!   secreto del vault o una clave privada sale (`scan_upload`).
//! - Los errores que vuelven al agente no llevan la ruta resuelta (puede tener el usuario,
//!   `/home/tester`) ni nada de la conexion.

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::mask::{self, Masker};
use crate::model::{Mode, Policy};
use crate::policy;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Download,
    Upload,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Download => "download",
            Direction::Upload => "upload",
        }
    }
}

/// Lo que se muestra en la cola de aprobacion de un pedido de archivo.
#[derive(Clone, Debug, Serialize)]
pub struct FileRequest {
    pub direction: Direction,
    /// La ruta remota, como la pidio el agente.
    pub path: String,
    /// Bytes que se bajan o se suben.
    pub size: u64,
    /// Subir desde un archivo de esta PC: cual (solo lo ve la persona).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    /// Subir sobre un archivo que ya existe.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub replaces: bool,
}

#[derive(Debug, Serialize)]
pub struct DownloadResult {
    /// La ruta como la pidio el agente (no la resuelta: puede llevar el usuario). No va el
    /// tamano original: con un archivo que es solo una contrasena, diria cuanto mide.
    pub path: String,
    /// El texto, con los secretos reemplazados por ***.
    pub content: String,
    /// Cuantos secretos se taparon.
    pub masked: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct UploadResult {
    pub path: String,
    pub size: u64,
    /// Ya existia y se reemplazo.
    pub replaced: bool,
    pub duration_ms: u64,
}

// ------------------------------------------------------------------ rutas remotas

/// Lo que pide el agente, como ruta absoluta y sin `.` ni `..` (sin resolver enlaces, eso
/// lo hace el servidor). `~` y las relativas son desde `home`.
pub fn remote_abs(path: &str, home: &str) -> std::result::Result<String, String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("the path is empty".into());
    }
    if p.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
        return Err("the path has control characters".into());
    }
    let full = if p == "~" {
        home.to_string()
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else if p.starts_with('~') {
        return Err("`~user` paths are not supported; write the full path".into());
    } else if p.starts_with('/') {
        p.to_string()
    } else {
        format!("{home}/{p}")
    };
    let mut out: Vec<&str> = Vec::new();
    for comp in full.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    Ok(format!("/{}", out.join("/")))
}

/// Las formas de una ruta contra las que se miran las reglas: tal cual y, si esta en el
/// home, tambien como `~/...` (para reglas como `path:~/.aws`).
pub fn rule_forms(abs: &str, home: &str) -> Vec<String> {
    let mut v = vec![abs.to_string()];
    let home = home.trim_end_matches('/');
    if !home.is_empty() {
        if abs == home {
            v.push("~".into());
        } else if let Some(rest) = abs.strip_prefix(&format!("{home}/")) {
            v.push(format!("~/{rest}"));
        }
    }
    v
}

/// La regla `path:` (global o del host) que toca alguna de las rutas, como motivo para el
/// agente. En modo Free no se mira, igual que en `run`.
pub fn remote_blocked(paths: &[String], pol: &Policy, global_deny: &[String]) -> Option<String> {
    if pol.mode == Mode::Free {
        return None;
    }
    paths
        .iter()
        .find_map(|p| policy::path_blocked(p, global_deny.iter().chain(pol.deny.iter())))
        .map(|r| format!("blocked by rule `{r}`"))
}

/// Carpeta y nombre de una ruta absoluta normalizada.
fn split_parent(abs: &str) -> Option<(&str, &str)> {
    let (dir, name) = abs.rsplit_once('/')?;
    if name.is_empty() {
        return None;
    }
    Some((if dir.is_empty() { "/" } else { dir }, name))
}

// ------------------------------------------------------------------ contenido

/// Texto: UTF-8 valido y sin bytes NUL. Lo demas es binario y no se puede enmascarar.
pub fn as_text(b: &[u8]) -> Option<&str> {
    if b.contains(&0) {
        return None;
    }
    std::str::from_utf8(b).ok()
}

/// Lo que se encontro en algo que el agente quiere subir.
#[derive(Debug, Default, PartialEq)]
pub struct Scan {
    /// No sale en ningun modo: tiene un secreto del vault o una clave privada.
    pub hard: Option<String>,
    /// Se le muestran a la persona en modo aprobacion.
    pub warnings: Vec<String>,
    /// Tiene `***`, que es como Ethereon tapa los secretos al bajar: subirlo encima del
    /// original reemplazaria los secretos de verdad por `***`.
    pub masked_marker: bool,
}

pub fn scan_upload(data: &[u8], m: &Masker) -> Scan {
    let mut s = Scan::default();
    let text = String::from_utf8_lossy(data);
    if m.secret_hits(data) > 0 {
        s.hard = Some("it contains a secret stored in Ethereon's vault".into());
    } else if mask::has_private_key(&text) {
        s.hard = Some("it contains a private key".into());
    }
    match as_text(data) {
        Some(t) => {
            let (_, n) = mask::mask_patterns(t);
            if n > 0 {
                s.warnings.push(format!("looks like it contains {n} secret{} (password, token or key)", if n == 1 { "" } else { "s" }));
            }
            if t.contains(mask::MASK) {
                s.masked_marker = true;
                s.warnings.push("contains ***, which is how Ethereon hides secrets: if it came from a download, the real secrets would be replaced".into());
            }
        }
        None => s.warnings.push("binary file: Ethereon cannot check what is inside".into()),
    }
    s
}

pub fn human_size(n: u64) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.1} KB", n as f64 / 1024.0),
        n => format!("{:.1} MB", n as f64 / (1024.0 * 1024.0)),
    }
}

// ------------------------------------------------------------------ archivos locales

/// Que archivos de esta PC puede subir el agente. La carpeta de datos de Ethereon y los
/// lugares con credenciales estan cerrados; las reglas `path:` globales tambien valen aca.
pub struct LocalGuard {
    blocked: Vec<PathBuf>,
    deny: Vec<String>,
}

/// Carpetas y archivos con credenciales, desde el home.
const HOME_BLOCKED: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".password-store",
    ".terraform.d",
    ".config/gcloud",
    ".config/gh",
    ".config/rclone",
    ".claude/.credentials.json",
    ".netrc",
    "_netrc",
    ".git-credentials",
    ".pgpass",
    ".npmrc",
    ".pypirc",
    ".vault-token",
];

/// En Windows: el almacen de credenciales, DPAPI, los perfiles de los navegadores y los
/// gestores de contrasenas (relativos a la variable de entorno).
const WIN_BLOCKED: &[(&str, &str)] = &[
    ("APPDATA", "Microsoft"),
    ("APPDATA", "Mozilla"),
    ("APPDATA", "Bitwarden"),
    ("APPDATA", "KeePassXC"),
    ("LOCALAPPDATA", "Microsoft/Credentials"),
    ("LOCALAPPDATA", "Microsoft/Vault"),
    ("LOCALAPPDATA", "Microsoft/Edge/User Data"),
    ("LOCALAPPDATA", "Google/Chrome/User Data"),
    ("LOCALAPPDATA", "BraveSoftware"),
    ("SystemRoot", "System32/config"),
];

/// Nombres de archivo que son claves o secretos, esten donde esten.
const NAMES_BLOCKED: &[&str] = &[
    "id_rsa", "id_dsa", "id_ecdsa", "id_ed25519", "id_ecdsa_sk", "id_ed25519_sk", "authorized_keys", ".env", ".netrc", "_netrc",
    ".git-credentials", ".pgpass",
];
const EXT_BLOCKED: &[&str] = &["pem", "key", "ppk", "pfx", "p12", "kdbx", "jks", "keystore", "gpg", "ovpn"];

/// En Windows las rutas no distinguen mayusculas.
fn fold(s: &str) -> String {
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_string()
    }
}

/// La ruta con `/`, sin el `\\?\` que agrega `canonicalize` en Windows.
fn slashed(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/UNC/").map(|r| format!("//{r}")).unwrap_or(s);
    s.strip_prefix("//?/").map(String::from).unwrap_or(s)
}

fn under(p: &Path, root: &Path) -> bool {
    let (p, r) = (fold(&slashed(p)), fold(&slashed(root)));
    Path::new(&p).starts_with(Path::new(&r))
}

impl LocalGuard {
    /// `data_dir`: la carpeta de Ethereon. `home`: la del usuario. `deny`: la lista global.
    pub fn new(data_dir: &Path, home: &Path, deny: &[String]) -> LocalGuard {
        let mut blocked = vec![data_dir.to_path_buf()];
        blocked.extend(HOME_BLOCKED.iter().map(|r| home.join(r)));
        for (var, rel) in WIN_BLOCKED {
            if let Some(base) = std::env::var_os(var) {
                blocked.push(PathBuf::from(base).join(rel));
            }
        }
        // Resueltas, para compararlas con la ruta resuelta (enlaces, junctions, mayusculas).
        let blocked = blocked.into_iter().map(|p| p.canonicalize().unwrap_or(p)).collect();
        LocalGuard { blocked, deny: deny.iter().map(|r| fold(r)).collect() }
    }

    /// El archivo que se puede subir, ya resuelto, o por que no.
    pub fn check(&self, path: &str) -> std::result::Result<PathBuf, String> {
        let p = path.trim();
        if p.is_empty() || p.contains('\0') {
            return Err("the local path is empty".into());
        }
        // Una ruta de red se abre antes de poder mirarla, y Windows le manda al servidor
        // las credenciales de la sesion: se rechaza sin tocarla.
        if p.starts_with("\\\\") || p.starts_with("//") {
            return Err("network and device paths are not allowed".into());
        }
        let absolute = if cfg!(windows) {
            let b = p.as_bytes();
            b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
        } else {
            p.starts_with('/')
        };
        if !absolute {
            return Err("the local path must be absolute".into());
        }
        let real = Path::new(p).canonicalize().map_err(|_| "the local file does not exist or cannot be read".to_string())?;
        let s = slashed(&real);
        if s.starts_with("//") {
            return Err("network and device paths are not allowed".into());
        }
        if self.blocked.iter().any(|b| under(&real, b)) {
            return Err("that local folder holds credentials or Ethereon's own data".into());
        }
        let name = fold(&real.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        if NAMES_BLOCKED.contains(&name.as_str()) || name.starts_with(".env.") || EXT_BLOCKED.contains(&ext) || name.ends_with("_history") {
            return Err("that local file looks like a key or a secrets file".into());
        }
        if let Some(r) = policy::path_blocked(&fold(&s), &self.deny) {
            return Err(format!("the local path is blocked by rule `{r}`"));
        }
        let meta = std::fs::metadata(&real).map_err(|_| "the local file cannot be read".to_string())?;
        if !meta.is_file() {
            return Err("the local path is not a regular file".into());
        }
        Ok(real)
    }

    /// `check`, y despues abre el archivo y confirma que lo abierto es lo que se miro: un
    /// archivo comun, en la misma ruta despues de abrirlo (si lo cambiaron por un enlace en
    /// el medio, la ruta resuelta ya no coincide) y, en Unix, con un solo nombre (un enlace
    /// duro a `~/.ssh/id_rsa` tendria una ruta inocente). Lee hasta `max + 1` bytes.
    pub fn read(&self, path: &str, max: usize) -> std::result::Result<(PathBuf, Vec<u8>), String> {
        use std::io::Read;
        let real = self.check(path)?;
        let f = std::fs::File::open(&real).map_err(|_| "the local file cannot be read".to_string())?;
        let meta = f.metadata().map_err(|_| "the local file cannot be read".to_string())?;
        if !meta.is_file() {
            return Err("the local path is not a regular file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.nlink() > 1 {
                return Err("the local file has more than one name (a hard link): copy it first".into());
            }
        }
        if Path::new(path.trim()).canonicalize().ok().as_ref() != Some(&real) {
            return Err("the local file changed while it was being checked".into());
        }
        let mut data = Vec::new();
        f.take(max as u64 + 1).read_to_end(&mut data).map_err(|_| "the local file cannot be read".to_string())?;
        Ok((real, data))
    }
}

// ------------------------------------------------------------------ SFTP

/// Un error de SFTP para el agente: solo el tipo, sin rutas ni texto del servidor.
pub fn sftp_error(e: SftpError) -> Error {
    Error::Invalid(match e {
        SftpError::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => "no such file or folder on the host".into(),
            StatusCode::PermissionDenied => "permission denied on the host".into(),
            StatusCode::OpUnsupported => "the host's SFTP server does not support that".into(),
            _ => "the host refused the file operation".into(),
        },
        SftpError::Timeout => "the SFTP request timed out".into(),
        _ => "the SFTP connection failed".into(),
    })
}

/// La carpeta de inicio del usuario en el host (`~`).
pub async fn home(s: &SftpSession) -> Result<String> {
    s.canonicalize(".").await.map_err(sftp_error)
}

/// Un archivo remoto listo para bajar.
#[derive(Debug, PartialEq)]
pub struct DownloadPlan {
    /// Resuelta por el servidor (enlaces incluidos).
    pub real: String,
    pub size: u64,
}

pub async fn plan_download(s: &SftpSession, abs: &str) -> Result<DownloadPlan> {
    let real = s.canonicalize(abs).await.map_err(sftp_error)?;
    let meta = s.metadata(&real).await.map_err(sftp_error)?;
    if meta.is_dir() {
        return Err(Error::Invalid("that is a folder: download one file at a time".into()));
    }
    if !meta.file_type().is_file() {
        return Err(Error::Invalid("that is not a regular file".into()));
    }
    Ok(DownloadPlan { real, size: meta.size.unwrap_or(0) })
}

/// Lee hasta `max + 1` bytes: si llega a mas de `max`, el archivo es muy grande (o crece,
/// como los de `/proc`, que dicen medir 0).
pub async fn read_capped(s: &SftpSession, path: &str, max: usize) -> Result<Vec<u8>> {
    let mut f = s.open(path).await.map_err(sftp_error)?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await.map_err(|_| Error::Invalid("could not read the file on the host".into()))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > max {
            break;
        }
    }
    Ok(out)
}

/// Donde se va a escribir un archivo subido.
#[derive(Debug, PartialEq)]
pub struct UploadPlan {
    /// La carpeta resuelta por el servidor mas el nombre.
    pub real: String,
    pub dir: String,
    pub name: String,
    pub exists: bool,
    /// Permisos del que se reemplaza, para dejarle los mismos al nuevo.
    pub permissions: Option<u32>,
}

pub async fn plan_upload(s: &SftpSession, abs: &str) -> Result<UploadPlan> {
    let (dir, name) = split_parent(abs).ok_or_else(|| Error::Invalid("the path must name a file".into()))?;
    let dir = s.canonicalize(dir).await.map_err(|e| match sftp_error(e) {
        Error::Invalid(m) if m.starts_with("no such") => Error::Invalid("the folder does not exist on the host".into()),
        other => other,
    })?;
    let real = crate::files::rjoin(&dir, name);
    let (exists, permissions) = match s.symlink_metadata(&real).await {
        Ok(m) if m.file_type().is_symlink() => {
            return Err(Error::Invalid("the target is a symbolic link: write to the real file instead".into()));
        }
        Ok(m) if m.is_dir() => return Err(Error::Invalid("the target is a folder: give the full path of the file".into())),
        Ok(m) if !m.file_type().is_file() => return Err(Error::Invalid("the target is not a regular file".into())),
        Ok(m) => (true, m.permissions),
        Err(SftpError::Status(st)) if st.status_code == StatusCode::NoSuchFile => (false, None),
        Err(e) => return Err(sftp_error(e)),
    };
    Ok(UploadPlan { real, dir, name: name.to_string(), exists, permissions })
}

/// Para subir: el home, la ruta pedida ya normalizada y donde quedaria. El error de
/// `plan_upload` va aparte para que el que llama mire primero las reglas: una ruta
/// prohibida se rechaza sin decir si existe.
pub async fn locate_upload(s: &SftpSession, path: &str) -> Result<(String, String, Result<UploadPlan>)> {
    let home = home(s).await?;
    let abs = remote_abs(path, &home).map_err(Error::Invalid)?;
    let plan = plan_upload(s, &abs).await;
    Ok((home, abs, plan))
}

/// Escribe en un temporal de la misma carpeta y lo pone en el lugar del destino. Nunca
/// se escribe a traves del archivo existente (ni de un enlace duro a otro lado).
pub async fn write_replace(s: &SftpSession, plan: &UploadPlan, data: &[u8]) -> Result<()> {
    let tmp = crate::files::rjoin(&plan.dir, &format!(".{}.ethereon-{}", plan.name, hex::encode(crate::vault::random::<4>())));
    let res: Result<()> = async {
        let mut f = s
            .open_with_flags(&tmp, OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE)
            .await
            .map_err(sftp_error)?;
        f.write_all(data).await.map_err(|_| Error::Invalid("could not write the file on the host".into()))?;
        f.shutdown().await.map_err(|_| Error::Invalid("could not write the file on the host".into()))?;
        drop(f);
        if let Some(p) = plan.permissions {
            let attrs = FileAttributes { permissions: Some(p & 0o7777), ..FileAttributes::empty() };
            let _ = s.set_metadata(&tmp, attrs).await;
        }
        if plan.exists {
            s.remove_file(&plan.real).await.map_err(sftp_error)?;
        }
        s.rename(&tmp, &plan.real).await.map_err(sftp_error)
    }
    .await;
    if res.is_err() {
        let _ = s.remove_file(&tmp).await;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Credential, CredentialKind, VaultData};

    #[test]
    fn remote_paths_normalize() {
        let h = "/home/tester";
        assert_eq!(remote_abs("notes.txt", h).unwrap(), "/home/tester/notes.txt");
        assert_eq!(remote_abs("~/a/../b", h).unwrap(), "/home/tester/b");
        assert_eq!(remote_abs("~", h).unwrap(), "/home/tester");
        assert_eq!(remote_abs("/etc/../etc/./shadow", h).unwrap(), "/etc/shadow");
        assert_eq!(remote_abs("/../../etc/passwd", h).unwrap(), "/etc/passwd");
        assert_eq!(remote_abs("tmp/../../../root/.ssh/id_rsa", h).unwrap(), "/root/.ssh/id_rsa");
        assert!(remote_abs("", h).is_err());
        assert!(remote_abs("~root/.ssh", h).is_err());
        assert!(remote_abs("a\nb", h).is_err());
        assert_eq!(rule_forms("/home/tester/.aws/config", h), vec!["/home/tester/.aws/config".to_string(), "~/.aws/config".into()]);
        assert_eq!(split_parent("/etc/x.conf"), Some(("/etc", "x.conf")));
        assert_eq!(split_parent("/x"), Some(("/", "x")));
        assert_eq!(split_parent("/"), None);
    }

    #[test]
    fn remote_rules_by_mode() {
        let deny = policy::default_denylist();
        let pol = |mode| Policy { mode, ..Default::default() };
        let check = |p: &str, mode| {
            let abs = remote_abs(p, "/home/tester").unwrap();
            remote_blocked(&rule_forms(&abs, "/home/tester"), &pol(mode), &deny)
        };
        for m in [Mode::ReadOnly, Mode::Approval, Mode::Guarded] {
            assert!(check("~/.ssh/authorized_keys", m).is_some());
            assert!(check("x/../.ssh/id_rsa", m).is_some(), "`..` cannot hide .ssh");
            assert!(check("/etc/shadow", m).is_some());
            assert!(check(".bash_history", m).is_some());
            assert!(check("/etc/nginx/nginx.conf", m).is_none());
        }
        // Free no mira la lista, como en `run`.
        assert!(check("~/.ssh/authorized_keys", Mode::Free).is_none());
        // La lista propia del host, tambien con `~/`.
        let mut p = pol(Mode::Guarded);
        p.deny = vec!["path:~/.aws".into(), "path:/srv/secrets".into()];
        let forms = |x: &str| rule_forms(&remote_abs(x, "/home/tester").unwrap(), "/home/tester");
        assert!(remote_blocked(&forms("/home/tester/.aws/credentials"), &p, &deny).is_some());
        assert!(remote_blocked(&forms("/srv/secrets/db.txt"), &p, &deny).is_some());
        assert!(remote_blocked(&forms("/srv/app/db.txt"), &p, &deny).is_none());
    }

    fn masker(secret: &str) -> Masker {
        let mut v = VaultData::default();
        v.credentials.push(Credential {
            id: "x".into(),
            kind: CredentialKind::Password,
            username: String::new(),
            secret: secret.into(),
            passphrase: String::new(),
            certificate: String::new(),
            note: String::new(),
        });
        Masker::from_vault(&v)
    }

    #[test]
    fn uploads_never_carry_vault_secrets() {
        let m = masker("Sup3r-S3cret-Value");
        assert!(scan_upload(b"db_pass Sup3r-S3cret-Value\n", &m).hard.is_some());
        assert!(scan_upload(b"\x00\x01Sup3r-S3cret-Value", &m).hard.is_some(), "also inside a binary");
        let key = b"-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbg==\n-----END OPENSSH PRIVATE KEY-----\n";
        assert!(scan_upload(key, &m).hard.is_some());
        let s = scan_upload(b"server { listen 80; }\n", &m);
        assert_eq!(s, Scan::default());
        let s = scan_upload(b"password=abc123xyz\n", &m);
        assert!(s.hard.is_none() && s.warnings.len() == 1);
        let s = scan_upload(b"token: ***\n", &m);
        assert!(s.masked_marker);
        let s = scan_upload(b"\x7fELF\x00\x00", &m);
        assert!(s.hard.is_none() && s.warnings[0].contains("binary"));
    }

    #[test]
    fn text_or_binary() {
        assert_eq!(as_text(b"hola\n"), Some("hola\n"));
        assert_eq!(as_text("año".as_bytes()), Some("año"));
        assert_eq!(as_text(b"a\x00b"), None);
        assert_eq!(as_text(b"\xff\xfe"), None);
        assert_eq!(human_size(10), "10 B");
        assert_eq!(human_size(2048), "2.0 KB");
    }

    #[test]
    fn local_sources_are_restricted() {
        let base = tempfile::tempdir().unwrap();
        let home = base.path().join("home");
        let data = base.path().join("ethereon-data");
        let proj = home.join("project");
        for d in [&home.join(".ssh"), &home.join(".aws"), &data, &proj, &home.join(".claude")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let file = |p: &Path, body: &str| {
            std::fs::write(p, body).unwrap();
            p.to_string_lossy().to_string()
        };
        let ok = file(&proj.join("app.conf"), "listen 80");
        let ssh = file(&home.join(".ssh/config"), "Host x");
        let aws = file(&home.join(".aws/credentials"), "[default]");
        let vault = file(&data.join("vault.json"), "{}");
        let creds = file(&home.join(".claude/.credentials.json"), "{}");
        let settings = file(&home.join(".claude/settings.json"), "{}");
        let pem = file(&proj.join("server.pem"), "x");
        let env = file(&proj.join(".env"), "A=1");
        let env2 = file(&proj.join(".env.production"), "A=1");
        let hist = file(&proj.join(".bash_history"), "ls");
        let g = LocalGuard::new(&data, &home, &policy::default_denylist());

        assert!(g.check(&ok).is_ok());
        assert!(g.check(&settings).is_ok(), "~/.claude is fine, only its credentials file is not");
        for p in [&ssh, &aws, &vault, &creds, &pem, &env, &env2, &hist] {
            assert!(g.check(p).is_err(), "should block {p}");
        }
        // `..` no sale de una carpeta permitida hacia una cerrada.
        let sneaky = format!("{}{}..{}.ssh{}config", proj.display(), std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
        assert!(g.check(&sneaky).is_err());
        // Relativas, de red, carpetas y lo que no existe.
        assert!(g.check("app.conf").is_err());
        assert!(g.check(r"\\attacker\share\x").is_err());
        assert!(g.check("//attacker/share/x").is_err());
        assert!(g.check(&proj.to_string_lossy()).is_err());
        assert!(g.check(&proj.join("missing.txt").to_string_lossy()).is_err());
        // Una carpeta de la lista no se esquiva con mayusculas (en Windows) ni con un enlace.
        #[cfg(windows)]
        assert!(g.check(&ssh.to_uppercase()).is_err());
        #[cfg(unix)]
        {
            let link = proj.join("innocent");
            std::os::unix::fs::symlink(home.join(".ssh/config"), &link).unwrap();
            assert!(g.check(&link.to_string_lossy()).is_err());
            let hard = proj.join("innocent-too");
            std::fs::hard_link(home.join(".ssh/config"), &hard).unwrap();
            assert!(g.read(&hard.to_string_lossy(), 1024).is_err(), "a hard link has an innocent path");
        }
        // Leer: hasta el limite mas uno, para que el que llama vea que se paso.
        assert_eq!(g.read(&ok, 1024).unwrap().1, b"listen 80");
        assert_eq!(g.read(&ok, 3).unwrap().1.len(), 4);
        assert!(g.read(&ssh, 1024).is_err());
    }
}
