# IMPL — agent-bridge research fixes

> Branch `agent/agent-bridge-fixes`. Implements the frozen contract in
> [DESIGN-AGENT-BRIDGE-FIXES.md](DESIGN-AGENT-BRIDGE-FIXES.md), grounded in the
> live-CLI verification in [VERIFY-AGENT-BRIDGE-FIXES.md](VERIFY-AGENT-BRIDGE-FIXES.md).
> Built by workflows wf_3f3c0007-739 (verify+design) and wf_af2c8876-3a3
> (implement+build+review); D1 landed manually before the limit reset.

## What shipped

| Change | Where | Status |
|---|---|---|
| Cursor / Copilot / Antigravity `model_flag: None → Some("--model")` | registry.rs | ✅ live-verified |
| Cursor `Replay → NativeResume`, **ledger-gated** (`resume_requires_ledger: true`) | registry.rs + tier.rs | ✅ |
| Per-agent model override (`attached_model` setting → seeds `model_override`) | config.rs, app.rs | ✅ |
| Speed → effort flags for **Codex** (`-c model_reasoning_effort`) | registry.rs `EffortArgs` + drive | ✅ live-verified |
| Spawn-time session ledger (`sessions/ledger.rs`, cwd via `DriveOutcome.cwd`) | bridge + daemon | ✅ |
| ACP gate: non-empty overrides force the CLI route | lib.rs `drive_with_overrides` | ✅ |

## Two live corrections during implementation (contract assumptions overturned)

1. **Copilot `--effort` REFUTED.** The optional smoke (`copilot -p "..." --effort low`
   under node 24) hard-errored: `Model "auto" does not support reasoning effort
   configuration`. Since the answer path always uses the default `auto` model, sending
   effort args would break **every** Copilot fast/deep answer. → Copilot `effort_args: None`
   (prose-only, like Cursor). Codex is the sole live-verified effort row.
2. **Codex effort key ACCEPTED (despite exit 1).** `codex exec -c
   'model_reasoning_effort="low"'` exited 1, but the banner echoed `reasoning effort:
   low` — the key parsed and applied; the exit-1 is the pre-existing ChatGPT-account
   model-block (identical with the flag omitted). → Codex effort row KEPT.

Both are recorded in the registry row comments with dates.

## Descoped (per DESIGN doc, not implemented)

- **Antigravity tier flip** — store split: `agy` writes `~/.gemini/antigravity-cli/`,
  Bluey reads `~/.gemini/antigravity/`; every listable id fails `trajectory not found`.
  Model flag still shipped; tier stays Replay.
- **Claude `--effort`** — refuted on claude 2.0.42 (no such flag); would break every drive.
- **Overlay model-picker UI** — the *IPC plumbing* is now wired end-to-end (see below);
  only the visible picker widget is deferred to a UI follow-up.

## Frontend IPC link (added after review)

A post-implementation check found the daemon accepted `attached_model` but no frontend
emitter sent it. The `model` field is now threaded through the live attach path (the
`agent_attach_requested` OverlayEvent — NOT the dead `agent_attach` Tauri command, which
has no daemon handler):
- `ui/src/lib/client.ts` — `MeetingClient.attach(kind, sessionId?, model?)`
- `ui/src/lib/tauriClient.ts` — the Tauri `attach` impl spreads `model` into the event
- `native/macos/.../main.swift` — `emitAgentAttachRequested(kind:sessionId:model:)`
  (defaulted `model: nil`, existing caller unchanged)

All three params are OPTIONAL, so the current single caller (`App.tsx`, passes only
kind+sessionId) is unaffected; a future model-picker widget just passes the third arg.
Verified: `tsc --noEmit` clean, `cargo check -p cue-meeting-overlay` builds, `swiftc
-parse` clean. No visible UI element yet — this is the wire, not the widget.

## Verification (all `--target aarch64-apple-darwin`)

- `cargo fmt --all` — clean
- `cargo clippy --workspace -- -D warnings` — clean (only pre-existing `block v0.1.6`
  transitive future-incompat note)
- `cargo build -p cue-agent-bridge -p cue-core -p cue-daemon` — passes
- Tests: **cue-agent-bridge 628 lib + 22 integration**, **cue-core 124**,
  **cue-daemon 241 lib** — all pass, 0 failures. 5 bridge tests `#[ignore]` (live-smoke gated).

### One pre-existing out-of-scope failure (NOT ours)

`cargo test -p cue-daemon` (full, incl. integration) fails to COMPILE
`tests/live_transcript_emit.rs` (E0063: `LiveTranscriptEvent` missing `audio_secs`/`kind`/
`ledger`). That struct lives in `cue-core/src/meeting.rs`, modified by the **parallel STT
effort** (uncommitted `M meeting.rs` + untracked decisions-`ledger.rs` files, last touched
by commit 4df5f62). It is not in this change's diff and the DESIGN file-ownership fence
forbids touching it. The cue-daemon **lib** tests (all our contract logic) compile and pass.

## Review

Adversarial review (2 agents): **0 blockers, 1 minor** — a stale test in
`model_resolve.rs` (`agent_with_no_model_flag_proposes_byot_directly`) still used
`AgentKind::Cursor` with a comment "Cursor has model_flag: None", contradicting the flip.
**Fixed**: retargeted to `AgentKind::Windsurf` (genuinely flagless) so the None branch is
actually exercised; comment corrected. Re-verified green.
