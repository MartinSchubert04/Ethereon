//! El explorador de archivos de la persona por el protocolo que tenga el host: SFTP (el de
//! siempre, `files.rs`), SCP o FTP / FTPS. Las tres tienen las mismas operaciones, asi la
//! interfaz no sabe por donde va cada archivo. Nada de aca pasa por las reglas ni lo usa el
//! agente (que solo tiene SFTP, en `transfer.rs`).
//!
//! - **SCP** va por la misma conexion SSH (con sus bastiones). Para listar, crear, renombrar
//!   y borrar corre `ls`, `mkdir`, `mv` y `rm` (un servidor Unix); las transferencias usan
//!   el protocolo de `scp -t` / `scp -f`, que tambien entienden equipos de red sin SFTP.
//! - **FTP / FTPS** (explicito, `AUTH TLS`) con `suppaftp`. El certificado de FTPS se
//!   verifica contra los de Windows. Cada transferencia abre su propia conexion: FTP no deja
//!   listar mientras otra transferencia usa la sesion.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use zeroize::Zeroizing;

use crate::files::{local_name, rjoin, rname, FileEntry, Sftp};
use crate::ssh::{self, Conn};
use crate::{Error, Result};

const CHUNK: usize = 128 * 1024;

type Progress<'a> = &'a (dyn Fn(u64, &str) + Sync);

fn io_err(e: std::io::Error) -> Error {
    Error::Invalid(e.to_string())
}

fn cancelled() -> Error {
    Error::Invalid("cancelled".into())
}

pub enum Remote {
    Sftp(Sftp),
    Scp(Scp),
    Ftp(Ftp),
}

impl Remote {
    /// "SFTP", "SCP", "FTP" o "FTPS", para la interfaz y la auditoria.
    pub fn proto(&self) -> &'static str {
        match self {
            Remote::Sftp(_) => "SFTP",
            Remote::Scp(_) => "SCP",
            Remote::Ftp(f) if f.p.tls => "FTPS",
            Remote::Ftp(_) => "FTP",
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Remote::Sftp(s) => s.is_closed(),
            Remote::Scp(s) => s.conn.is_closed(),
            // FTP se reconecta solo.
            Remote::Ftp(_) => false,
        }
    }

    pub async fn home(&self) -> Result<String> {
        match self {
            Remote::Sftp(s) => s.home().await,
            Remote::Scp(s) => s.home().await,
            Remote::Ftp(f) => f.home().await,
        }
    }

    pub async fn canonical(&self, path: &str) -> Result<String> {
        match self {
            Remote::Sftp(s) => s.canonical(path).await,
            Remote::Scp(s) => s.canonical(path).await,
            Remote::Ftp(f) => f.canonical(path).await,
        }
    }

    pub async fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        match self {
            Remote::Sftp(s) => s.list(path).await,
            Remote::Scp(s) => s.list(path).await,
            Remote::Ftp(f) => f.list(path).await,
        }
    }

    pub async fn mkdir(&self, path: &str) -> Result<()> {
        match self {
            Remote::Sftp(s) => s.mkdir(path).await,
            Remote::Scp(s) => s.sh(&format!("mkdir -- {}", q(path))).await.map(drop),
            Remote::Ftp(f) => f.mkdir(path).await,
        }
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        match self {
            Remote::Sftp(s) => s.rename(from, to).await,
            Remote::Scp(s) => s.sh(&format!("mv -- {} {}", q(from), q(to))).await.map(drop),
            Remote::Ftp(f) => f.rename(from, to).await,
        }
    }

    pub async fn remove(&self, path: &str) -> Result<()> {
        match self {
            Remote::Sftp(s) => s.remove(path).await,
            Remote::Scp(s) => s.sh(&format!("rm -rf -- {}", q(path))).await.map(drop),
            Remote::Ftp(f) => f.remove(path).await,
        }
    }

    pub async fn size_of(&self, path: &str) -> Result<u64> {
        match self {
            Remote::Sftp(s) => s.size_of(path).await,
            Remote::Scp(s) => s.size_of(path).await,
            Remote::Ftp(f) => f.size_of(path).await,
        }
    }

    /// Sube un archivo o carpeta local adentro de la carpeta remota `into`.
    pub async fn upload(&self, local: &Path, into: &str, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        match self {
            Remote::Sftp(s) => s.upload(local, into, progress, cancel).await,
            Remote::Scp(s) => s.upload(local, into, progress, cancel).await,
            Remote::Ftp(f) => f.upload(local, into, progress, cancel).await,
        }
    }

    /// Baja un archivo o carpeta remota adentro de la carpeta local `into`.
    pub async fn download(&self, remote: &str, into: &Path, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        match self {
            Remote::Sftp(s) => s.download(remote, into, progress, cancel).await,
            Remote::Scp(s) => s.download(remote, into, progress, cancel).await,
            Remote::Ftp(f) => f.download(remote, into, progress, cancel).await,
        }
    }

    /// De un servidor a otro. Entre dos SFTP va directo; si no, pasa por una carpeta
    /// temporal de esta PC (el progreso cuenta la bajada y la subida a la mitad cada una).
    pub async fn copy_to(&self, other: &Remote, remote: &str, into: &str, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        if let (Remote::Sftp(a), Remote::Sftp(b)) = (self, other) {
            return a.copy_to(b, remote, into, progress, cancel).await;
        }
        let tmp = std::env::temp_dir().join(format!("ethereon-copy-{}-{}", std::process::id(), millis()));
        std::fs::create_dir_all(&tmp).map_err(io_err)?;
        let half = |n: u64, f: &str| progress(n / 2, f);
        let r = async {
            self.download(remote, &tmp, &half, cancel).await?;
            other.upload(&tmp.join(local_name(rname(remote))?), into, &half, cancel).await
        }
        .await;
        let _ = std::fs::remove_dir_all(&tmp);
        r
    }
}

