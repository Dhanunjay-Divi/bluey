# IMPL: PHASE-620A1 — Jobs Business Messaging No-Egress Kernel

> **Codex preflight:** Loaded `$bluey-ops`, reconciled the Phase 614B handoff and Phase 620 plans
> against the isolated worktree, and did not use the SSD archive.

**Status:** local source implementation and verification complete; no production capability or
external effect is authorized

**Branch:** `feat/phase-620a-jobs-business-messaging-simulator`

**Base:** `f9cd591e57ba9ef4541fd54982f7db8b6ba55b08`

**Implementation commit:** `8707778be27e1940682af5c763a7ccd107bc50a0`

**Tree:** `f142b9dfa103557fa13e4d28404b15cb256fc4df`

**Pre-commit staged binary-diff SHA-256:**
`0b3cae3f11af7affd0734f26b4189aefffb5bdcde9be946fffeffb6f4280ebf3`

## Scope

**Does:**

- implement an original deterministic TypeScript/Rust contract kernel for WhatsApp Business
  Platform and Apple Messages for Business owner-command planning;
- enforce a strict bounded ASCII grammar, universal `STOP` precedence, denial-only `PAUSE`, and
  step-up-only chat `APPROVE`;
- reject personal WhatsApp, QR/device sessions, personal iMessage, and unattended SMS through a
  typed raw-input boundary;
- admit only closed synthetic account, connection, endpoint, subject, read-set, and evidence
  identifiers;
- bind canonical commands, immutable plans, operations, simulated receipts, and SHA-256 values to
  exact Jobs/source/integrity revisions;
- distinguish safe pre-request failure from non-retryable post-start ambiguity;
- pin provider-specific accepted, closed, delivered, and read truth ceilings to exact synthetic
  evidence families;
- require `PREPARE` to remain `simulated_no_effect`; and
- add CI containment proving the test support is absent from production TypeScript output,
  production imports, Rust release builds, binaries, Docker consumers, and managed-release paths.

**Does NOT:**

- add a route, worker, database table, durable consent/suppression record, UI, provider adapter,
  credential, OAuth flow, callback, webhook, phone number, or production flag;
- connect to WhatsApp, Apple, Gmail, LinkedIn, or any competitor/private account;
- send a message, email, application, calendar action, or employer/recruiter effect;
- automate personal WhatsApp, personal iMessage, QR/device sessions, or background SMS;
- claim provider eligibility, delivery/read evidence, production readiness, or Phase 620
  completion; or
- push, merge, rebase, retarget, deploy, or enable an existing production flag.

## Files Created / Modified

The implementation commit contains 15 files, 4,088 insertions, and two deletions.

| Area | Purpose |
| --- | --- |
| Contract and tests | TypeScript test support/test/fixture and Rust verifier/tests |
| Test-only compilation | Dedicated strict no-emit TypeScript config, package typecheck wiring, and default-off Rust test declaration |
| Containment and CI | Static no-egress/artifact guard, CI self-test integration, and CI/release workflows |
| Rust exposure seam | Feature-gated module registration and release rejection |
| Documentation | Round authority and changelog |

No file under `docs/reviews/` changed. The meeting-owned checkout
`/Users/uno/Downloads/cue` was not touched.

## Implementation Decisions

- TypeScript support lives under `jobs/automation/tests/support`, outside the production compiler's
  exact `src/**/*.ts` include and outside package exports.
- Rust support is behind the default-off `business-messaging-simulator-test-support` feature.
  Enabling that feature in a release build is a compile-time error.
- The containment guard allows the TypeScript kernel to import only `node:crypto` `createHash` and
  the Rust kernel to use only the exact serialization, SHA-256, formatting, and unit-test imports.
  It also rejects ambient time APIs, randomness, dynamic imports, network, filesystem, process, browser,
  credential, database, and Jobs mutation seams.
- Provider policy is closed by provider, conversation mode, and evidence family. Cross-provider
  evidence fails closed.
