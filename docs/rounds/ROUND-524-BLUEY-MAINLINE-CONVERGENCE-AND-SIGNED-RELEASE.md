# Round 524 - Bluey Mainline Convergence And Signed Release

Date: 2026-07-16

Branch: `codex/bluey-interrupted-asks-round519-20260712`

Backup task: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Status: complete; exact source promoted, signed native release published, and
production acceptance passed

## Trigger

The owner asked Bluey to finish every reviewed non-Sashreek branch and
interrupted ask, merge the useful work into the canonical mainline, and perform
one signed manual deployment without GitHub Actions or Keychain prompts.

## Mainline Reconciliation

The release branch is a strict descendant of `origin/main`; main has no unique
commit that is absent from the branch. Rounds 520-523 audited the remaining
remote branches and preserved worktrees by patch identity and behavior.

The following Sashreek-owned refs remain explicitly excluded:

- `origin/agent/agent-bridge`
- `origin/agent/agent-bridge-fixes`
- `origin/agent/meeting-frontend`
- `origin/agent/parakeet-stt`
- `origin/meeting-main`

No remaining non-Sashreek branch is safe or useful to merge wholesale:

- `codex/bluey-branch-reconciliation-20260712` and
  `codex/bluey-jobs-20260710` have no patch-unique launch work.
- `codex/bluey-stream-attachments-20260704` contains historical behavior that
  is represented by newer implementations; merging it would restore obsolete
  trial, provider, device, and release code.
- The preserved Jobs/Coach worktree contains incomplete desktop handoff,
  workspace, IPC, and Windows dependencies. Reviewed Jobs behavior is already
  on main, while the incomplete dependency chain remains deliberately hidden.

## Fixed In This Release

### Account And Credential Isolation

- Account-token updates use generation-aware compare-and-swap writes.
- A delayed refresh cannot replace a newer login or restore credentials after
  logout.
- Local profile and token identity must agree; malformed profiles fail closed.
- OS Keychain integration remains explicit opt-in only.

### Local And Cloud Session Ownership

- Local sessions carry `owner_account_id` and all dashboard history, active
  session, archive, title, and delete operations are owner-scoped.
- Account change or revocation clears the previous account's visible session
  and stops active paid work.
- SQLite and PostgreSQL sync reject duplicate IDs in a batch, cross-account
  reparenting, missing parents, and parent mismatches.
- Stable IDs and immediate transactions keep retries idempotent.

### Audio And Transcript Finalization

- Meeting end invalidates racing audio-start generations.
- Stop waits for the bounded STT tail before persisting and settling the
  session, preventing the final spoken words from remaining unsent.
- Deepgram session credentials are carried in a request header, not a URL query
  parameter.

### Answer Safety And Continuity

- Trusted internal envelopes are validated structurally. Untrusted user text is
  never trusted merely because it starts with `Question:`.
- Adversarial multi-paragraph disclosure requests are blocked across server and
  daemon answer paths.
- Safe partial output survives provider timeout, upstream disconnect, and
  terminal-settlement failures instead of disappearing.
- Coding prompts retain approach, complete code, explanation, complexity, edge
  cases, and full in-place follow-up replacement behavior.
- System-design follow-ups continue the existing artifact rather than losing
  the prior design.

### Signed-Out Recovery

- macOS signed-out status and balance badges are real click targets that open
  the existing sign-in flow.
- The click actions are disabled immediately after authentication so signed-in
  header dragging remains unchanged.

### Windows Release Integrity

- The native overlay protocol fixture now writes UTF-8 test bytes without
  implementation-defined signed-character casts, so strict MSVC `/WX` builds
  remain portable.
- The top-level Windows packager no longer suppresses native helper output or
  ignores failed child-process exit codes.
- Packaging now verifies every required overlay, audio, speech, daemon, and
  stable process-identity artifact before creating the release archive.
- The GitHub release workflow also fails closed when a required Windows helper
  is absent instead of publishing a partial package.

## Verification Before Commit

- Full root workspace tests passed.
- Full server all-target suite passed, including 71 integration tests.
- Root and server warnings-denied Clippy passed.
- Dashboard unit tests passed: 24 tests across 2 files.
- Dashboard TypeScript and production Vite build passed.
- macOS overlay parsed successfully with `swiftc`.
- Release hygiene scan passed all clean and rejection self-tests.
- Shell syntax, Rust formatting, whitespace, and `git diff --check` passed.
- A clean Windows builder completed the full `scripts/build-windows.ps1` path:
  Rust release binaries, overlay protocol tests, native overlay, audio driver,
  local speech helper, alias verification, and ZIP packaging.
- The release secret/dev-flag scan is rerun against the staged artifacts before
  publish.

## Deliberate Boundaries

- Raw mic/system audio bytes are not retained after transcription by default.
  The audit bundle records technical/session evidence but does not provide
  source-audio QA replay.
