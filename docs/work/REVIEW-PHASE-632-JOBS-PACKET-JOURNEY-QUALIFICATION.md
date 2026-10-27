# REVIEW: Phase 632 Jobs packet journey qualification

Date: 2026-10-27 (America/New_York)
Base: `abd0e38584df8f7e1069e4a1a504d60f566ddd5d`
Reviewer role: independent source and evidence review
Verdict: 🟢 accept for this bounded local Phase 632 batch; not production or release approval

## Scope reviewed

- `server/examples/jobs-nonprod-browser-fixture.rs`, complete new debug fixture bridge.
- `server/Cargo.toml` and `server/src/lib.rs`, feature/example and release guards.
- Existing signed production-positive fixture and the eligibility/approval paths used by the bridge.
- `jobs/portal/nonprod/browser-api.integration.mjs`, complete Phase 632 child diff.
- `jobs/scripts/nonprod-browser-api-report.mjs` and its complete focused test diff.
- `jobs/scripts/run-nonprod-browser-api-tests.mjs`, complete launcher diff.
- `docs/rounds/ROUND-632-JOBS-PACKET-JOURNEY-QUALIFICATION.md` and the current
  implementation record.

No product or test source was edited by this reviewer. No Cargo command, service, browser, database,
provider, production account, application, email, deployment or external action was run.

## Resolved validation findings

### P1 — factual request was not bound to the returned and persisted packet

The initial child sent `mode: factual` but checked only selected strings in the UI. A server that
ignored the requested mode and returned an enhanced packet containing the same selected facts could
still advance `packet_review`.

The final child requires the returned resume version and provenance to say `factual`, binds both the
resume and provenance to the exact fixture job, reads the actual visible resume paper, reloads the
review dialog, loads the persisted resume version from the production API and deep-compares it with
the original response. `packet_review` advances only after those checks.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:464-503`.

### P1 — any unrelated 409 could masquerade as the missing-Track approval fence

The initial child proved queue booleans were false and accepted any 409 from approval. That did not
establish the missing source/current approved Track authority as the causal fence; an unrelated
eligibility conflict could have produced the same status.

The final child requires no signed-posting hard failures, the exact first
`career_track_policy_review_required` reason, an absent source resume and `needs_review` Track
authority, then requires the 409 body to equal that exact policy reason. Before and after snapshots
prove the application/approval receipt, browser sessions, auto-submit authority, attempt
reservations, packet metering, workflow commands, balance ledger/balance and used-packet count are
unchanged.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:504-528`.

## Fixture and launcher review

- The helper is both feature-gated and debug-only. `integration-test-support` is absent from default
  features; the example declares it as required; both the library and example reject release builds.
  There is no seed HTTP route.
- Non-Unix builds contain only a closed unsupported-platform exit. On Unix, the helper accepts no
  arguments and rejects owner-database and PostgreSQL trust-override environment.
- Before fixture mutation it validates the exact canonical mode-0700 root and parent, fixed-system
  UID, complete running launcher marker, bounded mode-0600 single-link request, strict request schema,
  generated 32-byte nondevelopment data key, exact loopback/nondefault PostgreSQL URL and the live
  server's data directory, address, port, database and user.
- The helper requires the existing verified synthetic account, saved profile/preferences,
  completed onboarding and exact owned active Track with application identity. It calls only the
  shared integration-test-support fixture, which publishes discovery, original-source, ATS and
  integrity authority through their production lifecycle APIs. It does not insert eligibility rows,
  edit the candidate, invoke providers or create a production fixture key.
- The result is a bounded create-new mode-0600 file containing only version and job ID. Panics and
  internal database errors are reduced to closed diagnostic codes rather than connection/account/key
  content.
- The launcher hashes the helper source, server feature declaration/library guard and all prior
  portal/report sources; admits the exact private helper and default API binary paths by hash; and
  passes only the exact owned helper path to the child. The report requires two API processes plus a
  distinct clean and absent fixture process.

## Packet and denial review

- The signed fixture job remains a synthetic Greenhouse posting tied to the existing real Track.
  Preparation clicks the real Matches job row and `Prepare application` button, captures the actual
  browser POST and requires its exact fixture-job/factual/Review-first body. The child reloads the
  workspace, opens the real Applications review dialog and verifies the Review-first row, factual
  kit, unavailable automation state and nonempty truthful candidate content in the rendered resume.
