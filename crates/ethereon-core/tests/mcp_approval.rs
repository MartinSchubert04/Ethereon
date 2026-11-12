//! Un `run` que espera aprobacion por MCP: las cabeceras SSE salen enseguida (Claude Code
//! corta a los 60 s un POST sin cabeceras), y si el agente corta, la aprobacion se retira.
//! No necesita los contenedores: la aprobacion se pide antes de conectar.

use ethereon_core::model::*;
use ethereon_core::{Broker, Notifier};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Quiet;
impl Notifier for Quiet {
    fn event(&self, _: &str, _: serde_json::Value) {}
    fn notify(&self, _: &str, _: &str) {}
}

#[tokio::test(flavor = "multi_thread")]
async fn approval_streams_and_withdraws_on_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().to_path_buf(), Arc::new(Quiet));
    b.create("master-password-1").unwrap();
    b.write(|d| {
        d.hosts.push(Host {
            name: "web".into(),
            hostname: "127.0.0.1".into(),
            port: 1,
            user: "tester".into(),
            policy: Some(Policy { mode: Mode::Approval, ..Default::default() }),
            ..Default::default()
        });
        Ok(())
    })
    .unwrap();
    let (_, token) = b.create_device("pc", None, vec![]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = ethereon_core::mcp::router(b.clone()).into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let body = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
        "name":"run","arguments":{"host":"web","command":"touch /tmp/x","reason":"test"},"_meta":{"progressToken":3}}})
    .to_string();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    s.write_all(req.as_bytes()).await.unwrap();

    // Las cabeceras llegan sin esperar a que alguien apruebe.
    let mut buf = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf)).await.expect("headers right away").unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).to_string();
    assert!(head.starts_with("HTTP/1.1 200") && head.contains("text/event-stream"), "{head}");
    assert_eq!(b.approvals().len(), 1, "the command waits for a person");

    // El agente corta: la aprobacion no queda colgada en la interfaz.
    drop(s);
    let gone = async {
        while !b.approvals().is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), gone).await.expect("approval withdrawn after the agent disconnects");

    // Con la conexion abierta, la decision llega como evento y el stream termina.
    let req = req.replace("Content-Length", "Connection: close\r\nContent-Length");
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    s.write_all(req.as_bytes()).await.unwrap();
    let wait = async {
        loop {
            if let Some(a) = b.approvals().first() {
                break a.id;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let id = tokio::time::timeout(Duration::from_secs(5), wait).await.unwrap();
    b.resolve(id, ethereon_core::broker::Decision::Reject { note: Some("not now".into()) }).unwrap();
    let mut out = String::new();
    tokio::time::timeout(Duration::from_secs(5), s.read_to_string(&mut out)).await.unwrap().unwrap();
    assert!(out.contains("event: message\ndata: {") && out.contains("a person rejected this command: not now"), "{out}");
}
