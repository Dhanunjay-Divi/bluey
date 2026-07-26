//! IPC for the meeting overlay — two complementary paths to the daemon.
//!
//! ## 1. Pull path (TCP) — [`DaemonLink`] + [`request`]
//! The overlay's `agent_*` commands (discovery / attach / sessions / connectors)
//! talk to the daemon's TCP IPC (`127.0.0.1:57321`, newline-delimited JSON
//! `DaemonRequest`/`DaemonResponse`) — the SAME contract the dashboard + CLI use.
//! We deliberately shuttle raw JSON `Value`s rather than depend on `cue-core`'s
//! types: the request/response shapes are stable and the UI's TS types already
//! match the wire, so the shell stays lean and decoupled.
//!
//! ## 2. Push / stream path (Unix socket) — [`start`] + [`overlay_send`]
//! The daemon spawns this overlay with `--bluey-overlay-socket <path>` and
//! `--bluey-overlay-session-token <token>` (also via the
//! `BLUEY_OVERLAY_SOCKET` / `BLUEY_OVERLAY_SESSION_TOKEN` env vars) and LISTENS on
//! that Unix socket. We CONNECT to it and:
//!   - read newline-delimited `cue_core::overlay::OverlayCommand` JSON from the
//!     daemon → forward each verbatim as a Tauri event `overlay://command` to the
//!     web UI (which parses the `"type"` tag and renders it — e.g. the
//!     `push_card` → `update_card`* answer-streaming flow);
//!   - take raw `OverlayEvent` JSON strings from the web UI (via the
//!     [`overlay_send`] invoke command) → stamp the session `"token"` and write
//!     them back to the daemon, `\n`-terminated.
//!
//! This mirrors the proven interview-overlay bridge in
//! `crates/cue-overlay-tauri/src/ipc.rs` exactly; the socket path is ADDITIVE to
//! the TCP pull path above. When no socket is passed (standalone dev), [`start`]
//! is a no-op and the UI runs on its mock adapter.

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

const DAEMON_ADDR: &str = "127.0.0.1:57321";

// ---------------------------------------------------------------------------
// Pull path (TCP) — DaemonLink + request
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Push / stream path (Unix socket) — EventSender + overlay_send + start
// ---------------------------------------------------------------------------

/// Tauri-managed channel so the [`overlay_send`] command can push UI events to
/// the socket-writer task.
pub struct EventSender(pub Mutex<Option<mpsc::UnboundedSender<String>>>);

/// JS calls this (via invoke) to send an `OverlayEvent` to the daemon. The arg is
/// the event as a JSON STRING (no double-encoding, unlike `event.emit`). The
/// socket writer injects the session `"token"` before sending.
#[tauri::command]
pub async fn overlay_send(state: tauri::State<'_, EventSender>, event: String) -> Result<(), ()> {
    let guard = state.0.lock().await;
    if let Some(tx) = guard.as_ref() {
        let _ = tx.send(event);
    }
    Ok(())
}

/// CLI args the daemon passes (mirrors the interview overlay's arg names).
pub struct IpcArgs {
    pub socket: Option<String>,
    pub token: String,
}

impl IpcArgs {
    pub fn from_process() -> Self {
        // CLI args take precedence; fall back to env vars (the daemon's
        // direct-exec launch path passes BLUEY_OVERLAY_SOCKET /
        // BLUEY_OVERLAY_SESSION_TOKEN as env, while the test harness uses the
        // --bluey-overlay-socket flags).
        let mut socket = std::env::var("BLUEY_OVERLAY_SOCKET")
            .ok()
            .filter(|s| !s.is_empty());
        let mut token = std::env::var("BLUEY_OVERLAY_SESSION_TOKEN").unwrap_or_default();
        let args: Vec<String> = std::env::args().collect();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--bluey-overlay-socket" => {
                    socket = args.get(i + 1).cloned();
                    i += 1;
                }
                "--bluey-overlay-session-token" => {
                    token = args.get(i + 1).cloned().unwrap_or_default();
                    i += 1;
                }
                _ => {}
            }
            i += 1;
        }
        IpcArgs { socket, token }
    }
}

/// Start the socket bridge. If no socket was passed (e.g. launched standalone for
/// dev), this is a no-op and the UI runs on whatever it renders by default (its
/// mock adapter).
pub fn start(app: &AppHandle, args: IpcArgs) {
    let Some(socket_path) = args.socket.clone() else {
        eprintln!("[meeting-overlay] no --bluey-overlay-socket; running UI-only (no daemon)");
        return;
    };
    let token = args.token.clone();
    let app = app.clone();

    // Channel: UI events (raw JSON strings) → socket writer. The sender is held
    // in Tauri state so the `overlay_send` invoke command can push to it.
    let (event_tx, event_rx) = mpsc::unbounded_channel::<String>();
    if let Some(state) = app.try_state::<EventSender>() {
        if let Ok(mut guard) = state.0.try_lock() {
            *guard = Some(event_tx.clone());
        }
    }

    // Spawn the async connection on a dedicated tokio runtime thread.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("meeting overlay ipc runtime");
        rt.block_on(async move {
            run_connection(app, socket_path, token, event_rx).await;
        });
    });
}