fn millis() -> u128 {
    std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

/// Una ruta entre comillas simples para `sh`.
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

// ---------------------------------------------------------------------------------- SCP

pub struct Scp {
    conn: Arc<Conn>,
}

/// Los pasos de una subida por SCP, en orden (el protocolo entra y sale de carpetas).
enum Step {
    Enter(String),
    File(PathBuf, String, u64),
    Leave,
}

impl Scp {
    pub fn new(conn: Arc<Conn>) -> Self {
        Scp { conn }
    }

    /// Corre un comando y devuelve la salida; un codigo distinto de 0 es un error con lo
    /// que dijo por stderr.
    async fn sh(&self, cmd: &str) -> Result<String> {
        self.conn.touch();
        let r = ssh::exec(&self.conn, cmd, None, Duration::from_secs(60), 8 << 20).await.map_err(|e| Error::Invalid(format!("ssh: {e}")))?;
        if r.exit_code.unwrap_or(1) != 0 {
            let err = String::from_utf8_lossy(&r.stderr).trim().to_string();
            let first = cmd.split_whitespace().next().unwrap_or("");
            return Err(Error::Invalid(if err.is_empty() { format!("`{first}` failed on the server") } else { err }));
        }
        Ok(String::from_utf8_lossy(&r.stdout).to_string())
    }

    async fn home(&self) -> Result<String> {
        Ok(self.sh("pwd").await?.trim().to_string())
    }

    async fn canonical(&self, path: &str) -> Result<String> {
        let p = q(path);
        Ok(self.sh(&format!("cd -- {p} 2>/dev/null && pwd || readlink -f -- {p}")).await?.trim().to_string())
    }

    async fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        // -L sigue los enlaces (un enlace a carpeta se abre como carpeta); -n sin nombres de
        // usuario (no hacen falta y no tienen espacios).
        let out = self.sh(&format!("LC_ALL=C ls -lanL -- {}", q(path))).await?;
        Ok(out.lines().filter_map(parse_ls).filter(|e| e.name != "." && e.name != "..").collect())
    }

    async fn size_of(&self, path: &str) -> Result<u64> {
        let out = self.sh(&format!("du -sk -- {}", q(path))).await?;
        Ok(out.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0) * 1024)
    }

    /// Un canal con `scp` corriendo del otro lado.
    async fn channel(&self, cmd: &str) -> Result<russh::ChannelStream<russh::client::Msg>> {
        self.conn.touch();
        let ch = self.conn.target().channel_open_session().await.map_err(|e| Error::Invalid(format!("ssh: {e}")))?;
        ch.exec(true, cmd).await.map_err(|e| Error::Invalid(format!("ssh: {e}")))?;
        Ok(ch.into_stream())
    }

    async fn upload(&self, local: &Path, into: &str, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        let steps = plan(local)?;
        let stream = self.channel(&format!("scp -r -t -- {}", q(into))).await?;
        let (r, mut w) = tokio::io::split(stream);
        let mut r = BufReader::new(r);
        ack(&mut r).await?;
        for step in steps {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            self.conn.touch();
            match step {
                Step::Enter(name) => {
                    w.write_all(format!("D0755 0 {name}\n").as_bytes()).await.map_err(io_err)?;
                    ack(&mut r).await?;
                }
                Step::Leave => {
                    w.write_all(b"E\n").await.map_err(io_err)?;
                    ack(&mut r).await?;
                }
                Step::File(path, name, size) => {
                    w.write_all(format!("C0644 {size} {name}\n").as_bytes()).await.map_err(io_err)?;
                    ack(&mut r).await?;
                    let mut f = tokio::fs::File::open(&path).await.map_err(io_err)?;
                    let mut buf = vec![0u8; CHUNK];
                    let mut left = size;
                    while left > 0 {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(cancelled());
                        }
                        let n = f.read(&mut buf[..CHUNK.min(left as usize)]).await.map_err(io_err)?;
                        if n == 0 {
                            return Err(Error::Invalid(format!("{name} changed while it was being sent")));
                        }
                        w.write_all(&buf[..n]).await.map_err(io_err)?;
                        left -= n as u64;
                        progress(n as u64, &name);
                    }
                    w.write_all(&[0]).await.map_err(io_err)?;
                    ack(&mut r).await?;
                }
            }
        }
        w.shutdown().await.map_err(io_err)?;
        Ok(())
    }

    async fn download(&self, remote: &str, into: &Path, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        let stream = self.channel(&format!("scp -r -f -- {}", q(remote))).await?;
        let (r, mut w) = tokio::io::split(stream);
        let mut r = BufReader::new(r);
        let mut dirs = vec![into.to_path_buf()];
        w.write_all(&[0]).await.map_err(io_err)?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            self.conn.touch();
            let mut line = Vec::new();
            let n = r.read_until(b'\n', &mut line).await.map_err(io_err)?;
            if n == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&line[1..]).trim_end().to_string();
            let here = dirs.last().unwrap().clone();
            match line[0] {
                b'C' => {
                    let (size, name) = header(&text)?;
                    let path = here.join(local_name(&name)?);
                    w.write_all(&[0]).await.map_err(io_err)?;
                    let mut f = tokio::fs::File::create(&path).await.map_err(io_err)?;
                    let mut left = size;
                    let mut buf = vec![0u8; CHUNK];
                    while left > 0 {
                        if cancel.load(Ordering::Relaxed) {
                            drop(f);
                            let _ = std::fs::remove_file(&path);
                            return Err(cancelled());
                        }
                        let k = r.read(&mut buf[..CHUNK.min(left as usize)]).await.map_err(io_err)?;
                        if k == 0 {
                            return Err(Error::Invalid("the connection closed in the middle of a file".into()));
                        }
                        f.write_all(&buf[..k]).await.map_err(io_err)?;
                        left -= k as u64;
                        progress(k as u64, &name);
                    }
                    f.flush().await.map_err(io_err)?;
                    ack(&mut r).await?;
                    w.write_all(&[0]).await.map_err(io_err)?;
                }
                b'D' => {
                    let (_, name) = header(&text)?;
                    let path = here.join(local_name(&name)?);
                    std::fs::create_dir_all(&path).map_err(io_err)?;
                    dirs.push(path);
                    w.write_all(&[0]).await.map_err(io_err)?;
                }
                b'E' => {
                    if dirs.len() > 1 {
                        dirs.pop();
                    }
                    w.write_all(&[0]).await.map_err(io_err)?;
                }
                b'T' => w.write_all(&[0]).await.map_err(io_err)?,
                1 | 2 => return Err(Error::Invalid(format!("scp: {text}"))),
                _ => return Err(Error::Invalid(format!("scp: unexpected answer `{}`", String::from_utf8_lossy(&line).trim()))),
            }
        }
        Ok(())
    }
}

