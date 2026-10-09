# Phase 626 — isolated Pinky + Bluey integration preprod

Date: 2026-10-08. Updated: 2026-10-09.
Status: offline preparation verified; dedicated VM provisioned; **application not deployed**.

Integration operations entry point:
[`docs/ops/PINKY-BLUEY-INTEGRATION-RUNBOOK.md`](../ops/PINKY-BLUEY-INTEGRATION-RUNBOOK.md).
Git-backed skill:
[`pinky-bluey-integration-ops`](../skills/pinky-bluey-integration-ops/SKILL.md).
Load it alongside `$bluey-ops` and `$pinky-ops` for this integration.

## Checkpoint history and current gate

The owner's original stop-at-25%-remaining gate was reached at 76% used / 24%
remaining. Work was checkpointed, then the owner explicitly lifted that limit
and authorized continuation to 0%. The interrupted first draft was committed
as Pinky `095b0ebf` and the initial Bluey handoff as `0ca7ef2f`; they were not
merge/deploy approvals. Subsequent review found URL-role misbinding and
malformed-input traceback/read-boundary defects. Preserve that failed-draft
history rather than presenting it as verified tooling.

Checkouts to resume:

- Bluey: `/Users/uno/.codex/worktrees/bluey-pinky-integration/cue`.
- Pinky: `/Users/uno/.codex/worktrees/pinky-bluey-integration/pinky-git`.

I1 no longer depends on the full production host: a separate VM was approved
and provisioned. It still requires isolated application resources and accepted
Pinky runtime-base review. The active Pinky task was notified of the isolated scope and asked for
an accepted base; receipt of that message does not prove agreement or a response.
Final I0 test/review evidence is in the matching Phase 626 work review.

