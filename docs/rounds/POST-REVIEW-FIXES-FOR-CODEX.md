# Round Handoff: Post-Review Fixes — Kiro → Codex

```
Branch:     codex/bluey-ai-site
Tip:        5b8cff8 (kiro review doc)
Author:     Kiro
Owner:      Codex (implementation)
Reviewer:   Kiro (will verdict codex's fixes)
Source review: docs/reviews/REVIEW-CODEX-BLUEY-AI-SITE-BY-KIRO-2026-06-12.md
```

## Context

Kiro line-by-line reviewed the 153-commit `codex/bluey-ai-site` batch
(`8ea42ac..a29d2f1`). Full findings in the source review doc above.

Verdict was 🟢 ACCEPT — the money path (Square + streaming billing),
auth OTP, and the capture-visible release gate are all sound. **One
elevated-risk item needs implementation work**, plus a few minor
non-blocking notes you can take or defer.

## The contract for this round

1. Codex implements the items below (P0 first).
2. Codex writes a fix/implementation doc back to Kiro at
   `docs/rounds/POST-REVIEW-FIXES-FOR-KIRO-REVIEW.md` with: what changed,
   why, files touched, verification commands run + observed results, and
   areas most likely wrong.
3. Kiro reviews codex's fixes and writes a verdict at
   `docs/reviews/REVIEW-POST-REVIEW-FIXES-BY-KIRO.md`.
4. Pipeline gate every commit (fmt + clippy -D warnings + tests + the
   observability analyzer `--check-only` + acceptance smoke).

---

## P0 — Signed release manifest for auto-update (REQUIRED before wider alpha)

**Source finding:** A-1 in the review doc.

`crates/cue-cli/src/update.rs` auto-updates on every `bluey on` by
default, silently (brief ESC window), and `bash`-executes a downloaded
`install.sh`. The only integrity guarantee today is TLS + trust that
`bluey.sh` hosting is not compromised. The manifest `sha256` gives zero
protection against a compromised host (the attacker sets both the
manifest and the artifact), and the `contains("Bluey one-line installer")`
content check is trivially spoofable. This is the ed25519 signed-manifest
P0 from the Observability round-close; auto-update shipped before it.

### Required implementation

1. **Sign the manifest.** Produce `latest.json` plus a detached
   `latest.json.sig` (ed25519 signature over the exact bytes of
   `latest.json`). Signing happens in the release/publish step
   (`scripts/publish-bluey-release.sh`) using an ed25519 private key the
   operator holds offline / in a secret store — never in the repo.

2. **Embed the public key in the CLI at build time.** A compile-time
   constant (e.g. `const BLUEY_UPDATE_PUBKEY: &str = "<base64 ed25519 pub>";`)
   so a compromised host cannot swap the key. Document the key-rotation
   story (ship N and N+1 keys during a rotation window).

3. **Verify before trusting any manifest field.** In `check_for_update`:
   fetch `latest.json` + `latest.json.sig`, verify the signature against
   the embedded pubkey, and ONLY parse/trust the manifest if the signature
   is valid. Reject (and skip the update) on signature failure with a clear
   log line. The artifact `sha256` must come from the verified manifest.

4. **Drop the spoofable content-marker check** as a security control (keep
   it only as a friendly "this isn't shell" sanity message if you like —
   but it is not a trust boundary).

5. **Until the signing pipeline is live, default to check-and-notify.**
   If `BLUEY_UPDATE_PUBKEY` / a valid signature is not available, do NOT
   silent-auto-install — print "Bluey <version> available; run `bluey update`"
   and continue on the current version. This makes code execution require
   an explicit user action until signing lands. Add a
   `BLUEY_UPDATE_ALLOW_UNSIGNED=1` dev escape hatch for local testing only.

### Crate suggestion

`ed25519-dalek` (already common in the ecosystem) for verification.
Verification is a few lines; the harder part is the publish-side signing
script + key handling. Keep the private key out of the repo and out of
CI logs.

### Tests to add

- Manifest with a valid signature → accepted.
- Manifest with a tampered body (signature no longer matches) → rejected,
  update skipped.
- Manifest with a missing/invalid signature → rejected (or check-and-notify
  if no pubkey configured).
- Round-trip: sign a fixture manifest with a test key, verify with the
  test pubkey.

---

## P1 — Auto-update default posture (pairs with P0)

Even after signing lands, consider whether every-launch silent auto-install
is the posture you want for alpha. Options:

- **Check-and-notify by default**, auto-install opt-in
  (`BLUEY_AUTO_UPDATE=1`). Safer; the customer chooses when to update.
- **Silent auto-install by default** (current), opt-out
  (`BLUEY_SKIP_UPDATE`). Smoother; relies fully on the signature.

Kiro recommendation: check-and-notify for alpha (less surprising, and a
bad release can't silently brick every customer mid-session), flip to
silent auto-install once the signed pipeline + a rollback story are
proven. Your call — document whichever you pick.

---

## Minor notes (take or defer; none blocking)

- **N-1 broader trace_id sweep. CLOSED by kiro at `66e5fa3`** — do NOT
  pick this up. The 4 managed handlers (complete, complete_stream, embed,
  transcribe) now carry `trace_id` in the `usage::record` billing-record
  logs. Pure observability (24 ins / 4 del), no billing/idempotency/
  control-flow touched; streaming path moves `trace_id` into the
  `async_stream` so deferred billing logs inherit it. Verified: server
  build + clippy -D clean, 156 server tests, analyzer --check-only clean,
  acceptance smoke 8/8.

- **N-2 web site visual QA.** `web/` is static (landing/account/install/link).
  Low security risk; needs an operator/codex visual pass on a browser,
  not a line review.

- **N-3 cosmetic.** Square idempotency reuses the `stripe_webhook_events`
  table with a `square:` event-id prefix. Functionally correct; the table
  name is now a slight misnomer. Not worth a migration.

---

## Suggested split

- **Codex:** P0 (signed manifest — publish-side signing + CLI verify +
  default posture) and P1 (default posture decision). This is your
  territory (CLI update path + release tooling).
- **Kiro:** N-1 (trace_id sweep) is DONE — committed at `66e5fa3` in
  parallel with this handoff. No remaining kiro scope in this round;
  kiro will review codex's P0 fix doc.

## Working-tree note

Per the collaboration contract §6: whoever has uncommitted changes owns
the tree. P0 is in `crates/cue-cli/src/update.rs` + `scripts/` + release
tooling — codex territory, no overlap with the server handler call sites
N-1 would touch. So we can run P0 (codex) and N-1 (kiro) in parallel if
you want, as long as we don't both edit the same files.

## Pipeline gate

Every commit:
```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings   # workspace + server
cargo test --all-targets                    # workspace + server
python3 scripts/analyze-tracing-calls.py --check-only
bash scripts/observability-acceptance-smoke.sh
```
