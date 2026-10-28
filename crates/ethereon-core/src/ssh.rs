//! Conexiones SSH: saltos por bastion, verificacion de la clave del host, login y
//! ejecucion de comandos con limites de tiempo y de tamano.

use russh::client::{self, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{decode_secret_key, Certificate, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::ChannelMsg;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

use crate::model::{Credential, CredentialKind};

/// Un salto de la cadena (el ultimo es el host de destino).
#[derive(Clone)]
pub struct Hop {
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    pub cred: Credential,
    /// Huella confirmada; None = todavia no la confirmo nadie.
    pub expected: Option<String>,
}

impl Hop {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.hostname, self.port)
    }
}

#[derive(Debug, Clone, serde::Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SshError {
    #[error("host key for {addr} is not confirmed yet ({fingerprint}); confirm it in Ethereon")]
    HostKeyUnknown { host: String, addr: String, fingerprint: String, algorithm: String },
    #[error("HOST KEY CHANGED for {addr}: expected {expected}, got {got}. Connection blocked")]
    HostKeyChanged { host: String, addr: String, expected: String, got: String },
    #[error("authentication failed for {user}@{host}: {detail}")]
    Auth { host: String, user: String, detail: String },
    #[error("could not connect to {host}: {detail}")]
    Connect { host: String, detail: String },
}

type Seen = Arc<Mutex<Option<(String, String)>>>;

pub struct Client {
    expected: Option<String>,
    seen: Seen,
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, k: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let key = match k {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(c) => PublicKey::new(c.public_key().clone(), ""),
        };
        let fp = fingerprint(&key);
        let ok = self.expected.as_deref() == Some(fp.as_str());
        *self.seen.lock().unwrap() = Some((fp, key.algorithm().to_string()));
        Ok(ok)
    }
}

/// Una conexion abierta. Se guardan todos los saltos: si se suelta un bastion, se cae
/// el tunel que lleva al siguiente.
pub struct Conn {
    handles: Vec<Handle<Client>>,
    pub last_used: Mutex<Instant>,
}

impl Conn {
    pub fn target(&self) -> &Handle<Client> {
        self.handles.last().unwrap()
    }
    pub fn is_closed(&self) -> bool {
        self.handles.iter().any(|h| h.is_closed())
    }
    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }
}

fn config() -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(900)),
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    })
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn connect(hops: &[Hop]) -> Result<Conn, SshError> {
    let mut handles: Vec<Handle<Client>> = Vec::new();
    for hop in hops {
        let seen: Seen = Arc::new(Mutex::new(None));
        let handler = Client { expected: hop.expected.clone(), seen: seen.clone() };
        let conn_err = |d: String| SshError::Connect { host: hop.name.clone(), detail: d };
        let fut = async {
            match handles.last() {
                None => client::connect(config(), (hop.hostname.as_str(), hop.port), handler).await,
                Some(prev) => {
                    let ch = prev
                        .channel_open_direct_tcpip(hop.hostname.clone(), hop.port as u32, "127.0.0.1", 0)
                        .await?;
                    client::connect_stream(config(), ch.into_stream(), handler).await
                }
            }
        };
        let res = tokio::time::timeout(CONNECT_TIMEOUT, fut).await.map_err(|_| conn_err("timed out".into()))?;
        let mut h = match res {
            Ok(h) => h,
            Err(e) => {
                // Si fallo por la huella, se dice cual.
                if let Some((fp, alg)) = seen.lock().unwrap().clone() {
                    return Err(match &hop.expected {
                        None => SshError::HostKeyUnknown { host: hop.name.clone(), addr: hop.addr(), fingerprint: fp, algorithm: alg },
                        Some(exp) if *exp != fp => SshError::HostKeyChanged {
                            host: hop.name.clone(),
                            addr: hop.addr(),
                            expected: exp.clone(),
                            got: fp,
                        },
                        _ => conn_err(e.to_string()),
                    });
                }
                return Err(conn_err(e.to_string()));
            }
        };
        auth(&mut h, hop).await?;
        handles.push(h);
    }
    Ok(Conn { handles, last_used: Mutex::new(Instant::now()) })
}

/// Solo la huella del primer salto que todavia no esta confirmado (para mostrarla).
pub async fn probe(hops: &[Hop]) -> Result<Option<SshError>, SshError> {
    match connect(hops).await {
        Ok(_) => Ok(None),
        Err(e @ (SshError::HostKeyUnknown { .. } | SshError::HostKeyChanged { .. })) => Ok(Some(e)),
        Err(e) => Err(e),
    }
}

