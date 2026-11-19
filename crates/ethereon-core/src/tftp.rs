//! Servidor TFTP (RFC 1350) para equipos de red: un switch o un AP baja su firmware o su
//! configuracion con `copy tftp: flash:` desde una carpeta de esta PC, y puede subir la
//! suya si se permite escribir. Con las opciones de RFC 2347-2349 (`blksize`, `tsize`,
//! `timeout`), que hacen las transferencias mucho mas rapidas en los equipos que las piden.
//!
//! Lo prende y lo apaga la persona desde la interfaz; no pasa por el vault ni por el agente.
//! Solo sirve archivos de adentro de la carpeta elegida: rutas con `..`, absolutas o con
//! letra de unidad se rechazan.

use serde::Serialize;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;

const RRQ: u16 = 1;
const WRQ: u16 = 2;
const DATA: u16 = 3;
const ACK: u16 = 4;
const ERROR: u16 = 5;
const OACK: u16 = 6;

const DEFAULT_BLOCK: usize = 512;
const MAX_BLOCK: usize = 65464;
const RETRIES: u32 = 5;

/// Lo que pasa en el servidor, para mostrarlo en la interfaz.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TftpEvent {
    Start { peer: String, file: String, write: bool, size: Option<u64> },
    Done { peer: String, file: String, write: bool, bytes: u64, ms: u64 },
    Fail { peer: String, file: String, write: bool, error: String },
}

pub type Log = Arc<dyn Fn(TftpEvent) + Send + Sync>;

/// Un servidor corriendo. Se apaga al soltarlo (o con `stop`).
pub struct TftpServer {
    pub dir: PathBuf,
    pub port: u16,
    pub write: bool,
    task: tokio::task::JoinHandle<()>,
}

impl TftpServer {
    /// Escucha en `0.0.0.0:port`. Falla si el puerto esta ocupado (otro TFTP, como Tftpd64)
    /// o si la carpeta no existe.
    pub async fn start(dir: PathBuf, port: u16, write: bool, log: Log) -> std::io::Result<Self> {
        if !dir.is_dir() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} is not a folder", dir.display())));
        }
        let sock = UdpSocket::bind(("0.0.0.0", port)).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                std::io::Error::new(e.kind(), format!("port {port} is in use (is another TFTP server like Tftpd64 running?)"))
            } else {
                e
            }
        })?;
        let port = sock.local_addr()?.port();
        let root = dir.clone();
        let task = tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            loop {
                let Ok((n, peer)) = sock.recv_from(&mut buf).await else { continue };
                let pkt = buf[..n].to_vec();
                let (root, log) = (root.clone(), log.clone());
                tokio::spawn(async move { session(root, write, peer, pkt, log).await });
            }
        });
        Ok(TftpServer { dir, port, write, task })
    }

    pub fn stop(self) {
        self.task.abort();
    }
}

impl Drop for TftpServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Un pedido de lectura o escritura.
#[derive(Debug, PartialEq)]
struct Request {
    write: bool,
    file: String,
    options: Vec<(String, String)>,
}

fn parse_request(p: &[u8]) -> Option<Request> {
    let op = u16::from_be_bytes([*p.first()?, *p.get(1)?]);
    if op != RRQ && op != WRQ {
        return None;
    }
    let mut parts = p[2..].split(|&b| b == 0).map(|s| String::from_utf8_lossy(s).to_string());
    let file = parts.next()?;
    let _mode = parts.next()?;
    let rest: Vec<String> = parts.filter(|s| !s.is_empty()).collect();
    let options = rest.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_lowercase(), c[1].clone())).collect();
    Some(Request { write: op == WRQ, file, options })
}

/// La ruta dentro de `root`, o None si se quiere salir de ahi.
fn safe_path(root: &Path, file: &str) -> Option<PathBuf> {
    let rel = file.replace('\\', "/");
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return None;
    }
    let p = Path::new(rel);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) || rel.contains(':') {
        return None;
    }
    Some(root.join(p))
}

fn error_packet(code: u16, msg: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(5 + msg.len());
    v.extend_from_slice(&ERROR.to_be_bytes());
    v.extend_from_slice(&code.to_be_bytes());
    v.extend_from_slice(msg.as_bytes());
    v.push(0);
    v
}

