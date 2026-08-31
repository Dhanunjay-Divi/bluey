# IMPL: Phase 623 - Hosted Answer And UI Recovery

> **Codex preflight:** Load `$bluey-ops` before implementation and verify its
> memory against the current repository state and task-specific docs.

## Scope

**Does:**

- Restores the hosted managed-answer request path without weakening the private
  prompt disclosure boundary.
- Preserves exact signed-client compatibility from v0.1.97 through v0.1.104.
- Reduces managed General-mode prompt duplication while keeping specialized,
  direct/BYOK, and explicit session rules.
- Produces bounded desktop error handling for a disclosure-block response.
- Refines the existing native macOS overlay instead of replacing the hosted API
  or daemon architecture.
- Restores a genuinely compact 112 by 30 pill with one visible listen/pause
  control; Ask and power remain available in the expanded workspace.
- Improves light-theme contrast, history readability, confirmation and shortcut
  layouts, keyboard/accessibility labels, and the bounded 1040 by 620 focus
  workspace.
- Adds a one-time, focus-safe post-sign-in coachmark anchored to the real
  Shortcuts control and keeps shortcut help readable at every overlay opacity.
- Clarifies that the browser already carries the desktop connection code, so
  sign-in needs one explicit secure confirmation and manual entry is fallback.
- Restores rolling-stable Rust 1.98 strict-Clippy compatibility without changing
  wire behavior.

**Does NOT:**

- Deploy the server, publish a release, merge to `main`, or change production
  flags.
- Change billing, account authentication, provider secrets, model routing, or
  keychain behavior.
- Weaken capture exclusion or ship any capture-visible development flag.
- Implement a Windows UI equivalent or claim physical Windows validation.
- Prove a production latency target; the change removes known duplicate prompt
  work and requires a live post-deployment remeasurement.
- Implement or enable automatic remote diagnostics. New UI actions use the
  existing non-blocking local logger; the typed bounded diagnostic bus, consent,
  authenticated server ingest, and private retention are Phase 624 scope.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-core/src/prompt_contracts.rs` | Modified | Shared exact managed wire contracts and compatibility allowlist |
| `crates/cue-cloud-client/src/error.rs` | Modified | Bounded disclosure-block error variant |
| `crates/cue-cloud-client/src/client.rs` | Modified | Allowlisted error parsing for complete and stream paths |
| `crates/cue-llm/src/bluey_managed.rs` | Modified | Managed LLM error mapping and regression coverage |
| `crates/cue-cli/src/app.rs` | Modified | Clear one-confirmation desktop sign-in copy |
| `crates/cue-core/src/ipc_auth.rs` | Modified | Rust 1.98 fixed-width byte conversion |
| `crates/cue-daemon/src/app.rs` | Modified | Prompt compaction, sign-in copy, discrete UI diagnostics, compatibility fixes, and tests |
| `crates/cue-daemon/src/audio/framer.rs` | Modified | Preallocated buffer swap and refill preservation for Rust 1.98 |
| `crates/cue-daemon/src/audio/system_capture.rs` | Modified | Typed PCM parsing for Rust 1.98 |
| `crates/cue-daemon/tests/system_audio_integration.rs` | Modified | Load-safe bounded helper-startup assertions |
| `crates/cue-daemon/src/rag_indexer.rs` | Modified | Exhaustive bounded cloud-error mapping |
| `crates/cue-rag/src/store.rs` | Modified | Typed embedding-byte parsing for Rust 1.98 |
| `server/src/api/router.rs` | Modified | Exact contract authority and untrusted-field guard |
| `server/src/api/router/completion.rs` | Modified | Completion authority plumbing |
| `server/src/api/router/streaming_completion.rs` | Modified | External and internal authority selection |
| `server/src/api/router/embeddings.rs` | Modified | Narrow Axum error-envelope compatibility annotation |
| `server/src/api/router/transcribe.rs` | Modified | Narrow Axum error-envelope compatibility annotation |
| `server/src/api/stt.rs` | Modified | Typed PCM parsing for Rust 1.98 |
| `server/src/api/router/tests.rs` | Modified | Contract, guard, and internal-authority regressions |
| `server/tests/integration_e2e.rs` | Modified | Current and legacy HTTP completion coverage |
| `native/macos/cue-overlay/Sources/cue-overlay/main.swift` | Modified | Compact pill and expanded-workspace UX/accessibility polish |
| `native/macos/cue-overlay/Sources/cue-overlay/ShortcutCoachmarkView.swift` | Added | Accessible control-anchored post-sign-in help |
| `web/assets/bluey-site.js` | Modified | One-confirmation browser device-link experience |
| `web/index.html` | Modified | Fallback-code download and account instructions |
| `docs/work/FIX-584-hosted-managed-answer-contract.md` | Added | Bug diagnosis, security boundary, and test handoff |
| `docs/work/FIX-585-light-shortcuts-and-desktop-signin.md` | Added | Light-theme, coachmark, and sign-in handoff diagnosis |
| `docs/work/FIX-586-rust-1-98-clippy-compatibility.md` | Added | Rolling-stable compiler compatibility record |
| `docs/work/FIX-587-system-audio-startup-test-race.md` | Added | Load-safe bounded helper-startup regression record |
| `docs/work/IMPL-PHASE-623-HOSTED-UI-RECOVERY.md` | Added | Batch scope and verification record |
| `docs/work/REVIEW-PHASE-623-HOSTED-UI-RECOVERY.md` | Added | Source review and remaining merge gates |
| `CHANGELOG.md` | Modified | Unreleased customer-visible changes |

## Build & Test

Confirmed on the current worktree:

```bash
# Passed on the final Rust source diff.
cargo +1.98.0 test --workspace
cargo +1.98.0 clippy --workspace --all-targets -- -D warnings
(cd server && cargo +1.98.0 test)
(cd server && cargo +1.98.0 clippy --all-targets -- -D warnings)
node --check web/assets/bluey-site.js

