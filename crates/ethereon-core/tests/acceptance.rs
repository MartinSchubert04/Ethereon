//! Pruebas de aceptacion de la spec contra servidores SSH reales (tests/ssh-fixture), mas
//! las de archivos del agente por SFTP (`file_transfer`, y por MCP en `mcp_over_http`).
//!
//!   bash crates/ethereon-core/tests/ssh-fixture/up.sh <carpeta>
//!   ETHEREON_TEST_KEY=<carpeta>/key cargo test -p ethereon-core --test acceptance -- --ignored --nocapture

use std::sync::Arc;
use ethereon_core::broker::Decision;
use ethereon_core::model::*;
use ethereon_core::ssh::SshError;
use ethereon_core::transfer::Direction;
use ethereon_core::{Broker, Error, Notifier};

struct Quiet;
impl Notifier for Quiet {
    fn event(&self, _: &str, _: serde_json::Value) {}
    fn notify(&self, _: &str, _: &str) {}
}

const PW: &str = "tester-pass-123";
const VAULT_TOKEN: &str = "Sup3r-S3cret-Value";

fn cred(id: &str, kind: CredentialKind, secret: &str) -> Credential {
    Credential { id: id.into(), kind, username: String::new(), secret: secret.into(), passphrase: String::new(), certificate: String::new(), note: String::new() }
}

