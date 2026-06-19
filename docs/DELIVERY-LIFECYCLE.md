# Bluey Delivery Lifecycle

> **CI/CD pipeline, environments, promotion rules, rollback model.**
> Mirrors Pinky's `DELIVERY-LIFECYCLE.md` shape but Bluey-only.
>
> Last updated: 2026-06-13, signed update manifest gate.

---

## 1. Environments

| Environment | Where | Stack | Purpose |
|---|---|---|---|
| **dev** | uno (Apple Silicon) at `/Users/uno/Downloads/cue/` | full Bluey workspace | feature work, all builds + smoke runs here |
| **preprod** | owned local machines: uno Mac + Windows bench over SSH/Tailscale when needed | local release artifacts, local installs, local smoke | release candidates validated without spending GitHub Actions credits |
| **prod** | GitHub Actions + production host/artifact store | production release job, signed/hashed artifacts, Caddy/bluey-server/static pages | customer-facing endpoint and auditable release publication |

**State 2026-06-04:** preprod validation is intentionally local. Do not
use GitHub Actions for preprod loops. Use GitHub Actions only for the
production release job after the same artifact has passed local smoke.

For Layer 3 (product server) the same triplet repeats — dev / preprod /
prod — once R14.9 lands.

---

## 2. Build pipeline (dev → tarball)

The complete chain that runs locally on uno before any release:

```bash
cargo fmt --all --check                                          # formatter check
cargo clippy --all-targets -- -D warnings                        # lint with -D
cargo build --all-targets --release                              # release build all crates
cargo test --all-targets                                         # 392 cargo tests
( cd crates/cue-dashboard/ui && npm test && npm run build )      # 15 vitest + UI build
swift build -c release --package-path native/macos/cue-overlay   # Swift overlay
swift build -c release --package-path native/macos/cue-whisper   # Swift whisper
swift build -c release --arch x86_64 (overlay/audio/whisper)     # x86_64 cross
cargo build --release --target x86_64-apple-darwin               # Rust x86_64 cross
make package-darwin-arm64                                         # arm64 tarball
make package-darwin-universal                                     # universal lipo
bash scripts/smoke-test.sh                                        # e2e smoke
git -P diff --check main..HEAD                                    # whitespace check
```

Every step must pass before a tag is applied. Codex enforces this on
review. See `docs/OPERATIONS-RUNBOOK.md` for individual step debugging.

---

## 3. Release cadence

- **Major (`v0.X.0`)** — feature releases. v0.1.0 = first GA.
- **Minor (`v0.X.Y`)** — bug fixes, small improvements. No breaking IPC.
- **Pre-release (`v0.X.0-alpha.Z`, `-beta.Z`)** — internal testing cuts.
  Used during the alpha → GA chain (we shipped `v0.1.0-alpha` then
  promoted to `v0.1.0`).

Tagging happens on `main` only, after a chain review by codex returns
🟢. Tags are annotated (`git tag -a`) with release notes inline so the
GitHub Release UI (when used) auto-populates.

---

## 4. Promotion rules

Golden rule: **build once, deploy that exact stored artifact to preprod,
smoke it, then promote the same artifact to prod.** Do not rebuild
between preprod and prod, even for "tiny" fixes. A fix creates a new
release id, a new stored artifact, and a fresh preprod smoke.

The promote model is **append-only**: we never modify a previously
published release directory. Each release lives at its own immutable
path; promotion is just swapping the `latest` symlink or publishing the
already-smoked artifact.

```
dev (uno)            local preprod benches       prod
make package-* →     install/smoke same bits →   GitHub release job
                     Mac + Windows as needed     publishes/promotes
                                                 the smoked version
```

**Promotion gates:**

1. **dev → local preprod**: full pipeline green on uno + codex/Kiro
   chain review 🟢, then install the single release artifact locally on
   the owned Mac/Windows benches. No GitHub Actions preprod run.
2. **local preprod → prod**: clean install smoke passes for every
   claimed platform, then production GitHub Actions may package/publish
   the already-smoked version. If production CI rebuilds, its output must
   match the local release id/commit and pass the artifact checks below.

Promotion is explicit, not auto. Auto-promotion of unverified bits to
prod is the kind of thing that ships outages.

**Mandatory version/hash/signature precheck before prod:**

- CLI `bluey --version`, daemon `bluey-daemon --version`, overlay helper
  version, server `/health` version, and static web version/release id
  all match the intended release id and commit.
- Release tarball SHA256 matches `SHA256SUMS.txt`.
- Native helper SHA sidecars match the bundled helper binaries.
- macOS helper binaries pass the current release-signature expectation:
  ad-hoc `codesign --verify` for unsigned alpha; Developer ID/notarized
  signature if that release line has moved to signing.
- `latest.json` has a detached Ed25519 signature at `latest.json.sig`.
  The CLI build embeds the matching raw public key via
  `BLUEY_UPDATE_PUBKEY=<base64 raw 32-byte ed25519 public key>`. A
  signed manifest pins both the platform artifact SHA256 and the platform
  installer SHA256 (`install.sh` on macOS/Linux, `install.ps1` on Windows);
  unsigned manifests are notify-only and cannot install unless
  `BLUEY_UPDATE_ALLOW_UNSIGNED=1` is set for local testing.
- If any check disagrees, stop. Do not promote. Produce a new release id
  and run local preprod again.

---

## 5. Rollback model

Because each release is an immutable directory, rollback is just
swapping the `latest` symlink back:

```bash
# on the distribution server
cd /var/www/bluey
ln -sfn releases/v0.0.9 latest.tmp
mv -Tf latest.tmp latest
```

Clients reading `/latest.json` see the rolled-back version on the next
fetch (60s cache).

For Layer 3 (product server) rollback is more involved because the
server has stateful concerns (DB schema, in-flight requests). When that
lands, this section gains a Layer-3 rollback subsection.

---

## 6. Branch model

```
main                    ←  tagged releases live here
  ↑
feat/phase-3-round-N    ←  active round work; rebased/merged into main
                            after codex 🟢 + pipeline green
```

Each round = one feature branch. Branches are short-lived (a few days
to a week) and merged with `--no-ff` so the chain history is preserved.

R7-R11 stack was deliberately preserved as separate branches even
after merge so any commit can be cherry-picked back if needed. Same
for R12+.

---

## 7. CI / CD

**State 2026-06-04:** preprod stays local by policy. GitHub Actions is
reserved for production packaging/publishing and should not be used for
iterative preprod smoke, because that burns credits/minutes without
adding useful confidence.

When CI lands (`docs/GITHUB-ACTIONS-SETUP.md`, when written):

- Pull request: optional targeted checks only when a reviewer asks for
  them; normal preprod loops stay local.
- Merge to main: tag/release metadata checks.
- Release tag (`v*`): production artifact packaging + manifest/hash
  generation + publication.
- Manual workflow_dispatch: production promote/publish of the exact
  release id that passed local preprod smoke.

Until CI exists, the dev-on-uno + codex-review path is the substitute.
Treat any commit that hasn't been through that path as untested.
