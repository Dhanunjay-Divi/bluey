//! IPC bridge between the daemon and the web UI.
//!
//! The daemon spawns this overlay with `--bluey-overlay-socket <path>` and
//! `--bluey-overlay-session-token <token>`, then LISTENS on that Unix socket.
//! We CONNECT to it and:
//!   - read newline-delimited `OverlayCommand` JSON from the daemon → forward
//!     each as a Tauri event `overlay://command` to the web UI (which renders it);
//!   - take `overlay://event` payloads from the web UI (raw OverlayEvent JSON)
//!     → stamp the session `token` and write them back to the daemon.
//!
//! Wire format matches the old Swift overlay exactly: each event line is the
//! OverlayEvent JSON object with a `"token"` field added, `\n`-terminated.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

/// Tauri-managed channel so the `overlay_send` command can push UI events to the
/// socket-writer task.
pub struct EventSender(pub Mutex<Option<mpsc::UnboundedSender<String>>>);

/// JS calls this (via invoke) to send an OverlayEvent to the daemon. The arg is
/// the event as a JSON STRING (no double-encoding, unlike event.emit).
#[tauri::command]
pub async fn overlay_send(state: tauri::State<'_, EventSender>, event: String) -> Result<(), ()> {
    let guard = state.0.lock().await;
    if let Some(tx) = guard.as_ref() {
        let _ = tx.send(event);
    }
    Ok(())
}

/// CLI args the daemon passes (mirrors the Swift overlay's arg names).
pub struct IpcArgs {
    pub socket: Option<String>,
    pub token: String,
}

impl IpcArgs {
    pub fn from_process() -> Self {
        // CLI args take precedence; fall back to env vars (the daemon's direct-exec
        // launch path passes BLUEY_OVERLAY_SOCKET / BLUEY_OVERLAY_SESSION_TOKEN as
        // env, while the test harness uses --bluey-overlay-socket flags).
        let mut socket = std::env::var("BLUEY_OVERLAY_SOCKET").ok().filter(|s| !s.is_empty());
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

/// Start the bridge. If no socket was passed (e.g. launched standalone for dev),
/// this is a no-op and the UI runs on whatever it renders by default.
pub fn start(app: &AppHandle, args: IpcArgs) {
    let Some(socket_path) = args.socket.clone() else {
        eprintln!("[overlay] no --bluey-overlay-socket; running UI-only (no daemon)");
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
            .expect("overlay ipc runtime");
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
            eprintln!("[overlay] failed to connect IPC socket {socket_path}: {e}");
            return;
        }
    };
    eprintln!("[overlay] connected to daemon socket {socket_path}");
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
                // Forward the raw command JSON to the web UI; it parses `type`.
                let _ = app.emit("overlay://command", line.to_string());
            }
            Ok(None) => {
                eprintln!("[overlay] daemon socket closed (EOF)");
                break;
            }
            Err(e) => {
                eprintln!("[overlay] socket read error: {e}");
                break;
            }
        }
    }
    writer.abort();
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
            eprintln!("[overlay] dropping non-object UI event: {raw_json}");
            return;
        }
    };
    let mut guard = write_half.lock().await;
    let _ = guard.write_all(line.as_bytes()).await;
    let _ = guard.write_all(b"\n").await;
    let _ = guard.flush().await;
}
