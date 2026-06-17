# Review: Managed Customer Path

Date: 2026-06-17
Reviewer: Codex
Verdict: 🟡 Accept with live-smoke requirement

## Findings

No code blockers found in the implemented scope.

The important production risk is not compile-time now; it is live-path validation. The daemon and overlay compile and tests pass, but the customer promise depends on an installed/signed-in account reaching `bluey-server` for both STT and managed answer streaming.

## What I Checked

- Overlay model menu no longer emits raw OpenAI/Anthropic model names.
- Normal daemon answer requests default to `CueManaged` lanes.
- Overlay-provided raw providers are ignored unless `BLUEY_DEV_DIRECT_PROVIDERS=1`.
- Direct STT and direct vision providers require explicit dev flags.
- Managed answers call `BlueyManagedProvider`, not the legacy OpenAI-compatible managed shim.
- Managed streaming rejects an EOF that never sends a final finished/billing chunk.
- Vision image data URLs are shared between the direct provider path and the managed provider request path.

## Verification

```bash
cargo check -p cue-daemon
cargo test -p cue-daemon --all-targets
cargo clippy -p cue-daemon --all-targets -- -D warnings
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

All passed locally.

## Required Live Smoke Before Calling This Done

1. Install/login as a normal Bluey user with no provider keys in the shell.
2. Click `Listen` and verify real mic/system transcript appears.
3. Ask a typed question in `Auto` and verify a managed answer streams.
4. Switch to `Instant`, `Balanced`, and `Deep` and verify each still returns through the server.
5. Use `Screen`/vision and verify server-side vision route is used.
6. Confirm no provider API key exists in desktop logs, environment-required docs, or UI output.

## Notes for Kiro

This round intentionally does not redesign the overlay. It fixes the routing contract beneath the UI: normal users should never need provider keys locally, and model controls should be Bluey product lanes rather than direct provider/model selection.
