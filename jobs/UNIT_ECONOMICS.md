# Bluey Jobs Unit Economics

Internal operating model. Customer-facing UI should show plan inclusions and prices, not infrastructure or margin details.

Checked July 10, 2026.

## Competitive target update — October 28, 2026

The tables below are historical planning assumptions, not current measured costs,
verified live entitlements, or a new pricing authorization. The owner now requests
beating Tsenta's reported $0.05 per application. Its public
[pricing page](https://tsenta.com/pricing), checked October 28, advertises these
monthly USD allowances:

| Plan | Monthly price | Applications | Price per application at full use |
| --- | ---: | ---: | ---: |
| Starter | $19 | 600 | $0.031667 |
| Pro | $39 | 1,500 | $0.026000 |
| Power | $99 | 4,500 | $0.022000 |

These are advertised customer prices, not Tsenta's internal costs or independent
proof of its delivery. Unused allowance increases effective customer price; billing
period, geography and terms must match any comparison. The $0.05 target alone would
not beat these published full-use prices.

Our existing $0.125 cloud-packet budget is 2.5 times $0.05 and approximately 5.68
times $0.022, before shared service costs. Its $0.06 AI component already exceeds
$0.05. These assumptions therefore do **not** establish a competitive delivery
cost. A completed packet is also not necessarily a successful employer submission.
Do not present either metric as the other or change customer prices from this note.

The pending cost workstream must report separately:

- Effective customer price, variable delivery cost, fully allocated recurring
  service cost and gross margin, at low/base/high allowance utilization.
- Total discovery, verification, AI/document, browser/runner, retry/failed/unknown
  attempt, idle capacity, storage, networking, communication, monitoring, payment,
  refund and support costs divided by distinct receipt-verified submissions.
- Source/ATS mix, sample size, observation period, success and intervention rates,
  duplicate rate, match quality and p50/p95 latency alongside cost. Low-cost failures
  and unmatched submissions cannot establish an end-to-end win.
- Per-user monthly and 100-user six-month forecasts, measured inputs versus explicit
  assumptions, break-even scale and enforceable budgets. Keep one-time development
  expense separate from recurring service economics.

Candidate optimizations are reusable verified profile facts, bounded truthful
tailoring, shared source-verification caches with appropriate freshness, deterministic
supported-ATS paths before semantic fallback, economical quality-tested model routing,
and higher runner utilization. All require measurements and must preserve consent,
tenant isolation, apply-once constraints, truthful facts and genuine receipts.

Delivery tracking: existing Asana cost task `1219993283940797`. No measured cost
advantage, release readiness, unlimited source coverage or new price is established.

### Feasibility boundary, not a new plan

At the published Power full-use price of $0.022 per application, a hypothetical
60% gross margin permits at most $0.0088 total recurring COGS per verified
submission; 70% permits $0.0066. These are arithmetic ceilings, not achieved costs,
chosen margin requirements or authorization to match/change prices. Costs such as
payment fees, support and idle capacity consume part of that ceiling before an AI
call or browser step. Low utilization must be modelled separately.

Consequently, a permanently running visual agent plus repeated large-model
reasoning for every step cannot be assumed competitive. The next measured
comparison should separate deterministic provider execution, genuinely necessary
semantic fallback and user-assisted exceptions. Reuse verified candidate facts and
role-grounded document work only with exact revision/consent/freshness bindings;
stop runners at completion or intervention; enforce per-attempt time/dollar caps.
Compare quality and actual receipts alongside costs. These are test candidates,
not claims that all optimizations or safe budget enforcement are already shipped.

### Current implementation measurement audit — October 28, 2026

Read-only source inspection, not runtime or invoice qualification:

- Managed resume generation already limits output, retries, deadlines and spend
  exposure. Provider attempt holds and settlement retain accepted and non-accepted
  outcomes, token counts, provider/model and latency. A positive configured rolling
  cap is required for paid admission. Source: `server/src/api/jobs_resume_generation.rs`,
  `server/src/db/jobs_provider_cost_holds.rs` and `server/src/config.rs`.
- Accepted generation cost alone is an incomplete numerator. Rejected, failed,
  timeout and uncertain provider attempts also consume resources. Missing usage is
  conservatively reserved/settled; it is not evidence of a free call.
- Application-to-generation allowance reservations and receipt/evidence validators
  provide existing attribution and denominator seams. An aggregate must validate
  submitted envelopes and exact evidence bindings, not simply count a status string.
  Source: `server/src/db/jobs/applications.rs`, `jobs_generation_allowance.rs`,
  `customer_data.rs`, `server/src/db/jobs.rs` and `server/src/api/jobs.rs`.
- There is no complete browser/runner COGS ledger in the inspected paths. Lease,
  browser-session and runtime timestamps are coarse wall-time proxies, not measured
  billed active/idle/startup/intervention time, proxy usage or invoice charges.
- Whole-cent rounded cost records support conservative spend admission but cannot
  establish precise sub-cent aggregate delivery costs. Durable usage events lack a
  direct application/generation key; opaque holds can be rejoined in trusted code
  only while physically retained. The spend-truth retention threshold is fixed at
  31 days by `UPSTREAM_SPEND_TRUTH_RETENTION_MS`; the configurable rolling spend
  window is a separate admission rule. Pruning uses `updated_at_ms`, while report
  cohort membership uses immutable `created_at_ms`. Retention is not a completeness
  witness: no durable observation watermark exists, and the earliest retained row
  does not prove history start. These records cannot by themselves support a
  reproducible six-month attribution report.
- Applied rate-version and invoice provenance, discovery/document/storage/network,
  communications, payment/refund/support and shared idle-capacity allocations remain
  measurement gaps. Unknown costs must render the report incomplete, never zero.

November 1 source correction: the smallest next measurement slice is a bounded
read-only partial-provider-exposure sample within the fixed retention threshold.
For one explicit account/window cohort, include both unresolved held projections
and all settled hold amounts, with each hold contributing exactly once. Usage rows
annotate provenance only; they must not add another amount or multiply rows, and
duplicate linkage fails the report. Revalidate stored receipt/evidence metadata and
report distinct metadata-consistent application IDs separately; do not treat that
count as employer delivery or a verified-success denominator. Completeness and the
per-success ratio remain unavailable without a durable observation watermark and
independently verified delivery. Mark lease wall time `unpriced_proxy` and runner
cost `unverified`. Test a metadata-consistent stored submission with accepted and
rejected model attempts, an unresolved hold, an unknown-delivery retry, and a failed
or released application: every exposed hold appears once, valid zero-settlement
released holds remain non-exposed, and no unverified application becomes a success.
Later precise immutable cost attribution and rate/invoice snapshots need their own
reviewed bounded phase; do not rewrite existing billing/authorization accounting to
create a cheap result. This correction changes neither prices nor billing policy.

## Pricing

| Plan | Price | Included completed packets | Browser | Inboxes | Packet overage |
| --- | ---: | ---: | --- | ---: | ---: |
| Free | $0 | 5 reviewed | Review only | 1 | $0.50 |
| Pro | $29/month | 50 | Local | 2 | $0.50 |
| Cloud | $49/month | 100 | Local and cloud | 5 | $0.50 |

Additional independent inboxes are $4/month. Aliases and application-email strings are not charged. Extra inbox fees should be added to the recurring Jobs invoice rather than collected as separate $4 payments, avoiding another fixed processor fee.

Annual plans stay off until at least 60 days of production usage establishes P50 and P95 browser, model, intervention, support, refund, and fraud costs.

## Direct Cost Assumptions

These are planning budgets, not promises.

- Square API payments: 2.9% + $0.30 per online transaction.
- AI packet budget: $0.06, based on a representative 20,000 input and 5,000 output tokens on GPT-5.4 mini plus a second-pass buffer. Current published raw cost for that example is about $0.0375.
- Pro/local completed packet budget: $0.085 including AI, document generation, workflow actions, storage, and search.
- Cloud completed packet budget: $0.125 including the local packet budget plus browser time, proxy variance, screenshots, and constrained semantic fallback.
- Connected inbox operating budget: $0.25/month for notification processing, cursors, storage, renewal, and support. Gmail API calls are currently free under quota; this budget prevents us from treating sync as costless.
- Paid-account shared infrastructure allocation: $1/month at useful scale.
- Support, refund, fraud, and dispute reserve: 8% of plan revenue.

Official inputs:

- Square fees: https://squareup.com/us/en/payments/our-fees
- Temporal Cloud pricing: https://temporal.io/pricing and https://docs.temporal.io/cloud/pricing
- Browserbase pricing: https://www.browserbase.com/pricing
- Gmail quotas and push notifications: https://developers.google.com/workspace/gmail/api/reference/quota and https://developers.google.com/workspace/gmail/api/guides/push
- Microsoft Graph throttling and change notifications: https://learn.microsoft.com/en-us/graph/throttling and https://learn.microsoft.com/en-us/graph/change-notifications-overview
- Google Pub/Sub pricing: https://cloud.google.com/pubsub/pricing
- GPT-5.4 mini pricing: https://developers.openai.com/api/docs/models/gpt-5.4-mini

## Margin Model

| Plan scenario | Revenue | Estimated direct COGS | Contribution | Margin |
| --- | ---: | ---: | ---: | ---: |
| Pro, 50/50 packets used | $29.00 | $9.21 | $19.79 | 68.2% |
| Pro, 30/50 packets used | $29.00 | $7.51 | $21.49 | 74.1% |
| Cloud, 100/100 packets used | $49.00 | $20.39 | $28.61 | 58.4% |
| Cloud, 60/100 packets used | $49.00 | $15.39 | $33.61 | 68.6% |

Formulas:

- Pro full: `50 × $0.085 + 2 × $0.25 + $1 infra + $1.14 payment + $2.32 reserve = $9.21`.
- Cloud full: `100 × $0.125 + 5 × $0.25 + $1 infra + $1.72 payment + $3.92 reserve = $20.39`.
- A $0.50 Cloud overage has about $0.125 packet cost plus an allocated share of the prepaid-balance processor fee. The target contribution margin remains above 65%.
- A $4 inbox slot has about $0.25 sync cost plus support, reserve, and incremental invoice fees. The target contribution margin remains above 70% when billed with the plan.

This table excludes CAC, general R&D, founder time, and company-wide fixed costs. At low volume, fixed Temporal, browser, database, search, and observability minimums dominate. Finance should report both direct contribution margin and fully loaded operating margin.

## Guardrails

- Track P50, P95, and maximum cost per committed packet by ATS, runner, model route, retry count, and intervention type.
- Deterministic ATS adapters run first. Semantic browser fallback gets a strict time and dollar ceiling, then hands control to the user.
- Meter once per canonical job when a final packet is downloaded, approved, queued, or submitted. Retries, regeneration, and handoffs never double-charge.
- Email/calendar sync is push-based; continuous polling is prohibited.
- Cloud browser sessions stop when the application is submitted, blocked, handed off, or idle.
- If Cloud full-utilization contribution stays below 60% for two billing periods, either move Cloud to $59/month or reduce the included allowance to 75 before adding discounts.
- Review this model monthly against processor statements, model invoices, browser hours, proxy bandwidth, support time, refunds, and disputes.

## Competitive Check

The Jobs market ranges from low-cost search/automation tools to higher-touch application agents. Bluey's price should be defended by per-job resume uniqueness, local/cloud continuity, receipts, interventions, multi-inbox status sync, and shared Bluey balance rather than by maximizing application volume.
