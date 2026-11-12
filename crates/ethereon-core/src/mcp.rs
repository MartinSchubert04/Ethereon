//! Servidor MCP (transporte "Streamable HTTP").
//!
//! Casi todo se contesta con JSON. `tools/call` se contesta como SSE cuando el cliente lo
//! acepta: Claude Code corta un POST cuyas cabeceras no llegan en 60 s, y una aprobacion
//! puede esperar minutos. Con SSE las cabeceras salen enseguida, cada `PROGRESS_EVERY`
//! va un `notifications/progress` (Claude Code reinicia con eso su limite de inactividad,
//! 5 min) y al final la respuesta. Si el agente corta, se suelta la llamada y la
//! aprobacion pendiente se retira (ver `Broker::wait_approval`).
//!
//! Claude Code se conecta con:
//!   claude mcp add --transport http ethereon http://127.0.0.1:47321/mcp \
//!     --header "Authorization: Bearer eth_..."
//!
//! Herramientas: `list_hosts`, `run`, y `download` / `upload` (archivos por SFTP, solo en
//! los hosts SSH que lo tienen prendido en sus reglas; ver `transfer.rs`). No hay ninguna
//! para desbloquear, leer secretos ni cambiar reglas: eso no existe del lado del agente.

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use futures_util::StreamExt;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::audit::Entry;
use crate::broker::{AgentCtx, Broker};
use crate::Error;

/// Cada cuanto avisa que una llamada larga sigue viva.
const PROGRESS_EVERY: Duration = Duration::from_secs(15);

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Ethereon runs commands on the user's hosts for you: SSH servers and devices on a serial \
console cable (switches, routers, access points). You never get passwords, keys, IPs or port names: call list_hosts \
to see what exists (with a description of each host), then run(host, command). \
Output has secrets replaced by ***. Each host has a mode: read_only (only listed commands), approval (a person \
approves each command: always give a clear `reason`), guarded (anything except forbidden commands like rm -rf or \
reading ~/.ssh) or unrestricted (anything). \
On a host with transport \"serial\" there is no shell and no exit code: send the device's own CLI commands one at a \
time (for example `show version`), and you get the output up to the next prompt, plus that prompt (`Switch#`). \
Pagers are handled for you. Commands that wait for a confirmation ([confirm], yes/no) time out: avoid them. \
Hosts that list_hosts shows with \"files\": true also take download(host, path) and upload(host, path, content or \
local_path) over SFTP, with the same mode and forbidden paths as run (read_only hosts: download only). download \
returns the text with secrets replaced by ***; binary files are refused. Never upload a file you edited from a \
download that had masked secrets: the *** would replace the real ones. \
If a command or transfer is rejected, do not try to get around the rule; tell the user.";

pub fn router(b: Arc<Broker>) -> Router {
    Router::new()
        .route("/mcp", post(handle).get(|| async { StatusCode::METHOD_NOT_ALLOWED }).delete(|| async { StatusCode::OK }))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(b)
}

pub async fn serve(b: Arc<Broker>, addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(b).into_make_service_with_connect_info::<SocketAddr>()).await
}

