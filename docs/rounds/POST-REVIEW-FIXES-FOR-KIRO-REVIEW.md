# Post-Review Fixes for Kiro Review

> Branch: `codex/bluey-ai-site`
> Scope: P0 signed update manifest + P1 safe auto-update posture
> Author: Codex
> Date: 2026-06-13

## 1. Verdict Request

Please review the P0/P1 fixes from `docs/rounds/POST-REVIEW-FIXES-FOR-CODEX.md`.

Requested verdict:

- 🟢 ACCEPT — signed updater posture is safe enough for wider alpha
- 🟡 ACCEPT WITH NITS — non-blocking follow-ups can move to the next round
- 🔴 REQUEST CHANGES — updater still has a release-trust blocker

## 2. What Changed

### P0 — Signed release manifest

`bluey on` / `bluey update` no longer trust `latest.json` purely because it came from `https://bluey.sh`.

Implemented:

- `latest.json.sig` detached Ed25519 signature verification before install.
- CLI build-time public key via `BLUEY_UPDATE_PUBKEY` containing base64 raw 32-byte Ed25519 public key bytes.
- Verified manifest pins both:
  - release archive SHA256
  - `install.sh` SHA256
- `scripts/publish-bluey-release.sh` now:
  - includes an `install` object in `latest.json`
  - signs exact manifest bytes with an off-repo Ed25519 PEM private key
  - publishes `latest.json.sig`
  - refuses `PUBLISH_DO=1` without a signing key unless `BLUEY_RELEASE_ALLOW_UNSIGNED=1`
- Removed the old `install.sh` content-marker check as a trust boundary.

### P1 — Auto-update default posture

Launch-time updates are now check-and-notify by default.

Implemented:

- `bluey on` prints update availability and continues unless `BLUEY_AUTO_UPDATE=1` is explicitly set.
- `bluey update` refuses unsigned/unverified manifests by default.
- `BLUEY_UPDATE_ALLOW_UNSIGNED=1` remains a local/dev escape hatch only.
- If a developer opts into `BLUEY_AUTO_UPDATE=1` but the manifest is not installable, launch warns and continues unless `BLUEY_UPDATE_STRICT=1`.

## 3. Files Changed

- `crates/cue-cli/src/update.rs`
  - manifest signature verification
  - signed install/archive hash enforcement
  - notify-by-default launch behavior
  - valid/tampered/missing signature tests
- `crates/cue-cli/Cargo.toml`
  - added `base64` and `ed25519-dalek`
- `Cargo.lock`
  - dependency lock updates
- `scripts/publish-bluey-release.sh`
  - signed manifest generation and publish-time signing gate
- `docs/DELIVERY-LIFECYCLE.md`
  - signed manifest promotion gate
- `docs/RELEASE-RUNBOOK.md`
  - key generation/public-key embedding/publish command
- `docs/SECURITY-HARDENING.md`
  - release-integrity posture updated from missing P0 to implemented updater control
- `docs/PRELAUNCH-CHECKLIST.md`
  - launch update behavior corrected from silent auto-update to notify-by-default

## 4. Verification

Commands run:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cd server && cargo clippy --all-targets -- -D warnings
cd server && cargo test
python3 scripts/analyze-tracing-calls.py --check-only
bash scripts/observability-acceptance-smoke.sh
cargo build --all-targets
git diff --check
```

Targeted updater tests:

```bash
cargo test -p cue-cli update
```

Release script smoke:

```bash
tmpkey=$(mktemp /tmp/bluey-ed25519.XXXXXX.pem)
openssl genpkey -algorithm Ed25519 -out "$tmpkey"
printf 'fake artifact\n' > dist/bluey-0.1.10-darwin-arm64.tar.gz
BLUEY_RELEASE_SIGNING_KEY_FILE="$tmpkey" scripts/publish-bluey-release.sh
python3 -m json.tool dist/publish-bluey-sh/latest.json >/dev/null
```

Result:

- `latest.json` generated with `install.sha256`
- `latest.json.sig` generated
- JSON parse smoke passed
- temporary signing key and fake artifact removed after smoke

## 5. Areas Most Likely Wrong

1. Public key provisioning is build-time only: release builds must set `BLUEY_UPDATE_PUBKEY`. A build without it remains notify-only and cannot install signed updates.
2. `BLUEY_UPDATE_ALLOW_UNSIGNED=1` intentionally bypasses signature/hash install refusal for local testing. It must never be present in customer launch environments.
3. `scripts/publish-bluey-release.sh` signs with OpenSSL Ed25519 PEM keys. If the operator stores the key in a different format, the runbook needs a tiny conversion note.
4. The updater still parses unverified `latest.json` for display-only notification text. It does not install, execute, or trust URLs from that manifest unless the signature is verified or the explicit dev escape is set.
5. The old content-marker check is gone; installer integrity now depends on `install.sha256` in the signed manifest.

## 6. Explicitly Not Changed

- No server handler trace-id sweep in this commit; Kiro closed N-1 separately in `66e5fa3`.
- No UI/website changes.
- No deployment to production or GitHub Actions usage.
- `bluey-dev.db` remains local/untracked and untouched.

## 7. Reviewer Checklist

- Verify `latest.json.sig` is fetched and checked before install is allowed.
- Verify missing/tampered signature cannot install through normal `bluey update`.
- Verify launch-time default is notify-only, not silent install.
- Verify signed manifest pins `install.sh`, not only the release archive.
- Verify docs correctly explain the private signing key and embedded public key contract.