async fn session(root: PathBuf, allow_write: bool, peer: SocketAddr, pkt: Vec<u8>, log: Log) {
    let Some(req) = parse_request(&pkt) else { return };
    // Cada transferencia sale de un puerto nuevo (el "TID" del protocolo).
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)).await else { return };
    if sock.connect(peer).await.is_err() {
        return;
    }
    let peer_s = peer.ip().to_string();
    let fail = |error: String| TftpEvent::Fail { peer: peer_s.clone(), file: req.file.clone(), write: req.write, error };
    let Some(path) = safe_path(&root, &req.file) else {
        let _ = sock.send(&error_packet(2, "access violation")).await;
        return log(fail("the path goes outside the TFTP folder".into()));
    };
    if req.write && !allow_write {
        let _ = sock.send(&error_packet(2, "uploads are disabled")).await;
        return log(fail("uploads are disabled".into()));
    }
    let started = Instant::now();
    let res = if req.write { receive(&sock, &path, &req, &peer_s, &log).await } else { send(&sock, &path, &req, &peer_s, &log).await };
    match res {
        Ok(bytes) => log(TftpEvent::Done { peer: peer_s.clone(), file: req.file.clone(), write: req.write, bytes, ms: started.elapsed().as_millis() as u64 }),
        Err(e) => {
            let _ = sock.send(&error_packet(0, &e)).await;
            log(fail(e));
        }
    }
}

/// Las opciones que se aceptan: (tamaño de bloque, timeout, las que se contestan en el OACK).
fn negotiate(req: &Request, size: Option<u64>) -> (usize, Duration, Vec<(String, String)>) {
    let mut block = DEFAULT_BLOCK;
    let mut timeout = Duration::from_secs(3);
    let mut ack = Vec::new();
    for (k, v) in &req.options {
        match k.as_str() {
            "blksize" => {
                if let Ok(n) = v.parse::<usize>() {
                    // 1468 entra en un paquete Ethernet sin fragmentar; mas grande se acepta si se pide.
                    block = n.clamp(8, MAX_BLOCK);
                    ack.push((k.clone(), block.to_string()));
                }
            }
            "timeout" => {
                if let Ok(n) = v.parse::<u64>() {
                    if (1..=255).contains(&n) {
                        timeout = Duration::from_secs(n);
                        ack.push((k.clone(), n.to_string()));
                    }
                }
            }
            "tsize" => ack.push((k.clone(), size.map(|s| s.to_string()).unwrap_or_else(|| v.clone()))),
            _ => {}
        }
    }
    (block, timeout, ack)
}

fn oack(opts: &[(String, String)]) -> Vec<u8> {
    let mut v = OACK.to_be_bytes().to_vec();
    for (k, val) in opts {
        v.extend_from_slice(k.as_bytes());
        v.push(0);
        v.extend_from_slice(val.as_bytes());
        v.push(0);
    }
    v
}

/// Manda `pkt` y espera el ACK de `block`, reintentando. Ignora ACKs viejos (duplicados).
async fn send_and_wait(sock: &UdpSocket, pkt: &[u8], block: u16, timeout: Duration) -> Result<(), String> {
    let mut buf = vec![0u8; 1024];
    for _ in 0..RETRIES {
        sock.send(pkt).await.map_err(|e| e.to_string())?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(left, sock.recv(&mut buf)).await {
                Err(_) => break, // se reintenta
                Ok(Err(e)) => return Err(e.to_string()),
                Ok(Ok(n)) if n >= 4 => {
                    let op = u16::from_be_bytes([buf[0], buf[1]]);
                    let b = u16::from_be_bytes([buf[2], buf[3]]);
                    if op == ACK && b == block {
                        return Ok(());
                    }
                    if op == ERROR {
                        return Err(format!("the device cancelled: {}", String::from_utf8_lossy(&buf[4..n]).trim_end_matches('\0')));
                    }
                }
                Ok(Ok(_)) => {}
            }
        }
    }
    Err("the device stopped answering".into())
}

