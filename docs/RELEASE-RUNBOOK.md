# Bluey Release Runbook

> **Step-by-step procedure for cutting a new Bluey release.**
> Mirrors Pinky's `RELEASE-RUNBOOK.md`.
>
> Last updated: 2026-06-13, post signed-updater hardening.

For the high-level lifecycle (environments, branches, promotion gates),
read `docs/DELIVERY-LIFECYCLE.md` first. This doc is the concrete
checklist.

## Deployment Execution Policy

Use this as the default rule when deciding how to build, test, and deploy:

- Preprod may use local Mac and Windows machines plus direct SSH/local scripts
  for fast iteration, smoke tests, and live validation.
- Production should use GitHub Actions or another auditable signed promotion
  path. Manual production deploys are allowed only for owner-approved emergency
  hotfixes, and the exception must be recorded in the round doc.
- Never publish an unsigned `latest.json`, an artifact without a checksum, or a
  manifest whose version/SHA does not match the release artifact.
- Build one immutable release id for operator tracking:
  `<version>-<commit12>`.
- Production must receive the exact artifact that passed preprod. Do not rebuild
  separately for production.
- Serialize production deploys with one active promotion at a time. GitHub
  Actions should use workflow concurrency with `cancel-in-progress: false`.
- For desktop releases, installers and client artifacts must pass:
  `scripts/release-hygiene-scan.sh`,
  `scripts/publish-bluey-release.sh` artifact scan,
  live manifest signature verification, installer MIME checks, artifact SHA
  checks, and unpacked binary version checks.
- For server/API releases, production must pass cloud preflight, backup/restore
  status, billing config, webhook config, and ledger/balance reconciliation
  checks before deploy.

**Golden rule:** build the release artifact once, store it, deploy that same
artifact to preprod, smoke preprod, then promote that exact stored artifact to
prod. Never rebuild between preprod and prod.

---

## Release Readiness Gate

Treat this as the stop-the-line gate before every preprod deploy and every
production promote.

Required before preprod:

- [ ] Intended commit is committed and pushed.
- [ ] A numbered `docs/rounds/ROUND-NNN-*.md` exists for meaningful changes.
- [ ] Release id is recorded as `<version>-<commit12>`.
- [ ] `scripts/release-hygiene-scan.sh` passes.
- [ ] Relevant platform checks pass:
  - Rust checks/tests for touched crates.
  - macOS Swift build or parse check for touched macOS overlay/helper code.
  - Windows syntax/build check for touched Windows overlay/helper code.
  - Server/API tests for routing, billing, web search, auth, export/delete, or
    storage changes.
- [ ] Release artifact is built with the production updater public key.
- [ ] `scripts/publish-bluey-release.sh` artifact scan passes locally before
  any publish.
- [ ] `bash scripts/test-publish-bluey-release.sh` passes the deterministic
  stage, immutable-installer checksum, and publication-order fixture.
- [ ] No secrets, dev capture-visible flags, local-only auth bypass flags, or
  plaintext provider keys are present in repo, docs, logs, or artifacts.
- [ ] If the release touches sign-in, account deletion, credits, auto-reload,
  usage billing, provider dispatch, web search, STT, embeddings, exports, or
  object storage, record a second-pass safety review in the round doc.

Required before production:

- [ ] Production receives the exact preprod artifact. Do not rebuild.
- [ ] Disk/storage check passed:
  [`docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md`](./ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md).
- [ ] Production backup status is healthy, or an on-demand backup and restore
  drill were completed immediately before promote.
- [ ] `scripts/bluey-cloud-preflight.sh` passes against the production env.
- [ ] Billing provider config is explicit and production webhook config is
  correct for the target environment.
- [ ] Billing changes have reconciliation proof: provider payment, local ledger,
  balance, usage rows, entitlement, refund/dispute state, and auto-reload state
  all agree for at least one test account.
- [ ] Search/provider changes have quota/cooldown/fallback proof so a 429 or
  provider loop cannot burn user credits or Bluey spend uncontrolled.
