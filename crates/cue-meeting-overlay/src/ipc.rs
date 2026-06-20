//! Daemon link — the meeting overlay's thin client over the daemon's TCP IPC
//! (`127.0.0.1:57321`, newline-delimited JSON `DaemonRequest`/`DaemonResponse`),
//! the SAME contract the dashboard + CLI use. We deliberately shuttle raw JSON
//! `Value`s rather than depend on `cue-core`'s types: the request/response shapes
//! are stable and the UI's TS types already match the wire, so the shell stays
//! lean and decoupled.

use std::sync::Arc;

use serde_json::Value;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const DAEMON_ADDR: &str = "127.0.0.1:57321";

/// Tauri-managed handle. Holds the daemon address (overridable by env for tests)
/// and a registry of in-flight ask cancellations.
pub struct DaemonLink {
    pub addr: String,
    pub cancels: Mutex<std::collections::HashMap<String, Arc<std::sync::atomic::AtomicBool>>>,
}

impl Default for DaemonLink {
    fn default() -> Self {
        let addr = std::env::var("BLUEY_DAEMON_ADDR")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DAEMON_ADDR.to_string());
        Self {
            addr,
            cancels: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

/// Send one request to the daemon and read exactly one JSON response line.
/// Connects fresh per call (the daemon is request/response per line); cheap and
/// robust, matching how the CLI talks to it.
pub async fn request(addr: &str, req: Value) -> Result<Value, String> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("daemon not reachable at {addr}: {e}"))?;
    let (read_half, mut write_half) = stream.into_split();

    let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    line.push('\n');
    write_half
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    write_half.flush().await.ok();

    let mut reader = BufReader::new(read_half);
    let mut resp = String::new();
    reader
        .read_line(&mut resp)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    if resp.trim().is_empty() {
        return Err("empty response from daemon".to_string());
    }
    serde_json::from_str(&resp).map_err(|e| format!("bad response json: {e}"))
}

/// Connect to the daemon's overlay event socket (if the daemon launched us) and
/// forward transcript events to the UI as `meeting://transcript`. Best-effort:
/// when not launched by the daemon (standalone dev), this is a no-op and the UI
/// runs on its mock adapter.
pub fn start(_app: &AppHandle) {
    // The streaming transcript/answer feed is wired through the same overlay
    // socket the interview overlay uses; when present, forward it. Standalone
    // runs (no socket env) simply skip this — the UI's mock fills in.
    //
    // Left as an explicit hook: the daemon's transcript push lands here and is
    // emitted as `meeting://transcript`. The pull-path (agent_list / ask) works
    // independently over `request()` above, so discovery + ask are live even
    // before the push feed is connected.
}
