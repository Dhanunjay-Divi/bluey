# Bluey Jobs Unit Economics

Internal operating model. Customer-facing UI should show plan inclusions and prices, not infrastructure or margin details.

Checked July 10, 2026.

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