async fn trust_all(b: &Broker, host: &str) {
    for _ in 0..3 {
        match b.test_host(host).await.unwrap() {
            Some(SshError::HostKeyUnknown { addr, fingerprint, algorithm, .. }) => {
                println!("confirmo {addr} {fingerprint}");
                b.trust_host_key(&addr, &fingerprint, &algorithm).await.unwrap();
            }
            None => return,
            Some(e) => panic!("{e}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita los contenedores de tests/ssh-fixture"]
async fn acceptance() {
    let key = std::fs::read_to_string(std::env::var("ETHEREON_TEST_KEY").expect("ETHEREON_TEST_KEY")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().to_path_buf(), Arc::new(Quiet));
    b.create("master-password-1").unwrap();
    b.write(|d| {
        d.credentials.push(cred("bastion-key", CredentialKind::PrivateKey, &key));
        d.credentials.push(cred("target-pw", CredentialKind::Password, PW));
        d.credentials.push(cred("target-sudo", CredentialKind::SudoPassword, PW));
        d.credentials.push(cred("db-token", CredentialKind::ApiToken, VAULT_TOKEN));
        d.hosts.push(Host {
            name: "bastion".into(),
            hostname: "127.0.0.1".into(),
            port: 2222,
            user: "tester".into(),
            credential: "bastion-key".into(),
            policy: Some(Policy { mode: Mode::ReadOnly, ..Default::default() }),
            ..Default::default()
        });
        d.hosts.push(Host {
            name: "app".into(),
            hostname: "target".into(),
            port: 22,
            user: "tester".into(),
            credential: "target-pw".into(),
            sudo_credential: Some("target-sudo".into()),
            jump: vec!["bastion".into()],
            env: Env::Test,
            description: "internal app server".into(),
            policy: Some(Policy { mode: Mode::Guarded, ..Default::default() }),
            ..Default::default()
        });
        Ok(())
    })
    .unwrap();
    let (_, token) = b.create_device("test-pc", Some(1), vec![]).unwrap();
    let ctx = b.authenticate(&token, "127.0.0.1", "s1").unwrap();

    // Sin huella confirmada no hay conexion.
    let e = b.agent_run(&ctx, "app", "uptime", false, "check").await.unwrap_err();
    assert!(matches!(e, Error::Ssh(SshError::HostKeyUnknown { .. })), "{e}");
    trust_all(&b, "app").await;

    // El agente no ve IPs, usuarios ni credenciales.
    let hosts = serde_json::to_string(&b.agent_list_hosts(&ctx).unwrap()).unwrap();
    assert!(hosts.contains("internal app server") && !hosts.contains("127.0.0.1") && !hosts.contains("target-pw"));

    // El sistema del host (para el icono) se detecta una vez y queda en el vault, sin llegar al agente.
    let app = b.read(|d| d.hosts.iter().find(|h| h.name == "app").cloned()).unwrap().unwrap();
    b.detect_os("app", &b.connection(&app).await.unwrap()).await.unwrap();
    let os = b.read(|d| d.hosts.iter().find(|h| h.name == "app").and_then(|h| h.os.clone())).unwrap();
    println!("os -> {os:?}");
    assert!(os.is_some_and(|o| !o.is_empty()));
    assert!(!serde_json::to_string(&b.agent_list_hosts(&ctx).unwrap()).unwrap().contains("\"os\""));

    // 1. Un archivo con un secreto del vault vuelve con ***.
    let r = b.agent_run(&ctx, "app", "cat notes.txt", false, "read notes").await.unwrap();
    println!("cat -> {:?}", r.stdout);
    assert!(!r.stdout.contains(VAULT_TOKEN) && r.stdout.contains("***") && r.masked == 1);

    // 2. sudo sin que la contrasena aparezca en la salida, la auditoria ni los procesos.
    let r = b.agent_run(&ctx, "app", "id -u", true, "need root").await.unwrap();
    println!("sudo id -u -> {:?} {:?} exit {:?}", r.stdout, r.stderr, r.exit_code);
    assert_eq!(r.stdout.trim(), "0");
    assert_eq!(r.masked, 0, "the password never reached the output");
    let (slow, ps) = tokio::join!(
        b.agent_run(&ctx, "app", "sleep 2; echo done", true, "slow"),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            b.agent_run(&ctx, "app", "ps -ef", false, "processes").await
        }
    );
    let ps = ps.unwrap();
    assert!(ps.stdout.contains("sudo -k -S"), "sudo was running while ps looked: {}", ps.stdout);
    assert_eq!(ps.masked, 0, "the password is not in any command line");
    assert_eq!(slow.unwrap().stdout.trim(), "done");
    let audit = std::fs::read_to_string(dir.path().join("audit.jsonl")).unwrap();
    assert!(!audit.contains(PW) && !audit.contains(VAULT_TOKEN));

    // 3. Un comando prohibido escondido en un pipe o en $( ) se rechaza.
    for cmd in ["ls | rm -rf /tmp/x", "echo $(cat /etc/shadow)", "uptime; `shutdown now`", "cat ~/.ssh/authorized_keys"] {
        let e = b.agent_run(&ctx, "app", cmd, false, "x").await.unwrap_err();
        println!("{cmd} -> {e}");
        assert!(e.to_string().contains("rejected"), "{cmd}: {e}");
    }
    // Solo lectura: lo que no esta en la lista no corre.
    assert!(b.agent_run(&ctx, "bastion", "touch x", false, "x").await.is_err());
    assert_eq!(b.agent_run(&ctx, "bastion", "df -h | head -2", false, "x").await.unwrap().exit_code, Some(0));

    // Aprobacion: una persona cambia el comando antes de aprobarlo.
    b.write(|d| {
        d.hosts.iter_mut().find(|h| h.name == "app").unwrap().policy = Some(Policy { mode: Mode::Approval, ..Default::default() });
        Ok(())
    })
    .unwrap();
    let pending = b.agent_run(&ctx, "app", "echo hola", false, "say hi");
    let approve = async {
        loop {
            if let Some(a) = b.approvals().first() {
                assert_eq!(a.reason, "say hi");
                b.resolve(a.id, Decision::ApproveEdited { command: "echo chau".into() }).unwrap();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    let (r, _) = tokio::join!(pending, approve);
    assert_eq!(r.unwrap().stdout.trim(), "chau");

    // 4. Con el vault bloqueado, todo falla.
    b.lock().await;
    assert!(matches!(b.agent_run(&ctx, "app", "uptime", false, "x").await, Err(Error::Locked)));
    assert!(b.authenticate(&token, "", "").is_err());
    b.unlock("master-password-1").unwrap();
    assert!(b.audit_verify().unwrap().is_ok());

    // 5. Si cambia la clave del host, la conexion se bloquea.
    b.write(|d| {
        d.hosts.iter_mut().find(|h| h.name == "app").unwrap().policy = Some(Policy { mode: Mode::Guarded, ..Default::default() });
        d.known_hosts.iter_mut().find(|k| k.addr == "target:22").unwrap().fingerprint = "SHA256:somethingelse".into();
        Ok(())
    })
    .unwrap();
    b.drop_connections().await;
    let e = b.agent_run(&ctx, "app", "uptime", false, "x").await.unwrap_err();
    println!("key changed -> {e}");
    assert!(matches!(e, Error::Ssh(SshError::HostKeyChanged { .. })));
    assert_eq!(b.alerts().len(), 1);

    // Interruptor general.
    b.set_agent_enabled(false).unwrap();
    assert!(b.agent_list_hosts(&ctx).is_err());
}

/// Cambia las reglas de un host. Sin limite de frecuencia que moleste: la prueba hace mas
/// de 30 pedidos por minuto al mismo host.
fn set_policy(b: &Broker, host: &str, p: Policy) {
    b.write(|d| {
        d.hosts.iter_mut().find(|h| h.name == host).unwrap().policy = Some(Policy { max_per_minute: 1000, ..p });
        Ok(())
    })
    .unwrap();
}

/// Archivos del agente por SFTP contra los contenedores: bajar enmascarado, reglas `path:`
/// que no se esquivan con `..` ni con enlaces, binarios y tamano, subir sin pisar un enlace
/// duro, archivos locales cerrados, solo lectura, aprobacion y auditoria.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita los contenedores de tests/ssh-fixture"]
async fn file_transfer() {
    let key = std::fs::read_to_string(std::env::var("ETHEREON_TEST_KEY").expect("ETHEREON_TEST_KEY")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().join("data"), Arc::new(Quiet));
    b.create("master-password-1").unwrap();
    b.write(|d| {
        d.credentials.push(cred("bastion-key", CredentialKind::PrivateKey, &key));
        d.credentials.push(cred("target-pw", CredentialKind::Password, PW));
        d.credentials.push(cred("db-token", CredentialKind::ApiToken, VAULT_TOKEN));
        d.hosts.push(Host {
            name: "bastion".into(),
            hostname: "127.0.0.1".into(),
            port: 2222,
            user: "tester".into(),
            credential: "bastion-key".into(),
            policy: Some(Policy { mode: Mode::ReadOnly, files: true, ..Default::default() }),
            ..Default::default()
        });
        d.hosts.push(Host {
            name: "app".into(),
            hostname: "target".into(),
            port: 22,
            user: "tester".into(),
            credential: "target-pw".into(),
            jump: vec!["bastion".into()],
            policy: Some(Policy { mode: Mode::Free, max_per_minute: 1000, ..Default::default() }),
            ..Default::default()
        });
        Ok(())
    })
    .unwrap();
    trust_all(&b, "app").await;
    let (_, token) = b.create_device("pc", None, vec![]).unwrap();
    let ctx = b.authenticate(&token, "127.0.0.1", "s1").unwrap();
    let run = |cmd: &'static str| {
        let (b, ctx) = (&b, &ctx);
        async move { b.agent_run(ctx, "app", cmd, false, "setup").await.unwrap().stdout }
    };
    // Lo que el agente no deberia poder alcanzar, armado con `run` en modo Free.
    run("rm -f innocent sshdir up.txt hard.txt script.sh big.txt rand.bin; ln -s .ssh/authorized_keys innocent; ln -s .ssh sshdir; \
         head -c 100 /dev/urandom > rand.bin; head -c 2048 /dev/zero | tr '\\0' a > big.txt; \
         printf '#!/bin/sh\\necho v1\\n' > script.sh; chmod 750 script.sh")
        .await;

    // Apagado por defecto.
    let e = b.agent_download(&ctx, "app", "notes.txt", "x").await.unwrap_err().to_string();
    assert!(e.contains("not enabled"), "{e}");
    set_policy(&b, "app", Policy { mode: Mode::Guarded, files: true, ..Default::default() });

    // Bajar: el texto vuelve enmascarado y la ruta resuelta (con el usuario) no aparece.
    let r = b.agent_download(&ctx, "app", "notes.txt", "read notes").await.unwrap();
    println!("download -> {r:?}");
    assert_eq!(r.content, "the db secret is ***\n");
    assert_eq!((r.masked, r.path.as_str()), (1, "notes.txt"));
    assert!(!serde_json::to_string(&r).unwrap().contains("\"size\""), "the raw size is not given");
    assert!(!serde_json::to_string(&r).unwrap().contains("/home/tester"));

    // Lo prohibido no pasa: directo, con `..`, por un enlace, ni `/etc/shadow`.
    for p in ["~/.ssh/authorized_keys", "/tmp/../home/tester/.ssh/authorized_keys", "innocent", "/etc/../etc/shadow", "sshdir/authorized_keys"] {
        let e = b.agent_download(&ctx, "app", p, "x").await.unwrap_err().to_string();
        println!("download {p} -> {e}");
        assert!(e.contains("rejected") && e.contains("path:"), "{p}: {e}");
        assert!(!e.contains("/home/tester") && !e.contains("target") && !e.contains("127.0.0.1"), "{e}");
    }
    let e = b.agent_download(&ctx, "app", "rand.bin", "x").await.unwrap_err().to_string();
    assert!(e.contains("binary"), "{e}");
    let e = b.agent_download(&ctx, "app", "nothing-here.txt", "x").await.unwrap_err().to_string();
    assert!(e.contains("no such file") && !e.contains("/home/tester"), "{e}");
    let e = b.agent_download(&ctx, "app", "/tmp", "x").await.unwrap_err().to_string();
    assert!(e.contains("folder"), "{e}");
    set_policy(&b, "app", Policy { mode: Mode::Guarded, files: true, max_file_kb: 1, ..Default::default() });
    let e = b.agent_download(&ctx, "app", "big.txt", "x").await.unwrap_err().to_string();
    assert!(e.contains("limit"), "{e}");
    set_policy(&b, "app", Policy { mode: Mode::Guarded, files: true, ..Default::default() });

    // Subir: nuevo, sin pisar lo que existe, y reemplazando con overwrite.
    let r = b.agent_upload(&ctx, "app", "up.txt", Some("hello\n"), None, false, "write").await.unwrap();
    assert!(!r.replaced && r.size == 6);
    assert_eq!(run("cat up.txt").await, "hello\n");
    let e = b.agent_upload(&ctx, "app", "up.txt", Some("again\n"), None, false, "write").await.unwrap_err().to_string();
    assert!(e.contains("already exists"), "{e}");
    // Reemplazar no escribe a traves de un enlace duro: el otro nombre queda como estaba.
    run("ln up.txt hard.txt").await;
    let r = b.agent_upload(&ctx, "app", "hard.txt", Some("replaced\n"), None, true, "write").await.unwrap();
    assert!(r.replaced);
    assert_eq!(run("cat hard.txt; cat up.txt").await, "replaced\nhello\n");
    // Los permisos del que se reemplaza quedan, y no quedan temporales.
    b.agent_upload(&ctx, "app", "script.sh", Some("#!/bin/sh\necho v2\n"), None, true, "update").await.unwrap();
    assert_eq!(run("stat -c %a script.sh; ./script.sh; ls -a | grep -c ethereon- || true").await, "750\nv2\n0\n");

    // Subir a lo prohibido: directo, con `..`, por una carpeta enlace, sobre un enlace.
    for p in ["~/.ssh/authorized_keys", "/tmp/../home/tester/.ssh/new", "sshdir/evil", "innocent"] {
        let e = b.agent_upload(&ctx, "app", p, Some("ssh-ed25519 AAAA evil"), None, true, "x").await.unwrap_err().to_string();
        println!("upload {p} -> {e}");
        assert!(e.contains("rejected") || e.contains("symbolic link"), "{p}: {e}");
    }
    set_policy(&b, "app", Policy { mode: Mode::Free, ..Default::default() });
    assert!(!run("cat .ssh/authorized_keys; ls .ssh").await.contains("evil"));
    set_policy(&b, "app", Policy { mode: Mode::Guarded, files: true, ..Default::default() });
    // Un secreto del vault no sale, ni escrito en el pedido.
    let e = b.agent_upload(&ctx, "app", "leak.txt", Some(&format!("token={VAULT_TOKEN}")), None, false, "x").await.unwrap_err().to_string();
    assert!(e.contains("secret stored in Ethereon") && !e.contains(VAULT_TOKEN), "{e}");
    // `***` encima de un archivo que existe (lo bajado con secretos tapados) no se sube en Guarded.
    let e = b.agent_upload(&ctx, "app", "up.txt", Some("db secret ***\n"), None, true, "x").await.unwrap_err().to_string();
    assert!(e.contains("***"), "{e}");

    // Desde esta PC: un archivo comun sube; el vault de Ethereon no.
    let local = dir.path().join("deploy.conf");
    std::fs::write(&local, "listen 8080\n").unwrap();
    let r = b.agent_upload(&ctx, "app", "/tmp/deploy.conf", None, Some(&local.to_string_lossy()), true, "deploy").await.unwrap();
    assert_eq!(r.size, 12);
    assert_eq!(run("cat /tmp/deploy.conf").await, "listen 8080\n");
    let vault = dir.path().join("data").join("vault.json");
    let e = b.agent_upload(&ctx, "app", "/tmp/v", None, Some(&vault.to_string_lossy()), false, "x").await.unwrap_err().to_string();
    assert!(e.contains("Ethereon's own data"), "{e}");

    // Solo lectura: baja, no sube.
    let r = b.agent_download(&ctx, "bastion", "/etc/hostname", "which box").await.unwrap();
    assert!(!r.content.trim().is_empty());
    let e = b.agent_upload(&ctx, "bastion", "/tmp/x", Some("x"), None, false, "x").await.unwrap_err().to_string();
    assert!(e.contains("read-only"), "{e}");

    // Aprobacion: la persona ve archivo, direccion y tamano; aprueba uno y rechaza otro.
    set_policy(&b, "app", Policy { mode: Mode::Approval, files: true, ..Default::default() });
    let bb = &b;
    // Si el pedido falla antes de llegar a la cola, no se queda esperando para siempre.
    let decide = |approve: bool| async move {
        for _ in 0..200 {
            if let Some(a) = bb.approvals().first().cloned() {
                bb.resolve(a.id, if approve { Decision::Approve } else { Decision::Reject { note: Some("not now".into()) } }).unwrap();
                return a;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("no approval request arrived");
    };
    let (r, a) = tokio::join!(b.agent_upload(&ctx, "app", "approved.txt", Some("ok\n"), None, true, "needs a file"), decide(true));
    r.unwrap();
    let f = a.file.expect("a file request");
    assert_eq!((f.direction, f.path.as_str(), f.size), (Direction::Upload, "approved.txt", 3));
    assert_eq!(a.reason, "needs a file");
    let (r, a) = tokio::join!(b.agent_download(&ctx, "app", "notes.txt", "read"), decide(false));
    assert!(r.unwrap_err().to_string().contains("rejected this transfer: not now"));
    assert_eq!(a.file.unwrap().direction, Direction::Download);

    // La auditoria tiene cada transferencia con tamano y hash del archivo crudo.
    let log = b.audit().search("download notes.txt", 10);
    let ok = log.iter().find(|e| e.decision == "allowed").unwrap();
    assert_eq!(ok.bytes, Some(36));
    assert_eq!(ok.output_sha256, ethereon_core::audit::sha256_hex(format!("the db secret is {VAULT_TOKEN}\n").as_bytes()));
    assert!(log.iter().any(|e| e.decision == "rejected"));
    let raw = std::fs::read_to_string(dir.path().join("data").join("audit.jsonl")).unwrap();
    assert!(!raw.contains(VAULT_TOKEN) && !raw.contains(PW));
    assert!(b.audit_verify().unwrap().is_ok());
    set_policy(&b, "app", Policy { mode: Mode::Free, ..Default::default() });
    run("rm -f innocent sshdir up.txt hard.txt script.sh big.txt rand.bin approved.txt /tmp/deploy.conf").await;
}

/// El servidor MCP por HTTP, como lo usa Claude Code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita los contenedores de tests/ssh-fixture"]
async fn mcp_over_http() {
    let key = std::fs::read_to_string(std::env::var("ETHEREON_TEST_KEY").expect("ETHEREON_TEST_KEY")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().to_path_buf(), Arc::new(Quiet));
    b.create("master-password-1").unwrap();
    b.write(|d| {
        d.credentials.push(cred("k", CredentialKind::PrivateKey, &key));
        d.hosts.push(Host {
            name: "bastion".into(),
            hostname: "127.0.0.1".into(),
            port: 2222,
            user: "tester".into(),
            credential: "k".into(),
            policy: Some(Policy { mode: Mode::ReadOnly, ..Default::default() }),
            ..Default::default()
        });
        Ok(())
    })
    .unwrap();
    trust_all(&b, "bastion").await;
    let (_, token) = b.create_device("pc", None, vec![]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = ethereon_core::mcp::router(b.clone()).into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let post = |body: serde_json::Value, auth: &str| {
        let req = format!(
            "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {auth}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.to_string().len(),
            body
        );
        async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        }
    };
    let init = post(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}), &token).await;
    assert!(init.contains("mcp-session-id") && init.contains("\"ethereon\""), "{init}");
    let tools = post(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), &token).await;
    assert!(tools.contains("list_hosts") && tools.contains("\"run\""));
    let run = post(
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"run","arguments":{"host":"bastion","command":"whoami","reason":"test"}}}),
        &token,
    )
    .await;
    println!("{}", run.lines().last().unwrap());
    assert!(run.contains("tester") && run.contains("exit_code"));

    // Archivos por MCP: las herramientas existen, list_hosts dice que el host las tiene, y
    // en solo lectura se baja (enmascarado) pero no se sube.
    assert!(tools.contains("\"download\"") && tools.contains("\"upload\""));
    set_policy(&b, "bastion", Policy { mode: Mode::ReadOnly, files: true, ..Default::default() });
    let call = |id: u32, name: &str, args: serde_json::Value| {
        post(serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}}), &token)
    };
    let hosts = call(5, "list_hosts", serde_json::json!({})).await;
    assert!(hosts.contains(r#"\"files\": true"#), "{hosts}");
    let down = call(6, "download", serde_json::json!({"host":"bastion","path":"/etc/hostname","reason":"which box"})).await;
    println!("{}", down.lines().last().unwrap());
    assert!(down.contains(r#"\"content\""#) && down.contains(r#""isError":false"#), "{down}");
    let up = call(7, "upload", serde_json::json!({"host":"bastion","path":"/tmp/x","content":"hi","reason":"x"})).await;
    assert!(up.contains("read-only") && up.contains(r#""isError":true"#), "{up}");
    let ssh = call(8, "download", serde_json::json!({"host":"bastion","path":"~/.ssh/authorized_keys","reason":"x"})).await;
    assert!(ssh.contains("path:.ssh") && !ssh.contains("/home/tester") && !ssh.contains("127.0.0.1"), "{ssh}");
    let bad = post(serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}), "eth_wrong").await;
    assert!(bad.starts_with("HTTP/1.1 401"));
}

/// Pegar una imagen en una terminal SSH: queda en ~/.cache/ethereon/paste, solo para el
/// dueño, y cada pegado es un archivo nuevo.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita los contenedores de tests/ssh-fixture"]
async fn paste_image() {
    let key = std::fs::read_to_string(std::env::var("ETHEREON_TEST_KEY").expect("ETHEREON_TEST_KEY")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().to_path_buf(), Arc::new(Quiet));
    b.create("master-password-1").unwrap();
    let host = Host {
        name: "bastion".into(),
        hostname: "127.0.0.1".into(),
        port: 2222,
        user: "tester".into(),
        credential: "k".into(),
        ..Default::default()
    };
    b.write(|d| {
        d.credentials.push(cred("k", CredentialKind::PrivateKey, &key));
        d.hosts.push(host.clone());
        Ok(())
    })
    .unwrap();
    trust_all(&b, "bastion").await;
    let sftp = ethereon_core::files::Sftp::open(b.connection(&host).await.unwrap()).await.unwrap();
    let png = b"\x89PNG\r\n\x1a\nnot really";
    let path = sftp.paste_image(png).await.unwrap();
    println!("{path}");
    assert!(path.starts_with("/home/tester/.cache/ethereon/paste/paste-") && path.ends_with(".png"), "{path}");
    let s = sftp.session();
    assert_eq!(s.read(path.as_str()).await.unwrap(), png);
    assert_eq!(s.metadata(path.as_str()).await.unwrap().permissions.unwrap() & 0o777, 0o600);
    assert_eq!(s.metadata("/home/tester/.cache/ethereon/paste").await.unwrap().permissions.unwrap() & 0o777, 0o700);
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    assert_ne!(sftp.paste_image(png).await.unwrap(), path);
}

/// Un servidor que solo acepta los intercambios de claves de un F5 (ECDH NIST y los de
/// SHA-1, en el puerto 2223 del fixture): se conecta con ecdh-sha2-nistp256.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita los contenedores de tests/ssh-fixture"]
async fn legacy_kex() {
    let key = std::fs::read_to_string(std::env::var("ETHEREON_TEST_KEY").expect("ETHEREON_TEST_KEY")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().to_path_buf(), Arc::new(Quiet));
    b.create("master-password-1").unwrap();
    b.write(|d| {
        d.credentials.push(cred("k", CredentialKind::PrivateKey, &key));
        d.hosts.push(Host {
            name: "f5".into(),
            hostname: "127.0.0.1".into(),
            port: 2223,
            user: "tester".into(),
            credential: "k".into(),
            ..Default::default()
        });
        Ok(())
    })
    .unwrap();
    trust_all(&b, "f5").await;
    assert!(b.test_host("f5").await.unwrap().is_none());
}