- Jobs-to-Coach handoff, local Workspaces, unattended workers, mailbox/calendar
  OAuth, and ATS certification remain dependency-gated product work.
- PostgreSQL/pgvector is the cloud source of truth, but scalable
  tenant-filtered database KNN remains a separate measured rollout gate.
- The signed Windows package receives static/package checks in this round. A
  real Windows launch, audio, update, and overlay canary is still required
  before broad public rollout.

## Deployment Evidence

The exact tested source was already the tip of `origin/main` when production
promotion began. The documentation-only evidence commit that closes this round
is a strict descendant and does not change the deployed runtime source.

- Deployed source/mainline commit:
  `5cf31b9d56660c2d65b8fd2dc706260ee543f656`
- Server release ID: `round524-5cf31b9d5666`
- Server release directory:
  `/opt/bluey-releases/round524-5cf31b9d5666`
- Native version: `0.1.101`
- macOS arm64 artifact SHA-256:
  `23e9aafbf33c7838426a1181dc2a15a5bbe0b3eff3c9650b190fa12ad8a0ed57`
- Windows x86-64 artifact SHA-256:
  `25393722e4a18721bd7b82820000f198022c60cfcf92a478238305f4c621cbb7`
- Production API SHA-256:
  `fd3ff9f92b30a1420e6cf8aa4ae9597af66fab7ca8321138f9d2d39d291fbbd2`
- Public `latest.json` SHA-256:
  `a6a183af0277460914f9af82dca4f3761ad5ecb8b45913718221fbdd34291bb7`
- Database backup:
  `/var/backups/bluey-api/hourly/bluey-postgres-20260716T184041Z.pgdump`
- Database backup bytes: `24037173`
- Rollback snapshot:
  `/var/backups/bluey-api/releases/20260716T184040Z-before-round524-5cf31b9d5666`

The PostgreSQL backup checksum and `pg_restore --list` both passed before
promotion, and the backup was copied to R2.

## Publish Method

The release was built from the exact source archive
`bluey-0.1.101-5cf31b9d-source.tar.gz`, whose SHA-256 is:

`5833401173b2b902528ec14bb0d83344cced058f77c3611e748a23f7412b400c`

The native artifacts were signed and published with the manual release path.
No GitHub Actions deployment and no Keychain access were used.

The macOS archive contains 18 arm64 Mach-O files. The Windows archive contains
15 PE32+ x86-64 files. Both packages passed secret scans, version checks, helper
identity checks, and byte-identity checks for their stable process aliases.
Neither package contains capture-visible development flags.

## Live Acceptance

- The signed `latest.json` signature verified.
- `install.sh` and `install.ps1` returned the expected script MIME types.
- Both public artifact hashes matched the signed manifest.
- The unpacked macOS and Windows binaries report `0.1.101`.
- `/health` returned `200` for browser, `bluey-cloud-client/0.1.101`, and
  `bluey-cli/0.1.101` user agents and reported the exact deployed source commit.
- `/auth/captcha/config` returned `200` with Turnstile enabled.
- Unsigned `/account/me` returned `401`.
- Public `/api/jobs/internal/discovery/lease` returned `404`.
- `/llms.txt` returned `410`.
- Googlebot received `200`; GPTBot received `403`.
- The full Cloudflare/origin verifier passed, including direct-origin HTTPS
  blocking and redirect-only direct HTTP.
- `bluey-api`, `bluey-jobs-api`, and `caddy` are active and enabled with
  `NRestarts=0`.
- The Jobs API and Caddy binaries/configuration were intentionally unchanged:
  - Jobs API SHA-256:
    `7201dd4f8b9b674c946ab5c301d5a4a15efbfaadc8bd8e3e4644b5cab2b86b84`
  - Caddy SHA-256:
    `91bfba4d2266825d3d31a81ae2c125ee279c1393370afca9ccefd2419ddeae70`
- Production disk usage was `53%` with approximately `28 GB` free.
- The post-deploy API scan found no panic, fatal error, provider `429`,
  capacity-busy response, or dropped-connection signature.

Cloudflare Free-plan Bot Fight Mode remains disabled because its blanket
JavaScript challenge breaks native clients. Turnstile, AI crawler denial,
Browser Integrity Check, application/Valkey limits, the API burst rule, strict
TLS, Caddy policy, and the Cloudflare-only origin firewall remain active.

## Residual Gates

No reviewed, launch-ready non-Sashreek code remains outside main. Two old Codex
worktrees still contain dirty experimental files, but Rounds 520-523 show that
their safe slices are already represented and that bulk-merging them would
restore obsolete or incomplete code.

The remaining items are explicit product or hardware gates, not hidden release
drift:

- physical Windows launch, audio, update, and overlay canary;
- raw source-audio retention remains deliberately disabled;
- Jobs-to-Coach, local Workspaces, unattended workers, mailbox/calendar OAuth,
  and ATS certification remain dependency-gated;
- tenant-filtered pgvector KNN remains a measured rollout gate.