- The read-only PostgreSQL snapshot uses a validated account identifier through psql variable
  binding on stdin. It observes reservations, packet metering, workflow commands, balance events,
  account balance and used packets without adding a public diagnostic endpoint.
- Distribution flags are not treated as proof. The child checks the exact Track-policy eligibility
  reason and matching approval response, then independently proves no approval, execution,
  workflow, session or financial authority was created.
- The seven-case report parser requires exact version, fields and ordering; seven passes; no
  failures; two clean/absent API PIDs; one distinct clean/absent fixture PID; and bounded positive
  request counters. Partial, reordered, duplicated, fabricated, extra-field, signaled, nonzero and
  ambiguous-liveness evidence fails closed.

## Reviewer-observed checks

- `node --check` passed for the child, report module/test and launcher.
- `node --test jobs/scripts/nonprod-browser-api-report.test.mjs jobs/scripts/nonprod-temporal-boundary.test.mjs`
  passed 7/7 with zero failures, skips or cancellations.
- `git diff --check abd0e385 -- jobs server/Cargo.toml server/examples/jobs-nonprod-browser-fixture.rs`
  passed on the corrected source.

These lightweight checks do not compile Rust or execute PostgreSQL, APIs or Chromium.

## Historical runtime attempts

Parent-reported evidence below was not independently reproduced and is not a positive qualification:

- Runtime 1 passed the retained five cases, then failed closed at `packet_fixture`. The diagnostic
  was initially reduced to a generic assertion, so it did not establish a root cause. PostgreSQL
  shut down and the exact root/private parent were cleaned.
- A bounded diagnostic parser was then added. It retains at most 1,024 stderr bytes and surfaces
  only the final JSON line when it has the exact helper failure event and a bounded lowercase code;
  every arbitrary, malformed, signaled or spawn failure becomes `fixture_unclassified` without
  echoing stderr content.
- Runtime 2 again passed the retained five cases and identified
  `fixture_track_identity_required`. The ordinary onboarding Track intentionally had no application
  identity binding. The final child now edits that Track through the real Settings UI, waits for the
  exact successful-save status, and verifies the persisted verified identity while the source
  resume remains absent and policy remains `needs_review`. No helper guard was relaxed.
- Runtime 3 executed all seven child cases and reached clean report/PID checks, but the outer
  launcher correctly rejected a source-hash mismatch caused by late harness edits after its snapshot.
  PostgreSQL shut down and the exact root was cleaned. This is a coordination/source-freeze failure,
  not a product defect and not a passing qualification.

## Final executable evidence and limits

Parent-reported final evidence below was not independently reproduced:

- The final frozen Runtime 4 completed with exit zero in session 16387. It built fresh
  default-feature API binaries in 81 seconds, then the separate feature-gated debug example in 71
  seconds. The helper's four focused guard tests passed 4/4, and strict Clippy for the
  feature-gated example passed with warnings denied on the same source.
- PostgreSQL 17.10 with vector 0.8.3 and real Chromium exercised the actual Matches UI preparation
  path. The exact version-3 report contained seven tests, seven ordered passes and no failures, with
  55 Jobs requests and ten account requests.
- Both API processes and the helper exited with code zero and null signal. The outer parser observed
  all three distinct PIDs absent and accepted every unchanged source/harness hash.
- The PostgreSQL wrapper reported zero status, clean shutdown and server exit. The exact final root
  was absent; all four Phase 632 private parents were empty; filtered process inspection found no
  owned executable; and host free space was 39 GiB after cleanup.
- A scope check found `server/src`, `crates`, `native`, `infra`, `web` and `.github` unchanged from
  the base. Product Rust therefore retains the Phase 629 1,838-test/full-Clippy baseline; Phase 632
  does not claim a fresh all-target or full Rust suite.

This round cannot prove signup/login/OAuth, hosted TLS, provider/ATS behavior against a live employer,
real application or receipt, mailbox/calendar delivery, runner execution, object storage, production
capacity/signing, desktop/overlay integration, production data, deployment or release readiness.

## Current verdict

🟢 **ACCEPT FOR THE BOUNDED LOCAL BATCH** — no remaining P0-P3 finding in the reviewed scope.
The exact frozen browser/API/PostgreSQL packet qualification and cleanup passed according to the
parent-reported terminal evidence above. This is not production, deployment or release approval and
does not expand the explicitly listed proof limits.