/// Lo que hay que mandar para subir `local` (archivo o carpeta), en orden.
fn plan(local: &Path) -> Result<Vec<Step>> {
    fn walk(p: &Path, out: &mut Vec<Step>) -> Result<()> {
        let name = p.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Invalid("bad file name".into()))?.to_string();
        if name.contains('\n') {
            return Err(Error::Invalid(format!("SCP can't send a name with a line break: {name:?}")));
        }
        if p.is_dir() {
            out.push(Step::Enter(name));
            for e in std::fs::read_dir(p).map_err(io_err)? {
                walk(&e.map_err(io_err)?.path(), out)?;
            }
            out.push(Step::Leave);
        } else {
            let size = std::fs::metadata(p).map_err(io_err)?.len();
            out.push(Step::File(p.to_path_buf(), name, size));
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(local, &mut out)?;
    Ok(out)
}

/// La respuesta de `scp` a cada paso: 0 = bien, 1 o 2 = error con un mensaje.
async fn ack<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> Result<()> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b).await.map_err(|e| Error::Invalid(format!("scp: {e}. Is scp installed on the server?")))?;
    if b[0] == 0 {
        return Ok(());
    }
    let mut msg = String::new();
    let _ = r.read_line(&mut msg).await;
    Err(Error::Invalid(format!("scp: {}", msg.trim())))
}

