# Pinky + Bluey integration operations

Updated: 2026-10-09. Codex preflight: load `$bluey-ops`, `$pinky-ops` and
`$pinky-bluey-integration-ops`. Read the current repository `AGENTS.md` and
the [Phase 626 plan](../rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md).

Latest source-only handoff: [selected text and overlay continuation](../work/PHASE-626-SELECTED-TEXT-CONTINUATION-20261010.md).
It records the exact cross-repo boundary and open validation gates. No new live
pin or feature enablement is implied by that implementation checkpoint.

## Product decision and ownership

Pinky owns login, the compact native overlay and web UI. Bluey is the AI
backend for this phase. A separate Bluey desktop, second pill, second login or
connect-code UX is not required. Keep existing standalone Bluey intact; future
standalone work is deferred. Two repositories and independently deployable
backends remain; this is not a repository merge or shared remote/AI lifecycle.

No third repository is needed for this batch. Keep the integration adapter/UI
in Pinky and reusable AI behavior in Bluey; preserve both with feature-branch
commits and coordinated PRs. Do not fork a copied backend into Pinky.
Standalone Bluey must still run without Pinky: retain device linking/login,
existing account auth, wallet/metering, routing, streaming and session history.
Pinky delegation is additive at its own reviewed boundary, not a replacement
for standalone auth. A future standalone UI effort can resume independently.

The old `codex/ai-assist-button-20260921` branch is a comparison source only.
Port individual reviewed behavior against an accepted current Pinky base.
Never import its entire diff or delete ownership checks to decouple AI from
remote sessions. The active Pinky team owns current media/auth/payment/caption
work; its source pointer and overlap agreement must be recorded before porting.

## Current evidence and open gates

| Gate | Observed state | Remaining work |
| --- | --- | --- |
| I0 offline profile | 16 tests and independent review passed | connect to a reviewed deploy controller later |
| Dedicated host | approved $7 host; exact locally built text artifacts active, independent users/roots/signing secrets | resource/load qualification, no production sizing claim |
| Runtime base | Pinky `b7de1545` two-product website on clean accepted base; Bluey `625b9131` | reconcile additive changes with active Pinky team before promotion |
| DNS/TLS/application | public TLS and fresh synthetic auth/lifecycle/isolation passed | broader recovery/quality/physical gates |
| Identity/AI/billing | Bluey PG1/1, focused38/38, all-target935/935 + strict Clippy; Pinky targeted race + Node44/Python26; exact account/lifecycle live tests passed | synthetic accounting only; real billing policy/acceptance open |
| Native/UI | actual Mac text stream/Stop/reopen; compact account light/dark/320px navigation; Mac/Windows helper + Windows CLI build/smoke passed | full device/Windows UI/media acceptance; strict STAR factual quality failed |
| Promotion | not approved | I7 exact-artifact handoff and owner approval |

A VM being Active alone is not an application launch. A profile PASS is not proof
of secret independence, SSO, billing, health, signing or deployment authority.

Current runtime source/validation detail:
[`IMPL-PHASE-626-ASSIST-RUNTIME.md`](../work/IMPL-PHASE-626-ASSIST-RUNTIME.md) and
[`REVIEW-PHASE-626-ASSIST-RUNTIME.md`](../work/REVIEW-PHASE-626-ASSIST-RUNTIME.md).
Pinky's branch is `codex/bluey-integration-runtime-20261009`, based on clean
`4e0e4e793dbf021cc0cec6ed16aea13f338379b2` plus additive preparation commits.
Pinky owns the visible compact panel and authenticated same-origin API; Bluey
reuses its managed stream/accounting core with exact external-account authority.
Synthetic credit is confined to two generated, non-admin test identities.
No real billing, remote relay/media, customer data or Jobs activation is included
in the first text slice. The owner approved reuse of selected existing Bluey
provider keys. Exactly the first OpenAI and Anthropic keys were installed via
strict pinned SSH pipes into the dedicated root-0600 provider file, without
copying production envs/customer data/signing/payment/storage keys. Only the
dedicated Bluey unit was restarted; NRestarts remained zero and health passed.

### Latest text-preprod receipt — 2026-10-09