/// Lectura (el equipo baja un archivo).
async fn send(sock: &UdpSocket, path: &Path, req: &Request, peer: &str, log: &Log) -> Result<u64, String> {
    let mut f = tokio::fs::File::open(path).await.map_err(|_| {
        // El codigo 1 es "file not found" para el equipo.
        "file not found".to_string()
    })?;
    let size = f.metadata().await.ok().map(|m| m.len());
    log(TftpEvent::Start { peer: peer.into(), file: req.file.clone(), write: false, size });
    let (bs, timeout, opts) = negotiate(req, size);
    if !opts.is_empty() {
        send_and_wait(sock, &oack(&opts), 0, timeout).await?;
    }
    let mut block: u16 = 1;
    let mut total = 0u64;
    let mut buf = vec![0u8; bs];
    loop {
        let mut n = 0;
        while n < bs {
            let k = f.read(&mut buf[n..]).await.map_err(|e| e.to_string())?;
            if k == 0 {
                break;
            }
            n += k;
        }
        let mut pkt = DATA.to_be_bytes().to_vec();
        pkt.extend_from_slice(&block.to_be_bytes());
        pkt.extend_from_slice(&buf[..n]);
        send_and_wait(sock, &pkt, block, timeout).await?;
        total += n as u64;
        if n < bs {
            return Ok(total);
        }
        block = block.wrapping_add(1);
    }
}