fn tools() -> Value {
    json!([
        {
            "name": "list_hosts",
            "description": "List the hosts you can use: name, transport (ssh, or serial for a console cable), group, \
                environment (prod/test/dev), tags, a description of what runs there, the rule mode (read_only, approval, \
                guarded, unrestricted) and whether sudo is available.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "run",
            "description": "Run a command on a host and get stdout, stderr, the exit code and the duration. On a serial \
                host there is no exit code: you get the device's output up to its prompt, and the prompt itself. \
                Secrets in the output are replaced by ***. The whole command is checked (pipes, ;, $( ), redirects). \
                On hosts in approval mode this waits until a person approves it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": { "type": "string", "description": "Host name from list_hosts." },
                    "command": { "type": "string", "description": "Shell command (sh syntax, no heredocs), or the device's CLI command on a serial host." },
                    "reason": { "type": "string", "description": "Why you need this; shown to the person who approves it." },
                    "sudo": { "type": "boolean", "description": "Run as root. Ethereon types the sudo password; never call sudo yourself.", "default": false }
                },
                "required": ["host", "command", "reason"],
                "additionalProperties": false
            }
        },
        {
            "name": "download",
            "description": "Read a text file from an SSH host over SFTP (only hosts that list_hosts shows with \"files\": true). \
                You get its content in the response, with secrets replaced by ***; nothing is written on this PC. Binary \
                files, folders and files over the host's size limit are refused, and so are forbidden paths (like ~/.ssh). \
                Relative paths and ~ start at the login user's home. On hosts in approval mode this waits until a person approves it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": { "type": "string", "description": "Host name from list_hosts." },
                    "path": { "type": "string", "description": "Path of the file on the host." },
                    "reason": { "type": "string", "description": "Why you need this file; shown to the person who approves it." }
                },
                "required": ["host", "path", "reason"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "upload",
            "description": "Write a file on an SSH host over SFTP (only hosts that list_hosts shows with \"files\": true, and not \
                read_only ones). Give the text in `content`, or a file on this PC in `local_path` (absolute path; folders with \
                credentials, keys, .env files and Ethereon's own data are refused). Nothing that contains a secret stored in \
                Ethereon or a private key is uploaded. An existing file is only replaced with `overwrite: true`. On hosts in \
                approval mode this waits until a person approves it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": { "type": "string", "description": "Host name from list_hosts." },
                    "path": { "type": "string", "description": "Path of the file to write on the host. Its folder must exist." },
                    "content": { "type": "string", "description": "The file's text. Use this or local_path." },
                    "local_path": { "type": "string", "description": "Absolute path of a file on this PC to upload. Use this or content." },
                    "overwrite": { "type": "boolean", "description": "Replace the file if it already exists.", "default": false },
                    "reason": { "type": "string", "description": "Why you need this; shown to the person who approves it." }
                },
                "required": ["host", "path", "reason"],
                "additionalProperties": false
            }
        }
    ])
}

fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_err(id: &Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

fn tool_text(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

async fn handle(
    State(b): State<Arc<Broker>>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    // Una pagina web no puede hablarle al broker (proteccion contra DNS rebinding).
    if let Some(origin) = headers.get("origin").and_then(|o| o.to_str().ok()) {
        let local = ["http://127.0.0.1", "http://localhost", "https://127.0.0.1", "https://localhost"];
        if !local.iter().any(|p| origin.starts_with(p)) {
            return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
        }
    }
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .unwrap_or("")
        .trim()
        .to_string();
    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, "missing device token").into_response();
    }
    let session = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let client = peer.ip().to_string();

    // Con el vault abierto se sabe si el token es valido; bloqueado no, y se deja pasar
    // hasta la herramienta, que responde "bloqueado" (un 401 haria que Claude Code
    // intente un login OAuth que no existe).
    let ctx = match b.authenticate(&token, &client, &session) {
        Ok(c) => Some(c),
        Err(Error::Locked) => None,
        Err(e) => return (StatusCode::UNAUTHORIZED, e.to_string()).into_response(),
    };

    let sse = headers.get("accept").and_then(|v| v.to_str().ok()).is_some_and(|a| a.contains("text/event-stream"));
    if sse && body.get("method").and_then(|m| m.as_str()) == Some("tools/call") && body.get("id").is_some() {
        return stream_call(b, ctx, session, client, body);
    }

    let mut new_session = None;
    let reply = if let Value::Array(items) = body {
        let mut out = Vec::new();
        for it in items {
            if let Some(r) = one(&b, ctx.as_ref(), &session, &client, it, &mut new_session).await {
                out.push(r);
            }
        }
        (!out.is_empty()).then_some(Value::Array(out))
    } else {
        one(&b, ctx.as_ref(), &session, &client, body, &mut new_session).await
    };
    let mut resp = match reply {
        Some(v) => Json(v).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    };
    if let Some(s) = new_session {
        resp.headers_mut().insert("mcp-session-id", HeaderValue::from_str(&s).unwrap());
    }
    resp
}

