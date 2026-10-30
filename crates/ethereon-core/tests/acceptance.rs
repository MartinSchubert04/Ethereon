//! Pruebas de aceptacion de la spec contra servidores SSH reales (tests/ssh-fixture).
//!
//!   bash crates/ethereon-core/tests/ssh-fixture/up.sh <carpeta>
//!   ETHEREON_TEST_KEY=<carpeta>/key cargo test -p ethereon-core --test acceptance -- --ignored --nocapture

use std::sync::Arc;
use ethereon_core::broker::Decision;
use ethereon_core::model::*;
use ethereon_core::ssh::SshError;
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
    let bad = post(serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}), "eth_wrong").await;
    assert!(bad.starts_with("HTTP/1.1 401"));
}