- [ ] Active user/session risk is acceptable and documented.

Required immediately after production:

- [ ] `scripts/bluey-release-live-verify.sh <version>` passes with the release
  public key or signing key available to the operator.
- [ ] `https://bluey.sh/latest.json` reports the intended version.
- [ ] `latest.json.sig` verifies against the release Ed25519 public key.
- [ ] The signed manifest points to `/releases/vX.Y.Z/install.sh` and
  `/releases/vX.Y.Z/install.ps1`; both immutable bytes match their manifest
  SHA256 and `SHA256SUMS.txt` entries.
- [ ] `/install.sh` returns `application/x-shellscript`.
- [ ] `/install.ps1` returns `application/x-powershell`.
- [ ] Live artifact SHA matches `SHA256SUMS.txt`.
- [ ] Unpacked live binaries report the intended version.
- [ ] Server health, billing health, and a signed-in desktop smoke pass for any
  release touching auth, billing, routing, STT, or overlay behavior.
- [ ] Monitor logs for the first 15 minutes and record any exception in the
  round doc.

---

## 0. Pre-flight

- [ ] All branches in scope are merged into `main` (no rebase needed).
- [ ] Codex chain review on the merge target has returned 🟢.
- [ ] `docs/rounds/PHASE-3-ROUND-N-PLAN.md` is up to date for the round.
- [ ] Model freshness gate in `docs/MODEL-ROUTING.md` is complete for this
      release: provider docs checked, route table/pricing reviewed, capacity
      notes reviewed, and live smoke plan recorded.
- [ ] `FUTURE-IMPLEMENTATIONS.md` has the items shipped this release moved
      to its `## Cleanup / archived` section.
- [ ] `docs/release/RELEASE-vX.Y.Z.md` is drafted (release notes).
- [ ] `DECISIONS.md` has any new decisions captured.

---

## 1. Pipeline gate (run on uno)

```bash
cd /Users/uno/Downloads/cue
git -P checkout main
git -P pull --rebase

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
cargo test --all-targets
( cd crates/cue-dashboard/ui && npm test && npm run build )
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
swift build -c release --package-path native/macos/cue-overlay --arch x86_64
swift build -c release --package-path native/macos/cue-audio --arch x86_64
swift build -c release --package-path native/macos/cue-whisper --arch x86_64
cargo build --release --target x86_64-apple-darwin
git -P diff --check
```

If anything fails, fix on a feature branch and merge before tagging.

---

## 2. Build artifacts

Production release archives contain only the terminal CLI, daemon, and trusted
native helpers. The dashboard UI must still pass its source tests/build in the
pipeline gate, but no Tauri `Bluey.app` bundle is built or published.

```bash
make package-darwin-arm64
make package-darwin-universal
BLUEY_UPDATE_PUBKEY="$(
  cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64
)" make package-windows-x86_64-gnu
ls -la \
  dist/bluey-*-darwin-*.tar.gz \
  dist/bluey-*-darwin-*.tar.gz.sha256 \
  dist/bluey-*-windows-x86_64.zip \
  dist/bluey-*-windows-x86_64.zip.sha256
```

Expected outputs:

```
dist/bluey-X.Y.Z-darwin-arm64.tar.gz
dist/bluey-X.Y.Z-darwin-arm64.tar.gz.sha256
dist/bluey-X.Y.Z-darwin-universal.tar.gz
dist/bluey-X.Y.Z-darwin-universal.tar.gz.sha256
dist/bluey-X.Y.Z-windows-x86_64.zip
dist/bluey-X.Y.Z-windows-x86_64.zip.sha256
```

The GNU cross-package command is the macOS/Linux fallback when a clean
Windows/MSVC builder is unavailable. `scripts/build-windows.ps1` and the
Windows release runner remain the canonical native Windows validation path.

---

## 3. Smoke test the artifact

Smoke the archive you will publish. Do not smoke a local `target/release`
binary and then publish a different archive.