Read the [exact-artifact acceptance/handoff](../work/PHASE-626-TEXT-PREPROD-ACCEPTANCE.md).
Pinky live source is `b7de1545ef6e10a13f3c5d37bf30998e68d14560`;
Bluey live source is `625b9131db0ef6e8b89cd28e9f7a595a12c0350e`.
The latest two-product shell keeps Remote access/Pinky AI separate with one
existing Billing entry; it does not enable real AI pricing or mixed checkout.
Runtime-race/Node50/Python26, exact local Linux build, live auth/CSRF/tenant
checks and actual light/dark/320px/hash/keyboard browser tests passed. b68 is
the immediate Pinky rollback. Latest native QA is `ae3eef3d`: selectable
non-overlapping CC+AI, unchanged pill/panel, local guarded shortcuts, queued
Mac AppKit/both-arch and Dell native tests/builds, actual Mac synthetic-caption
+ real text/themed QA. Read Pinky's `BLUEY-INTEGRATION-PRODUCT-SHELL-20261009.md`
and `BLUEY-INTEGRATION-CC-AI-LAYOUT-20261009.md` rounds for exact hashes.
Only dedicated Pinky API was restarted; Bluey/configuration/data/credit,
other environments, production and Jobs are unchanged. Private unsigned
native QA is not an installer/updater release. Physical Windows/DPI/media,
STAR grounding, transcript memory, billing and percentile gates remain open.

Preceding b68 compact-site and 925 overlay checkpoints follow as historical
evidence, not current live source or current native design:
The superseding UI-only cutover is recorded in the receipt's compact-website
section. Local runtime Go + Node45/Python26, exact Linux build, fresh live
negative/lifecycle checks and visible light/dark/320px header/menu/keyboard
checks passed. c8 remains the Pinky rollback. A newer overlay-only native QA
checkpoint `92513204243940148d2c77d84c2af65101bcf425` passed queued Mac Swift
presentation/action-guard tests, arm64/x86_64 helpers/CLI, Dell C state tests and
helper compilation, and actual Mac Ask/Stop/Hide/reopen. Pill116×36/panel320×320
remain unchanged. See Pinky's integration feature checkout
`docs/rounds/BLUEY-INTEGRATION-OVERLAY-FIRST-20261009.md` for hashes/residues.
This did not replace the live backend/web artifacts or publish a signed updater;
Windows physical UI, visual themes and controlled live selection soak stay open.
Exact Windows Go1.26.5 CLI compile/offline argument smoke passed; physical UI
and full Windows suite stay open. GitHub publishing is pending authorized
noninteractive Git transport; verified private feature bundles retain source.
No Keychain access or history rewrite was used to work around SSH denial.
Both reviewed single-role cutovers verified process binary hashes and health;
NRestarts=0. No bootstrap, seed, DB, key, credit or configuration replacement.
The exact account route allowlist extension preserved signing/auth checks;
no other upstream routes were enabled. Website owns account/history/balance;
actual AI input/output belongs to the native compact panel. Account history is
metadata-only; unavailable data is never fabricated as zero. Native final
request ownership and bounded Stop/Done reconciliation have race tests and
visible Mac text evidence. Consult the receipt for exact hashes and limitations.
The selected provider credentials are the explicit owner-approved exception;
no full production environment or customer data was copied.

Owner-approved guidance from the active Pinky thread identifies its current
c75808f3 plus concurrent dirty work as navigation only, not a frozen integration
baseline. Preserve subscription/customer/provider attachment, durable checkout
idempotency, paid-through/downgrade dates and separate AI entitlement. Next UX
direction is Remote Access / Pinky AI with one Billing entrypoint; no real add-on
or prices are activated. Read Pinky's integration-feature
`docs/work/PINKY-AI-PRODUCT-BILLING-BOUNDARY-20261009.md` first. The additive Pinky
integration runbook is absent from the active release tree and must be included
in a coordinated promotion; do not bulk-import or edit its owned release work.

Fresh public negative/lifecycle tests passed with zero provider dispatch.
Earlier source556525 real synthetic Short delivered 88 words at913ms first text /1548ms total;
Default 257 words at 784ms/3318ms; STAR 108 words at1433ms/1901ms.
Transport, settlement, Short length and STAR labels passed. **Manual strict
STAR factual quality failed**: inferred query-plan/access-pattern details;
a second visible answer inflated a one-time verified result. Prompt assertions
and delivery success are not semantic correctness. Keep production promotion
held, retain the failed evidence, and implement source-grounded story authority
before claiming dependable personal interview answers. Timings are individual
observations, not percentiles or proof that these models are always fastest.
Otter memory, audio consent, full physical-device acceptance and real billing
remain open. Bounded actual Mac independent text QA passed; it does not certify
Windows physical UI, Intel hardware, media or a signed installer. The b68 opt-in
shell now fixes the earlier phone header overflow; broader device QA stays open.

