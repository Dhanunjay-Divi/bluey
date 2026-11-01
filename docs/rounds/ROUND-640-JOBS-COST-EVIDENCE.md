# Round640 — Read-only Jobs provider-exposure evidence

Date:2026-11-01. Base `32ca96dbfbe151b1ca0294ad945ce58a4276087f`.
Branch `feat/phase-640-jobs-cost-evidence`.
Workspace `/Users/uno/.codex/worktrees/jobs-cost-evidence/cue`.
Status: bounded test-only prototype implemented and independently qualified:
strict all-target Clippy,16 focused SQLite cases, one physically verified private
PostgreSQL case and1,933 all-target server tests pass, with source/cleanup proof.
See IMPL/REVIEW for exact runtime778/91ef source and subsequent one-document
inverse proof. No public observer/owner-database driver, verified employer delivery,
historical completeness, whole COGS or release acceptance is established.

## Authority and independence

This is the existing Asana cost task `1219993283940797`, not another delivery
board or a replacement for the full Jobs goal. Local Phase639 image qualification
remains separate and unfinished in its original workspace. Do not
change those bytes, meeting/overlay/Pinky files, shared owner services or
production. This branch starts at its accepted local checkpoint and deliberately
does not copy Phase639's uncommitted image-harness successors.

The existing `COST-PHASE-638-COMPETITIVE-ACCEPTANCE.md` defines the report's
accounting intent. Correct its retention wording from current code: physical
spend-truth retention is a fixed31 days (`UPSTREAM_SPEND_TRUTH_RETENTION_MS`),
while the configurable spend window is an admission rule. Pruning uses updated
time; membership must use immutable created time. No existing durable observation
watermark proves arbitrary historical completeness.

Current source also constrains verification: `settle_with_usage` overwrites the
usage event's cost with the settled amount. Usage has no independent provenance
column or retained raw pre-maximum non-exact cost. The API's original request
fingerprint hashes supplied object bytes and original request material that
ready metadata alone cannot reconstruct. Report those limits explicitly; do not
invent a provenance field or claim fingerprint-content recomputation.

## Outcome and exclusions

Read actual isolated synthetic SQLite/PostgreSQL rows for one explicit account,
attempt window, report cutoff and receipt closeout window. Report each exposed
hold once, annotate usage without fanout, and report distinct metadata-consistent
applications from read-only stored-receipt/evidence validation separately from
independently verified success. Missing/ambiguous attribution,
provenance, completeness and cost stay visible; no zero-by-default economics.

This is partial provider exposure, not invoice COGS, complete per-application
attribution, a six-month forecast, measured runner cost or competitive superiority.
It must not change spend admission, settlement, billing or balances, expose a
public endpoint, query an owner database, or contact a model/provider/employer.
No push/deploy/publication/flags/paid infrastructure or real messages/applications.

## Ordered tasks and acceptance

### D0 — Exact source and reusable validation design

- Confirm clean branch/base and current scope/hash functions, hold/usage schema,
  receipt/evidence validators and real physical retention.
- Define one bounded report schema with immutable windows, as-of, closeout lag,
  retention and explicit completeness state; cap rows/bytes/work.
- Propose the narrow read-only receipt-validation adapter before implementation.
  It must validate durable frozen submission authority, not call a mutating
  receipt-persistence operation or pretend current live leases still exist.
  If necessary evidence is absent, return unverifiable, never accept a status
  string or caller-supplied success Boolean as receipt authority.
- Distinguish metadata-consistent stored submissions from independently
  reverified employer delivery/object contents. Missing evidence remains a hold
  on the stronger claim, not a reason to rename the weaker result as success.

### D1 — Actual read-only report path

Pseudocode:

```text
guard isolated physical database + synthetic account + owned output
begin consistent read-only snapshot
bind explicit attempt-created window, receipt-submitted window and cutoff
select all account-scoped holds in the attempt window, one request hash each
build annotation map from authoritative server attempt events by request hash
reject duplicate matching usage; classify missing/mismatched linkage explicitly
sum held projections OR each settled amount exactly once, never both
keep exact-usage-priced / estimated / missing conservative / held buckets apart
revalidate stored submission bundles/evidence via the narrow read-only adapter
count DISTINCT validated application IDs, with late/out-of-window cases separate
assess retention and durable observation-start evidence, not earliest-row inference
emit bounded canonical private report, with unknown total/invoice costs explicit
end snapshot; prove all relevant database state unchanged
```

- Use `account_scope_hash`, `generation_scope_hash` and existing request hashing;
  do not reconstruct historically absent attempt IDs or join by job equality.
- No production report route/CLI surface, new schema, cleanup, settlement or
  pruning is authorized as a side effect of report generation.
- Surface valid released holds separately before usage-linkage classification;
  require zero settlement/missing provenance/no matching attempt event. Preserve
  usage timestamp precision and unmatched/cross-window counts. Hold-declared
  provenance is not independent invoice or raw-provider-rate validation.
- Zero validated successes means unavailable ratio, not0. Even exact usage-priced
  cents are not invoice-reconciled costs. Rounding stays conservative for billing.

### D2 — Real isolated fixtures and adversarial evidence

- Establish a fresh fixture observation boundary by actual empty-state/readback,
  then seed through existing reserve/settle/application/receipt lifecycles.
- Cases: valid stored receipt with accepted+rejected attempts, failed application,
  unresolved hold, uncertain delivery exposure, duplicate usage, zero successes,
  missing/ambiguous attribution, cross-account and cross-window/late records,
  repeated reports, short/expired history and invalid/tampered receipt evidence.
- SQLite and actual private PostgreSQL must agree on amounts, classification,
  distinct receipt denominator and unchanged-state evidence. A JSON arithmetic
  fixture alone is not the requested database/receipt integration.
- No fresh fixture witness can retrospectively prove historical completeness.
  Never manufacture an invoice, billed runner amount or employer submission.
- Reuse the existing physically guarded fresh-per-case PostgreSQL wrapper and
  backend-neutral final-submission fixture through narrowly widened test-only
  helper visibility. No new example or competing database launcher is needed
  if these existing guards cover the actual selected cases. Extend before/after
  evidence in new tests for holds/usage/evidence/object families, leaving older
  snapshot-helper behavior unchanged.

### D3 — Review and gates

Parent owns source freezes, actual execution and evidence. Run focused tests,
formatting, strict Clippy and proportionate full server/Jobs guards through the
canonical temporary launcher/shared machine lock. All owned data/Cargo/process
state must be removed on success/failure/signals. Do not lower disk guards.
Independent review checks tenant isolation, no writes/fanout/double count,
historical-receipt semantics, missing-cost labels and exact test coverage.
Record IMPL/REVIEW and prospective privacy including new paths before checkpoint.

## Ownership and completion

Parent owns plan/docs/integration/execution. One bounded builder may own the
agreed source modules after the D0 design is reviewed; a separate reviewer owns
review findings only. Workers are not alone and must preserve other edits.
No build/database/service/test/commit from workers without a parent assignment.
Sol owns the two agreed core modules and inline tests after Terra's honest
partial handoff. Parent owns only test-helper visibility plus docs/integration.

The batch is not done until the actual database/receipt report and both-backend
negative/mutation evidence pass. The overall Jobs goal still requires actual
runner/browser receipts, sources/providers/OAuth, full cost attribution,
C2C/MCP/messaging and the separately authorized release gates.