```bash
tmp=$(mktemp -d)
BLUEY_ARCHIVE=dist/bluey-X.Y.Z-darwin-universal.tar.gz \
  BLUEY_INSTALL_DIR="$tmp/bluey" \
  BLUEY_BIN_DIR="$tmp/bin" \
  bash scripts/install.sh
"$tmp/bin/bluey" on
sleep 2
pgrep -fl "bluey-daemon|bluey-overlay" | head
"$tmp/bin/bluey" off
sleep 1
pgrep -fl "bluey-daemon|bluey-overlay" || echo "clean exit"
rm -rf "$tmp"
```

Both `bluey on` and `bluey off` must succeed. Daemon + overlay must
spawn from the canonical install dir, not from the dev build directory.

---

## 4. Update release docs

```bash
vim docs/release/RELEASE-vX.Y.Z.md
```

Required sections:
- Released date + audience
- Supported platforms table (✅/❌ with reason)
- What's in this release (capture / overlays / screen-share privacy / dashboard)
- Code signing status
- Known gaps (link to `FUTURE-IMPLEMENTATIONS.md`)
- Verification log (the pipeline output above)
- Artifact table with name + size + sha256
- Test count progression
- Model freshness table with:
  - checked date
  - OpenAI / Anthropic / Gemini / Deepgram / embedding model ids
  - pricing snapshot date
  - changed route candidates, if any
  - live-smoke trace ids or explicit waiver

---

## 5. Tag the release

```bash
TAG=vX.Y.Z
SHA=$(git -P rev-parse --short HEAD)

git -P tag -a "$TAG" -m "$(cat <<EOF
$TAG - <one-line summary>

<paragraph about what shipped>

Pipeline: cargo fmt + clippy -D + N tests + builds + smoke + git diff
--check all green on commit $SHA.

Artifacts:
  dist/bluey-X.Y.Z-darwin-arm64.tar.gz       sha256 ...
  dist/bluey-X.Y.Z-darwin-universal.tar.gz   sha256 ...
EOF
)"
```

Verify:

```bash
git -P tag -l "$TAG"
git -P show --stat "$TAG" | head -20
```

**No `git push`** unless the user explicitly says push (standing rule).

---

## 6. Publish to distribution server (when one exists)

This step publishes the already-built artifact. If a code/doc fix is
needed after preprod smoke, stop here, create a new commit and release id,
then rebuild a new artifact and restart preprod smoke from section 1.

```bash
BLUEY_RELEASE_SIGNING_KEY_FILE=/secure/off-repo/bluey-release-ed25519.pem \
PUBLISH_HOST=<host> PUBLISH_PATH=/var/www/bluey \
  PUBLISH_DO=1 \
  bash scripts/publish-bluey-release.sh
```

To mirror the same signed release files into R2/S3-compatible durable storage
while still serving public downloads from `bluey.sh`, add:

```bash
BLUEY_RELEASE_MIRROR_DESTINATION=s3://bluey-prod/releases/bluey-sh \
BLUEY_RELEASE_MIRROR_ENDPOINT_URL=https://<cloudflare-account-id>.r2.cloudflarestorage.com
```

The mirror stores root convenience copies of `install.sh` and `install.ps1`,
`latest.json`, `latest.json.sig`, and the versioned release directory. The
versioned directory contains the installer copies pinned by the signed
manifest and `SHA256SUMS.txt`; root installers are never part of that mutable
trust path.

The matching raw Ed25519 public key must be embedded in the CLI build:

```bash
export BLUEY_UPDATE_PUBKEY="$(
  openssl pkey -in /secure/off-repo/bluey-release-ed25519.pem -pubout -outform DER \
    | tail -c 32 | base64 | tr -d '\n'
)"
cargo build --release -p cue-cli --bin bluey
```