/// Escritura (el equipo sube un archivo). Se escribe a un temporal y se renombra al final,
/// asi un corte no deja el archivo a medias.
async fn receive(sock: &UdpSocket, path: &Path, req: &Request, peer: &str, log: &Log) -> Result<u64, String> {
    let size = req.options.iter().find(|(k, _)| k == "tsize").and_then(|(_, v)| v.parse().ok());
    log(TftpEvent::Start { peer: peer.into(), file: req.file.clone(), write: true, size });
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tftp-part");
    let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    let (bs, timeout, opts) = negotiate(req, size);
    // La primera respuesta: OACK si hubo opciones, si no ACK 0.
    let mut reply = if opts.is_empty() { [ACK.to_be_bytes(), 0u16.to_be_bytes()].concat() } else { oack(&opts) };
    let mut expect: u16 = 1;
    let mut total = 0u64;
    let mut buf = vec![0u8; bs + 4];
    let result: Result<u64, String> = async {
        loop {
            let mut got = None;
            for _ in 0..RETRIES {
                sock.send(&reply).await.map_err(|e| e.to_string())?;
                match tokio::time::timeout(timeout, sock.recv(&mut buf)).await {
                    Ok(Ok(n)) if n >= 4 && u16::from_be_bytes([buf[0], buf[1]]) == DATA => {
                        let b = u16::from_be_bytes([buf[2], buf[3]]);
                        if b == expect {
                            got = Some(n - 4);
                            break;
                        }
                        // Un bloque repetido: se vuelve a mandar el ultimo ACK (arriba).
                    }
                    Ok(Ok(n)) if n >= 4 && u16::from_be_bytes([buf[0], buf[1]]) == ERROR => {
                        return Err(format!("the device cancelled: {}", String::from_utf8_lossy(&buf[4..n]).trim_end_matches('\0')));
                    }
                    Ok(Err(e)) => return Err(e.to_string()),
                    _ => {}
                }
            }
            let n = got.ok_or("the device stopped sending")?;
            f.write_all(&buf[4..4 + n]).await.map_err(|e| e.to_string())?;
            total += n as u64;
            reply = [ACK.to_be_bytes(), expect.to_be_bytes()].concat();
            if n < bs {
                sock.send(&reply).await.map_err(|e| e.to_string())?;
                return Ok(total);
            }
            expect = expect.wrapping_add(1);
        }
    }
    .await;
    match result {
        Ok(t) => {
            f.flush().await.map_err(|e| e.to_string())?;
            drop(f);
            tokio::fs::rename(&tmp, path).await.map_err(|e| e.to_string())?;
            Ok(t)
        }
        Err(e) => {
            drop(f);
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

/// Un cliente minimo, para los tests y para probar el servidor desde la app.
pub async fn get(server: SocketAddr, file: &str, blksize: Option<usize>) -> std::io::Result<Vec<u8>> {
    let sock = UdpSocket::bind(("127.0.0.1", 0)).await?;
    let mut req = RRQ.to_be_bytes().to_vec();
    req.extend_from_slice(file.as_bytes());
    req.extend_from_slice(b"\0octet\0");
    if let Some(b) = blksize {
        req.extend_from_slice(format!("blksize\0{b}\0").as_bytes());
    }
    sock.send_to(&req, server).await?;
    let mut out = Vec::new();
    let mut bs = DEFAULT_BLOCK;
    let mut buf = vec![0u8; MAX_BLOCK + 4];
    loop {
        let (n, from) = tokio::time::timeout(Duration::from_secs(3), sock.recv_from(&mut buf))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "no answer"))??;
        let op = u16::from_be_bytes([buf[0], buf[1]]);
        match op {
            OACK => {
                if let Some(b) = blksize {
                    bs = b;
                }
                sock.send_to(&[0, ACK as u8, 0, 0], from).await?;
            }
            DATA => {
                let block = [buf[2], buf[3]];
                out.extend_from_slice(&buf[4..n]);
                sock.send_to(&[0, ACK as u8, block[0], block[1]], from).await?;
                if n - 4 < bs {
                    return Ok(out);
                }
            }
            _ => return Err(std::io::Error::other(String::from_utf8_lossy(&buf[4..n]).trim_end_matches('\0').to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn requests() {
        let r = parse_request(b"\0\x01fw/image.bin\0octet\0blksize\01468\0tsize\00\0").unwrap();
        assert!(!r.write);
        assert_eq!(r.file, "fw/image.bin");
        assert_eq!(r.options, vec![("blksize".into(), "1468".into()), ("tsize".into(), "0".into())]);
        assert!(parse_request(b"\0\x03xx").is_none());
    }

    #[test]
    fn paths() {
        let root = Path::new("C:/tftp");
        assert_eq!(safe_path(root, "a/b.bin"), Some(root.join("a/b.bin")));
        assert_eq!(safe_path(root, "/a.bin"), Some(root.join("a.bin")));
        assert_eq!(safe_path(root, "..\\secret"), None);
        assert_eq!(safe_path(root, "a/../../x"), None);
        assert_eq!(safe_path(root, "C:/Windows/win.ini"), None);
        assert_eq!(safe_path(root, ""), None);
    }

    async fn server(write: bool) -> (tempfile::TempDir, TftpServer, Arc<Mutex<Vec<TftpEvent>>>) {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev = events.clone();
        let s = TftpServer::start(dir.path().to_path_buf(), 0, write, Arc::new(move |e| ev.lock().unwrap().push(e))).await.unwrap();
        (dir, s, events)
    }

    #[tokio::test]
    async fn read_file() {
        let (dir, s, events) = server(false).await;
        // 3 bloques justos de 512: hace falta el bloque vacio del final.
        let data: Vec<u8> = (0..1536u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(dir.path().join("fw.bin"), &data).unwrap();
        let addr: SocketAddr = format!("127.0.0.1:{}", s.port).parse().unwrap();
        assert_eq!(get(addr, "fw.bin", None).await.unwrap(), data);
        assert_eq!(get(addr, "fw.bin", Some(1468)).await.unwrap(), data);
        assert!(get(addr, "nope.bin", None).await.is_err());
        assert!(get(addr, "../x", None).await.is_err());
        tokio::time::sleep(Duration::from_millis(50)).await;
        let ev = events.lock().unwrap();
        assert!(ev.iter().any(|e| matches!(e, TftpEvent::Done { bytes: 1536, .. })));
        assert!(ev.iter().any(|e| matches!(e, TftpEvent::Fail { .. })));
    }

    #[tokio::test]
    async fn write_disabled() {
        let (_dir, s, _) = server(false).await;
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sock.send_to(b"\0\x02up.cfg\0octet\0", ("127.0.0.1", s.port)).await.unwrap();
        let mut buf = [0u8; 100];
        let (n, _) = sock.recv_from(&mut buf).await.unwrap();
        assert_eq!(u16::from_be_bytes([buf[0], buf[1]]), ERROR);
        assert!(String::from_utf8_lossy(&buf[4..n]).contains("disabled"));
    }

    #[tokio::test]
    async fn write_file() {
        let (dir, s, _) = server(true).await;
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sock.send_to(b"\0\x02cfg/switch.cfg\0octet\0", ("127.0.0.1", s.port)).await.unwrap();
        let mut buf = [0u8; 600];
        let (_, from) = sock.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..4], &[0, 4, 0, 0]);
        let body = vec![7u8; 700];
        for (i, chunk) in body.chunks(512).enumerate() {
            let mut pkt = vec![0, 3];
            pkt.extend_from_slice(&((i + 1) as u16).to_be_bytes());
            pkt.extend_from_slice(chunk);
            sock.send_to(&pkt, from).await.unwrap();
            let (_, _) = sock.recv_from(&mut buf).await.unwrap();
            assert_eq!(&buf[..4], &[0, 4, 0, (i + 1) as u8]);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(std::fs::read(dir.path().join("cfg/switch.cfg")).unwrap(), body);
    }
}
