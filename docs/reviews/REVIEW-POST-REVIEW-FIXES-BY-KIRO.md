# REVIEW: Post-Review Fixes (P0 signed manifest + P1 auto-update posture)

**Branch:** `codex/bluey-ai-site`
**Commit reviewed:** `6821bfd fix(update): verify signed release manifests`
**Source handoff:** `docs/rounds/POST-REVIEW-FIXES-FOR-KIRO-REVIEW.md`
**Reviewer:** Kiro
**Date:** 2026-06-13
**Method:** Read on uno only; nothing pulled/run locally; no subagents.

---

## Verdict

🟢 **ACCEPT** — the signed-updater posture closes the A-1 elevated-risk
item from `REVIEW-CODEX-BLUEY-AI-SITE-BY-KIRO-2026-06-12.md`. A compromised
`bluey.sh` host can no longer push a malicious auto-update. Two minor
non-blocking nits for the next round (documentation-level), listed at the
end.

---

## Pipeline at `6821bfd`

```
cargo fmt --all --check                       CLEAN
cargo clippy --all-targets -- -D warnings     CLEAN (workspace + server)
cargo test -p cue-cli update                  10 passed (new signature tests)
cargo test --all-targets                      528 passed, 0 failed
cd server && cargo test                       156 passed, 0 failed
analyze-tracing-calls.py --check-only         (clean, per codex run)
observability-acceptance-smoke.sh             8/8 (per codex run)
```

---

## What I verified line-by-line

### P0.1 — Signature verified before any install-affecting trust — ✅

- `ManifestTrust` enum: `Verified` / `UnsignedAllowed(reason)` /
  `Unverified(reason)`. `permits_install()` is true only for
  Verified/UnsignedAllowed; `is_verified()` only for Verified.
- `verify_manifest_signature`: decodes the 32-byte ed25519 pubkey,
  decodes the detached 64-byte signature (base64 or raw),
  `VerifyingKey::from_bytes`, `verify(manifest_bytes, &signature)`.
  Standard `ed25519-dalek`. Correct.
- `check_for_update` computes `manifest_trust` from
  `verify_hosted_manifest` and carries it on the `UpdatePlan`. It does
  parse manifest fields for **notify-only display**, but install is gated
  entirely through `ensure_update_installable` — so manifest fields are
  parsed-but-not-trusted-for-install unless the signature verifies.

### P0.2 — Build-time public key — ✅

- `embedded_update_pubkey` = `option_env!("BLUEY_UPDATE_PUBKEY")`. The key
  is baked into the binary at compile time; a compromised host cannot swap
  it. A build without the key stays notify-only and cannot install signed
  updates (correct fail-safe).

### P0.3 — Tampered / missing signature cannot install — ✅

- Missing pubkey or signature-fetch failure → `manifest_trust_from_error`
  → `Unverified` (unless `BLUEY_UPDATE_ALLOW_UNSIGNED`) →
  `permits_install()` false → `ensure_update_installable` bails.
- Tampered manifest → signature verify fails → `Unverified` → install
  refused.
- Tests prove all three: `verifies_valid_release_manifest_signature`,
  `rejects_tampered_release_manifest_signature`,
  `rejects_missing_release_manifest_signature`,
  `unverified_manifest_is_not_installable_by_default`.

### P0.4 — install.sh + artifact sha256 pinned in the signed manifest — ✅

- `ensure_update_installable` refuses if `install_sha256` is None or
  `artifact_sha256` is None (unless ALLOW_UNSIGNED).
- `download_install_script(url, expected_sha256)` computes `sha256_hex` of
  the fetched bytes and bails on mismatch; bails if expected is None and
  not ALLOW_UNSIGNED. The old spoofable `contains("Bluey one-line installer")`
  trust check is gone — replaced by a `#!/` shell sanity check only (not a
  trust boundary, per the doc claim — confirmed).
- `install_update` passes `BLUEY_ARTIFACT_SHA256` (signed-manifest value)
  to install.sh, which verifies the artifact against it.