/// `C0644 1234 nombre` (sin la letra) -> (1234, "nombre").
fn header(text: &str) -> Result<(u64, String)> {
    let mut it = text.splitn(3, ' ');
    let _mode = it.next();
    let size = it.next().and_then(|s| s.parse().ok());
    let name = it.next();
    match (size, name) {
        (Some(s), Some(n)) if !n.is_empty() && !n.contains('/') && n != ".." => Ok((s, n.to_string())),
        _ => Err(Error::Invalid(format!("scp: bad header `{text}`"))),
    }
}

/// Una linea de `ls -lanL`: `drwxr-xr-x  2 1000 1000  4096 Nov 19 10:30 nombre con espacios`.
fn parse_ls(line: &str) -> Option<FileEntry> {
    let mut rest = line;
    let mut fields = Vec::new();
    for _ in 0..8 {
        let t = rest.trim_start();
        let end = t.find(char::is_whitespace)?;
        fields.push(&t[..end]);
        rest = &t[end..];
    }
    let perms = fields[0];
    if perms.len() < 10 || !"-dlcbps".contains(&perms[..1]) {
        return None;
    }
    // El nombre es lo que queda despues de un solo espacio (puede empezar con espacios).
    let name = rest.strip_prefix(' ').unwrap_or(rest).to_string();
    if name.is_empty() {
        return None;
    }
    Some(FileEntry {
        name,
        dir: perms.starts_with('d'),
        link: false,
        size: fields[4].parse().unwrap_or(0),
        mtime: None,
        mode: Some(mode_of(perms)),
    })
}

/// `rwxr-xr-x` -> 0o755.
fn mode_of(perms: &str) -> u32 {
    perms.chars().skip(1).take(9).enumerate().fold(0, |m, (i, c)| if c == '-' { m } else { m | 1 << (8 - i) })
}

// ---------------------------------------------------------------------------------- FTP

type FtpStream = suppaftp::tokio::AsyncRustlsFtpStream;

/// Como llegar a un servidor FTP. Se guarda para reconectar.
#[derive(Clone)]
pub struct FtpParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Zeroizing<String>,
    pub tls: bool,
}

pub struct Ftp {
    p: FtpParams,
    s: tokio::sync::Mutex<(FtpStream, Instant)>,
}

fn ftp_err(e: suppaftp::FtpError) -> Error {
    let s = e.to_string();
    if s.contains("invalid peer certificate") || s.contains("UnknownIssuer") {
        return Error::Invalid(format!("ftps: the server's certificate is not trusted by Windows ({s})"));
    }
    Error::Invalid(format!("ftp: {s}"))
}

