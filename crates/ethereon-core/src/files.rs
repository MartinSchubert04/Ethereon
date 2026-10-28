//! Archivos para la persona: SFTP sobre una conexion del pool y el disco de esta PC.
//! El agente no usa nada de aca (no pasa por las reglas); la interfaz deja cada accion
//! en la auditoria.
//!
//! Las transferencias recorren carpetas con una pila (sin recursion async), avisan el
//! progreso por un callback y se pueden cancelar con un flag.

use russh_sftp::client::SftpSession;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::ssh::Conn;
use crate::{Error, Result};

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub dir: bool,
    pub link: bool,
    pub size: u64,
    /// Segundos desde 1970.
    pub mtime: Option<u64>,
    /// Permisos Unix (solo remotos).
    pub mode: Option<u32>,
}

const CHUNK: usize = 128 * 1024;

fn sftp_err(e: impl std::fmt::Display) -> Error {
    let s = e.to_string();
    // El protocolo devuelve un "Failure" generico para casi todo lo que el servidor niega.
    if s.contains("Failure: Failure") || s.ends_with(": Failure") {
        return Error::Invalid("the server refused it: the name may already exist, or you may not have permission".into());
    }
    // "No such file: No such file" -> "No such file"
    let s = match s.split_once(": ") {
        Some((a, b)) if a == b => a.to_string(),
        _ => s,
    };
    Error::Invalid(format!("sftp: {}", s.to_lowercase()))
}

fn io_err(e: std::io::Error) -> Error {
    Error::Invalid(e.to_string())
}

/// Une rutas remotas (siempre con `/`).
pub fn rjoin(base: &str, name: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// El ultimo tramo de una ruta remota.
pub fn rname(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

pub struct Sftp {
    conn: Arc<Conn>,
    s: SftpSession,
}

impl Sftp {
    pub async fn open(conn: Arc<Conn>) -> Result<Self> {
        conn.touch();
        let ch = conn.target().channel_open_session().await.map_err(sftp_err)?;
        ch.request_subsystem(true, "sftp").await.map_err(sftp_err)?;
        let s = SftpSession::new(ch.into_stream()).await.map_err(|e| {
            sftp_err(format!("{e}. Is the SFTP subsystem enabled on the server?"))
        })?;
        Ok(Self { conn, s })
    }

    pub fn is_closed(&self) -> bool {
        self.conn.is_closed()
    }

    pub async fn home(&self) -> Result<String> {
        self.conn.touch();
        self.s.canonicalize(".").await.map_err(sftp_err)
    }

    pub async fn canonical(&self, path: &str) -> Result<String> {
        self.conn.touch();
        self.s.canonicalize(path).await.map_err(sftp_err)
    }

    pub async fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        self.conn.touch();
        let mut out = Vec::new();
        for e in self.s.read_dir(path).await.map_err(sftp_err)? {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let ft = e.file_type();
            let m = e.metadata();
            let link = ft.is_symlink();
            // Un enlace a una carpeta se abre como carpeta.
            let dir = ft.is_dir() || (link && self.s.metadata(rjoin(path, &name)).await.map(|m| m.is_dir()).unwrap_or(false));
            out.push(FileEntry { name, dir, link, size: m.size.unwrap_or(0), mtime: m.mtime.map(u64::from), mode: m.permissions });
        }
        Ok(out)
    }

    pub async fn mkdir(&self, path: &str) -> Result<()> {
        self.conn.touch();
        self.s.create_dir(path).await.map_err(sftp_err)
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.conn.touch();
        self.s.rename(from, to).await.map_err(sftp_err)
    }

    /// Borra un archivo o una carpeta con todo lo que tiene.
    pub async fn remove(&self, path: &str) -> Result<()> {
        self.conn.touch();
        let meta = self.s.symlink_metadata(path).await.map_err(sftp_err)?;
        if !meta.is_dir() {
            return self.s.remove_file(path).await.map_err(sftp_err);
        }
        // Primero todo el arbol, despues las carpetas de la mas honda a la mas alta.
        let mut dirs = vec![path.to_string()];
        let mut stack = vec![path.to_string()];
        while let Some(d) = stack.pop() {
            for e in self.s.read_dir(&d).await.map_err(sftp_err)? {
                let name = e.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let p = rjoin(&d, &name);
                if e.file_type().is_dir() {
                    dirs.push(p.clone());
                    stack.push(p);
                } else {
                    self.s.remove_file(&p).await.map_err(sftp_err)?;
                }
            }
            self.conn.touch();
        }
        for d in dirs.iter().rev() {
            self.s.remove_dir(d).await.map_err(sftp_err)?;
        }
        Ok(())
    }

    /// Cuantos bytes hay en una ruta remota (para la barra de progreso).
    pub async fn size_of(&self, path: &str) -> Result<u64> {
        let meta = self.s.metadata(path).await.map_err(sftp_err)?;
        if !meta.is_dir() {
            return Ok(meta.size.unwrap_or(0));
        }
        let mut total = 0;
        let mut stack = vec![path.to_string()];
        while let Some(d) = stack.pop() {
            for e in self.s.read_dir(&d).await.map_err(sftp_err)? {
                let name = e.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                if e.file_type().is_dir() {
                    stack.push(rjoin(&d, &name));
                } else {
                    total += e.metadata().size.unwrap_or(0);
                }
            }
        }
        Ok(total)
    }

    /// Sube un archivo o carpeta local adentro de la carpeta remota `into`.
    pub async fn upload(&self, local: &Path, into: &str, progress: &(dyn Fn(u64, &str) + Sync), cancel: &AtomicBool) -> Result<()> {
        let name = local.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Invalid("bad file name".into()))?;
        let mut stack = vec![(local.to_path_buf(), rjoin(into, name))];
        while let Some((l, r)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Invalid("cancelled".into()));
            }
            self.conn.touch();
            if l.is_dir() {
                if !self.s.try_exists(&r).await.unwrap_or(false) {
                    self.s.create_dir(&r).await.map_err(sftp_err)?;
                }
                for e in std::fs::read_dir(&l).map_err(io_err)? {
                    let e = e.map_err(io_err)?;
                    let n = e.file_name().to_string_lossy().to_string();
                    stack.push((e.path(), rjoin(&r, &n)));
                }
            } else {
                let mut src = tokio::fs::File::open(&l).await.map_err(io_err)?;
                let mut dst = self.s.create(&r).await.map_err(sftp_err)?;
                let mut buf = vec![0u8; CHUNK];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        drop(dst);
                        let _ = self.s.remove_file(&r).await; // no dejar un archivo a medias
                        return Err(Error::Invalid("cancelled".into()));
                    }
                    let n = src.read(&mut buf).await.map_err(io_err)?;
                    if n == 0 {
                        break;
                    }
                    dst.write_all(&buf[..n]).await.map_err(sftp_err)?;
                    progress(n as u64, rname(&r));
                }
                dst.shutdown().await.map_err(sftp_err)?;
            }
        }
        Ok(())
    }

    /// Baja un archivo o carpeta remota adentro de la carpeta local `into`.
    pub async fn download(&self, remote: &str, into: &Path, progress: &(dyn Fn(u64, &str) + Sync), cancel: &AtomicBool) -> Result<()> {
        let mut stack = vec![(remote.to_string(), into.join(rname(remote)))];
        while let Some((r, l)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Invalid("cancelled".into()));
            }
            self.conn.touch();
            let meta = self.s.metadata(&r).await.map_err(sftp_err)?;
            if meta.is_dir() {
                std::fs::create_dir_all(&l).map_err(io_err)?;
                for e in self.s.read_dir(&r).await.map_err(sftp_err)? {
                    let n = e.file_name();
                    if n == "." || n == ".." {
                        continue;
                    }
                    stack.push((rjoin(&r, &n), l.join(&n)));
                }
            } else {
                let mut src = self.s.open(&r).await.map_err(sftp_err)?;
                let mut dst = tokio::fs::File::create(&l).await.map_err(io_err)?;
                let mut buf = vec![0u8; CHUNK];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        drop(dst);
                        let _ = std::fs::remove_file(&l);
                        return Err(Error::Invalid("cancelled".into()));
                    }
                    let n = src.read(&mut buf).await.map_err(sftp_err)?;
                    if n == 0 {
                        break;
                    }
                    dst.write_all(&buf[..n]).await.map_err(io_err)?;
                    progress(n as u64, rname(&r));
                }
                dst.flush().await.map_err(io_err)?;
            }
        }
        Ok(())
    }
}

