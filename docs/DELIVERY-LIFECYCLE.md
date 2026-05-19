# Bluey Delivery Lifecycle

> **CI/CD pipeline, environments, promotion rules, rollback model.**
> Mirrors Pinky's `DELIVERY-LIFECYCLE.md` shape but Bluey-only.
>
> Last updated: 2026-05-19, post v0.1.0 GA.

---

## 1. Environments

| Environment | Where | Stack | Purpose |
|---|---|---|---|
| **dev** | uno (Apple Silicon) at `/Users/uno/Downloads/cue/` | full Bluey workspace | feature work, all builds + smoke runs here |
| **preprod** (future) | distribution droplet preprod path | nginx + static **OR** bluey-server Rust binary | release candidates served before prod swap |
| **prod** (future) | distribution droplet prod path | same as preprod | the customer-facing endpoint |

**State 2026-05-19:** preprod and prod are not yet provisioned. v0.1.0 is
distributed by manual file transfer from uno. R14.8 stands up the first
real server.

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

The promote model is **append-only**: we never modify a previously
published release directory. Each release lives at its own immutable
path; promotion is just swapping the `latest` symlink.

```
dev (uno)            preprod                 prod
make package-* →     publish.sh →            promote.sh
                     (PUBLISH_DO=1)          (admin gesture)
                                             swap symlink to point
                                             at the new version
```

**Promotion gates:**

1. **dev → preprod**: full pipeline green on uno + codex chain review 🟢.
2. **preprod → prod**: at least one external smoke test passes (clean
   Mac install + `bluey on/off` end-to-end, or equivalent for whichever
   platform).

Promotion is explicit, not auto. Auto-promotion of unverified bits to
prod is the kind of thing that ships outages.

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

**State 2026-05-19:** no CI yet. Everything runs locally on uno.

When CI lands (`docs/GITHUB-ACTIONS-SETUP.md`, when written):

- Pull request: full pipeline (fmt, clippy -D, build, test, npm, swift,
  diff check).
- Merge to main: same + tag check + release-notes generation.
- Release tag (`v*`): build all artifacts + run installer smoke + push
  to preprod via publish.sh.
- Manual workflow_dispatch: promote preprod → prod.

Until CI exists, the dev-on-uno + codex-review path is the substitute.
Treat any commit that hasn't been through that path as untested.
