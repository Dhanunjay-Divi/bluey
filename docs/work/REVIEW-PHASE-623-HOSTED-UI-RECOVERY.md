# REVIEW: Phase 623 - Hosted Answer And UI Recovery

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory
> against the current repository state and commit range.

**Commit range:** `755e7d71..HEAD`
**Reviewer:** Codex source self-review plus independent source and visual QA
**Date:** 2026-08-30

## Per-Task Review

### Managed answer trust boundary

| Field | Value |
|-------|-------|
| Files | `cue-core` prompt contracts, server router, router unit and HTTP integration tests |
| Verdict | 🟢 accept |

**Findings:**

- The fixed Bluey contract is no longer scanned as caller-authored text.
- Trust is not inferred from a broad prefix. Only three exact signed-release
  contracts and the exact answer-rule separator are recognized.
- Every caller-controlled field and appended answer-rule tail remains subject
  to disclosure scanning.
- Unknown contracts, malformed tails, empty tails, direct disclosure requests,
  and confusable-character disclosure requests fail closed.
- The independent review initially identified old-client compatibility as a
  blocker. Exact v0.1.97-v0.1.101 contracts, pinned lengths and SHA-256 hashes,
  and HTTP compatibility coverage resolve that finding without fuzzy matching.

### Desktop error and latency path

| Field | Value |
|-------|-------|
| Files | cloud client, managed LLM adapter, daemon, RAG indexer |
| Verdict | 🟢 accept |

**Findings:**

- The desktop recognizes only the allowlisted structured block reason and does
  not expose arbitrary server error bodies.
- Streaming and buffered completion paths converge on the same bounded error.
- Managed General mode omits its duplicate instruction block; explicit modes,
  direct/BYOK paths, and explicit session rules remain covered by tests.
- This is a concrete request-size optimization, not yet evidence of a specific
  production latency result.

### macOS compact pill and expanded workspace

| Field | Value |
|-------|-------|
| Files | `native/macos/cue-overlay/Sources/cue-overlay/main.swift`, `ShortcutCoachmarkView.swift` |
| Verdict | 🟢 accept |

**Findings:**

- The pill is 112 by 30 with a logo, wordmark, state dot, and one independent
  listen/pause target. Ask and power actions remain available in the expanded
  workspace instead of widening the collapsed pill.
- The full visible 22 by 22 control rail is actionable. Its glyph and
  accessibility label match Start, Cancel connecting, and Pause states.
- Light-theme contrast, confirmation overlays, history rows, search, rename,
  shortcut alignment, and accessibility labels are materially improved.
- Focus mode uses a bounded 1040 by 620 workspace instead of occupying the
  entire display.
- Swift parse/build and visible QA passed for the compact pill, expansion,
  light/dark themes, shortcut help, history, and focus workspace.
- Automated edge clicks verified the entire 22 by 22 run target. Automated drag
  gestures expanded rather than moved the pill, so physical pointer drag and
  position persistence remain packaged-mac certification checks; the source
  drag handlers are unchanged from `origin/main`.
- Capture-visible settings were used only for local visual QA and are not part
  of the release configuration.
- The post-sign-in coachmark is anchored to the real Shortcuts button, appears
  once, does not replace active overlays or history, and defers when a text
  editor has focus. Directly opening Shortcuts consumes the tip.
- Light coachmark and full shortcut-modal screenshots confirm opaque, readable
  text and controls even though the surrounding overlay remains translucent.

### Desktop sign-in handoff

| Field | Value |
|-------|-------|
| Files | CLI, daemon, native overlay, hosted account JavaScript and HTML |
| Verdict | 🟢 accept |

**Findings:**

- The signed browser URL still carries the one-time code and server approval
  remains explicit; no device is silently attached.
- Copy now presents one `Connect this Bluey` confirmation. Re-entering the code
  is accurately described as a fallback.
- No token persistence, account authority, keychain, or authentication endpoint
  changed.

### Diagnostics and Rust 1.98 gates

| Field | Value |
|-------|-------|
| Files | Daemon action classification plus Rust compatibility sites in root and server |
| Verdict | 🟢 accept for Phase 623 scope |

**Findings:**

- Discrete native actions enter the existing metadata-only non-blocking local
  log. Shown/hidden acknowledgements and continuous opacity events do not.
- An initial per-event session-audit draft was rejected during review because
  it could fsync and consume audit capacity on the answer path; no such write is
  present in the final diff.
- Full click-to-pixel correlation and optional remote diagnostic sharing remain
  explicit Phase 624 work, not a Phase 623 claim.
- Root and server strict Clippy pass on Rust 1.98. The code changes are typed
  chunk conversions, a preallocated replacement-buffer swap, and narrow Axum
  error-envelope lint annotations; public response behavior is unchanged.
- The system-audio stub emitted the exact protocol handshake and 64,000-byte
  PCM payload. Its integration test now honors the existing five-second outer
  deadline rather than treating the first 500-millisecond quiet poll as a
  terminal failure; five repeated focused runs and the final workspace run
  passed.

## Cross-Task Findings

- The answer-path fix and UI recovery are cohesive for the hosted desktop: a
  working managed response path now has bounded recovery copy and a compact,
  usable native surface.
- No billing, authentication authority, provider-secret, keychain,
  capture-exclusion, or production-flag boundary changed.
- No merge, deployment, release publication, or physical Windows validation is
  represented by this source batch.

## Build & Test Verification

```text
Final full root workspace tests: passed
Final full server tests: passed (805 unit + 78 HTTP integration + auxiliary tests)
Final root strict Clippy for all targets: passed
Final server strict Clippy for all targets: passed
Final root and server debug builds: passed
Current prompt-contract compatibility tests: passed
Current server contract/guard unit tests: passed
Current legacy signed-client HTTP integration test: passed
Current Swift syntax parse: passed
Current Swift debug build: passed
Current four embedded Swift behavior suites: passed
Current compact-pill and expanded-workspace visual QA: passed
Current light post-sign-in coachmark and shortcut-modal visual QA: passed
Current hosted JavaScript syntax check: passed
Current Rust 1.98 root and server strict Clippy: passed
Both Rust formatting checks and git diff --check: passed
Physical packaged Windows/macOS release certification: not performed
```

## Overall Verdict

🟢 **ACCEPT** - The final source diff, exact compatibility boundary,
compact-pill interaction model, accessibility semantics, full test suites, and
strict lint gates are sound. Deployment and customer release remain separate
physical-artifact gates and are not represented by this verdict. Independent
review agrees: source/merge is green; packaged deployment remains yellow until
physical macOS drag/persistence and packaged macOS/Windows certification pass.

## Follow-ups for Next Batch

- Remeasure managed time-to-first-token after the matching server build is
  deployed to an approved environment.
- Run physical packaged macOS and Windows certification before release.
- Give unsupported contract skew its own bounded update-oriented reason instead
  of reusing disclosure-block recovery copy.
- Implement the separately reviewed Phase 624 typed diagnostic bus and remove
  remaining answer-hot-path synchronous support-audit persistence before
  claiming full click-to-pixel bottleneck attribution.
