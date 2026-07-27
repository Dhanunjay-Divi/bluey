//! Tauri commands — the bridge the UI's `tauriClient.ts` calls. Each maps one
//! MeetingClient method to the daemon's agent IPC (the proven contract), returns
//! the typed array straight through, and `meeting_ask` streams answer chunks back
//! to the UI over Tauri events.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::ipc::{request, DaemonLink, EventSender};

/// Pull a named array field out of a `DaemonResponse` Value, or surface the
/// daemon's `error` message.
fn array_field(resp: Value, field: &str) -> Result<Value, String> {
    if resp.get("type").and_then(Value::as_str) == Some("error") {
        return Err(resp
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("daemon error")
            .to_string());
    }
    Ok(resp.get(field).cloned().unwrap_or_else(|| json!([])))
}

#[tauri::command]
pub async fn agent_list(link: State<'_, DaemonLink>) -> Result<Value, String> {
    let resp = request(&link.addr, json!({ "type": "agent_list" })).await?;
    array_field(resp, "agents")
}

#[tauri::command]
pub async fn agent_attach(
    link: State<'_, DaemonLink>,
    kind: String,
    session_id: Option<String>,
) -> Result<Value, String> {
    let resp = request(
        &link.addr,
        json!({ "type": "agent_attach", "kind": kind, "session_id": session_id }),
    )
    .await?;
    array_field(resp, "agents")
}

#[tauri::command]
pub async fn agent_detach(link: State<'_, DaemonLink>) -> Result<Value, String> {
    let resp = request(&link.addr, json!({ "type": "agent_detach" })).await?;
    array_field(resp, "agents")
}

#[tauri::command]
pub async fn agent_sessions(link: State<'_, DaemonLink>, kind: String) -> Result<Value, String> {
    let resp = request(
        &link.addr,
        json!({ "type": "agent_sessions", "kind": kind }),
    )
    .await?;
    array_field(resp, "sessions")
}

#[tauri::command]
pub async fn agent_models(link: State<'_, DaemonLink>, kind: String) -> Result<Value, String> {
    let resp = request(&link.addr, json!({ "type": "agent_models", "kind": kind })).await?;
    array_field(resp, "models")
}

#[tauri::command]
pub async fn agent_connectors(link: State<'_, DaemonLink>, kind: String) -> Result<Value, String> {
    let resp = request(
        &link.addr,
        json!({ "type": "agent_connectors", "kind": kind }),
    )
    .await?;
    array_field(resp, "connectors")
}

/// Coverage of the meeting-relevant context sources for the attached agent —
/// the onboarding coverage meter's data (calendar/slack/email/tickets +
/// Bluey's own memory connector, each connected-or-missing with a guided
/// connect hint).
#[tauri::command]
pub async fn source_coverage(link: State<'_, DaemonLink>) -> Result<Value, String> {
    let resp = request(&link.addr, json!({ "type": "source_coverage" })).await?;
    array_field(resp, "sources")
}

#[tauri::command]
pub async fn set_agent_session_history(
    link: State<'_, DaemonLink>,
    enabled: bool,
) -> Result<(), String> {
    let resp = request(
        &link.addr,
        json!({ "type": "set_agent_session_history", "enabled": enabled }),
    )
    .await?;
    // Treat any non-error response as success.
    array_field(resp, "_ignored").map(|_| ())
}

/// Start the interactive cloud-calendar OAuth connect flow for `provider`
/// (`"google"` | `"microsoft"`). The daemon opens the system browser, waits for
/// the OAuth code, and stores the tokens in the OS keychain — this command
/// resolves once that round-trip completes (or the daemon errors/times out).
#[tauri::command]
pub async fn calendar_connect(link: State<'_, DaemonLink>, provider: String) -> Result<(), String> {
    let resp = request(
        &link.addr,
        json!({ "type": "calendar_connect_start", "provider": provider }),
    )
    .await?;
    // Ok / error only; surface the daemon's error message if the flow failed.
    array_field(resp, "_ignored").map(|_| ())
}

/// Report the current cloud-calendar connection state (one row per provider).
#[tauri::command]
pub async fn calendar_status(link: State<'_, DaemonLink>) -> Result<Value, String> {
    let resp = request(&link.addr, json!({ "type": "calendar_connect_status" })).await?;
    array_field(resp, "connections")
}

/// Clear the stored tokens for `provider`, disconnecting that cloud calendar.
#[tauri::command]
pub async fn calendar_disconnect(
    link: State<'_, DaemonLink>,
    provider: String,
) -> Result<(), String> {
    let resp = request(
        &link.addr,
        json!({ "type": "calendar_disconnect", "provider": provider }),
    )
    .await?;
    array_field(resp, "_ignored").map(|_| ())
}

