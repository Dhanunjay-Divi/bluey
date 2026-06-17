# Managed Customer Path for Kiro Review

Date: 2026-06-17
Branch: `codex/bluey-ai-site`

## Intent

Bluey customer paths should not depend on provider keys in the desktop environment. A normal user should sign in once, then:

`overlay / CLI -> daemon -> bluey-server -> provider`

This round moves the Listen and model-button behavior in that direction:

- Desktop direct STT keys are no longer consulted by default.
- Overlay model choices now send managed lane labels instead of raw OpenAI/Anthropic model names.
- Daemon answer routing now calls `BlueyManagedProvider` directly for `CueManaged` routes.
- Direct provider keys remain available only behind explicit developer flags.

## Changed Files

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Model menu payloads now emit managed lanes:
    - Instant -> `provider=managed`, `model=instant`, `mode=instant`
    - Balanced -> `provider=managed`, `model=balanced`, `mode=balanced`
    - Deep -> `provider=managed`, `model=deep`, `mode=deep`
    - Auto remains `provider=auto`.

- `crates/cue-daemon/src/app.rs`
  - Added dev gates:
    - `BLUEY_DEV_DIRECT_PROVIDERS=1`
    - `BLUEY_DEV_DIRECT_STT=1`
    - `BLUEY_DEV_DIRECT_VISION=1`
  - Normal `default_answer_request` and overlay requests use managed Bluey lanes unless the direct-provider dev gate is enabled.
  - Managed routes now call `BlueyManagedProvider` with an account-backed cloud client, preserving request id, session id, image data URLs, streaming, billing metadata, cost labels, and latency metadata.
  - Managed streams must produce a finished chunk before the overlay treats the answer as complete.
  - Vision fallback prefers signed-in Bluey accounts; direct screenshot providers are dev-gated.
  - STT config still supports local whisper/mock modes, but direct cloud STT provider keys require an explicit dev gate. Signed-in accounts use the server relay path.
  - Updated unit tests so normal overlay routes assert managed-lane behavior.

## Customer Behavior After This Round

- Clicking `Listen` should use the user's linked Bluey account and server STT relay when cloud STT is needed.
- Selecting `Auto`, `Instant`, `Balanced`, `Deep`, or screen/vision paths should route to Bluey-managed server lanes.
- Provider secrets stay on the server for normal users.
- A developer can still test direct desktop providers, but only by opting in with the `BLUEY_DEV_DIRECT_*` flags.

## Verification

```bash
cargo check -p cue-daemon
cargo test -p cue-daemon --all-targets
cargo clippy -p cue-daemon --all-targets -- -D warnings
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

Observed results:

- `cargo check -p cue-daemon` ✅
- `cargo test -p cue-daemon --all-targets` ✅ 202 lib tests plus integration tests passed; 0 failures
- `cargo clippy -p cue-daemon --all-targets -- -D warnings` ✅
- `swift build -c release --package-path native/macos/cue-overlay` ✅
- `git diff --check` ✅

## Areas Most Likely Wrong

1. `call_bluey_managed_provider` is covered by compile and route tests, but still needs a live account smoke to verify real server streaming, billing metadata, and cost labels through the overlay.
2. STT relay correctness depends on the saved account token/API URL being valid; live Listen smoke remains required.
3. The old `provider_client_config(CueManaged)` compatibility path remains for older env-based compat flows, but normal answer routing now bypasses it for `BlueyManagedProvider`.
4. Dev direct-provider flags are intentionally narrow. If a local developer expects `OPENAI_API_KEY` alone to drive the desktop, they now need `BLUEY_DEV_DIRECT_PROVIDERS=1` or the specific STT/vision dev flag.

## Reviewer Ask

Please verify:

- No customer path reads raw provider keys unless a `BLUEY_DEV_DIRECT_*` flag is set.
- Overlay model menu labels map to the intended managed lane.
- Managed stream EOF without `finished` does not produce a successful completed overlay answer.
- Live account Listen/Answer smoke works against the deployed Bluey server.
