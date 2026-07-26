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

/// The last `show_meeting_banner` command JSON, stored so the banner webview can
/// PULL it on mount via [`get_pending_banner`]. Event delivery (`emit`/`emit_to`)
/// to the banner webview proved unreliable — the banner window is `visible:false`
/// at boot and its webview's event channel isn't wired when the daemon's
/// show-banner burst fires, so 0 commands ever reached it (measured on-screen).
/// A pull command sidesteps event timing entirely: the webview asks for the
/// pending banner the moment its JS runs. Cleared on hide.
pub struct PendingBanner(pub std::sync::Mutex<Option<String>>);

/// The banner webview calls this on mount to fetch the current meeting-prep
/// banner (the raw `show_meeting_banner` JSON line, same shape the event bus
/// would have delivered). Returns `None` when no banner is pending.
#[tauri::command]
pub fn get_pending_banner(state: tauri::State<'_, PendingBanner>) -> Option<String> {
    state.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

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
                if line.contains("\"show_meeting_banner\"") {
                    // Store for the pull-based get_pending_banner command (the
                    // reliable delivery path — see PendingBanner docs).
                    {
                        use tauri::Manager;
                        let pending = app.state::<PendingBanner>();
                        *pending.0.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some(line.to_string());
                    }
                }
                #[cfg(target_os = "macos")]
                if line.contains("\"show_meeting_banner\"") {
                    // GUARANTEED DELIVERY to the banner webview: the broadcast
                    // `app.emit` below races the banner webview's listener (the
                    // webview only starts loading when the window is first shown,
                    // which is triggered by THIS very command), so the banner never
                    // received any command (measured: 0 commands seen). Re-emit the
                    // command DIRECTLY to the "banner" window on a short delay, a
                    // few times, so it lands after the webview's listener is up.
                    {
                        let app_re = app.clone();
                        let payload = line.to_string();
                        std::thread::spawn(move || {
                            for _ in 0..5 {
                                std::thread::sleep(
                                    std::time::Duration::from_millis(400),
                                );
                                let _ = app_re.emit_to(
                                    "banner",
                                    "overlay://command",
                                    payload.clone(),
                                );
                            }
                        });
                    }
                    let app_handle = app.clone();
                    let _ = app_handle.clone().run_on_main_thread(move || {
                        #[allow(deprecated)]
                        use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
                        use tauri_nspanel::WebviewWindowExt;
                        let Some(w) = app_handle.get_webview_window("banner") else {
                            return;
                        };
                        {
                            // Pin to the TOP-RIGHT of the current monitor like a
                            // real system notification, instead of the hardcoded
                            // x:980 (which assumes a screen width and can land
                            // under the meeting overlay). 20px inset from the
                            // top-right corner.
                            match w.current_monitor() {
                                Ok(Some(monitor)) => {
                                    let scale = monitor.scale_factor();
                                    let screen = monitor.size().to_logical::<f64>(scale);
                                    let inset = 20.0;
                                    let banner_w = 360.0;
                                    let x = (screen.width - banner_w - inset).max(inset);
                                    let _ = w.set_position(tauri::LogicalPosition::new(
                                        x, inset,
                                    ));
                                }
                                _ => {}
                            }
                            // DO NOT call w.show() — Tauri's show() activates the
                            // app (makeKeyAndOrderFront) which yanks the user to the
                            // app's Space / away from their fullscreen tab. Instead
                            // configure the panel FIRST, then order it front WITHOUT
                            // activating. Set collection behavior BEFORE ordering so
                            // it lands on the CURRENT space, never switching spaces.
                            let _ = w.set_always_on_top(true);
                            if let Ok(panel) = w.to_panel() {
                                // Non-activating panel (bit 7) so it never becomes
                                // key / steals focus.
                                panel.set_style_mask(0 | (1 << 7));
                                // Level 5 — ABOVE the meeting overlay (level 4).
                                panel.set_level(5);
                                // Appear on the user's CURRENT space without moving
                                // them: CanJoinAllSpaces = show on whatever space is
                                // active; Stationary + IgnoresCycle keep it out of
                                // space-switch animation and Exposé cycling. No
                                // FullScreenAuxiliary (it forced a space change).
                                #[allow(deprecated)]
                                panel.set_collection_behaviour(
                                    NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle,
                                );
                                // order_front_regardless surfaces the panel WITHOUT
                                // activating the app or switching the Space.
                                panel.order_front_regardless();
                            } else {
                                // Fallback if panel conversion fails: at least show.
                                let _ = w.show();
                            }
                            // Kill the native window shadow/border that to_panel()
                            // re-adds — the banner is a transparent rounded card, so
                            // a grey window-rect shadow around it looks broken. Set
                            // it DIRECTLY on the banner's NSWindow (clear_window_chrome
                            // ran at boot but to_panel() re-added the shadow).
                            #[allow(unexpected_cfgs)]
                            if let Ok(ptr) = w.ns_window() {
                                use objc2::msg_send;
                                use objc2::runtime::AnyObject;
                                let ns = ptr as *mut AnyObject;
                                if !ns.is_null() {
                                    unsafe {
                                        let _: () = msg_send![ns, setHasShadow: false];
                                        let _: () = msg_send![ns, invalidateShadow];
                                    }
                                }
                            }
                            crate::macos::clear_window_chrome(&app_handle);
                            // LOCAL-TEST ONLY: same to_panel() re-hide fix as the
                            // meeting window — re-assert capture visibility for the
                            // banner so it shows in screenshare when the escape
                            // hatch is on. (The banner is user-facing; its
                            // contentProtected=true default hides it from capture.)
                            let capture_visible = std::env::var(
                                "BLUEY_MEETING_CAPTURE_VISIBLE",
                            )
                            .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
                            .unwrap_or(false);
                            if capture_visible {
                                let _ = w.set_content_protected(false);
                                crate::macos::set_sharing_read_only(&app_handle);
                            }
                            eprintln!(
                                "[banner] final is_visible={:?}",
                                w.is_visible()
                            );
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
