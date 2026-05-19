# PHASE-3-ROUND-13-PLAN.md

**Status:** In progress. Codex completed R13.1, R13.2, and the installer/package
alignment slice on 2026-05-18. Larger RAG, Windows whisper, and platform-matrix
items remain separate implementation rounds.

R13 absorbs:

1. The two non-blocking nits codex raised in REVIEW-PHASE-3-ROUND-12.md (one R12.2 follow-up, one R12.3 follow-up).
2. The two items deferred from R12 itself (sqlite-vec RAG, Windows whisper.cpp).
3. Whatever cross-platform matrix expansion the user signs off on.

---

## R13.1 — Reset overlay UI state on cancel/error, not just on submit

**Status:** Done in Codex R13 pass.

**Source:** codex R12 review, R12.2 finding.

**Today:** the AttachRequested handler flips state to AttachOpen; only AttachFilesRequested flips it back to Idle. If the dialog is cancelled or errors before submit (no AttachFilesRequested ever arrives), state stays AttachOpen forever and the gate stays permissive for that modal event type. Same shape for InstructionsRequested / InstructionsUpdated.

In practice the daemon owns the dialog and drives the normal submit path, so the bug is latent. Codex correctly flagged it as a real correctness issue regardless.

**Fix options:**

- **A.** Wrap the handler body in a `defer`-style guard (`scopeguard::defer!` or hand-rolled `OnDrop`) that resets the state to Idle if the function returns early or panics, and only commits the open-state on success.
- **B.** Model the modal lifecycle explicitly with an enum like `AttachLifecycle::Opening | Open(handle) | Closing` and have the daemon's internal handlers signal close on cancellation/error.

Recommend A — it is a 10-line change.

**Estimate:** 1 hour including tests.

---

## R13.2 — `generate_session_token` returns `Result<String>` instead of panicking

**Status:** Done in Codex R13 pass.

**Source:** codex R12 review, R12.3 finding.

**Today:** `getrandom::getrandom()` is wrapped in `.expect("OS random source unavailable")`. If the OS entropy source is somehow unavailable (extremely rare), the daemon panics instead of failing overlay startup gracefully. Codex flagged as non-blocking but suggested a future hardening round.

**Fix:** change `pub fn generate_session_token() -> String` to `pub fn generate_session_token() -> Result<String, getrandom::Error>` and propagate via the `?` operator at the two call sites in app.rs. spawn_overlay can map the error into its `anyhow::Error` chain.

**Estimate:** 30 min.

---

## R13.3 — sqlite-vec / ANN for RAG (deferred from R12)

**Source:** codex R11 final chain review (R12.4).

Linear cosine scan today; switch to a proper vector index. Adopt sqlite-vec (sqlite extension; no rebuild of the DB file) or usearch (separate index file). Schema migration on first run.

**Estimate:** 1 day.

---

## R13.4 — Windows real whisper.cpp (deferred from R12)

**Source:** codex R10 review (R12.5).

Port SwiftWhisper integration to Windows via whisper.cpp linked directly. JSON IPC stays compatible.

**Estimate:** 2-3 days. **Blocker:** clean Windows test machine. Schedule when one is available.

---

## R13.5 — Cross-platform support matrix expansion

**Source:** codex R12 review, cross-task finding.

R12 honestly scoped v0.1.0 to macOS arm64. R13 expands that matrix:

- **macOS x86_64:** `cargo build --target x86_64-apple-darwin` + `lipo` for a universal binary OR a separate Intel tarball. Smoke test on a clean Intel Mac. Estimate 1 hr code + 30 min validation.
- **Linux x86_64:** cross-compile via `cross` or build on a Linux box; verify CPAL audio capture works on PulseAudio + PipeWire. Estimate 3 hr.
- **Windows x86_64:** blocked on R13.4 (real whisper) and clean Windows bench access.

Once each platform passes its own smoke test, update INSTALL.md, web/index.html, and the release notes to reflect the actual support matrix.

**Estimate:** 4-6 hours plus per-platform validation; do macOS x86_64 first since it is the cheapest expansion of the supported set.

---

## R13.6 — Optional: telemetry counter for overlay-reader rejections

**Source:** codex R12 review, cross-task finding.

Today the production reader thread logs `warn!` on every rejected line (token mismatch / length / state). For ops we want a counter so spikes are visible. Codex suggested gating this on having a real telemetry sink + privacy policy, which we don't have for v0.1.0. Pencil in once a sink lands.

**Estimate:** 30 min code + however long the privacy review takes.

---

## R13.7 — Optional: tested `scripts/install.sh` for `curl | sh`

**Status:** Partially done in Codex R13 pass. `scripts/install.sh` now supports
macOS arm64 release/archive installs, checksum verification via
`SHA256SUMS.txt`, a local `BLUEY_ARCHIVE` path for validation, and the
versioned `~/.local/bluey/<version>` layout. Clean-machine validation is still
required before we make `curl | sh` the public primary install path.

**Source:** codex R12 review, cross-task finding.

If the GA call-to-action is `curl https://… | sh`, we need an actual install script: places binaries in `~/.local/bin` (or `/usr/local/bin` with sudo), sets up the launchd plist, smoke-tests on a clean Mac. If we keep manual `tar xzf` for v0.1.0, this is not needed.

**Decision needed before GA:** which install path is primary? `tar xzf` for v0.1.0 is a defensible answer — keep it manual until we've validated the script on a clean machine.

**Estimate:** 1-2 hours including clean-machine validation.

---

## Order of work (recommendation)

For "drop the -alpha suffix" GA path:

1. **R12 doc-scope fix wave** — done.
2. **R13.1** (overlay state reset on cancel) — done.
3. **R13.2** (token Result return) — done.
4. **Installer/package alignment** — done for macOS arm64 archive installs;
   clean-machine validation still pending.
5. **(decision: cross-platform scope)** — if user wants Intel Macs in v0.1.0,
   do R13.5 macOS x86_64 portion.
6. **Cut v0.1.0 GA** — drop -alpha suffix, tag v0.1.0 after validation.

For everything else:

7. **R13.3** (sqlite-vec) — separate dedicated round, 1 day.
8. **R13.4** (Windows whisper) — separate, when bench available.
9. **R13.6** (telemetry) — when sink/privacy land.
10. **R13.7 clean-machine validation** — only if `curl | sh` becomes the primary install path.
