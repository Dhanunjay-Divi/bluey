# Pinky + Bluey integration operations

Updated: 2026-10-09. Codex preflight: load `$bluey-ops`, `$pinky-ops` and
`$pinky-bluey-integration-ops`. Read the current repository `AGENTS.md` and
the [Phase 626 plan](../rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md).

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
| Dedicated host | Active; strict SSH identity check passed | bootstrap isolated users/services/resources |
| Runtime base | stable default is preparation-only | get accepted Pinky commit and modern guard closure |
| DNS/TLS/application | not configured or deployed | exact origins, TLS, dedicated resources and config |
| Identity/AI/billing | default-off foundation passed focused tests/strict Clippy; 909-test broad regression passed before lint-only fixes | remaining I2–I5 PostgreSQL, entitlement, dispatch, consent and accounting gates |
| Native/UI | no current live integration acceptance | Mac + Windows evidence; I6 |
| Promotion | not approved | I7 exact-artifact handoff and owner approval |

A VM being Active is not an application launch. A profile PASS is not proof
of secret independence, SSO, billing, health, signing or deployment authority.

Current source/validation detail:
[`IMPL-PHASE-626-DELEGATION.md`](../work/IMPL-PHASE-626-DELEGATION.md) and
[`REVIEW-PHASE-626-DELEGATION.md`](../work/REVIEW-PHASE-626-DELEGATION.md).
The lifecycle foundation cannot dispatch models or charge users. Its context
state `active` is ownership/lifetime only, not AI entitlement or billable time.
Pinky's matching signer is not wired into runtime yet. All implementation stays
on the two feature branches; the accepted Pinky runtime base remains open.

Continuation on 2026-10-09: Pinky adds an isolated simulated Assist UI and
bounded lifecycle client in new files only. These are preparation, not a native
pill, live AI, login, billing or release. Source review found/fixed credential-
domain and cross-identity late-token defects before reuse. Consult Pinky's
`BLUEY-INTEGRATION-TRANSPORT-20261009.md` and preview round for final checks.
The [runtime seam map](../work/PHASE-626-RUNTIME-SEAMS.md) records the missing
managed cancellation/entitlement/consent and accepted-base integration work.
Pinky has replied that the runtime hold remains in force; its dirty hardening
tree is not an accepted source to import. The owner requested its eventual
accepted-base/seam response be sent back to this coordinator.

## Source checkpoint — foundation, not release

| Repo | Code checkpoint | Branch |
| --- | --- | --- |
| Bluey | `764b0bce699d894b73e18db588208ebecdb91978` | `feat/phase-626-pinky-integration` |
| Pinky | `d6f0fa9f8e69c2fdb44694c89959f761ca77fffe` | `codex/bluey-isolated-integration` |

Later receipt-only commits do not change these tested source files. Both are
feature checkpoints, not merged/deployed release artifacts. Pinky draft PR
125 remains preparation-only; Bluey PR creation is deferred because its PR
event would automatically trigger hosted CI, prohibited for this preprod work.

Next agent: obtain the accepted current Pinky runtime commit/overlap agreement,
then wire the signer only behind isolated configuration and server-owned user
authority. Close live PostgreSQL/identity/entitlement/consent/cancellation gates
before model dispatch; implement I3–I6 UI, billing, streaming and physical tests
without touching existing Pinky environments or Jobs. Do not infer deployment
approval from this checkpoint. No qualified release artifact exists yet.

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

## Bootstrap contract — not executed yet

Use the declared role origins/roots/services in Pinky's
`deploy/bluey-integration.profile.json`. The profile is an offline schema;
actual resource receipts must be added separately. Planned origins are:

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
