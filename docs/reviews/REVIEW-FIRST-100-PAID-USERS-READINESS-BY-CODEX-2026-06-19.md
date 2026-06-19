# Review: First 100 Paid Users Readiness

Verdict: 🟡 ACCEPT WITH LIVE GATES

## Findings

- No code blocker in this docs-only round. The first-100 plan correctly keeps
  users free from Redis/Postgres/Docker setup and keeps provider secrets on
  Bluey infrastructure.
- The plan is honest about latency: one droplet is acceptable for the first 100
  controlled paid users, but not sufficient for worldwide ultra-low latency.
- Remaining launch blockers are live gates, not architecture ambiguity:
  Square webhook crediting, provider funding/smoke, signed installer/update
  verification, clean-Mac paid-alpha smoke, and off-host backup verification.

## Residual Risk

- Provider rate limits and first-token latency can still dominate user
  experience even if the droplet is healthy.
- A single droplet is a single point of failure. Acceptable for controlled
  alpha only if backups, monitoring, and operator response are real.
- Worldwide users may see slower captions until regional relays are added.

## Recommendation

Use `docs/deploy/FIRST-100-PAID-USERS.md` as the operating gate for the first
paid invites. Do not add scale components preemptively; add them when the
upgrade triggers fire.

