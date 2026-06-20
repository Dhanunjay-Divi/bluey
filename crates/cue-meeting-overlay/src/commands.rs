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
pub async fn agent_connectors(link: State<'_, DaemonLink>, kind: String) -> Result<Value, String> {
    let resp = request(
        &link.addr,
        json!({ "type": "agent_connectors", "kind": kind }),
    )
    .await?;
    array_field(resp, "connectors")
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
#[tauri::command]
pub async fn meeting_ask(
    app: AppHandle,
    link: State<'_, DaemonLink>,
    sender: State<'_, EventSender>,
    id: String,
    question: String,
) -> Result<(), String> {
    // Socket path: forward an `ask_requested` OverlayEvent; the answer streams
    // back over `overlay://command` (push_card → update_card*). No TCP round-trip.
    {
        let guard = sender.0.lock().await;
        if let Some(tx) = guard.as_ref() {
            let event = json!({ "type": "ask_requested", "question": question }).to_string();
            let _ = tx.send(event);
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