`latest.json.sig` is a detached signature over the exact bytes of
`latest.json`. The signed manifest also pins the platform installer
(`install.sh` on macOS/Linux, `install.ps1` on Windows) and archive
SHA256 values. Installer URLs in the manifest are immutable under
`releases/vX.Y.Z/`. The publisher exposes the detached signature and manifest
before replacing the root curl/irm convenience aliases, so a previously served
manifest can never checksum-pin newly replaced root bytes. Do not publish with
`BLUEY_RELEASE_ALLOW_UNSIGNED=1` outside local release testing.

Serve `latest.json` and `latest.json.sig` as static byte-identical files.
Do not run them through any CDN/proxy layer that rewrites, minifies,
pretty-prints, compresses in-place, or otherwise transforms the JSON bytes:
the client verifies the signature over the exact bytes it downloads.

Signing-key rotation is build-gated in this alpha. The CLI embeds one
raw Ed25519 public key at build time via `BLUEY_UPDATE_PUBKEY`; rotating
the private signing key, or recovering from a signing-key leak, requires
shipping a new CLI build with a new embedded public key. Future hardening
can embed current + next public keys to allow a rotation window.

Verify on the server:

```bash
ssh <host> 'curl -s http://localhost/latest.json | head'
ssh <host> 'ls -la /var/www/bluey/releases/'
```

`latest.json` should show the new version. The `latest` symlink should
point at the new release dir (atomic swap done by `publish.sh`).

Verify from the operator machine using the detached signature and live artifact:

```bash
BLUEY_RELEASE_PUBKEY_FILE=/secure/off-repo/bluey-release-ed25519.pub.pem \
  scripts/bluey-release-live-verify.sh X.Y.Z
```

If only the signing key is available to the release operator, the verifier can
derive the public key locally:

```bash
BLUEY_RELEASE_SIGNING_KEY_FILE=/secure/off-repo/bluey-release-ed25519.pem \
  scripts/bluey-release-live-verify.sh X.Y.Z
```

This command verifies `latest.json.sig`, installer MIME types, live artifact
SHA, both immutable installer SHA/size values, unpacked binary versions for
macOS, and absence of configured capture-visible dev markers in the shipped
daemon. `scripts/deploy-bluey-sh-manual.sh` fails before production mutation
unless `BLUEY_RELEASE_SIGNING_KEY_FILE` is available. Its existing
`BLUEY_RELEASE_ALLOW_UNSIGNED=1` escape is for explicit local/dev publishing
only, requires a non-production `PUBLISH_HOST` and `BLUEY_PUBLIC_BASE`, and
skips the live-signature gate with a visible warning.

For preprod -> prod promotion (after preprod soak):

```bash
BLUEY_RELEASE_SIGNING_KEY_FILE=/secure/off-repo/bluey-release-ed25519.pem \
PUBLISH_HOST=<prod-host> PUBLISH_PATH=/var/www/bluey \
  PUBLISH_DO=1 \
  bash scripts/publish-bluey-release.sh
```

That command must point prod at the same artifact/version that passed
preprod smoke. It must not run a build on the prod host.

---

## 7. Post-release

- [ ] Update `CHANGELOG.md`.
- [ ] Move shipped items in `FUTURE-IMPLEMENTATIONS.md` from active sections to
      Cleanup section.
- [ ] Append entries to `DECISIONS.md` for any decisions made this round.
- [ ] Open the next round's plan: `docs/rounds/PHASE-3-ROUND-{N+1}-PLAN.md`.
- [ ] Notify codex via `docs/work/HANDOFF-TO-CODEX-FROM-KIRO.md` if the
      next round needs review setup.

---

## 8. Rollback

If the release breaks badly post-tag:

1. Identify the last-known-good tag (e.g. previous release).
2. Swap the distribution server's `latest` symlink back (see
   `docs/DELIVERY-LIFECYCLE.md` §5).
3. Add a `## Rolled back` note to the release doc explaining what
   broke.
4. Open a hotfix branch (`fix/v0.1.1-rollback`), fix forward, ship a
   `v0.1.1` that does NOT have the regression.
5. Don't delete the broken tag — git history is permanent. Mark it as
   broken in `DECISIONS.md` instead.

Tags are never moved or rewritten. New version, new tag, same trail.