### Initial activation and historical public checks — 2026-10-09

- Pinky deployed source: `22ff76d6d1bdca1006673908f13df5f7682f9877`;
  build `hashes.txt` SHA256 `91f4b1c1213384172191ed17895ba0c8aeaa081be12e5aa0d4a7216b919635b2`.
- Bluey deployed source: `65ec499cf9b2460b4f609e33efd9a9b50dcbed0a`;
  binary SHA256 `be7cb72e56e414b39293a789e0e51618da3a091b28d6192b65b7fb6c06e6ac91`.
- `activate-assist-preprod.sh` passed artifact/config/systemd/Caddy validation
  and seeded three generated non-admin, plan-none synthetic accounts. Only two
  receive 50 cents test credit each; upstream cap is 100 cents per 24 hours.
  Root-only seed credentials never belong in Git, logs or this document.
- Public TLS test passed: health; anonymous denial; remote/standalone model
  routes unexposed; unsigned delegation denied; three real logins and immutable
  identities; CSRF/foreign Origin denial; available/available/not_added; all
  media unavailable; independent AI sessions; reordered Stop-before-Ask with
  zero dispatch; exact late Ask409; foreign Stop404; durable own-state recovery.
  Test client identifies itself honestly. Cloudflare rejected Python's default
  UA; no browser impersonation or protection weakening was used.
- A visible real text answer was delivered through the authenticated Pinky
  page without a remote subscription. Metadata: Haiku4.5, 706 input/283 output
  tokens, 4101ms recorded server latency, 1 cent synthetic customer debit.
  This baseline **failed Short UX expectations**: excessive length and literal
  Markdown emphasis. It is not a latest-model, conversational-voice, first-token
  percentile or overall quality PASS. Root cause and repairs remain under test.
- At that checkpoint, Haiku5.5/GPT6/voice/PG source was not deployed. Review found and repaired
  default fallback gating and tier-aware final pricing defects. Final local gate
  passed 932 all-target tests, PostgreSQL1/1, focused35/35 and strict Clippy;
  owned DB/build roots were removed. Build and live acceptance remain separate.
  Metadata GET200 only proves model access, not inference quality or latency.
- Existing Pinky environments, Bluey production and Jobs were not deployed or
  restarted. Relay remains503; no payment or media capability is active.

Activation is one-shot: do not rerun bootstrap/seed on this active environment.
Future cutover must preserve synthetic DBs/keys and validate exact artifacts.

Historical foundation source/validation detail follows (not current runtime):
[`IMPL-PHASE-626-DELEGATION.md`](../work/IMPL-PHASE-626-DELEGATION.md) and
[`REVIEW-PHASE-626-DELEGATION.md`](../work/REVIEW-PHASE-626-DELEGATION.md).
The lifecycle foundation cannot dispatch models or charge users. Its context
state `active` is ownership/lifetime only, not AI entitlement or billable time.
At that foundation checkpoint Pinky's signer was not wired into runtime and
the accepted runtime base remained open. The latest receipt above supersedes
these historical limitations; all work remains on feature branches.

Continuation on 2026-10-09: Pinky adds an isolated simulated Assist UI and
bounded lifecycle client in new files only. These are preparation, not a native
pill, live AI, login, billing or release. Source review found/fixed credential-
domain and cross-identity late-token defects before reuse. Consult Pinky's
`BLUEY-INTEGRATION-TRANSPORT-20261009.md` and preview round for final checks.
The [runtime seam map](../work/PHASE-626-RUNTIME-SEAMS.md) records the missing
managed cancellation/entitlement/consent and accepted-base integration work.
At that checkpoint, Pinky reserved overlapping runtime files; its dirty hardening
tree was not an accepted source to import. The owner's clarification below
narrows that hold to actual overlaps rather than all independent integration
work. A clean-current-base and precise mounting-seam response has been requested.

Owner clarification on 2026-10-09: this is a frontend-first integration of
Pinky's overlay/web UI with Bluey's existing AI core, not a rewrite of either
backend. Pinky's existing production availability is not a task blocker.
Independent UI and adapter work continues; only actual shared-file mounting or
auth/media/billing/native contract conflicts require a narrow overlap handoff.
Do not make unrelated Pinky hardening completion a prerequisite for the whole
integration. The public production version read returned source
`4e0e4e793dbf021cc0cec6ed16aea13f338379b2` and
`1.0.32-r803captionguard`; this is advertised metadata, not signed acceptance
of a new integration build. Production and existing preprod remain untouched.