# Passed on the current compatibility implementation.
cargo test -p cue-core prompt_contracts::tests
(cd server && cargo test every_supported_release_contract_is_accepted)
(cd server && cargo test every_supported_contract_accepts_benign_rules)
(cd server && cargo test --test integration_e2e \
  router_complete_accepts_exact_legacy_signed_release_contract)

# Passed on the current macOS overlay implementation.
swift build -c debug --package-path native/macos/cue-overlay
# Also passed with BLUEY_AUTH_UI_POLICY_TESTS (including
# ShortcutCoachmarkView.swift),
# BLUEY_MEETING_EVIDENCE_TESTS, BLUEY_CONTEXT_STAGING_TESTS, and
# BLUEY_OVERLAY_SEQUENCE_PROTOCOL_TESTS.
```

Visible macOS QA also passed for the 112 by 30 idle pill, its distinct
22 by 22 listen/pause hit target, click-to-expand behavior, the unchanged
expanded workspace path, light and dark themes, fitted shortcut help, readable
history, the bounded focus workspace, the light post-sign-in coachmark, and the
full light shortcut modal. Visual QA used development-only capture visibility;
that flag is not part of the release configuration.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| The pill exposes one inline control instead of three | The owner explicitly restored the small footprint; Ask and power remain in the expanded workspace. |
| Exact historical contracts remain accepted | Supporting only the newest contract would have broken still-signed v0.1.97-v0.1.101 clients. |
| Focus mode is bounded rather than full-screen | The bounded workspace preserves context and avoids turning the overlay into an oversized opaque surface. |
| Full diagnostics are a separate Phase 624 batch | Per-event synchronous audit persistence would harm the latency requirement; the production design needs a typed bounded queue, coalescing, consent, and a separate server ingest boundary. |

## Known Follow-ups

- Validate the exact packaged artifact on physical macOS and Windows before any
  customer release.
- Confirm pill drag and persisted position with a physical macOS pointer. The
  automated drag gesture expanded the pill even though the drag handlers are
  unchanged from `origin/main`.
- After server deployment, measure managed time-to-first-token with the same
  account, route, region, and prompt corpus used for the baseline.
- Consider a separate bounded `managed_contract_unsupported` response so a
  skewed unsigned/development client receives update guidance instead of the
  conservative disclosure-block copy.
- Implement Phase 624's typed, content-free diagnostic spine with one action ID
  across native input, daemon, audio/STT, RAG, managed routing, stream, and
  native first/final render. It must use `try_send`, bounded/coalesced queues,
  owner-only local storage, explicit remote consent, and server-only R2 access.

## Review Checklist (for reviewer)

- [x] Files match the described hosted-answer and macOS UI scope
- [x] Current and historical managed contracts use exact matching
- [x] Caller-controlled answer rules and request fields remain scanned
- [x] No arbitrary server body reaches the desktop error surface
- [x] Direct/BYOK and explicit modes keep their answer instructions
- [x] Compact-pill visual QA passes at 112 by 30
- [x] Light coachmark and shortcut modal remain readable at low overlay opacity
- [x] Delayed onboarding cannot replace active UI or steal composer focus
- [x] Device sign-in retains one explicit secure confirmation and fallback code
- [x] UI logging excludes continuous traffic and synchronous audit writes
- [x] Rust 1.98 root and server strict-Clippy gates pass
- [x] No deployment, release, keychain, billing, or capture-policy mutation
- [x] Final full workspace/server test and strict Clippy gates passed
- [x] Final independent source verdict is green with no merge blocker