async fn auth(h: &mut Handle<Client>, hop: &Hop) -> Result<(), SshError> {
    let fail = |d: String| SshError::Auth { host: hop.name.clone(), user: hop.user.clone(), detail: d };
    let c = &hop.cred;
    let ok = match c.kind {
        CredentialKind::Password => {
            let r = h.authenticate_password(&hop.user, c.secret.as_str()).await.map_err(|e| fail(e.to_string()))?;
            r.success() || keyboard_interactive(h, &hop.user, &c.secret).await.map_err(|e| fail(e.to_string()))?
        }
        CredentialKind::PrivateKey | CredentialKind::Certificate => {
            let pass = (!c.passphrase.is_empty()).then_some(c.passphrase.as_str());
            let key = decode_secret_key(&c.secret, pass).map_err(|e| fail(format!("could not read the key: {e}")))?;
            if !c.certificate.trim().is_empty() {
                let cert = Certificate::from_openssh(c.certificate.trim()).map_err(|e| fail(format!("bad certificate: {e}")))?;
                h.authenticate_openssh_cert(&hop.user, Arc::new(key), cert).await.map_err(|e| fail(e.to_string()))?.success()
            } else {
                let hash = h.best_supported_rsa_hash().await.ok().flatten().flatten();
                let k = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
                h.authenticate_publickey(&hop.user, k).await.map_err(|e| fail(e.to_string()))?.success()
            }
        }
        CredentialKind::SudoPassword | CredentialKind::ApiToken => {
            return Err(fail(format!("credential `{}` can't be used to log in", c.id)));
        }
    };
    if ok {
        Ok(())
    } else {
        Err(fail("rejected by the server".into()))
    }
}

async fn keyboard_interactive(h: &mut Handle<Client>, user: &str, password: &str) -> Result<bool, russh::Error> {
    let mut r = h.authenticate_keyboard_interactive_start(user, None).await?;
    for _ in 0..5 {
        match r {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = prompts.iter().map(|_| password.to_string()).collect();
                r = h.authenticate_keyboard_interactive_respond(answers).await?;
            }
        }
    }
    Ok(false)
}

#[derive(Debug, Default)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub truncated: bool,
    pub timed_out: bool,
}

/// Corre `command` en el destino. `stdin` (la contrasena de sudo) se escribe por la
/// entrada estandar y despues se cierra: nunca va en la linea de comando.
pub async fn exec(
    conn: &Conn,
    command: &str,
    stdin: Option<Zeroizing<String>>,
    timeout: Duration,
    max_bytes: usize,
) -> Result<ExecResult, russh::Error> {
    conn.touch();
    let mut ch = conn.target().channel_open_session().await?;
    ch.exec(true, command).await?;
    if let Some(s) = stdin {
        let line = Zeroizing::new(format!("{}\n", s.as_str()).into_bytes());
        ch.data(&line[..]).await?;
    }
    ch.eof().await?;

    let mut r = ExecResult::default();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let msg = match tokio::time::timeout_at(deadline, ch.wait()).await {
            Ok(m) => m,
            Err(_) => {
                r.timed_out = true;
                let _ = ch.close().await;
                break;
            }
        };
        match msg {
            Some(ChannelMsg::Data { data }) => push(&mut r.stdout, &data, max_bytes, &mut r.truncated),
            Some(ChannelMsg::ExtendedData { data, .. }) => push(&mut r.stderr, &data, max_bytes, &mut r.truncated),
            Some(ChannelMsg::ExitStatus { exit_status }) => r.exit_code = Some(exit_status as i32),
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => r.signal = Some(format!("{signal_name:?}")),
            Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
        // Mucho mas de lo que se va a devolver: no tiene sentido seguir bajando.
        if r.stdout.len() + r.stderr.len() >= max_bytes * 2 && r.truncated {
            let _ = ch.close().await;
            break;
        }
    }
    conn.touch();
    Ok(r)
}

fn push(buf: &mut Vec<u8>, data: &[u8], max: usize, truncated: &mut bool) {
    let room = max.saturating_sub(buf.len());
    if data.len() > room {
        *truncated = true;
    }
    buf.extend_from_slice(&data[..data.len().min(room)]);
}

/// Comillas simples para la shell remota.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Envuelve el comando para sudo: la contrasena se lee de stdin (`-S`), sin prompt
/// (`-p ''`), y `-k` fuerza a pedirla siempre, asi nunca queda en la entrada del comando.
pub fn sudo_wrap(command: &str) -> String {
    format!("sudo -k -S -p '' -- sh -c {}", sh_quote(command))
}

/// Shell interactiva con PTY, para la terminal de la interfaz.
pub async fn open_shell(conn: &Conn, cols: u32, rows: u32) -> Result<russh::Channel<client::Msg>, russh::Error> {
    conn.touch();
    let ch = conn.target().channel_open_session().await?;
    ch.request_pty(true, "xterm-256color", cols, rows, 0, 0, &[]).await?;
    ch.request_shell(true).await?;
    Ok(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sudo_wrap("ls /root"), "sudo -k -S -p '' -- sh -c 'ls /root'");
    }
}
