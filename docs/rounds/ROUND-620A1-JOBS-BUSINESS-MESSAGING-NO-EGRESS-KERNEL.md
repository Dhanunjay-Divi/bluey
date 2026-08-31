# Round 620A1 — Jobs Business Messaging No-Egress Kernel

> **Codex preflight:** Load `$bluey-ops` and reconcile this branch against Phase 614B and the
> approved Phase 620A simulator-first plan before implementation or review.

**Date:** 2026-08-30

**Branch:** `feat/phase-620a-jobs-business-messaging-simulator`

**Base:** `f9cd591e57ba9ef4541fd54982f7db8b6ba55b08`

**Status:** locally implemented and independently verified; no external effect is authorized

## Outcome Target

Implement the first executable Phase 620A slice as an original Bluey, deterministic, no-network
contract kernel. The kernel parses the closed owner-command grammar, applies STOP before every
granting path, binds immutable synthetic plans to exact provider/connection/consent/Jobs read-set
revisions, and produces only truthful simulated or step-up-required receipts.

This round is intentionally cross-runtime: TypeScript produces shared canonical vectors and Rust
verifies the same contract behind a non-default test-support feature. Neither runtime exposes a
route, worker, provider adapter, credential lookup, database mutation, UI surface, or production
feature flag.

## In Scope

- strict Phase 620A ASCII command grammar and typed rejections;
- universal STOP precedence and deterministic idempotent suppression projection;
- business-channel types for WhatsApp Business Platform and Apple Messages for Business;
- typed rejection of personal WhatsApp and unattended personal iMessage/SMS;
- synthetic-only endpoint and subject identifiers that cannot resolve to a real recipient;
- deterministic canonical command, plan, operation, and receipt hashes;
- exact synthetic connection, consent, parser, locale, and Jobs authority read-set binding;
- `APPROVE P-*` returning `step_up_required`, never approval authority;
- provider-truth ceilings: Apple success stops at `provider_accepted`; no generic delivery/read;
- pre-request failure versus post-request `side_effect_unknown` simulation without retry;
- shared TypeScript/Rust golden vectors; and
- static containment proving the kernel cannot import or invoke network, credential, provider,
  browser, process, generation, application mutation, or communication-dispatch seams.

## Out Of Scope

- provider registration, OAuth, credentials, callbacks, webhooks, WABA/MSP setup, or sandbox use;
- real phone numbers, email addresses, URLs, provider IDs, tokens, secrets, QR/device sessions, or
  personal-account automation;
- persistent connection, linking, MFA, consent, ingress, suppression, plan, lease, attempt,
  reconciliation, retention, deletion, or export tables;
- API routes, background workers, portal/native UI, user-facing availability, or provider egress;
- employer, recruiter, C2C, application, email, message, calendar, or social effects;
- widening `MailboxConnection`, `JobsCommunicationAction`, existing provider enums, or existing
  serialized receipts; and
- claiming Phase 620A completion, launch readiness, delivery, read, or provider acceptance.

## Runtime Boundary

### TypeScript

`jobs/automation/tests/support/business-messaging-simulator.ts` is imported directly by tests,
excluded from the production TypeScript build, and not exported from the automation package root.
It is pure and receives deterministic time, identifiers, read-set revisions, and scripted outcomes
as data.

### Rust

The Rust verifier is compiled only under
`business-messaging-simulator-test-support`. The feature is default-off and rejected in release
builds. It has no runtime route, worker, environment activation, database pool, async callback,
transport trait, or provider dependency.

### Shared vectors

`business-messaging-simulator-v1.json` freezes canonical inputs, normalized commands, plan bodies,
receipts, and SHA-256 values. Both runtimes must reject unknown fields and agree byte-for-byte.

## Acceptance Matrix

| Requirement | Required evidence |
| --- | --- |
| Defaults | Existing and proposed messaging/provider flags remain absent or `0`; no route/worker exists |
| Determinism | Repeated identical inputs produce byte-identical canonical bytes and hashes |
| Drift | Provider, endpoint, subject, consent, parser, Track/source/integrity, payload, or time drift changes the bound plan hash |
| Grammar | Phase 620A valid commands parse exactly; bad arity, limits, IDs, URLs, JSON, Markdown, multiline, control, Unicode, and prompt injection reject without planning |
| STOP | Mixed-case STOP is evaluated first, is idempotent, and blocks all later commands; START/RESUME cannot reactivate |
| Personal channels | Personal WhatsApp, QR/device sessions, unattended iMessage, and background SMS are closed typed rejections |
| Jobs authority | PREPARE requires an exact positive synthetic Phase 613/614/614B read set and still yields only `simulated_no_effect` |
| Approval | Chat APPROVE yields `step_up_required`; no approval, lease, request, or provider effect exists |
| Provider truth | WhatsApp statuses require exact pinned synthetic evidence; Apple never produces delivered/read |
| Ambiguity | Failure before request start is safe/no-effect; timeout after start is `side_effect_unknown` with no resend |
| Privacy | Canonical records contain no body, resume, email, phone, URL, token, header, secret, provider object, or credential |
| Containment | Static guard and runtime counters prove zero network, credential, provider, Jobs mutation, process, browser, and external-write attempts |
| Cross-runtime | TypeScript and Rust consume the same fixture and agree on every canonical byte/hash |

## Required Checks

```bash
npm run test --prefix jobs --workspace @bluey/jobs-automation -- \
  tests/business-messaging-simulator.test.ts
npm run typecheck --prefix jobs --workspace @bluey/jobs-automation
npm run build --prefix jobs --workspace @bluey/jobs-automation

CARGO_INCREMENTAL=0 cargo test --locked --manifest-path server/Cargo.toml \
  --no-default-features --features business-messaging-simulator-test-support \
  --test business_messaging_simulator
CARGO_INCREMENTAL=0 cargo check --locked --manifest-path server/Cargo.toml --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --manifest-path server/Cargo.toml --all-targets \
  -- -D warnings

node jobs/scripts/check-business-messaging-simulator-containment.mjs
git diff --check
```

Before acceptance, also run the full Jobs JavaScript gate, applicable Rust aggregate gates, default
release-containment check, privacy/provenance/schema guards, and independent source/security review.

## Flag And Effect Ledger

Every existing production/provider-write flag remains `0`. This round adds no production flag,
credential, provider account, callback, route, worker, network transport, application, email,
message, post, deployment, customer cohort, or external write.

## Successor Boundary

Durable Phase 620A authority requires a later additive SQLite 059/PostgreSQL 037 design for
connection, consent, authenticated envelope/item identity, suppression, plans, attempts, and
receipts. That successor must revalidate under database locks/time and may not reinterpret current
mailbox or communication history. Provider adapters remain later still and require separate legal,
security, provider, sandbox, canary, and launch authority.