/// Ask the attached agent.
///
/// **Preferred (socket / push path):** when the daemon launched us over the Unix
/// socket, the UI should ask by sending an `ask_requested` `OverlayEvent` via the
/// `overlay_send` invoke command. The daemon then streams the answer back as the
/// `push_card` → `update_card`* `OverlayCommand` sequence on `overlay://command`,
/// which the UI renders directly. That path needs no round-trip here.
///
/// To keep the UI's existing `MeetingClient.ask(id, question)` working even when
/// the socket is live, this command ALSO forwards an `ask_requested` event over
/// the socket writer when it is connected — the streamed answer arrives over
/// `overlay://command`, so no `meeting://answer/<id>` chunks are emitted here.
///
/// **Fallback (TCP pull path):** when no socket is connected (standalone dev, or
/// before the daemon wires the push feed), we fall back to the daemon's
/// request/response `Answer` IPC and emit the full text as `meeting://answer/<id>`
/// chunks so the UI still renders the thinking → answer flow.
// Tauri injects each argument by name from the JS `invoke` call, so the
// argument list IS the command's wire contract — collapsing them into a struct
// would change that contract. The three trailing fields are optional
// answer-shaping hints; exceeding clippy's 7-arg guard is expected for an IPC
// command and grouping them would not improve the call site.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn meeting_ask(
    app: AppHandle,
    link: State<'_, DaemonLink>,
    sender: State<'_, EventSender>,
    id: String,
    question: String,
    // Optional answer-shaping fields, mirroring the daemon's
    // OverlayEvent::AskRequested { provider, model, mode }. Forwarded verbatim
    // when present; absent fields keep the daemon's own defaults.
    mode: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    // Socket path: forward an `ask_requested` OverlayEvent; the answer streams
    // back over `overlay://command` (push_card → update_card*). No TCP round-trip.
    {
        let guard = sender.0.lock().await;
        if let Some(tx) = guard.as_ref() {
            // Build the event, including only the optional fields that are set so
            // an unpinned ask carries exactly { type, question } as before.
            let mut event = json!({ "type": "ask_requested", "question": question });
            if let Value::Object(map) = &mut event {
                if let Some(mode) = &mode {
                    map.insert("mode".into(), json!(mode));
                }
                if let Some(provider) = &provider {
                    map.insert("provider".into(), json!(provider));
                }
                if let Some(model) = &model {
                    map.insert("model".into(), json!(model));
                }
            }
            let _ = tx.send(event.to_string());
            return Ok(());
        }
    }

    // Fallback (no socket): TCP request/response Answer IPC.
    let cancelled = Arc::new(AtomicBool::new(false));
    link.cancels
        .lock()
        .await
        .insert(id.clone(), cancelled.clone());

    let channel = format!("meeting://answer/{id}");
    let req = json!({ "type": "answer", "request": { "question": question } });
    let resp = request(&link.addr, req).await;

    link.cancels.lock().await.remove(&id);
    if cancelled.load(Ordering::SeqCst) {
        return Ok(());
    }

    match resp {
        Ok(v) => {
            if v.get("type").and_then(Value::as_str) == Some("error") {
                let msg = v.get("message").and_then(Value::as_str).unwrap_or("error");
                let _ = app.emit(
                    &channel,
                    json!({ "text": format!("[{msg}]"), "done": true }),
                );
                return Ok(());
            }
            // Emit the answer text, then done. Tool/source rows arrive once the
            // daemon's Answer events carry them; until then the text answer (from
            // the user's own agent) is forwarded faithfully.
            let text = v
                .get("response")
                .and_then(|r| r.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let _ = app.emit(&channel, json!({ "text": text }));
            let _ = app.emit(&channel, json!({ "done": true }));
            Ok(())
        }
        Err(e) => {
            let _ = app.emit(&channel, json!({ "text": format!("[{e}]"), "done": true }));
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn meeting_ask_cancel(link: State<'_, DaemonLink>, id: String) -> Result<(), String> {
    if let Some(flag) = link.cancels.lock().await.get(&id) {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Open the NATIVE macOS file picker from the overlay's OWN GUI process and
/// return the chosen absolute paths (empty on cancel). The daemon is headless
/// and a daemon-spawned helper has no window-server access, so the dialog must
/// originate here, in a process that already owns a real window.
///
/// The overlay normally runs as an ACCESSORY app (no Dock icon, invisible to
/// screen capture). An accessory app's file dialog won't come to the front, so
/// we momentarily flip to REGULAR around the picker, then restore accessory —
/// the capture-invisibility promise still holds (the flip lasts only while the
/// modal is open, and the overlay window itself stays content-protected).
#[tauri::command]
pub async fn pick_context_files(app: AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    // Flip to a foreground policy so the native dialog is visible + focused.
    // In capture-visible test mode the overlay is already Regular; restoring to
    // Accessory afterward is still correct for the real (invisible) product.
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

    // The dialog plugin's blocking picker must run off the main thread (it spins
    // its own modal loop). `blocking_pick_files` returns None on cancel.
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .set_title("Attach files to Bluey")
            .add_filter(
                "Attachable files",
                &[
                    // Text / code / docs
                    "md", "markdown", "txt", "log", "csv", "tsv", "rst", "adoc", "rs", "swift", "c",
                    "h", "cpp", "hpp", "js", "jsx", "ts", "tsx", "py", "go", "java", "kt", "kts",
                    "cs", "rb", "php", "sql", "sh", "ps1", "toml", "yaml", "yml", "json", "html",
                    "css", "scss", "pdf", "doc", "docx", "rtf",
                    // Images — sent to the agent as pixels over ACP
                    "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp",
                ],
            )
            .blocking_pick_files()
    })
    .await
    .map_err(|e| format!("file dialog task failed: {e}"))?;

    // Restore the invisible accessory policy.
    #[cfg(target_os = "macos")]
    {
        let capture_visible = std::env::var("BLUEY_MEETING_CAPTURE_VISIBLE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if !capture_visible {
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }

    let paths = picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    Ok(paths)
}

/// Capture the screen to a PNG and return its path. The capture runs inside the
/// `BlueyShot.app` BUNDLE (launched via `/usr/bin/open`), NOT a bare binary:
/// macOS TCC (Screen Recording) can only be granted to an app with a bundle
/// identity — a bare binary is always silently denied (empty file). This mirrors
/// the proven `BlueyAudio.app` pattern. The caller (JS) then hands the path to
/// the daemon as `attach_files_requested`, which classifies the PNG as an image
/// and sends it to the agent as pixels over ACP.
///
/// Returns an error string if the bundle is missing, the tool fails, or the file
/// is empty (e.g. Screen Recording was not granted to BlueyShot yet).
#[tauri::command]
pub async fn capture_screenshot() -> Result<String, String> {
    #[cfg(not(target_os = "macos"))]
    {
        Err("screenshot capture is implemented on macOS only".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        let bundle = bluey_shot_app_bundle()
            .ok_or_else(|| "BlueyShot.app not found (screenshot helper not staged)".to_string())?;

        let dir = std::env::temp_dir().join("bluey-overlay-captures");
        std::fs::create_dir_all(&dir).map_err(|e| format!("create capture dir: {e}"))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let path = dir.join(format!("screenshot-{stamp}.png"));

        let path_for_task = path.clone();
        // `open -W -n <BlueyShot.app> --args --out <png>`: -W waits for it to
        // finish, -n launches a fresh instance, and macOS reads the bundle
        // identity so the capture is attributed to sh.bluey.shot (which holds the
        // Screen Recording grant).
        let status = tauri::async_runtime::spawn_blocking(move || {
            Command::new("/usr/bin/open")
                .arg("-W")
                .arg("-n")
                .arg(&bundle)
                .arg("--args")
                .arg("--out")
                .arg(&path_for_task)
                .status()
        })
        .await
        .map_err(|e| format!("screenshot task failed: {e}"))?
        .map_err(|e| format!("failed to launch BlueyShot.app: {e}"))?;

        if !status.success() {
            return Err("screenshot helper exited with an error".to_string());
        }
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > 0 => Ok(path.to_string_lossy().to_string()),
            Ok(_) | Err(_) => Err(
                "screen capture was denied — grant Screen Recording to BlueyShot in \
                 System Settings → Privacy & Security → Screen Recording, then try again"
                    .to_string(),
            ),
        }
    }
}

/// Resolve the `BlueyShot.app` bundle dir (not the inner binary) so it can be
/// launched via `/usr/bin/open` — the only way macOS reads the bundle identity
/// the Screen Recording grant attaches to. Mirrors the audio helper's resolver:
/// beside the overlay binary first (staged install), then the dev `.build` dir.
#[cfg(target_os = "macos")]
fn bluey_shot_app_bundle() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("BLUEY_SHOT_APP_BUNDLE") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let app = "BlueyShot.app";
    if let Ok(exe) = std::env::current_exe() {
        let mut dirs = Vec::new();
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
        if let Ok(canonical) = exe.canonicalize() {
            if let Some(dir) = canonical.parent() {
                dirs.push(dir.to_path_buf());
            }
        }
        for dir in dirs {
            let candidate = dir.join(app);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let dev_app = PathBuf::from(format!("native/macos/cue-shot/.build/{app}"));
    if dev_app.exists() {
        return Some(dev_app);
    }
    None
}

/// Hide the meeting-prep banner window (called by the BannerWindow UI after the
/// user warms up or dismisses). It's an NSPanel, so order it OUT via the panel
/// (the webview `.hide()` doesn't reliably hide a panel). The window is reused
/// (shown again on the next meeting), so we order-out rather than close it.
#[tauri::command]
pub fn hide_banner(
    app: AppHandle,
    event_id: Option<String>,
    start_epoch_secs: Option<i64>,
) -> Option<String> {
    use tauri::Manager;

    // Advance only the matching occurrence. If another due meeting is queued,
    // return it directly and keep the panel visible.
    let next = app
        .try_state::<crate::ipc::PendingBanner>()
        .and_then(|pending| pending.dismiss(event_id, start_epoch_secs));
    if next.is_some() {
        return next;
    }
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel("banner") {
            panel.order_out(None);
            return None;
        }
    }
    if let Some(w) = app.get_webview_window("banner") {
        let _ = w.hide();
    }
    None
}