/// TLS con los certificados de Windows (como el navegador), con ring como en todo Ethereon.
fn tls_connector() -> Result<suppaftp::tokio::AsyncRustlsConnector> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls_platform_verifier::Verifier::new(provider.clone()).map_err(|e| Error::Invalid(format!("tls: {e}")))?;
    let cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Invalid(format!("tls: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(cfg)).into())
}

/// Despues de cuanto sin uso se prueba la conexion antes de usarla.
const FTP_IDLE: Duration = Duration::from_secs(45);

impl Ftp {
    pub async fn open(p: FtpParams) -> Result<Self> {
        let s = Self::connect(&p).await?;
        Ok(Ftp { p, s: tokio::sync::Mutex::new((s, Instant::now())) })
    }

    async fn connect(p: &FtpParams) -> Result<FtpStream> {
        let addr = (p.host.as_str(), p.port);
        let mut s = tokio::time::timeout(Duration::from_secs(15), FtpStream::connect(addr))
            .await
            .map_err(|_| Error::Invalid(format!("ftp: no answer from {}:{} after 15 s", p.host, p.port)))?
            .map_err(ftp_err)?;
        if p.tls {
            s = s.into_secure(tls_connector()?, &p.host).await.map_err(ftp_err)?;
        }
        // Detras de NAT el servidor anuncia su IP privada para el modo pasivo: se usa la del
        // control.
        s.set_passive_nat_workaround(true);
        s.login(p.user.as_str(), p.password.as_str()).await.map_err(ftp_err)?;
        s.transfer_type(suppaftp::types::FileType::Binary).await.map_err(ftp_err)?;
        Ok(s)
    }

    /// La sesion de control, reconectando si el servidor la corto por inactividad.
    async fn ctl(&self) -> Result<tokio::sync::MutexGuard<'_, (FtpStream, Instant)>> {
        let mut g = self.s.lock().await;
        if g.1.elapsed() > FTP_IDLE && g.0.noop().await.is_err() {
            g.0 = Self::connect(&self.p).await?;
        }
        g.1 = Instant::now();
        Ok(g)
    }

    async fn home(&self) -> Result<String> {
        self.ctl().await?.0.pwd().await.map_err(ftp_err)
    }

    async fn canonical(&self, path: &str) -> Result<String> {
        let mut g = self.ctl().await?;
        g.0.cwd(path).await.map_err(ftp_err)?;
        g.0.pwd().await.map_err(ftp_err)
    }

    async fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        let mut g = self.ctl().await?;
        // MLSD es el listado con formato fijo; los servidores viejos solo tienen LIST.
        let lines = match g.0.mlsd(Some(path)).await {
            Ok(l) => l,
            Err(_) => g.0.list(Some(path)).await.map_err(ftp_err)?,
        };
        Ok(lines
            .iter()
            .filter_map(|l| l.parse::<suppaftp::list::File>().ok())
            .filter(|f| f.name() != "." && f.name() != "..")
            .map(|f| FileEntry {
                name: f.name().to_string(),
                dir: f.is_directory(),
                link: f.is_symlink(),
                size: f.size() as u64,
                mtime: f.modified().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()).filter(|s| *s > 0),
                mode: None,
            })
            .collect())
    }

    async fn mkdir(&self, path: &str) -> Result<()> {
        self.ctl().await?.0.mkdir(path).await.map_err(ftp_err)
    }

    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.ctl().await?.0.rename(from, to).await.map_err(ftp_err)
    }

    /// Si una ruta es una carpeta (si se puede entrar).
    async fn is_dir(&self, path: &str) -> Result<bool> {
        Ok(self.ctl().await?.0.cwd(path).await.is_ok())
    }

    async fn remove(&self, path: &str) -> Result<()> {
        if !self.is_dir(path).await? {
            return self.ctl().await?.0.rm(path).await.map_err(ftp_err);
        }
        let mut dirs = vec![path.to_string()];
        let mut stack = vec![path.to_string()];
        while let Some(d) = stack.pop() {
            for e in self.list(&d).await? {
                let p = rjoin(&d, &e.name);
                if e.dir {
                    dirs.push(p.clone());
                    stack.push(p);
                } else {
                    self.ctl().await?.0.rm(&p).await.map_err(ftp_err)?;
                }
            }
        }
        for d in dirs.iter().rev() {
            self.ctl().await?.0.rmdir(d).await.map_err(ftp_err)?;
        }
        Ok(())
    }

    async fn size_of(&self, path: &str) -> Result<u64> {
        if !self.is_dir(path).await? {
            return Ok(self.ctl().await?.0.size(path).await.map(|n| n as u64).unwrap_or(0));
        }
        let mut total = 0;
        let mut stack = vec![path.to_string()];
        while let Some(d) = stack.pop() {
            for e in self.list(&d).await? {
                if e.dir {
                    stack.push(rjoin(&d, &e.name));
                } else {
                    total += e.size;
                }
            }
        }
        Ok(total)
    }

    async fn upload(&self, local: &Path, into: &str, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        // Una conexion propia: la de control queda libre para listar.
        let mut s = Self::connect(&self.p).await?;
        let name = local.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Invalid("bad file name".into()))?;
        let mut stack = vec![(local.to_path_buf(), rjoin(into, name))];
        while let Some((l, r)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            if l.is_dir() {
                if s.cwd(&r).await.is_err() {
                    s.mkdir(&r).await.map_err(ftp_err)?;
                }
                for e in std::fs::read_dir(&l).map_err(io_err)? {
                    let e = e.map_err(io_err)?;
                    stack.push((e.path(), rjoin(&r, &e.file_name().to_string_lossy())));
                }
            } else {
                let mut src = tokio::fs::File::open(&l).await.map_err(io_err)?;
                let mut dst = s.put_with_stream(&r).await.map_err(ftp_err)?;
                if let Err(e) = pump(&mut src, &mut dst, rname(&r), progress, cancel).await {
                    let _ = dst.finish().await;
                    let _ = s.rm(&r).await; // no dejar un archivo a medias
                    return Err(e);
                }
                dst.finish().await.map_err(ftp_err)?;
            }
        }
        let _ = s.quit().await;
        Ok(())
    }

    async fn download(&self, remote: &str, into: &Path, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
        let mut s = Self::connect(&self.p).await?;
        let mut stack = vec![(remote.to_string(), into.join(local_name(rname(remote))?))];
        while let Some((r, l)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            if s.cwd(&r).await.is_ok() {
                std::fs::create_dir_all(&l).map_err(io_err)?;
                for e in self.list(&r).await? {
                    stack.push((rjoin(&r, &e.name), l.join(local_name(&e.name)?)));
                }
            } else {
                let mut src = s.retr_as_stream(&r).await.map_err(ftp_err)?;
                let mut dst = tokio::fs::File::create(&l).await.map_err(io_err)?;
                let res = pump(&mut src, &mut dst, rname(&r), progress, cancel).await;
                let _ = src.finish().await;
                if let Err(e) = res {
                    drop(dst);
                    let _ = std::fs::remove_file(&l);
                    return Err(e);
                }
                dst.flush().await.map_err(io_err)?;
            }
        }
        let _ = s.quit().await;
        Ok(())
    }
}

