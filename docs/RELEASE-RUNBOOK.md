# Bluey Release Runbook

> **Step-by-step procedure for cutting a new Bluey release.**
> Mirrors Pinky's `RELEASE-RUNBOOK.md`.
>
> Last updated: 2026-06-13, post signed-updater hardening.

For the high-level lifecycle (environments, branches, promotion gates),
read `docs/DELIVERY-LIFECYCLE.md` first. This doc is the concrete
checklist.

**Golden rule:** build the release artifact once, store it, deploy that
same artifact to preprod, smoke preprod, then promote that exact stored
artifact to prod. Never rebuild between preprod and prod.

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

```bash
make package-darwin-arm64
make package-darwin-universal
ls -la dist/bluey-*-darwin-*.tar.gz dist/bluey-*-darwin-*.tar.gz.sha256
```

Expected outputs:

```
dist/bluey-X.Y.Z-darwin-arm64.tar.gz
dist/bluey-X.Y.Z-darwin-arm64.tar.gz.sha256
dist/bluey-X.Y.Z-darwin-universal.tar.gz
dist/bluey-X.Y.Z-darwin-universal.tar.gz.sha256
```

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
- What's in this release (capture / overlays / stealth / dashboard)
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
SHA256 values. Do not publish with `BLUEY_RELEASE_ALLOW_UNSIGNED=1`
outside local release testing.

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

For preprod → prod promotion (after preprod soak):

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