async fn run_connection(
    app: AppHandle,
    socket_path: String,
    token: String,
    mut event_rx: mpsc::UnboundedReceiver<String>,
) {
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[meeting-overlay] failed to connect IPC socket {socket_path}: {e}");
            return;
        }
    };
    eprintln!("[meeting-overlay] connected to daemon socket {socket_path}");
    let (read_half, write_half) = stream.into_split();
    let write_half = Arc::new(Mutex::new(write_half));

    // Tell the daemon we're up (Ready event).
    {
        let ready = serde_json::json!({
            "type": "ready", "platform": "macos", "capture_excluded": true,
        });
        send_event(&write_half, &token, ready.to_string()).await;
    }

    // Writer task: drain UI events → socket (token-stamped).
    let writer = {
        let write_half = write_half.clone();
        let token = token.clone();
        tokio::spawn(async move {
            while let Some(raw) = event_rx.recv().await {
                send_event(&write_half, &token, raw).await;
            }
        })
    };

    // Reader: daemon commands → Tauri event to the UI.
    let mut lines = BufReader::new(read_half).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                #[cfg(target_os = "macos")]
                if line.contains("\"show_meeting_banner\"") {
                    let app_handle = app.clone();
                    let _ = app_handle.clone().run_on_main_thread(move || {
                        #[allow(deprecated)]
                        use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
                        use tauri_nspanel::WebviewWindowExt;
                        if let Some(w) = app_handle.get_webview_window("banner") {
                            let _ = w.show();
                            let _ = w.set_always_on_top(true);
                            if let Ok(panel) = w.to_panel() {
                                panel.set_level(4);
                                panel.set_style_mask(0 | (1 << 7));
                                #[allow(deprecated)]
                                panel.set_collection_behaviour(
                                    NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
                                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces,
                                );
                                panel.show();
                                panel.order_front_regardless();
                            }
                        }
                    });
                } else if line.contains("\"show\"") || line.contains("\"toggle\"") || line.contains("\"boot\"") {
                    let app_handle = app.clone();
                    let _ = app_handle.clone().run_on_main_thread(move || {
                        #[allow(deprecated)]
                        use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
                        use tauri_nspanel::WebviewWindowExt;
                        if let Some(w) = app_handle.get_webview_window("meeting") {
                            let _ = w.show();
                            let _ = w.set_always_on_top(true);
                            if let Ok(panel) = w.to_panel() {
                                panel.set_level(4);
                                panel.set_style_mask(0 | (1 << 7));
                                #[allow(deprecated)]
                                panel.set_collection_behaviour(
                                    NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
                                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces,
                                );
                                panel.show();
                                panel.order_front_regardless();
                            }
                            // LOCAL-TEST ONLY: to_panel() re-hides the window from
                            // screen capture (NSPanel default), overriding the
                            // startup skip of set_sharing_none. When the escape
                            // hatch is on, positively re-assert capture visibility
                            // AFTER the panel conversion so the overlay actually
                            // shows in Zoom/Teams/Meet. Never runs in production
                            // (the env var must never be set there).
                            let capture_visible = std::env::var(
                                "BLUEY_MEETING_CAPTURE_VISIBLE",
                            )
                            .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
                            .unwrap_or(false);
                            if capture_visible {
                                let _ = w.set_content_protected(false);
                                crate::macos::set_sharing_read_only(&app_handle);
                            }
                        }
                    });
                } else if line.contains("\"hide\"") {
                    let app_handle = app.clone();
                    let _ = app_handle.clone().run_on_main_thread(move || {
                        use tauri_nspanel::ManagerExt;
                        if let Ok(panel) = app_handle.get_webview_panel("meeting") {
                            panel.order_out(None);
                        } else if let Some(w) = app_handle.get_webview_window("meeting") {
                            let _ = w.hide();
                        }
                    });
                }
                // Forward the raw command JSON to the web UI; it parses `type`.
                let _ = app.emit("overlay://command", line.to_string());
            }
            Ok(None) => {
                eprintln!("[meeting-overlay] daemon socket closed (EOF)");
                break;
            }
            Err(e) => {
                eprintln!("[meeting-overlay] socket read error: {e}");
                break;
            }
        }
    }
    writer.abort();
    // The daemon is gone (clean shutdown OR crash/kill). The overlay is a child
    // launched via `open` and reparented to launchd, so it does NOT die with the
    // daemon on its own — without this it lingers as a zombie window and stale
    // overlays pile up across daemon restarts. Exit the process so exactly one
    // overlay is alive per daemon: a fresh daemon spawns a fresh overlay.
    eprintln!("[meeting-overlay] daemon connection lost — exiting overlay");
    app.exit(0);
}

/// Write one event: take the UI's JSON object string, inject `"token"`, append `\n`.
async fn send_event(
    write_half: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    token: &str,
    raw_json: String,
) {
    // Parse, inject token, re-serialize (so the token can't be spoofed/omitted).
    let line = match serde_json::from_str::<serde_json::Value>(&raw_json) {
        Ok(serde_json::Value::Object(mut obj)) => {
            if !token.is_empty() {
                obj.insert("token".into(), serde_json::Value::String(token.to_string()));
            }
            match serde_json::to_string(&serde_json::Value::Object(obj)) {
                Ok(s) => s,
                Err(_) => return,
            }
        }
        _ => {
            eprintln!("[meeting-overlay] dropping non-object UI event: {raw_json}");
            return;
        }
    };
    let mut guard = write_half.lock().await;
    let _ = guard.write_all(line.as_bytes()).await;
    let _ = guard.write_all(b"\n").await;
    let _ = guard.flush().await;
}