## Source checkpoint — foundation, not release

| Repo | Code checkpoint | Branch |
| --- | --- | --- |
| Bluey | `764b0bce699d894b73e18db588208ebecdb91978` | `feat/phase-626-pinky-integration` |
| Pinky | `d6f0fa9f8e69c2fdb44694c89959f761ca77fffe` | `codex/bluey-isolated-integration` |

Later receipt-only commits do not change these tested source files. Both are
feature checkpoints, not merged/deployed release artifacts. Pinky draft PR
125 remains preparation-only; Bluey PR creation is deferred because its PR
event would automatically trigger hosted CI, prohibited for this preprod work.

Historical next-agent instruction at this foundation checkpoint: obtain the accepted current Pinky runtime commit/overlap agreement,
then wire the signer only behind isolated configuration and server-owned user
authority. Close live PostgreSQL/identity/entitlement/consent/cancellation gates
before model dispatch; implement I3–I6 UI, billing, streaming and physical tests
without touching existing Pinky environments or Jobs. Do not infer deployment
approval from this checkpoint. No qualified artifact existed at that time.
Use the latest receipt and exact-artifact handoff for current continuation.

## Approved infrastructure inventory

DigitalOcean UI was used under the owner's explicit $7/month approval.
Creation and subsequent read-only provider-console/SSH checks were observed
on 2026-10-09; no API token was minted.

| Property | Observed value |
| --- | --- |
| Droplet | `bluey-pinky-assist-preprod` / ID `607491621` |
| Public IPv4 | `162.243.248.189` |
| Region / VPC | NYC2 / `default-nyc2` |
| Image | Ubuntu 24.04 LTS x64 |
| Size | Premium AMD `s-1vcpu-1gb-amd`; 1 vCPU, 1 GiB, 25 GiB |
| UI price | $7/month, $0.010/hour; no paid addons |
| Selected existing SSH key | `sudoku-vectorx-mac` |
| Monitoring | free monitoring enabled |
| Paid backups / volumes / managed DB | not selected |
| Filesystem | 24,883,167,232 bytes; 22,852,112,384 available at first SSH check |
| RAM | 961 MiB total; 648 MiB available; no swap at first check |
| Listening application ports | none; SSH and local DNS only |

Source: authenticated provider Droplet overview, provider web console
`hostname` and public host-key commands, then strict SSH `df -B1 /`, `free -m`,
`systemctl --failed --no-legend`, `ss -ltnp`. No failed unit was returned.
Do not use the initial resource samples as load-test evidence.

SSH ED25519 host fingerprint, independently obtained through the authenticated
provider console and matched before SSH use:

`SHA256:AAF9xvSf153ugRJ9o1VRRr3kBiueSElg4Nv5+SNl5kQ`

The project-local known-host file is private configuration, not a credential
or a Git artifact. Use strict host checking and ED25519 with that verified
entry. Never use `StrictHostKeyChecking=no` or trust `ssh-keyscan` alone.
If the host is rebuilt or its key changes, verify through the provider again.
Do not print private keys, env files, JWTs or provider secrets.

## Bootstrap contract — historical preparation; baseline activated

Use the declared role origins/roots/services in Pinky's
`deploy/bluey-integration.profile.json`. The profile is an offline schema;
actual resource receipts are recorded above. Isolated origins are:

- `https://assist-preprod.bluey.sh`: Pinky web/API.
- `https://relay-assist-preprod.bluey.sh`: isolated relay.
- `https://api-assist-preprod.bluey.sh`: Bluey AI API.

Create task-specific restricted OS service identities, separate data roots,
DB/schema, log/cache/object namespaces, signing/delegation material and test
payment configuration. Keep application ports loopback-only behind the exact
TLS proxy; pin cookies, issuer/audience and CORS/callback origins to this
environment. A separate hostname is not resource isolation. Never clone
production databases, env files, user login profiles or provider credentials
as a shortcut. Obtain dedicated narrowly scoped credentials where needed.

Do not call legacy deploy scripts with production defaults. No production or
existing Pinky preprod deploy lock, service, port, path, database, update feed
or bucket may be reused. Jobs services and flags remain untouched.