// ---- disco local

fn mtime_of(m: &std::fs::Metadata) -> Option<u64> {
    m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// Lista una carpeta local. Con la ruta vacia, en Windows lista las unidades.
pub fn list_local(path: &str) -> Result<Vec<FileEntry>> {
    if path.is_empty() {
        return Ok(drives());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(path).map_err(io_err)? {
        let Ok(e) = e else { continue };
        let Ok(m) = e.metadata() else { continue }; // sigue los enlaces
        let link = e.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        out.push(FileEntry {
            name: e.file_name().to_string_lossy().to_string(),
            dir: m.is_dir(),
            link,
            size: if m.is_dir() { 0 } else { m.len() },
            mtime: mtime_of(&m),
            mode: None,
        });
    }
    Ok(out)
}

fn drives() -> Vec<FileEntry> {
    #[cfg(windows)]
    {
        (b'A'..=b'Z')
            .map(|c| format!("{}:\\", c as char))
            .filter(|d| Path::new(d).exists())
            .map(|name| FileEntry { name, dir: true, link: false, size: 0, mtime: None, mode: None })
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![FileEntry { name: "/".into(), dir: true, link: false, size: 0, mtime: None, mode: None }]
    }
}

pub fn size_local(path: &Path) -> u64 {
    let Ok(m) = std::fs::metadata(path) else { return 0 };
    if !m.is_dir() {
        return m.len();
    }
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            match e.metadata() {
                Ok(m) if m.is_dir() => stack.push(e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

pub fn remove_local(path: &Path) -> Result<()> {
    let m = std::fs::symlink_metadata(path).map_err(io_err)?;
    if m.is_dir() {
        std::fs::remove_dir_all(path).map_err(io_err)
    } else {
        std::fs::remove_file(path).map_err(io_err)
    }
}

pub fn home_local() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths() {
        assert_eq!(rjoin("/home/a", "b"), "/home/a/b");
        assert_eq!(rjoin("/", "etc"), "/etc");
        assert_eq!(rname("/var/log/"), "log");
        assert_eq!(rname("file.txt"), "file.txt");
    }

    #[test]
    fn local_listing_and_size() {
        let dir = std::env::temp_dir().join(format!("eth-files-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.join("sub/b.bin"), [0u8; 10]).unwrap();
        let l = list_local(dir.to_str().unwrap()).unwrap();
        assert!(l.iter().any(|e| e.name == "sub" && e.dir));
        assert!(l.iter().any(|e| e.name == "a.txt" && e.size == 5));
        assert_eq!(size_local(&dir), 15);
        remove_local(&dir).unwrap();
        assert!(!dir.exists());
    }
}