/// Copia de un lado al otro de a pedazos, con progreso y cancelacion.
async fn pump<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(src: &mut R, dst: &mut W, name: &str, progress: Progress<'_>, cancel: &AtomicBool) -> Result<()> {
    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let n = src.read(&mut buf).await.map_err(io_err)?;
        if n == 0 {
            return Ok(());
        }
        dst.write_all(&buf[..n]).await.map_err(io_err)?;
        progress(n as u64, name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_lines() {
        let e = parse_ls("drwxr-xr-x  2 1000 1000  4096 Nov 19 10:30 my dir").unwrap();
        assert!(e.dir);
        assert_eq!(e.name, "my dir");
        assert_eq!(e.mode, Some(0o755));
        let f = parse_ls("-rw-r--r-- 1 0 0 1234 Jan  2  2025 notes.txt").unwrap();
        assert!(!f.dir);
        assert_eq!(f.size, 1234);
        assert_eq!(f.name, "notes.txt");
        assert!(parse_ls("total 12").is_none());
    }

    #[test]
    fn scp_header() {
        assert_eq!(header("0644 1234 file name.bin").unwrap(), (1234, "file name.bin".into()));
        assert!(header("0644 12 ../evil").is_err());
        assert!(header("0644 12 a/b").is_err());
    }

    #[test]
    fn quoting() {
        assert_eq!(q("it's"), r"'it'\''s'");
    }
}