- Test `verified_manifest_still_requires_installer_and_artifact_hashes`
  proves Verified-but-missing-hashes is refused, both-present is allowed.
- Defensive bonus: setting `BLUEY_UPDATE_INSTALL_URL` drops `install_sha256`
  to None → install refused unless ALLOW_UNSIGNED. An env-redirected
  installer can't run unsigned.

### P1 — Default check-and-notify, not silent install — ✅

- `maybe_update_before_on`: with no `BLUEY_AUTO_UPDATE`, it prints
  availability + posture and **returns without installing**. Silent
  auto-install only happens when `BLUEY_AUTO_UPDATE=1` AND
  `ensure_update_installable` passes (signature verified + hashes pinned).
- `BLUEY_UPDATE_ALLOW_UNSIGNED=1` remains a dev-only escape with explicit
  warnings (`print_update_posture`).
- `manual_update` (`bluey update`) refuses unsigned/unverified by default.

### Publish side — ✅

- `scripts/publish-bluey-release.sh` computes install.sh + artifact
  sha256 into `latest.json`, signs the exact bytes with
  `openssl pkeyutl -sign -rawin -inkey <ed25519 PEM>` → base64 →
  `latest.json.sig`. Pure-Ed25519 over raw bytes, matching the dalek
  `verify`.
- **Production gate**: `PUBLISH_DO=1` without a signing key refuses to
  publish unless `BLUEY_RELEASE_ALLOW_UNSIGNED=1` is explicitly set. A
  real production publish cannot ship an unsigned manifest by accident.

---

## The closed gap (A-1)

Before: auto-update trusted `bluey.sh` TLS + hosting; the manifest sha256
gave zero protection against a compromised host (attacker sets both); the
content marker was spoofable; install ran silently every launch.

After: the manifest must carry a valid ed25519 signature from a key the
operator holds off-host, verified against a pubkey embedded in the binary
at build time. A compromised host cannot forge the signature, cannot swap
the install.sh or artifact (both hashes are signature-pinned), and the
default posture is notify-only. **The compromised-host → all-customers
malware path is closed.**

The only residual trust is the embedded public key (compile-time, in the
binary) — an attacker who can rewrite the customer's binary has already
won, which is out of scope for any release-integrity scheme.

---

## Nits (non-blocking; next round or operator docs)

### N-1 🟡 Signature is over exact `latest.json` bytes — operator must serve static

The client verifies the signature against the exact bytes it fetches. If a
CDN/proxy ever re-serializes or minifies JSON in transit, the bytes change
and verification fails (fail-safe → notify-only, not a security hole, but a
broken-update footgun). The release runbook should state: serve
`latest.json` and `latest.json.sig` as byte-identical static files, no
transform layer. (Worth one line in `docs/RELEASE-RUNBOOK.md` if not
already there.)

### N-2 🟡 No in-band key rotation / revocation

The pubkey is single + build-time embedded. Rotating the signing key (or
recovering from a key compromise) requires shipping a new CLI build with a
new embedded pubkey — old clients cannot verify new-key manifests, and
there is no remote revocation. Acceptable for alpha (rotation = ship a new
release), but document it as a known limitation: if the signing key leaks,
the response is "ship a new signed CLI build with a rotated pubkey," not a
remote revoke. A future hardening could embed N + N+1 keys to allow a
rotation window. (Tracking only; not blocking.)

---

## Recommended action

1. **P0 closes 🟢.** Auto-update is safe enough for wider alpha.
2. N-1 (runbook one-liner on static serving) + N-2 (document key-rotation
   limitation) — fold into `docs/RELEASE-RUNBOOK.md` whenever convenient;
   neither blocks.
3. The A-1 finding in `REVIEW-CODEX-BLUEY-AI-SITE-BY-KIRO-2026-06-12.md` is
   now resolved.

This verdict closes the post-review round. Both kiro-side (N-1 trace_id,
`66e5fa3`) and codex-side (P0/P1, `6821bfd`) items are done and green.
