//! Servidor MCP (transporte "Streamable HTTP", respuestas JSON sin SSE).
//!
//! Claude Code se conecta con:
//!   claude mcp add --transport http ethereon http://127.0.0.1:47321/mcp \
//!     --header "Authorization: Bearer eth_..."
//!
//! Herramientas: `list_hosts` y `run`. No hay ninguna para desbloquear, leer secretos ni
//! cambiar reglas: eso no existe del lado del agente.

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::audit::Entry;
use crate::broker::{AgentCtx, Broker};
use crate::Error;

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Ethereon runs shell commands on the user's SSH hosts for you. You never get passwords, keys \
or IPs: call list_hosts to see what exists (with a description of each host), then run(host, command). \
Output has secrets replaced by ***. Each host has a mode: read_only (only listed commands), approval (a person \
approves each command: always give a clear `reason`), guarded (anything except forbidden commands like rm -rf or \
reading ~/.ssh) or unrestricted (anything). \
If a command is rejected, do not try to get around the rule; tell the user.";

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
            "description": "List the SSH hosts you can use: name, group, environment (prod/test/dev), tags, a description \
                of what runs there, the rule mode (read_only, approval, guarded, unrestricted) and whether sudo is available.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "run",
            "description": "Run a shell command on a host and get stdout, stderr, the exit code and the duration. \
                Secrets in the output are replaced by ***. The whole command is checked (pipes, ;, $( ), redirects). \
                On hosts in approval mode this waits until a person approves it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": { "type": "string", "description": "Host name from list_hosts." },
                    "command": { "type": "string", "description": "Shell command (sh syntax). No heredocs." },
                    "reason": { "type": "string", "description": "Why you need this; shown to the person who approves it." },
                    "sudo": { "type": "boolean", "description": "Run as root. Ethereon types the sudo password; never call sudo yourself.", "default": false }
                },
                "required": ["host", "command", "reason"],
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
    let Some(ctx) = ctx else {
        if name == "run" {
            b.audit_log(Entry {
                device: "unknown (vault locked)".into(),
                client: client.into(),
                host: args.get("host").and_then(|v| v.as_str()).unwrap_or("").into(),
                command: args.get("command").and_then(|v| v.as_str()).unwrap_or("").into(),
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
            let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let (host, command, reason) = (s("host"), s("command"), s("reason"));
            let sudo = args.get("sudo").and_then(|v| v.as_bool()).unwrap_or(false);
            if host.is_empty() || command.is_empty() {
                return tool_text("`host` and `command` are required".into(), true);
            }
            match b.agent_run(ctx, &host, &command, sudo, &reason).await {
                Ok(r) => {
                    let failed = r.exit_code != Some(0);
                    tool_text(serde_json::to_string_pretty(&r).unwrap(), failed && r.exit_code.is_none())
                }
                Err(e) => tool_text(e.to_string(), true),
            }
        }
        _ => tool_text(format!("unknown tool `{name}`"), true),
    }
}
