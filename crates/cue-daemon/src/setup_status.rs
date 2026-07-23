//! First-run setup status for the onboarding screen.
//!
//! Onboarding used to advance on clicks alone, so a user could reach "You're
//! ready" with no speech model on disk and no agent CLI installed — and only
//! discover it mid-meeting as "couldn't answer". This module reports the REAL
//! state of each prerequisite so onboarding can show live progress, offer the
//! fix in-product, and gate "ready" on setup actually being complete.

use std::sync::atomic::{AtomicU8, Ordering};

use cue_core::agent_ui::{SetupItem, SetupStatus};

/// Live model-download percent, published by the model-progress forwarder.
/// `101` = done, `255` = not started. An atomic (not a lock) because the
/// download loop writes it on every chunk.
static MODEL_PCT: AtomicU8 = AtomicU8::new(255);

/// Record model-download progress (0-100), or `None` once fully provisioned.
pub fn set_model_progress(percent: Option<u8>) {
    MODEL_PCT.store(percent.unwrap_or(101), Ordering::Relaxed);
}

/// Build the model item from what is actually on disk, falling back to live
/// download progress. Disk truth wins: a present model is `ready` even if this
/// process never downloaded it (prior run, installer preload, offline bundle).
fn model_item(models_present: bool) -> SetupItem {
    if models_present {
        return SetupItem {
            state: "ready".into(),
            detail: "Speech recognition ready — runs on this Mac".into(),
            percent: None,
            kind: None,
        };
    }
    match MODEL_PCT.load(Ordering::Relaxed) {
        255 => SetupItem {
            state: "missing".into(),
            detail: "Speech model not downloaded yet".into(),
            percent: None,
            kind: None,
        },
        101 => SetupItem {
            state: "ready".into(),
            detail: "Speech recognition ready — runs on this Mac".into(),
            percent: None,
            kind: None,
        },
        pct => SetupItem {
            state: "working".into(),
            detail: format!("Downloading speech model… {pct}%"),
            percent: Some(pct),
            kind: None,
        },
    }
}

/// Build the agent item. `capability` is the attached/best agent's snake_case
/// capability from the discovery list; `None` means no agent was found at all.
fn agent_item(agent: Option<(&str, &str, &str)>) -> SetupItem {
    match agent {
        None => SetupItem {
            state: "missing".into(),
            detail: "No coding agent found — Bluey answers through yours".into(),
            percent: None,
            kind: None,
        },
        Some((kind, name, capability)) => match capability {
            "needs_reauth" => SetupItem {
                state: "needs_login".into(),
                detail: format!("{name} — signed out"),
                percent: None,
                kind: Some(kind.to_string()),
            },
            "drive" => SetupItem {
                state: "ready".into(),
                detail: format!("{name} — connected"),
                percent: None,
                kind: Some(kind.to_string()),
            },
            other => SetupItem {
                state: "failed".into(),
                detail: format!("{name} — {}", other.replace('_', " ")),
                percent: None,
                kind: Some(kind.to_string()),
            },
        },
    }
}

/// Assemble the full status. `all_ready` gates onboarding's final step.
pub fn build(models_present: bool, agent: Option<(&str, &str, &str)>) -> SetupStatus {
    let model = model_item(models_present);
    let agent = agent_item(agent);
    let all_ready = model.state == "ready" && agent.state == "ready";
    SetupStatus {
        model,
        agent,
        all_ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_ready_only_when_model_and_agent_are_both_ready() {
        let s = build(true, Some(("cursor", "Cursor", "drive")));
        assert!(s.all_ready, "model on disk + drivable agent = ready");

        // A signed-out agent must NOT report ready: attaching it "works" and
        // then every ask fails, which is the exact trap this gates.
        let s = build(true, Some(("cursor", "Cursor", "needs_reauth")));
        assert!(!s.all_ready);
        assert_eq!(s.agent.state, "needs_login");
        assert_eq!(s.agent.kind.as_deref(), Some("cursor"));

        // No agent at all.
        let s = build(true, None);
        assert!(!s.all_ready);
        assert_eq!(s.agent.state, "missing");
    }

    #[test]
    fn model_on_disk_wins_over_stale_progress() {
        set_model_progress(Some(42));
        let s = build(true, None);
        assert_eq!(s.model.state, "ready", "disk truth beats live progress");
        assert!(s.model.percent.is_none());
    }

    #[test]
    fn download_in_flight_reports_percent() {
        set_model_progress(Some(47));
        let s = build(false, None);
        assert_eq!(s.model.state, "working");
        assert_eq!(s.model.percent, Some(47));
        assert!(s.model.detail.contains("47%"));
    }
}
