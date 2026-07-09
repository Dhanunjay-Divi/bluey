//! HTTP + JSON-RPC dispatch for the Bluey MCP server.
//!
//! Implements exactly the Streamable-HTTP surface the Batch-0 spike proved
//! every target CLI exercises: POST JSON-RPC (`initialize`, `tools/list`,
//! `tools/call`, `ping`), 202 for notifications, 405 for the optional GET
//! SSE stream (clients tolerate it — verified live), and strict auth on
//! every request (Host allow-list + bearer token).

use std::sync::Arc;

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::RwLock;

use crate::{tools, MeetingMemorySource};

/// Serve one accepted TCP connection (HTTP/1.1, keep-alive supported).
pub(crate) async fn serve_connection(
    stream: TcpStream,
    source: Arc<dyn MeetingMemorySource>,
    token: Arc<RwLock<String>>,
) -> Result<()> {
    let io = TokioIo::new(stream);
    let service = service_fn(move |req: Request<Incoming>| {
        let source = Arc::clone(&source);
        let token = Arc::clone(&token);
        async move { Ok::<_, hyper::Error>(handle_request(req, source, token).await) }
    });
    hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .await?;
    Ok(())
}

fn respond(status: StatusCode, body: Value) -> Response<Full<Bytes>> {
    let bytes = Bytes::from(body.to_string());
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(bytes))
        .expect("static response")
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::new()))
        .expect("static response")
}

async fn handle_request(
    req: Request<Incoming>,
    source: Arc<dyn MeetingMemorySource>,
    token: Arc<RwLock<String>>,
) -> Response<Full<Bytes>> {
    // Host allow-list: meeting memory never answers a DNS-rebound origin.
    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !(host.starts_with("127.0.0.1") || host.starts_with("localhost")) {
        return respond(StatusCode::FORBIDDEN, json!({"error": "loopback only"}));
    }

    // Bearer token: rotated per meeting by the daemon.
    let expected = token.read().await.clone();
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {expected}") {
        return respond(StatusCode::UNAUTHORIZED, json!({"error": "invalid token"}));
    }

    match *req.method() {
        // Optional server-initiated SSE stream — we have none (verified: all
        // target CLIs tolerate 405 here and proceed over POST).
        Method::GET => empty(StatusCode::METHOD_NOT_ALLOWED),
        // Session teardown (stateless server: nothing to tear down).
        Method::DELETE => empty(StatusCode::OK),
        Method::POST => {
            let body = match req.into_body().collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return respond(StatusCode::BAD_REQUEST, json!({"error": "body"})),
            };
            let rpc: Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => return respond(StatusCode::BAD_REQUEST, json!({"error": "bad json"})),
            };
            dispatch_rpc(rpc, source).await
        }
        _ => empty(StatusCode::METHOD_NOT_ALLOWED),
    }
}

async fn dispatch_rpc(rpc: Value, source: Arc<dyn MeetingMemorySource>) -> Response<Full<Bytes>> {
    let method = rpc.get("method").and_then(Value::as_str).unwrap_or("");
    let id = rpc.get("id").cloned();

    // Notifications (no id) are acknowledged and ignored.
    let Some(id) = id.filter(|v| !v.is_null()) else {
        return empty(StatusCode::ACCEPTED);
    };

    let result: Value = match method {
        "initialize" => {
            // Echo the client's protocol version: we serve the stable core
            // surface (tools) that every negotiated version shares.
            let version = rpc
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-03-26");
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "bluey-memory", "version": env!("CARGO_PKG_VERSION") },
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools::tool_definitions() }),
        "tools/call" => {
            let name = rpc
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = rpc
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let outcome = tools::call_tool(source.as_ref(), name, args).await;
            match outcome {
                Ok(text) => json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false,
                }),
                Err(message) => json!({
                    "content": [{ "type": "text", "text": message }],
                    "isError": true,
                }),
            }
        }
        other => {
            return respond(
                StatusCode::OK,
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("unknown method {other}") },
                }),
            )
        }
    };

    respond(
        StatusCode::OK,
        json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )
}
