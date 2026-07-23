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

/// Live memory-embedder (bge-small) prepare percent, same encoding as
/// [`MODEL_PCT`] (`101` = done, `255` = not started). Written by the eager
/// `prepare_memory` download loop so onboarding shows a "Preparing memory…" row.
static MEMORY_PCT: AtomicU8 = AtomicU8::new(255);

/// Record model-download progress (0-100), or `None` once fully provisioned.
pub fn set_model_progress(percent: Option<u8>) {
    MODEL_PCT.store(percent.unwrap_or(101), Ordering::Relaxed);
}

/// Record memory-embedder prepare progress (0-100), or `None` once ready.
/// Mirrors [`set_model_progress`] for the bge-small embedder.
pub fn set_memory_progress(percent: Option<u8>) {
    MEMORY_PCT.store(percent.unwrap_or(101), Ordering::Relaxed);
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

/// Build the memory-embedder item, mirroring [`model_item`]: disk truth wins
/// (a present embedder is `ready` even if this process never downloaded it),
/// else fall back to live prepare progress. `memory_present` is whether the
/// embedder files are already on disk.
fn memory_item(memory_present: bool) -> SetupItem {
    if memory_present {
        return SetupItem {
            state: "ready".into(),
            detail: "Memory ready — runs on this Mac".into(),
            percent: None,
            kind: None,
        };
    }
    match MEMORY_PCT.load(Ordering::Relaxed) {
        255 => SetupItem {
            state: "missing".into(),
            detail: "Memory model not prepared yet".into(),
            percent: None,
            kind: None,
        },
        101 => SetupItem {
            state: "ready".into(),
            detail: "Memory ready — runs on this Mac".into(),
            percent: None,
            kind: None,
        },
        pct => SetupItem {
            state: "working".into(),
            detail: format!("Preparing memory… {pct}%"),
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
///
/// `memory_present` is whether the on-device memory embedder is ready on disk.
/// On builds without the `local-memory` feature there is nothing to prepare, so
/// callers pass `true` (the memory item is `ready` and never gates onboarding).
pub fn build(
    models_present: bool,
    memory_present: bool,
    agent: Option<(&str, &str, &str)>,
) -> SetupStatus {
    let model = model_item(models_present);
    let memory = memory_item(memory_present);
    let agent = agent_item(agent);
    let all_ready = model.state == "ready" && memory.state == "ready" && agent.state == "ready";
    SetupStatus {
        model,
        memory,
        agent,
        all_ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `MODEL_PCT` / `MEMORY_PCT` are process-global atomics; tests that write
    /// them must not run concurrently or they stomp each other's expected
    /// percent. This mutex serializes exactly those tests (a lean, dep-free
    /// alternative to the `serial_test` crate). Tests that only pass
    /// `*_present = true` don't read the statics and need no guard.
    static PCT_GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn all_ready_only_when_model_memory_and_agent_are_all_ready() {
        let s = build(true, true, Some(("cursor", "Cursor", "drive")));
        assert!(
            s.all_ready,
            "model + memory on disk + drivable agent = ready"
        );

        // A signed-out agent must NOT report ready: attaching it "works" and
        // then every ask fails, which is the exact trap this gates.
        let s = build(true, true, Some(("cursor", "Cursor", "needs_reauth")));
        assert!(!s.all_ready);
        assert_eq!(s.agent.state, "needs_login");
        assert_eq!(s.agent.kind.as_deref(), Some("cursor"));

        // No agent at all.
        let s = build(true, true, None);
        assert!(!s.all_ready);
        assert_eq!(s.agent.state, "missing");
    }

    #[test]
    fn memory_not_ready_blocks_all_ready() {
        let _guard = PCT_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        // Memory embedder still preparing → onboarding must NOT be ready even
        // with speech model + a drivable agent.
        set_memory_progress(Some(60));
        let s = build(true, false, Some(("cursor", "Cursor", "drive")));
        assert!(!s.all_ready, "memory still preparing must block ready");
        assert_eq!(s.memory.state, "working");
        assert_eq!(s.memory.percent, Some(60));
        assert!(s.memory.detail.contains("60%"));
    }

    #[test]
    fn model_on_disk_wins_over_stale_progress() {
        let _guard = PCT_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        set_model_progress(Some(42));
        let s = build(true, true, None);
        assert_eq!(s.model.state, "ready", "disk truth beats live progress");
        assert!(s.model.percent.is_none());
    }

    #[test]
    fn memory_on_disk_wins_over_stale_progress() {
        let _guard = PCT_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        set_memory_progress(Some(42));
        let s = build(true, true, None);
        assert_eq!(s.memory.state, "ready", "disk truth beats live progress");
        assert!(s.memory.percent.is_none());
    }

    #[test]
    fn download_in_flight_reports_percent() {
        let _guard = PCT_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        set_model_progress(Some(47));
        let s = build(false, true, None);
        assert_eq!(s.model.state, "working");
        assert_eq!(s.model.percent, Some(47));
        assert!(s.model.detail.contains("47%"));
    }
}
