# Bluey Launch Gates And Risk Register

Updated: 2026-07-19

This is the short owner-level gate list before broader paid alpha or marketing.

## Launch Verdict

The signed live manifest and `INSTALL.md` define downloadable platforms. The
release record install-smokes Windows 11 only and does not claim Windows 10 or
a Linux desktop artifact. Broader marketing remains bounded by the unresolved
operational gates below.

## P1 Gates Before Broad Marketing

### 1. Reserve AI Cost Before Provider Dispatch

Risk:

- LLM/embedding/chunked transcription paths can spend provider money before the
  account/trial is safely reserved.

Needed:

- Server-side token estimates.
- Reserve estimated customer cost before provider dispatch.
- Settle final usage afterward.
- Apply to paid balance and trial seconds.

Acceptance:

- Concurrent trial requests cannot exceed allowance.
- Understated client token hints cannot bypass cost checks.
- A failed reservation prevents provider dispatch.

### 2. Make Auto Reload Durable

Risk:

- Process-local duplicate protection is not enough across restart/multi-server.

Needed:

- DB-backed reload attempts.
- Unique idempotency key per account/threshold window.
- Status lifecycle: pending, paid, failed, credited, reversed.

Acceptance:

- Replayed processor webhook credits once.
- Server restart cannot double-charge.
- Concurrent threshold crossings create one reload attempt.

### 3. Add Review Queue For Refund/Dispute Edge Cases

Risk:

- Unknown/unmapped refund or dispute events can be logged as processed without
  restricting abusive accounts.

Needed:

- Admin review queue for unmapped payment events.
- Manual restrict/reinstate workflow.
- Evidence package or at least searchable payment/account/request timeline.

Acceptance:

- Operator can find a Square payment ID and see account, credit, usage, and
  dispute state without raw transcript/doc contents.

### 4. Close Trial Abuse Cost Loops

Risk:

- Trial AI usage burns real provider money and needs stronger controls than a
  normal SaaS feature trial.

Needed:

- Turnstile remains enabled.
- Device/IP/email/domain velocity rules stay active.
- Per-account concurrent trial request cap.
- Trial reservation on expensive paths.
- Daily provider spend alerts/caps.

Acceptance:

- A script cannot create many trial accounts and run high-cost requests without
  hitting a hard stop.

### 5. Make Multi-Server Preflight Fail Closed

Risk:

- Without Valkey strict mode, provider cooldowns and rate limits are per-process.

Needed:

- `BLUEY_RATE_LIMIT_REDIS_STRICT=1` required for multi-server.
- Preflight fails if Redis/Valkey is unavailable.

Acceptance:

- Multi-server profile cannot pass without shared limiter state.

## P1 Gates Before Claiming Scalable Memory/RAG

### 1. Use pgvector KNN In Runtime Query

Risk:

- Fetching latest 2,000 rows and scoring in Rust is not scalable cloud memory.

Needed:

- Tenant-filtered pgvector KNN SQL.
- Query-plan assertions.
- Recall/perf test data sets.

Acceptance:

- 10k, 100k, and 1M chunk tests have recorded p50/p95 latency and recall.

### 2. Add Worker Plane

Risk:

- Retention/export/delete/OCR/embedding queues are contracts, not closed runtime
  guarantees.

Needed:

- Durable job table or worker runtime.
- Retry/backoff/dead-letter.
- Queue lag metrics and admin visibility.

Acceptance:

- Deletion/export/retention jobs can be traced from enqueue to done/fail.

## Windows Support Gates

Publishing a Windows artifact is not the same as broad Windows support. Before
expanding beyond the release-recorded Windows 11 x86-64 smoke:

- `docs/deploy/WINDOWS-PAID-ALPHA-READINESS.md` P0 passes on clean Windows.
- Install/update/uninstall are verified.
- Overlay, audio/STT, screen context, attachments, and account linking match the
  Mac promise or are clearly documented as limited.

## Operational Gates

- Clean Mac install passes from public installer.
- Paid-alpha smoke passes with real credits.
- Provider accounts are funded and provider dashboard caps/alerts are set.
- Off-host backups and restore/checksum verification pass.
- Support/refund/dispute mailbox owner is assigned.
- `bluey doctor` and logs export produce redacted support artifacts.
- Terms/privacy/refund/support pages match billing behavior.

## Doc Cleanup Gates

Before handing Bluey to new agents/operators:

- Mark `ARCHITECTURE.md` and `SERVER-REFERENCE.md` historical or refresh them.
- Update stale Stripe/Go/not-provisioned references.
- Resolve `$15` versus `$30` reload copy.
- Update `.github/PULL_REQUEST_TEMPLATE.md` from `docs/work/` to
  `docs/rounds/` and `docs/reviews/`.
- Add a docs index explaining canonical docs versus historical docs.

## Suggested Owner Sequence

1. Fix AI cost reservation.
2. Fix durable auto reload.
3. Fix refund/dispute review queue.
4. Clean docs drift.
5. Add Bluey search/GEO pages and `/llms.txt`.
6. Run clean-Mac paid-alpha smoke.
7. Invite a small engineering-heavy cohort.
8. Watch real provider spend and support load before broad marketing.
