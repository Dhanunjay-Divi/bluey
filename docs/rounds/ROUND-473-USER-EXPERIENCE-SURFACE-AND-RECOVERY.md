# Round 473 - User Experience Surface And Recovery

Date: 2026-07-10

Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Make Bluey's normal experience read as one calm product rather than a collection of model, route, provider, storage, and recovery internals. The intended flow is:

```text
Ask, speak, attach, or capture
-> Bluey shows what it is checking
-> Bluey starts the right answer automatically
-> structured work opens in the workbench
-> interruptions remain recoverable
-> the complete session saves automatically
```

This round is local only. It did not deploy, publish a release, run GitHub Actions, commit, or push.

## Product Surface

| Experience | Current state after this round |
| --- | --- |
| Automatic task handling | `Auto` remains the default. Provider names, model names, and internal lane names remain behind diagnostics. |
| Quick or Thorough override | macOS and Windows expose only `Auto`, `Quick`, and `Thorough`; the server still owns provider selection and fallback. |
| Live listening | Existing Deepgram Nova-3 streaming renders interim text immediately and waits for final transcript settling before answer submission. The default English profile is `en-IN` and remains overridable for other English accents. |
| Screen context | A capture is shown as pending context, travels with the submitted question, then leaves the composer and remains available through the session's files/context controls. |
| Files | Context items now carry `processing_status`. macOS and Windows label them `Reading`, `Ready`, or `Needs attention` instead of silently accepting unusable content. |
| Research | The managed server can recognize research needs, sanitize the query, enforce account safeguards, emit search/read progress, and return source attachments. macOS and Windows render clickable web-source chips, including while click-through is active. |
| Workbench | macOS opens code and structured artifacts automatically. Code/design follow-ups append a complete new version while preserving prior versions and keeping the conversation on the left. |
| Continue or Retry | Recoverable partial answers are preserved. macOS shows `Continue` or `Retry` on the affected answer; Windows changes the answer command to the matching recovery action. |
| Context indicator | macOS shows compact context names such as `Screen · Resume.pdf · +1`; file details remain behind the context control after send. Windows keeps the same information in its context chips. |
| Reliable history | Existing stable session/turn/response/context/transcript IDs, durable local persistence, cloud sync, audit bundles, and empty-shell filtering were reverified. |

## User-Facing Cleanup

- Normal overlay controls no longer expose `instant`, `balanced`, or `deep`.
- Workbench subtitles no longer show raw confidence percentages.
- Account Session History no longer shows provider/model diagnostics or the diagnostic panel.
- Public balance copy says `more detailed answers` instead of `deeper routes`.
- Public product copy says `automatic task handling` instead of `model routing`.
- Raw provider failures and 429 details remain in diagnostic logs and audit bundles, while the user sees a short retry/recovery action and reference.
- Source context is presented as sources, not as internal retrieval or RAG terminology.

## Recovery Contract

When a stream fails before useful answer text, Bluey shows a friendly `Retry` action. When useful text has already streamed, Bluey keeps it and offers `Continue` without asking the user to repeat the question. The audit path still records the exact internal error, request reference, route attempts, and visible UI state.

## Workbench Version Contract

- Streaming deltas for one answer update the current artifact in place.
- A completed coding or system-design follow-up creates a complete new workbench version.
- Earlier versions stay navigable.
- Conversation and explanation remain on the left; the active structured artifact remains on the right.

## Files Changed In This Round

- `crates/cue-core/src/overlay.rs`
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `web/assets/bluey-site.js`
- `web/index.html`

The worktree also contains substantial parallel work from earlier rounds. Nothing unrelated was reverted or cleaned.

## Verification

Passed locally:

```text
cargo test -p cue-core overlay --lib
cargo test -p cue-daemon user_facing_answer_error --lib
cargo test -p cue-daemon cloud::sync::tests --lib
cargo test -p cue-daemon transcript --lib
cargo test -p cue-daemon --lib
cargo test --manifest-path server/Cargo.toml answer_plan --lib
cargo test --manifest-path server/Cargo.toml db::sync::tests --lib
cargo test --manifest-path server/Cargo.toml deepgram_url --lib
xcrun swiftc -typecheck native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -std=c11 -DUNICODE -D_UNICODE -fsyntax-only native/windows/cue-overlay/main.c
node --check web/assets/bluey-site.js
cargo fmt --all -- --check
git diff --check
```

Full daemon result: 347 passed, 0 failed, 5 intentionally ignored hardware/Keychain tests.

## Remaining Gates

1. Windows still needs a full multi-version workbench equivalent to macOS. Its current artifact surface is more limited.
2. Search progress currently becomes visible through the managed answer stream; a future transport improvement can stream pre-search progress before retrieval completes.
3. Native visual QA on real macOS and Windows windows is still required before a signed release.
4. Real microphone/system-audio acceptance testing remains required because unit tests cannot prove hardware permissions, acoustic quality, or provider network latency.

These are explicit release gates. This round does not claim that the undeployed native surfaces have already passed live cross-platform QA.
