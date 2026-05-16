//! Integration tests for the production overlay hardening path.
//!
//! These exercise `validate_and_decode_overlay_line` (the gatekeeper used
//! inside the production `spawn_overlay` reader thread) with the same
//! input shapes a real overlay child would emit. Unlike the helper-only
//! tests in `overlay_security_integration.rs`, these prove the
//! production gatekeeping layer rejects bad inputs before forwarding to
//! the daemon's event channel.
//!
//! Codex called this out explicitly in REVIEW-PHASE-3-ROUND-11.md:
//! "Add production-path tests proving env override gating, tokenless
//!  event rejection, and state/length validation."
//!
//! Note: the helper is module-private. We exercise it through a thin
//! `#[cfg(test)]` re-export added in app.rs, OR via the public
//! NativeOverlayHandle path. We use the latter for env-override tests
//! and the former (re-export) for line-validation tests.

use cue_core::overlay_ipc::OverlayUiState;
use parking_lot::Mutex;
use std::sync::Arc;

// Re-export expected from app.rs (#[cfg(test)] only).
use cue_daemon::app_test_hooks::{
    validate_and_decode_overlay_line as validate_line, OverlayLineReject,
};

const TOK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn idle_state() -> Arc<Mutex<OverlayUiState>> {
    Arc::new(Mutex::new(OverlayUiState::Idle))
}

#[test]
fn token_match_pong_accepted_in_idle() {
    let state = idle_state();
    let line = format!(r#"{{"type":"pong","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(result.is_ok(), "pong with valid token should be accepted");
}

#[test]
fn token_mismatch_event_rejected() {
    let state = idle_state();
    let line = r#"{"type":"pong","token":"WRONG"}"#;
    let result = validate_line(line, TOK, state.as_ref());
    assert!(matches!(result, Err(OverlayLineReject::TokenMismatch)));
}

#[test]
fn tokenless_event_rejected_when_token_required() {
    let state = idle_state();
    let line = r#"{"type":"pong"}"#;
    let result = validate_line(line, TOK, state.as_ref());
    assert!(matches!(result, Err(OverlayLineReject::TokenMismatch)));
}

#[test]
fn legacy_mode_no_token_required() {
    // When daemon's expected token is empty, events are accepted without one.
    let state = idle_state();
    let line = r#"{"type":"pong"}"#;
    let result = validate_line(line, "", state.as_ref());
    assert!(result.is_ok());
}

#[test]
fn oversized_question_field_rejected() {
    let state = idle_state();
    let huge = "x".repeat(10 * 1024); // 10 KB > 4 KB cap
    let line = format!(r#"{{"type":"ask_requested","question":"{huge}","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        matches!(
            result,
            Err(OverlayLineReject::FieldTooLong {
                field: "question",
                ..
            })
        ),
        "oversized question must be rejected, got {result:?}"
    );
}

#[test]
fn oversized_instructions_field_rejected() {
    let state = idle_state();
    let huge = "x".repeat(20 * 1024); // 20 KB > 16 KB cap
    let line = format!(r#"{{"type":"instructions_updated","text":"{huge}","token":"{TOK}"}}"#);
    // Instructions are validated under the `instructions` key, but the
    // legacy event uses `text` — so this checks the text length cap which
    // is 64KB. 20KB passes that. Use the explicit instructions field.
    let line2 =
        format!(r#"{{"type":"instructions_updated","instructions":"{huge}","token":"{TOK}"}}"#);
    let result = validate_line(&line2, TOK, state.as_ref());
    assert!(
        matches!(
            result,
            Err(OverlayLineReject::FieldTooLong {
                field: "instructions",
                ..
            })
        ),
        "oversized instructions must be rejected, got {result:?}"
    );
    // The text-field path on instructions_updated also has 64KB cap;
    // a 20KB payload fits. But other event types use text= for transcripts.
    let _ = line; // silence warning
}

#[test]
fn oversized_path_in_paths_array_rejected() {
    let state = idle_state();
    let long_path = "p".repeat(2 * 1024); // 2 KB > 1 KB per-entry
    let line =
        format!(r#"{{"type":"attach_files_requested","paths":["{long_path}"],"token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        matches!(
            result,
            Err(OverlayLineReject::FieldTooLong {
                field: "paths[entry]",
                ..
            })
        ),
        "oversized path entry must be rejected, got {result:?}"
    );
}

#[test]
fn too_many_paths_rejected() {
    let state = idle_state();
    let paths: Vec<String> = (0..20).map(|i| format!("\"p{i}\"")).collect();
    let line = format!(
        r#"{{"type":"attach_files_requested","paths":[{}],"token":"{TOK}"}}"#,
        paths.join(",")
    );
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        matches!(
            result,
            Err(OverlayLineReject::FieldTooLong { field: "paths", .. })
        ),
        "too many paths must be rejected, got {result:?}"
    );
}

#[test]
fn line_too_long_rejected_before_parsing() {
    let state = idle_state();
    // 200 KB line — over 128 KB cap.
    let huge = "x".repeat(200 * 1024);
    let line = format!(r#"{{"type":"ask_requested","question":"{huge}","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        matches!(
            result,
            Err(OverlayLineReject::FieldTooLong {
                field: "<line>",
                ..
            })
        ),
        "oversized line must be rejected before JSON parse, got {result:?}"
    );
}

