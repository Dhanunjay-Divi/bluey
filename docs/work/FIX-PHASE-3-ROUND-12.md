# FIX-PHASE-3-ROUND-12.md

**Branch:** `feat/phase-3-round-12`
**Tip:** `27af6b5` (after fix wave)
**Stacked on:** `main` (post v0.1.0-alpha)
**Pipeline:** ✅ fmt + clippy `-D warnings` + build release + 361 cargo tests + 13 vitest tests + npm build + swift × 2 + `git diff --check` clean
**Codex verdict on R12 first review:** 🟡 ACCEPT WITH NITS (see `REVIEW-PHASE-3-ROUND-12.md`)
**Status of this fix wave:** all doc-scope nits cleared; non-blocking code nits folded into `PHASE-3-ROUND-13-PLAN.md`.

---

## Per-nit response

### Nit 1 — Platform support matrix vs. shipped artifacts (cross-task)

**Codex:** "Either produce and smoke-test the full matrix or narrow the docs/site/release notes to the artifact set that actually ships."

**Decision:** Narrow to macOS arm64 only for v0.1.0. Cross-platform matrix expansion is now `R13.5` — we will do macOS x86_64 first since it is the cheapest expansion, then Linux, then Windows (blocked on R13.4 / clean Windows bench).

**Files corrected:**

- `INSTALL.md` — completely rewritten. The advertised matrix is now arm64-only. Removed the Homebrew tap stanza, the Scoop stanza, the Linux + Intel Mac + Windows download URLs, and the "first launch permissions" claims that imply a polished GUI app. Added an explicit *"Requirements"* section, a *"Code signing and Gatekeeper"* section that explains the `xattr -d com.apple.quarantine` remediation honestly, and an *"Other platforms"* section that lists Intel/Linux/Windows as future work rather than a support promise.
- `web/index.html` — three claim corrections:
  1. Meta description was *"Bluey is a lightweight native AI overlay for macOS and Windows"* → now *"Bluey is a lightweight native AI overlay for macOS Apple Silicon (v0.1.0). … Windows and Linux support is on the roadmap."*
  2. Caption *"macOS and Windows native helpers"* → *"macOS native helpers for v0.1.0; Windows helpers planned for a later release."*
  3. Tile *"macOS uses ScreenCaptureKit and CoreAudio. Windows uses WASAPI."* → *"macOS (v0.1.0) uses ScreenCaptureKit and CoreAudio. A Windows port using WASAPI is planned."*
- `docs/release/RELEASE-v0.1.0-alpha.md` — added an explicit *"Supported platforms (v0.1.0-alpha)"* table at the top with ✅ macOS arm64 and ❌ for the other three platforms with brief reasons. Also added an *"Audience: internal alpha only — do not redistribute outside the team"* line under the released-on date.

### Nit 2 — Soften Gatekeeper / quarantine wording (cross-task)

**Codex:** "Should say signing/notarization are deferred and require clean-machine validation, not promise a platform security bypass."

**Files corrected:**

- `INSTALL.md` — the *"Code signing and Gatekeeper"* section now says signing is deferred, that browser-downloaded archives may attach `com.apple.quarantine`, that the curl-from-terminal path *typically* does not but is not guaranteed, and explicitly directs users to the `xattr -d com.apple.quarantine` remediation. *"Validate behaviour on a clean machine before distributing internally."*
- `docs/work/PHASE-3-ROUND-12-HANDOFF-FOR-CODEX-REVIEW.md` — replaced the absolute *"CLI binaries fetched from terminal are not subject to it"* and *"overlay child processes bypass quarantine"* claims with a hedged paragraph that lists the three factors that govern behaviour (browser vs. terminal download, direct vs. parent-process launch, macOS version enforcement defaults) and commits us to per-release clean-Mac validation.

### Non-blocking code nits → R13

The two non-blocking code nits raised in REVIEW-PHASE-3-ROUND-12.md are tracked in `PHASE-3-ROUND-13-PLAN.md`:

| Codex finding | R13 item | Estimate |
|---|---|---|
| R12.2: state stays AttachOpen if dialog is cancelled/errors before submit | R13.1 — reset overlay UI state on cancel/error via `defer`-style guard | 1 hr |
| R12.3: `generate_session_token` panics via `.expect()` if OS entropy unavailable | R13.2 — return `Result<String, getrandom::Error>` and propagate at call sites | 30 min |

Codex explicitly accepted these as deferrable. They will land in R13 alongside the deferred R12.4 (sqlite-vec RAG) and R12.5 (Windows whisper.cpp) items.

---

## What did NOT change

No code was modified in this fix wave. Codex accepted all R12 implementation work; the only nits were documentation/scope. The branch tip moves forward purely with doc updates.

---

## Verification

```
cargo fmt --all --check                                         ✅ (no code changes; still clean)
cargo clippy --all-targets -- -D warnings                       ✅
cargo build --all-targets --release                             ✅
cargo test --all-targets                                        ✅ 361 tests, 0 failures
(cd crates/cue-dashboard/ui && npm test)                        ✅ 13 vitest tests
(cd crates/cue-dashboard/ui && npm run build)                   ✅
swift build -c release --package-path native/macos/cue-overlay  ✅
swift build -c release --package-path native/macos/cue-whisper  ✅
git -P diff --check main..HEAD                                  ✅
```

---

## Re-review request (paste-ready)

> R12 doc-scope nits cleared. Branch `feat/phase-3-round-12` tip `27af6b5`.
>
> Per nit:
> 1. **Platform matrix:** narrowed v0.1.0 scope to macOS arm64 only across `INSTALL.md`, `web/index.html`, and `docs/release/RELEASE-v0.1.0-alpha.md`. Cross-platform expansion is now `R13.5` (macOS x86_64 first, then Linux, then Windows).
> 2. **Gatekeeper wording:** softened in `INSTALL.md` and `PHASE-3-ROUND-12-HANDOFF-FOR-CODEX-REVIEW.md`. No more "CLI binaries are not subject to Gatekeeper" or "overlay child processes bypass quarantine" claims. Now says signing is deferred, lists the factors that actually govern behaviour, and commits to per-release clean-Mac validation. The `xattr -d com.apple.quarantine` remediation is documented in INSTALL.md.
>
> Non-blocking code nits (R12.2 cancel-path state reset, R12.3 panic vs. Result) folded into `PHASE-3-ROUND-13-PLAN.md` as R13.1 and R13.2.
>
> Pipeline still green. No code changes in this fix wave; only docs.
>
> If 🟢, we drop the `-alpha` suffix and tag v0.1.0 GA on this same branch tip.