/// Contesta un `tools/call` como SSE: progreso cada `PROGRESS_EVERY` y despues la respuesta.
fn stream_call(b: Arc<Broker>, ctx: Option<AgentCtx>, session: String, client: String, msg: Value) -> Response {
    let token = msg.pointer("/params/_meta/progressToken").cloned();
    let work: Pin<Box<dyn Future<Output = Option<Value>> + Send>> = Box::pin(async move {
        one(&b, ctx.as_ref(), &session, &client, msg, &mut None).await
    });
    let tick = tokio::time::interval_at(tokio::time::Instant::now() + PROGRESS_EVERY, PROGRESS_EVERY);
    let events = futures_util::stream::unfold(Some((work, tick, 0u64)), move |state| {
        let token = token.clone();
        async move {
            let (mut work, mut tick, n) = state?;
            tokio::select! {
                reply = &mut work => reply.map(|v| (format!("event: message
data: {v}

"), None)),
                _ = tick.tick() => {
                    let n = n + 1;
                    let ev = match token {
                        Some(t) => {
                            let p = json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": { "progressToken": t, "progress": n } });
                            format!("event: message
data: {p}

")
                        }
                        // Sin token no hay progreso que mandar; un comentario mantiene viva la conexion.
                        None => ": waiting

".to_string(),
                    };
                    Some((ev, Some((work, tick, n))))
                }
            }
        }
    });
    let mut resp = axum::body::Body::from_stream(events.map(Ok::<_, Infallible>)).into_response();
    let h = resp.headers_mut();
    h.insert("content-type", HeaderValue::from_static("text/event-stream"));
    h.insert("cache-control", HeaderValue::from_static("no-cache"));
    resp
}

async fn one(b: &Broker, ctx: Option<&AgentCtx>, session: &str, client: &str, msg: Value, new_session: &mut Option<String>) -> Option<Value> {
    let id = msg.get("id").cloned()?; // sin id: notificacion, no se contesta
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOLS[0]);
            let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
            *new_session = Some(hex::encode(crate::vault::random::<16>()));
            rpc_ok(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "ethereon", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS
                }),
            )
        }
        "ping" => rpc_ok(&id, json!({})),
        "tools/list" => rpc_ok(&id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            rpc_ok(&id, call(b, ctx, session, client, name, &args).await)
        }
        "resources/list" => rpc_ok(&id, json!({ "resources": [] })),
        "prompts/list" => rpc_ok(&id, json!({ "prompts": [] })),
        _ => rpc_err(&id, -32601, &format!("method not found: {method}")),
    })
}

async fn call(b: &Broker, ctx: Option<&AgentCtx>, _session: &str, client: &str, name: &str, args: &Value) -> Value {
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let Some(ctx) = ctx else {
        let command = match name {
            "run" => Some(s("command")),
            "download" | "upload" => Some(format!("{name} {}", s("path"))),
            _ => None,
        };
        if let Some(command) = command {
            b.audit_log(Entry {
                device: "unknown (vault locked)".into(),
                client: client.into(),
                host: s("host"),
                command,
                decision: "locked".into(),
                ..Default::default()
            });
        }
        return tool_text("The Ethereon vault is locked. Ask the user to unlock it in the Ethereon app.".into(), true);
    };
    match name {
        "list_hosts" => match b.agent_list_hosts(ctx) {
            Ok(list) => tool_text(serde_json::to_string_pretty(&list).unwrap(), false),
            Err(e) => tool_text(e.to_string(), true),
        },
        "run" => {
            let (host, command, reason) = (s("host"), s("command"), s("reason"));
            let sudo = args.get("sudo").and_then(|v| v.as_bool()).unwrap_or(false);
            if host.is_empty() || command.is_empty() {
                return tool_text("`host` and `command` are required".into(), true);
            }
            match b.agent_run(ctx, &host, &command, sudo, &reason).await {
                Ok(r) => {
                    // Sin codigo de salida (se corto, vencio el tiempo) es un error de la
                    // herramienta; un codigo distinto de 0 es un resultado. En serial no hay
                    // codigo: lo que dice que termino es el prompt.
                    let broken = r.exit_code.is_none() && r.prompt.is_none();
                    tool_text(serde_json::to_string_pretty(&r).unwrap(), broken)
                }
                Err(e) => tool_text(e.to_string(), true),
            }
        }
        "download" => {
            let (host, path, reason) = (s("host"), s("path"), s("reason"));
            if host.is_empty() || path.is_empty() {
                return tool_text("`host` and `path` are required".into(), true);
            }
            match b.agent_download(ctx, &host, &path, &reason).await {
                Ok(r) => tool_text(serde_json::to_string_pretty(&r).unwrap(), false),
                Err(e) => tool_text(e.to_string(), true),
            }
        }
        "upload" => {
            let (host, path, reason) = (s("host"), s("path"), s("reason"));
            if host.is_empty() || path.is_empty() {
                return tool_text("`host` and `path` are required".into(), true);
            }
            let content = args.get("content").and_then(|v| v.as_str());
            let local = args.get("local_path").and_then(|v| v.as_str());
            let overwrite = args.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false);
            match b.agent_upload(ctx, &host, &path, content, local, overwrite, &reason).await {
                Ok(r) => tool_text(serde_json::to_string_pretty(&r).unwrap(), false),
                Err(e) => tool_text(e.to_string(), true),
            }
        }
        _ => tool_text(format!("unknown tool `{name}`"), true),
    }
}