#[test]
fn attach_files_requested_dropped_when_idle() {
    let state = idle_state(); // Idle
    let line =
        format!(r#"{{"type":"attach_files_requested","paths":["/tmp/foo"],"token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        matches!(result, Err(OverlayLineReject::StateNotAllowed { .. })),
        "AttachFilesRequested must be dropped in Idle state, got {result:?}"
    );
}

#[test]
fn attach_files_requested_accepted_when_attach_open() {
    let state = Arc::new(Mutex::new(OverlayUiState::AttachOpen));
    let line =
        format!(r#"{{"type":"attach_files_requested","paths":["/tmp/foo"],"token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        result.is_ok(),
        "AttachFilesRequested must be accepted in AttachOpen, got {result:?}"
    );
}

#[test]
fn instructions_updated_dropped_when_idle() {
    let state = idle_state();
    // legacy InstructionsUpdated uses `text` field
    let line = format!(r#"{{"type":"instructions_updated","text":"hi","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        matches!(result, Err(OverlayLineReject::StateNotAllowed { .. })),
        "InstructionsUpdated must be dropped in Idle state, got {result:?}"
    );
}

#[test]
fn malformed_json_returns_not_json() {
    let state = idle_state();
    let line = r#"this is not json"#;
    let result = validate_line(line, TOK, state.as_ref());
    assert!(matches!(result, Err(OverlayLineReject::NotJson)));
}

#[test]
fn transcript_text_with_inner_type_field_does_not_dispatch_inner() {
    // A transcript text field that LITERALLY contains another JSON-looking
    // payload must not be mistakenly parsed as that inner event. The outer
    // type is what matters; the text is just a string.
    // This is the prompt-injection-via-transcript class of attacks codex
    // asked to test in the original R11 hardening.
    let state = Arc::new(Mutex::new(OverlayUiState::AttachOpen));
    let inner = r#"\"type\":\"attach_files_requested\""#;
    let line = format!(r#"{{"type":"shown","token":"{TOK}","note":"contains {inner}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    // shown is allowed in any state; result should be Ok with Shown variant
    // and NOT AttachFilesRequested.
    match result {
        Ok(event) => {
            let dbg = format!("{event:?}");
            assert!(
                dbg.starts_with("Shown") || dbg.contains("Shown"),
                "outer type must be Shown not the injected inner type, got {dbg}"
            );
        }
        Err(e) => panic!("expected Ok(Shown), got {e:?}"),
    }
}

// ---------------------------------------------------------------------------
// R11 recheck #2: state-machine relaxation + Windows ask_requested fixture
// ---------------------------------------------------------------------------

#[test]
fn windows_style_ask_requested_with_token_accepted() {
    // Production fixture: this is the EXACT shape the Windows native overlay
    // (native/windows/cue-overlay/main.c emit_ask_event) emits when the user
    // submits a question. Field order matches the C printf:
    //   {"type":"ask_requested","question":"...","provider":"auto",
    //    "model":"","mode":"General","token":"..."}
    let state = idle_state();
    let line = format!(
        r#"{{"type":"ask_requested","question":"What is 2+2?","provider":"auto","model":"","mode":"General","token":"{TOK}"}}"#
    );
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        result.is_ok(),
        "Windows-style ask_requested must be accepted, got {result:?}"
    );
    if let Ok(event) = result {
        let dbg = format!("{event:?}");
        assert!(dbg.contains("AskRequested"), "wrong variant: {dbg}");
        assert!(dbg.contains("What is 2+2?"), "question lost: {dbg}");
    }
}

#[test]
fn windows_style_ask_requested_without_token_rejected() {
    // Same Windows fixture but token omitted -> daemon must reject.
    let state = idle_state();
    let line =
        r#"{"type":"ask_requested","question":"hi","provider":"auto","model":"","mode":"General"}"#;
    let result = validate_line(line, TOK, state.as_ref());
    assert!(matches!(result, Err(OverlayLineReject::TokenMismatch)));
}

#[test]
fn attach_requested_accepted_from_idle() {
    // AttachRequested is the user clicking "open the attach panel" from the
    // pill -- it is the entry-point that drives Idle -> AttachOpen. It MUST
    // be accepted from Idle, otherwise the panel can never open.
    let state = idle_state();
    let line = format!(r#"{{"type":"attach_requested","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        result.is_ok(),
        "AttachRequested must be accepted from Idle (entry-point event), got {result:?}"
    );
}

#[test]
fn instructions_requested_accepted_from_idle() {
    // InstructionsRequested is the user clicking "open instructions" from the
    // pill -- entry-point event that drives Idle -> InstructionsOpen.
    let state = idle_state();
    let line = format!(r#"{{"type":"instructions_requested","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(
        result.is_ok(),
        "InstructionsRequested must be accepted from Idle (entry-point event), got {result:?}"
    );
}

#[test]
fn attach_requested_accepted_from_attach_open() {
    // Harmless when already AttachOpen (user re-clicks); still accepted.
    let state = Arc::new(Mutex::new(OverlayUiState::AttachOpen));
    let line = format!(r#"{{"type":"attach_requested","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(result.is_ok());
}

#[test]
fn instructions_requested_accepted_from_instructions_open() {
    let state = Arc::new(Mutex::new(OverlayUiState::InstructionsOpen));
    let line = format!(r#"{{"type":"instructions_requested","token":"{TOK}"}}"#);
    let result = validate_line(&line, TOK, state.as_ref());
    assert!(result.is_ok());
}
