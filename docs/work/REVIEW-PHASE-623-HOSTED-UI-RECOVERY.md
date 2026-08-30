# REVIEW: Phase 623 - Hosted Answer And UI Recovery

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory
> against the current repository state and commit range.

**Commit range:** `755e7d71..a37ebdaa`
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
| Files | `native/macos/cue-overlay/Sources/cue-overlay/main.swift` |
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

## Cross-Task Findings

- The answer-path fix and UI recovery are cohesive for the hosted desktop: a
  working managed response path now has bounded recovery copy and a compact,
  usable native surface.
- No billing, authentication, provider-secret, keychain, capture-exclusion, or
  production-flag boundary changed.
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
