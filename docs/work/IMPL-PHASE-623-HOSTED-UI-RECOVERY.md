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

**Does NOT:**

- Deploy the server, publish a release, merge to `main`, or change production
  flags.
- Change billing, account authentication, provider secrets, model routing, or
  keychain behavior.
- Weaken capture exclusion or ship any capture-visible development flag.
- Implement a Windows UI equivalent or claim physical Windows validation.
- Prove a production latency target; the change removes known duplicate prompt
  work and requires a live post-deployment remeasurement.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-core/src/prompt_contracts.rs` | Modified | Shared exact managed wire contracts and compatibility allowlist |
| `crates/cue-cloud-client/src/error.rs` | Modified | Bounded disclosure-block error variant |
| `crates/cue-cloud-client/src/client.rs` | Modified | Allowlisted error parsing for complete and stream paths |
| `crates/cue-llm/src/bluey_managed.rs` | Modified | Managed LLM error mapping and regression coverage |
| `crates/cue-daemon/src/app.rs` | Modified | Prompt compaction, typed recovery copy, and tests |
| `crates/cue-daemon/src/rag_indexer.rs` | Modified | Exhaustive bounded cloud-error mapping |
| `server/src/api/router.rs` | Modified | Exact contract authority and untrusted-field guard |
| `server/src/api/router/completion.rs` | Modified | Completion authority plumbing |
| `server/src/api/router/streaming_completion.rs` | Modified | External and internal authority selection |
| `server/src/api/router/tests.rs` | Modified | Contract, guard, and internal-authority regressions |
| `server/tests/integration_e2e.rs` | Modified | Current and legacy HTTP completion coverage |
| `native/macos/cue-overlay/Sources/cue-overlay/main.swift` | Modified | Compact pill and expanded-workspace UX/accessibility polish |
| `docs/work/FIX-584-hosted-managed-answer-contract.md` | Added | Bug diagnosis, security boundary, and test handoff |
| `docs/work/IMPL-PHASE-623-HOSTED-UI-RECOVERY.md` | Added | Batch scope and verification record |
| `docs/work/REVIEW-PHASE-623-HOSTED-UI-RECOVERY.md` | Added | Source review and remaining merge gates |
| `CHANGELOG.md` | Modified | Unreleased customer-visible changes |

## Build & Test

Confirmed on the current worktree:

```bash
# Passed on the final Rust source diff.
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
(cd server && cargo test)
(cd server && cargo clippy --all-targets -- -D warnings)

# Passed on the current compatibility implementation.
cargo test -p cue-core prompt_contracts::tests
(cd server && cargo test every_supported_release_contract_is_accepted)
(cd server && cargo test every_supported_contract_accepts_benign_rules)
(cd server && cargo test --test integration_e2e \
  router_complete_accepts_exact_legacy_signed_release_contract)

# Passed on the current macOS overlay implementation.
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
swift build -c debug --package-path native/macos/cue-overlay
# Also passed with BLUEY_AUTH_UI_POLICY_TESTS,
# BLUEY_MEETING_EVIDENCE_TESTS, BLUEY_CONTEXT_STAGING_TESTS, and
# BLUEY_OVERLAY_SEQUENCE_PROTOCOL_TESTS.
```

Visible macOS QA also passed for the 112 by 30 idle pill, its distinct
22 by 22 listen/pause hit target, click-to-expand behavior, the unchanged
expanded workspace path, light and dark themes, fitted shortcut help, readable
history, and the bounded focus workspace. Visual QA used development-only
capture visibility; that flag is not part of the release configuration.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| The pill exposes one inline control instead of three | The owner explicitly restored the small footprint; Ask and power remain in the expanded workspace. |
| Exact historical contracts remain accepted | Supporting only the newest contract would have broken still-signed v0.1.97-v0.1.101 clients. |
| Focus mode is bounded rather than full-screen | The bounded workspace preserves context and avoids turning the overlay into an oversized opaque surface. |

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

## Review Checklist (for reviewer)

- [x] Files match the described hosted-answer and macOS UI scope
- [x] Current and historical managed contracts use exact matching
- [x] Caller-controlled answer rules and request fields remain scanned
- [x] No arbitrary server body reaches the desktop error surface
- [x] Direct/BYOK and explicit modes keep their answer instructions
- [x] Compact-pill visual QA passes at 112 by 30
- [x] No deployment, release, keychain, billing, or capture-policy mutation
- [x] Final full workspace/server test and strict Clippy gates passed
- [x] Final independent source verdict is green with no merge blocker