- Canonical records use closed synthetic registries rather than a generic `*_test_*` grammar, so
  disguised phone, token, secret, bearer, or provider identifiers cannot enter plaintext evidence.
- The raw Rust `Value` entrypoint resolves personal-channel aliases before strict business-input
  deserialization, matching the TypeScript typed rejection behavior.

## Build & Test Evidence

| Gate | Observed result |
| --- | --- |
| Focused TypeScript simulator | 71/71 passed |
| Automation support + production typecheck | Green |
| Automation production build | Green; no simulator JS or declaration emitted |
| Full Jobs JavaScript | 1,967 passed / 1 skipped: automation 791/1, browser 219, runner 308, workflows 300, portal 349 |
| Full Jobs typecheck/build | Green; Vite chunk-size warning only |
| Checked-in portal freshness | Green |
| Browser account deletion | 3/3 passed |
| Rust verifier | 14/14 passed |
| Rust focused module | 4/4 passed, 1,586 filtered |
| Full feature-bearing Rust library aggregate | 1,590/1,590 passed in 2,500.16 seconds |
| Rust formatting/check/strict Clippy | Green for verifier, feature library, Jobs API target, and default all targets |
| Default release binaries | Green; `bluey-server` SHA-256 `40debe5c7fead5d70952d51d8246b08b5e7aba099c71a63b43eb10238bb2cc25`; `bluey-jobs-api` SHA-256 `3ea349f3f6aaff8ae9d90f1a6c85bc0c66d5d41cac60dce38511aa2ab3a2eeb5` |
| Release-byte containment | Neither release binary contains business-messaging/simulator identifiers |
| Release feature rejection | Expected compile-time rejection, exit 101 |
| Containment and CI guard self-tests | Green after the full production Jobs build |
| Privacy gate | 2,712 tracked paths / 2,437 text files scanned |
| Schema parity | 102 tables / 86 indexes |
| Provenance/license gate | 663 lock entries / 631 unique versions / 1 audited override / 14 commit-pinned repositories |
| Browser release authority | 10/10 passed |
| Managed-cloud release authority | 17/17 passed |
| Independent source/security review | Green; no remaining P0, P1, or P2 |
| `git diff --check` | Green |

The full 1,590-test run supersedes an earlier interrupted aggregate and is the only aggregate
represented as passing.

## Deviations From Plan

| Deviation | Rationale |
| --- | --- |
| TypeScript kernel moved from `src` to `tests/support` | Independent review proved a non-exported `src` module was still emitted into production `dist`; the dedicated no-emit config preserves strict checking without release bytes. |
| Raw Rust value boundary added | Closed enum deserialization could not reproduce TypeScript's typed personal-channel rejection. |
| Synthetic identifiers changed to closed registries | Generic test-looking identifiers admitted secret-, token-, bearer-, and number-shaped plaintext. |
| Provider evidence and `PREPARE` ceilings tightened | Initial scripted outcomes could overstate provider truth or imply an effect for `PREPARE`. |
| Import allowlists hardened | Deny-only regexes could miss alternate filesystem/network imports or multiline Rust `use` declarations. |

## Known Follow-ups

- Phase 620A2 must design paired durable SQLite/PostgreSQL connection, consent, suppression,
  envelope/item identity, plan, attempt, and receipt authority before any route or worker exists.
- Provider adapters require separate eligibility, legal, security, sandbox, webhook, reconciliation,
  canary, and launch reviews.
- Personal WhatsApp/iMessage automation remains unsupported.
- Public limited-beta release gating for the existing review-first Bluey Jobs product is a separate
  release phase and must not expose this test-only kernel.

## Review Checklist

- [x] Files match the Round scope.
- [x] No unrelated or production-provider changes are included.
- [x] Acceptance criteria have TypeScript and Rust coverage.
- [x] Production build, binary, Docker, and workflow containment are guarded.
- [x] Privacy, schema, provenance, release-authority, and aggregate tests are green.
- [x] Independent source/security rereview found no remaining P0–P2.
- [x] No external effect, production flag, deployment, or provider write occurred.
