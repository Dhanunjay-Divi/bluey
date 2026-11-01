# Jobs competitive-cost acceptance

October 29, 2026. Read-only source review plus evidence documentation, not a
measured cost advantage, implemented cost report, price change or release approval.
Scope: Jobs only, Phase 638, HEAD `40949858`, 774-source manifest `3de70...`.
Tracking authority remains Asana cost task `1219993283940797`; do not create a
competing delivery board.

## Actual comparison

[Tsenta public pricing](https://tsenta.com/pricing), rechecked October 29, advertises
monthly USD $19/600, $39/1,500 and $99/4,500 applications. At full allowance use the
customer prices are $0.0316667, $0.026 and $0.022 per application. These are public claims and
customer prices, not independent delivery verification or Tsenta internal COGS.
Beating $0.05 alone would not beat these full-use offers. Unused allowance, billing
period and market change the effective customer comparison.

Bluey's historical $0.125 cloud-packet/$0.06 AI budgets are assumptions, not measured
receipt-verified delivery costs. A prepared packet is not an employer submission.
Do not lower billing, safeguards or receipt requirements to manufacture parity.

## Next measurable slice: partial provider exposure

The existing UNIT_ECONOMICS.md proposal near line 102 says all settled attempts.
That is insufficient: ProviderAttemptGuard::drop logs a failed conservative
settlement and may leave a hold (server/src/api/jobs_resume_generation.rs:1068).
The current generation-admission calculation already includes both held and
settled rows (server/src/db/jobs_provider_cost_holds.rs:395).

For one explicit account/window cohort, use `request_scope_hash` as the unique
base-row key (server/src/db/mod.rs:1565-1583). Classify each hold once and sum only
hold amounts; usage joins annotate provenance, never contribute another amount or
multiply base rows. More than one matching usage row is a report failure. Missing
provenance/linkage is an explicit incomplete bucket, not a guessed classification.

```text
held_unresolved_projection = sum(projected_cost_cents for held rows)
settled_exact_usage_priced = sum(settled_cost_cents for settled/exact rows)
settled_estimated_conservative = sum(settled_cost_cents for settled/estimated rows)
settled_missing_conservative = sum(settled_cost_cents for settled/missing rows)
provider_exposure = held_unresolved_projection + all settled hold amounts
verified_successes = COUNT(DISTINCT application_id after full receipt validation)
partial_exposure_per_success = provider_exposure / verified_successes
```

Held exposure is conservative unresolved exposure, NOT an invoice charge.
Non-exact settlements also use a conservative maximum of reported/projected cost
(server/src/db/jobs_provider_cost_holds.rs:735-744), not a measured invoice charge.
Keep all buckets separate and mark the result incomplete; even exact usage-priced
amounts are not invoice-reconciled COGS. Never
add the same hold's projection and settlement or duplicate its linked usage event.
Include exposed attempts for rejected/failed/timeout/uncertain work and applications
without success in the chosen cohort; do not select only successful generations.
Unknown attribution/cost is a visible gap, never a zero. Zero validated receipts
produces unavailable per-success cost, not zero or a divide-by-zero result.

Use only fully validated stored receipts, not a submitted status string, and count
distinct applications rather than receipt IDs/replays/aliases as the denominator.
Existing validation seams are server/src/api/jobs.rs:7642-7680,
7840-7877 and7940-7974. Choose a complete observation window inside the configured
retention (maximum 30 days plus 24-hour grace, server/src/config.rs:163-169).
Require `observation window + receipt closeout lag + safety margin < effective
configured hold retention`; the maximum 31-day horizon is not permission to ignore
a shorter configured retention. Fail the report if earliest-row completeness
cannot be established at the actual report cutoff.
Define start/end, report-as-of cutoff and receipt closeout lag explicitly. Hold
`created_at_ms` determines attempt-cohort membership; receipt `submitted_at_ms`
determines success-window membership. Never use mutable `updated_at_ms` for cohort
membership (settlement updates it, jobs_provider_cost_holds.rs:791-804). Report late
receipts and cross-window attempts separately so the period exposure/yield ratio
is not misrepresented as exact per-application attributable cost. Incomplete linkage or
expired holds preclude a complete historical aggregate; do not extrapolate six
months from retained rows or query an owner database in this phase.

Acceptance fixture: one valid receipt with accepted/rejected attempts; another
application failing without receipt; one unresolved hold after settlement failure;
one unknown-delivery retry; a duplicated usage reference; zero-success case; late
receipt/settlement and replay; and out-of-window/missing-attribution records.
Every distinct exposed hold appears once,
only the valid receipt increases the denominator, and all gaps stay visible.
This fixture/report is NOT implemented or runtime-qualified by this document.

## Remaining end-to-end proof

Current whole-cent usage and indirect retained linkage cannot establish precise
sub-cent, six-month application costs. A separately reviewed immutable ledger must
bind application/generation/run/receipt, quantity/unit, component, micro-unit amount,
rate version, estimated versus billed provenance and invoice reconciliation.
Do not replace conservative billing/admission accounting with financial estimates.

Runner startup/active/idle/intervention time, proxy/network, discovery, documents,
storage, communications, monitoring, payment/refund/support and shared capacity
remain necessary. Lease wall time is an unpriced proxy, not a billed runtime ledger.

Report customer effective price, partial provider exposure and complete recurring
COGS separately. At $0.022 revenue, hypothetical 60% margin allows $0.0088 total COGS;
this arithmetic is not an achieved cost or chosen plan. Per-user monthly and
100-user/six-month forecasts need measured utilization, verified receipts, all cost
components and fixed-capacity allocation. Forecast 600 user-months as:

```text
100 * 6 * non-application direct cost per user-month
+ 6 * shared fixed cost per month
+ total verified applications * variable delivery cost per verified success
```

The last term includes failed/retried attempt waste allocated to verified successes
and excludes every cost in the first two terms. If costs/usage change over time,
sum six monthly forecasts instead of assuming constant inputs. Shared fixed bills
are not multiplied by 600. With no measured success yield, that last term remains
unknown; quoting a precise six-month total would be misleading.

Compare success/duplicate/intervention rates, role-fit quality, truthful tailoring,
supported ATS coverage and p50/p95 latency alongside cost. The short-term engineering
priorities are deterministic certified ATS paths, exact-revision factual reuse,
bounded quality-tested AI fallback and stopping idle runners. They remain testable
optimizations, not a claim that we already beat Tsenta or are ready to launch.