Pinky preparation review: [draft PR #125](https://github.com/Dhanunjay-Divi/pinky/pull/125).
Both feature branches are pushed; this is not a mainline merge or deployment.
Bluey PR creation is deferred because its PR event automatically starts a
hosted cross-platform matrix; no hosted build/signing/deploy was dispatched.

## Project-scoped owner alerts — 2026-10-09

Owner explicitly enabled actionable blocker/input/model-change iMessage
alerts. Load `owner-alerts`; use its deduplicated sender with project
`bluey-pinky-integration`. Private authorization/configuration lives outside
Git. Do not place recipient details or message history in this repository.
The existing user-local `imsg` binary was reused: no toolkit reinstall or
global Homebrew change. The test-target capacity question was accepted by the
sender; phone delivery/read status is unverified. Do not resend the unchanged
condition or treat a successful send as hosting/deployment approval.

Load `bluey-ops` and `pinky-ops` before continuing. Repository source and current
release runbooks override historical skill snapshots. This document is the
cross-repository handoff; it grants no production or signing authority.

## Owner decision

Both products belong to the owner. Keep two repositories and independently
deployed backends. Pinky's existing preprod continues its own release work;
a separate integration deployment combines Pinky's frontend with Bluey's AI.
The owner will authorize promotion after joint validation. For this phase,
Pinky owns the entire customer AI experience; a separate Bluey desktop/UI is
deferred, not a launch dependency. Preserve Bluey's independent backend and
existing standalone product without spending this batch on its UI. Bluey Jobs is out
of scope, including its flags, database, workers and deployments.

### Repository and standalone compatibility

Use the existing Pinky and Bluey repositories; no third repository was created.
Pinky owns its integrated UI/client adapter, while Bluey owns the reusable AI
backend, authorization/metering and existing standalone client. Feature
branches/PRs preserve integration work without a copied full product tree.
Revisit a separate shared-contract package only if actual multiple-client
versioning/release needs justify it; it is not needed for preparation.

Standalone Bluey is preserved, not disabled or replaced. Keep its existing
desktop, device-link/login, account JWT, wallet, routing and session flows
compatible when adding Pinky delegation. Add explicit regression coverage for
those paths before promotion. Shared AI logic stays in Bluey; Pinky-specific
subject delegation belongs at a distinct reviewed server boundary, not in the
standalone client's authentication or UI. Future standalone work can continue
without Pinky running or installed.

## Bases and ownership

| Stream | Starting point | Owned branch |
| --- | --- | --- |
| Bluey integration coordination | `660f8d2bd19c22180c34630233300b351ea472e7` (`origin/main`) | `feat/phase-626-pinky-integration` |
| Pinky integration preparation | `5837197f81649c5841a94e17d4558984d3d65e52` (repository default) | `codex/bluey-isolated-integration` |
| Prior Pinky AI feature, reference only | `17202496e1005869a5da73dec131124ddd9b3249` | `codex/ai-assist-button-20260921` |

Feature worktrees live under `~/.codex/worktrees`, not new Downloads folders.
Do not stage, reset or clean either canonical checkout's owner changes.
Do not merge the entire old AI branch: it modifies native audio, WebRTC, DB
migrations, API and overlay files. The active Pinky team owns newer media,
authentication, payment and caption-plane changes. Ask that team for the
accepted integration base/seams before importing overlapping runtime files.
The stable default lacks several newer operations guards; it is a preparation
base, **not** an approved deployment candidate.

## Separate environments

| Target | URL | Status |
| --- | --- | --- |
| Existing Pinky preprod | `https://preprod-internal.pinky.sh` | reserved to Pinky team; do not mutate |
| Existing Pinky preprod relay | `https://relay-preprod-internal.pinky.sh` | reserved to Pinky team; do not mutate |
| Integration Pinky frontend/API | `https://assist-preprod.bluey.sh` | proposed, not provisioned |
| Integration relay | `https://relay-assist-preprod.bluey.sh` | proposed, not provisioned |
| Integration Bluey API | `https://api-assist-preprod.bluey.sh` | proposed, not provisioned |

The new hostnames are a plan, not evidence of DNS, TLS or a live application.
Read-only inventory found no established Bluey preprod; the historical
`api-test.bluey.dev` example is not provisioning evidence.

Use dedicated service names, data roots, database/schema, cache namespace,
object-store bucket/prefix, API/relay ports, JWT signing material, integration
credentials, webhook destinations, logs and synthetic users. Never copy
production/preprod databases, login files or environment files into the new
instance. Pin host-only cookies and exact-origin CORS/callback allowlists;
reject credentials from either other environment. A separate URL alone does
not isolate storage, identity or billing.

The owner approved one dedicated $7/month instance on 2026-10-09. Existing
droplets must not be reused for this integration. Any later reuse requires capacity and isolation review:
separate limited service identities, resource budgets and routing, no shared
deployment lock/paths, no restart of other services. New paid infrastructure
requires a stated budget and owner approval. Do not run legacy deploy scripts
with their production defaults to bootstrap this environment.

### Read-only capacity evidence — 2026-10-08

The known-host, noninteractive SSH probe of documented Bluey host `bluey-brain`
returned the following for `df -B1 /`:

| Filesystem | Bytes | Used | Available | Use |
| --- | --- | --- | --- | --- |
| `/dev/vda1` | 61,285,326,848 | 61,268,549,632 | 0 | 100% |

`systemctl is-active bluey-api.service bluey-jobs-api.service` returned active
for both. The service inventory also included Jobs discovery workers. No env,
credential, customer content or database was read. No cleanup, deployment,
restart or host mutation occurred. Adding integration services here is **held**.
The owner was asked to select another test target/budget or approve a separate
capacity fix; this task must not remove Jobs/release data to make space.

This capacity observation is historical. A later owner-approved package-cache
cleanup left 110,981,120 available bytes; retained backups were untouched.
Production remains unsuitable for integration. The separate VM and unresolved
backup-health findings are recorded in the integration runbook.

The Pinky preparation branch contains the reviewed offline draft of
`deploy/bluey-integration.profile.json` and an offline validator at
`scripts/ops/check-bluey-integration-profile.py`. A profile PASS means only
that its declared configuration meets the offline preparation contract; it
does not prove actual resources, secret independence or release eligibility.

## Product and trust contract

One compact Pinky overlay owns the integrated UI. No separate Bluey app,
second pill, second login or connect-code step is required for a Pinky user
adding AI. The old AI branch is reference material, not a mandated blueprint.
Keep Pinky's visual language and conditional controls. Remote
access must remain usable with AI disabled, unavailable or out of credit.

The intended path is:

`Pinky UI → authenticated Pinky assist API → delegated Bluey AI → SSE answer`

Provider credentials stay on Bluey. The desktop receives neither provider
keys nor a reusable Bluey service credential. A single login experience must
still preserve independent Bluey entitlement and metering. Establish a durable
Pinky immutable-subject → Bluey account binding with authenticated linking or
reviewed token exchange. Email equality alone is not proof of account ownership.
Delegation must bind issuer, audience, subject, environment, scopes, expiry and
request identity. A shared service user's wallet is not per-customer billing.

AI sessions have their own ownership/lifecycle, independent of remote codes.
Do not achieve independence by deleting the existing session ownership check.
Replace it with durable owned AI-session authority and request idempotency.
Stopping AI does not stop remote sharing; ending sharing does not silently
start/stop or alter AI billing. Account/entitlement changes revoke AI admission.

Viewer audio → AI is **off by default**. Playback, Listen, captions and AI
consent are separate. Opt-in must name the source and be visibly revocable;
bind it to the accepted viewer/session generation and AI session, recheck at
dispatch, and drop queued/stale input after revocation or source replacement.
Never undo R873's viewer-loopback exclusion to provide this feature. Mic,
screen and file context each retain their own explicit permission boundary.

Default answers use the qualified Instant route. Deeper routing is explicit;
do not assert model speed/quality from model names or stale price tables.
Record content-free stage timings and sanitized error codes. User-approved
session history is separate from diagnostics, with retention/export/deletion
and tenant isolation; do not log every private input by default.

## Billing proposal — not a shipped policy

- PAYG default: minimum $15 wallet top-up. Optional $15 auto-reload requires
  explicit consent, visible limits and durable idempotent provider handling.
- Optional $9 buys 60 active AI-session minutes, with Start, Stop and timer;
  retain unused time, no silent renewal, no simultaneous PAYG charge.
- Confirm provider economics, any fair-use limits, rounding, disconnect/idle
  semantics and refund policy before advertising or enabling hourly pricing.
- Integration tests use synthetic balances/test entitlements. No real payments,
  subscription changes or auto-reloads in this preparation slice.

## Evidence behind required changes

- Pinky AI reference `internal/api/assist_ask.go:167–190` requires a remote
  session and checks its owner. `internal/webrtc/host_overlay_assist.go:228–237`
  binds the controller's session ID to WebRTC's `s.code`. Independence needs
  API **and** native lifecycle changes, not just a frontend button.
- Reference `internal/assist/blueyclient.go:30` sets
  `/api/assist/ask/stream`; the Pinky server calls Bluey's
  `/router/complete/stream`. Preserve SSE cancellation and bounded parsing.
- Bluey `server/src/auth/middleware.rs:20` authenticates Bluey account JWTs;
  there is no evidence that Pinky identity delegation is already implemented.
- Bluey `server/src/config.rs:175` provides `BLUEY_PUBLIC_URL`;
  `crates/cue-cli/src/app.rs:264` provides `BLUEY_CLOUD_API_URL`.
- Pinky default `.github/workflows/deploy-preprod.yml:291–316` reserves
  existing preprod roots, services and origins. Never overwrite them.

Pinky line references above apply to the pinned AI reference branch, not the
default preparation branch. Re-resolve citations after selective porting.

## Ordered implementation and separate test scenarios

| Gate | Work | Required scenario/evidence |
| --- | --- | --- |
| I0 | preparation profile, negative isolation tests, coordination | reject old origins/roots/services and enabled unsafe defaults; no deploy |
| I1 | accepted Pinky base, isolated infrastructure/config | fresh resource identity, no shared DB/secrets/storage/update feed; old preprod unchanged |
| I2 | linked identity, AI sessions, entitlements | Pinky-only user has no AI; Bluey-added user asks without remote session; foreign/revoked identity denied |
| I3 | selective AI API/controller/UI port | streaming, cancellation, bounded history, no changes to remote state when AI fails |
| I4 | privacy sources and pack boundaries | default-off viewer audio, immediate revocation/replacement fences, cross-user pack denial |
| I5 | synthetic billing, latency and reliability | no double charge on retry; crash/disconnect reconciled; first-token percentiles/cost for fixed workloads |
| I6 | native and browser UX | actual Mac and Windows, light/dark, compact pill, keyboard/AT, reconnect/sleep; no mock-only claims |
| I7 | reviewed exact-artifact promotion handoff | both owners/teams verify pins, flags, migration/recovery/backup and physical evidence before production |

Keep fixtures/workloads distinct: AI-only/no remote; remote-only/no AI;
combined AI+remote; opted-in viewer source; revoked/stale source; identity
switch; zero-credit/provider timeout; attach/delete; sleep/reconnect; test
checkout/reload; old Pinky preprod regression. No hosted build dispatches or
real provider spend without the shared runner/cost gates.
Also run standalone Bluey regression scenarios: existing login/device linking,
account auth, AI streaming/cancel, wallet/usage and session history with Pinky
absent. Pinky delegation must not become a required dependency of these paths.

## Promotion handoff to Pinky

Promote reviewed changes through each repository's normal PR target, not a
direct main push or wholesale cross-repo merge. Supply exact source commits,
artifact digests/signatures, schema/migration dependency closure, configuration
and entitlement contract, test receipts, explicit unknowns and recovery plan.
Pinky promotes its UI/API/native slice; Bluey promotes its AI/delegation slice
in a coordinated compatible rollout. Tested artifacts are not rebuilt between
preprod and production. A passing profile/unit suite is not approval to promote.