Build on owner Mac/Windows or approved shared machines, not this 1 GiB host.
Owner clarified: no GitHub runners or hosted fallback for preprod; use local
Mac and Windows only. Production GitHub runners are a later coordinated Pinky
release activity, not authorized by a passing preprod test. Follow shared build/resource queues;
use isolated temporary test databases and automatically cleaned build roots.
Deploy exact verified artifacts; record source SHA, digest and migration closure.

This size is for low-traffic integration trials, not a production sizing claim.
Start with synthetic users and bounded concurrency. Track available RAM/disk,
OOM/restart count, request errors, queue wait and first-token percentiles.
Owner permitted measurement-driven increases later; record the actual proposed
price/configuration and never enable paid addons or resize speculatively.

## Identity, sessions and billing

Pinky authenticates the user. Bind its immutable subject to a Bluey account
through reviewed server-to-server provisioning/linking; matching email is not
ownership evidence. Existing Bluey balances must not be silently claimed by
email or pooled into one service account. Existing-account linking may require
explicit authenticated confirmation, but not a second desktop code flow.

Delegation binds environment, issuer, audience, immutable subject, Bluey
account, narrow AI scope, expiry, request ID and owned AI-session authority.
Provider keys stay server-side. Recheck entitlement/revocation when admitting
work; enforce tenant ownership for uploads, history and usage receipts.

AI-only users can start/stop owned AI sessions without remote codes. Remote
continues with AI off, credit exhausted, provider timeout or AI cancellation.
Neither remote lifetime nor captions consent implicitly starts AI billing.
Requests, reservation/settlement, cancellation and retries need durable
idempotency and crash reconciliation before charging is enabled.

$15 PAYG top-up and $9/60 active AI minutes are proposals. Keep test balances
and payment-provider test mode until economics, rounding, idle/disconnect,
refunds and unused-time semantics are approved. Never double-charge PAYG and
minutes; reload requires its own explicit opt-in and bounded limits.

## Privacy, sources and observability

Viewer audio to AI is off by default. Playback, Listen, captions and AI consent
are separate. Opt-in names a specific accepted source/session generation and
AI session. Recheck at dispatch; revoke immediately and drop stale queued data.
Preserve loopback exclusion. Mic, screen and attachments have independent
permissions and tenant-scoped retention/export/deletion.

Diagnostics record content-free stages, request/correlation IDs, sanitized
error codes, latency/cost and resource samples. Do not dump prompts, answers,
audio, screenshots, tokens or customer files into routine logs. User-approved
conversation history is a separate storage feature, not blanket logging.

## Release, rollback and handoff

Run the Phase 626 I2–I6 matrix: AI-only, remote-only, combined, revoked/stale
viewer source, identity switch, zero credit, provider timeout, retries/crashes,
uploads/deletion, sleep/reconnect and light/dark compact-pill accessibility.
Include standalone Bluey login/device-link, auth, streaming/cancel, wallet and
history regressions with no Pinky dependency.
Record physical Mac and Windows results separately from unit/mock tests.
Verify old Pinky preprod and production identities remain unchanged.

Promotion requires accepted source closure, reviewed migrations/configuration,
backup/recovery proof, exact artifact hashes/signatures and coordinated Pinky
and Bluey PR targets. No direct `main` push or rebuild between qualified test
artifact and promotion. Roll back only integration-owned services/artifacts;
never restart production/Jobs or roll back another agent's deployment.

Every handoff states: actual deployed source/artifact (or none), accepted base,
tests performed, gates still open, secret/resource ownership, URLs actually
live, cost and task-owned temporary residues. Owner alerts use the private
`owner-alerts` project `bluey-pinky-integration`; no receiver/history in Git.

## Separate production capacity incident

The existing Bluey host was full before this integration. Metadata showed
about 42.2 GiB under `/var/backups/bluey-api`, mostly hourly/daily dumps;
this is not integration data. An owner-approved `apt-get clean` was run;
subsequent free space was about 111 MB. Concurrent filesystem changes mean
that whole difference is not attributed to the package cache alone.

Bluey and Jobs API PIDs/restart counts remained unchanged in before/after
checks. No retained backup was deleted. Recent hourly attempts include
zero-byte files; the last successful daily is stale. Cleanup of configured
restore points is therefore a separate recovery incident, not ordinary cache
deletion. The independent review requires exact encrypted off-host copies,
fresh remote/local size/hash/catalog validation and per-file revalidation
before any emergency relocation, then fresh backup and durable offsite
read-back. `pg_restore -l` is catalog evidence, not a full restore drill.
Read Round 583 before altering backup credentials or the backup script.
