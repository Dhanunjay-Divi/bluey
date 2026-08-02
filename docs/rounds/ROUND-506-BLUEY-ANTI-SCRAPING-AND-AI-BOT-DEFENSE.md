# Round 506 - Bluey Anti-Scraping And AI-Bot Defense

Date: 2026-07-12

Status: **SOURCE VERIFIED; LIVE EDGE NOT COMPLETE**

This round materially raises the cost of bulk crawling, authenticated API
harvesting, worker-route probing, and client-bundle extraction. It does not
claim that a public site can be made impossible to copy. Anything delivered to
a browser can be observed. Bluey's defensible product logic stays on trusted
servers and workers; edge controls, origin isolation, authentication, rate
limits, telemetry, and legal terms are layered around it.

Do not mark this round complete in production until the live gate near the end
of this document is entirely green.

## Scope And Non-Goals

Protected assets:

- Bluey marketing content, pricing, and public product behavior.
- Authenticated Jobs profiles, matches, application packets, receipts, and
  evidence.
- ATS policy, matching, generation, eligibility, anti-fraud, and automation
  logic.
- Worker and local-browser control surfaces.
- Bluey availability, account integrity, and abuse telemetry.

Non-goals:

- DRM for HTML, CSS, JavaScript, or API responses already delivered to an
  authorized browser.
- Client-side obfuscation, disabled right-click, hidden URLs, or CORS as
  anti-scraping controls.
- Trusting crawler User-Agent strings as identity.
- Treating `robots.txt` as authorization. RFC 9309 explicitly defines crawler
  policy, not an access-control mechanism.

## Threat Model

| Actor | Primary path | Risk | Controls in this round |
| --- | --- | --- | --- |
| Standards-compliant AI crawler | Public HTML and assets | Training/search ingestion | Cloudflare AI behavior blocks, Content Signals, fallback robot directives |
| Spoofed browser crawler | High-rate traversal with normal UA | Bulk content and route extraction | Bot score/JA4 where available, WAF, edge velocity rules, asset/HTML ratio alerts |
| Authenticated harvester | Farmed or compromised accounts | Jobs dataset and product-behavior extraction | Account+trusted-client Redis buckets, minimized errors, account-scoped queries, anomaly alerts |
| Tenant-ID prober | Guessed IDs in authenticated routes | Cross-account disclosure | Account-scoped DB access and indistinguishable 404 responses |
| Worker-route attacker | Public `/api/jobs/internal/*` | Workflow manipulation and receipt forgery | Public 404 boundary, private ingress, short-lived signed requests, scope/audience/body binding, replay rejection |
| Local-run token thief | Stolen browser deep link | Generic runner control | Single-use launch ticket plus operation-specific account/application/run/profile/expiry capabilities |
| Origin bypass attacker | Historical origin IP | WAF/rate-limit bypass | Cloudflare proxy plus AOP/Tunnel and origin firewall restricted to Cloudflare ranges |
| Client-bundle analyst | Downloaded Jobs JavaScript | Proprietary prompts/policy/selectors | Server-owned decision logic, no portal automation dependency, no source maps |
| Account farmer | Repeated signup/login/OTP and low-volume scrape | Limits bypass and inventory extraction | Auth/OTP limits, Turnstile escalation, account/device signals, creation-burst alerts |

Trust boundaries:

1. Internet to Cloudflare.
2. Cloudflare to the origin through authenticated/restricted ingress.
3. Caddy to the public API and loopback-only Jobs API.
4. Browser session to authenticated Jobs routes.
5. Private Temporal/discovery/browser workers to signed internal routes.
6. Bluey Browser to one run-specific local capability set.
7. API replicas to shared Redis/Valkey replay and rate-limit state.
8. API/workers to encrypted database and object storage.

## Live Baseline Before Rollout

Fresh probes on 2026-07-12 produced:

```text
dns=165.227.77.152
llms=200
gptbot=200
missing_map=200
internal=401
workspace=401
jobs_xrobots=<missing>
server=server: Caddy
direct_https=200
direct_http=308
```

Interpretation:

- The Jobs customer API correctly requires authentication (`401`).
- The worker route also authenticates, but it is still publicly routable; the
  edge must make it indistinguishable with `404`.
- `/llms.txt`, a fake source map, and GPTBot still receive `200` in production.
- The historical origin remains directly reachable over HTTP and HTTPS.
- No enforcing Cloudflare proxy/WAF state is visible from these probes.

This is deliberately recorded as **before** evidence. There is no after
evidence because this turn did not have Cloudflare-zone or origin-firewall
credentials and did not deploy.

## Source Implementation

### Crawler and static policy

- `web/robots.txt` now reserves AI input/training rights with
  `Content-Signal: search=yes, ai-input=no, ai-train=no` and denies known
  self-identifying AI crawlers as fallback defense.
- `web/llms.txt` is removed. Marketing and sitemap references are removed.
- `ops/Caddyfile.example` returns explicit `410` for `/llms.txt` before SPA
  fallback.
- Jobs, API, account, device, link, reset, verification, and private document
  surfaces emit `X-Robots-Tag: noindex, nofollow, noarchive, nosnippet`.
- `/assets/*` and `/jobs/assets/*` resolve through explicit file serving. A
  missing asset or `.map` no longer reaches either SPA fallback.
- Caddy removes the `Server` response header.
- The Jobs Vite build has `sourcemap: false`; source checks fail if a `.map`
  appears under `web/`.
- A random, unadvertised Caddy canary path returns `404` while setting an
  access-log marker for incident alerting. It is not listed in `robots.txt`.

### Edge and origin runbook

`ops/CLOUDFLARE-BLUEY-EDGE-HARDENING.md` defines the production sequence:

1. Proxy `bluey.sh` and `www.bluey.sh` through Cloudflare.
2. Use Full (strict) TLS and Authenticated Origin Pulls or Cloudflare Tunnel.
3. Restrict origin TCP 80/443 to current Cloudflare ranges and explicit trusted
   monitors while preserving SSH recovery.
4. Block Cloudflare AI Training, Agent, and Search behavior classes.
5. Enable AI Crawl Control, violation reporting, bot defenses, and managed
   challenge/block rules based on behavior rather than UA alone.
6. Establish normal traffic in log/challenge mode before hard blocking tuned
   traversal and API patterns.
7. Validate Turnstile tokens server-side when repeated auth abuse escalates to
   a challenge.
8. Run the live verifier and preserve Cloudflare/Caddy/firewall evidence.

Keeping verified conventional search is a product tradeoff. It cannot
guarantee exclusion from every AI feature. Literal no-crawler mode requires
`User-agent: *` plus `Disallow: /` and noindex headers, which sacrifices SEO.

### Private worker authentication

The public Caddy host rejects `/api/jobs/internal/*` before the Jobs proxy.
Same-host workers use loopback; remote workers require a separate
Access/mTLS-protected private service.

Each worker request now carries an HMAC signature over:

```text
version
timestamp
nonce
worker identity
audience
operation scope
HTTP method
path
SHA-256 body hash
```

Properties:

- Audience is fixed to `bluey-jobs-api`.
- Timestamp tolerance is 90 seconds.
- Discovery, execution, receipt, intervention, application-state, and run-event
  paths have distinct scopes.
- Redis/Valkey reserves nonces atomically across API replicas; strict
  production mode fails closed.
- Current and previous signing keys support bounded rotation.
- Body ceilings are route-specific: 4 MiB ordinary, 32 MiB discovery snapshot,
  and 64 MiB verified receipt/evidence submission.
- Legacy shared bearer support compiles only in debug builds for compatibility
  tests.

### Local Bluey Browser capabilities

The one-time launch ticket is consumed by `/claim`. The API then issues
separate HMAC capabilities for `result` and `resume`, each bound to:

- Bluey account;
- application;
- run;
- identity-scoped browser profile;
- allowed operation;
- original ticket expiry;
- random capability nonce.

The browser validates capability shape, operation separation, run, and expiry,
then removes `_blueyCapabilities` from the application payload before it can be
written to documents, receipts, or local audit files. The release server does
not accept the root ticket for result/resume operations.

### Jobs API throttling and IDOR resistance

Jobs routes use shared Redis/Valkey buckets with local development fallback:

| Class | Default per minute | Burst |
| --- | ---: | ---: |
| Authenticated reads | 240 | 120 |
| Ordinary writes | 60 | 30 |
| Generation/commit/interview preparation | 12 | 6 |
| Identity/mailbox/OTP operations | 10 | 5 |
| Browser/run/intervention operations | 60 | 20 |
| Evidence/receipt uploads | 12 | 4 |

Authenticated keys combine account and trusted client IP. Local-run delivery
has a separate IP bucket before ticket/capability lookup. A denial returns
`429` with `Retry-After` and logs only account/client/rate class/path metadata,
not resume, answer, cookie, OTP, or payload content.

Public Jobs handlers derive tenant identity from the authenticated account.
Cross-account and nonexistent match IDs produce the same `404` behavior.
Worker/admin paths use separate service/admin boundaries. Evidence and receipt
routes have explicit body and item-count ceilings.

Follow-up before broad general availability: add cursor pagination to every
large Jobs collection and workspace subsection. Current rate limits and
account scoping prevent cross-tenant bulk extraction, but cursor-bounded
responses are still the stronger long-term contract for large accounts.

### Product-logic containment

- The portal no longer imports `@bluey/jobs-automation`.
- Interview preparation prompts, grounding rules, sensitive-field policy, and
  evidence selection moved out of the customer bundle. The dialog now renders
  server-owned preparation results and receipt metadata.
- Built bundle checks reject worker signing names, worker headers, coaching
  prompts/rules, and source maps.
- Public local/cloud runner copy is invitation-only beta unless the account has
  the corresponding entitlement.
- The browser renders state and sends user intent; eligibility, policy,
  generation, automation, and fraud decisions remain trusted-side.

### Terms and privacy

The July 12 source text adds:

- automated extraction, account farming, training, benchmarking, and access-
  control evasion restrictions;
- licensing and security-research contact at `security@bluey.sh`;
- narrow security metadata categories and purposes;
- an explicit statement that content, passwords, cookies, payment data, and
  verification codes should not enter ordinary security telemetry.

These terms are evidence and deterrence, not a technical block. Counsel review
is required before production publication.

## Verification Evidence

### Passing source and behavior checks

```text
Jobs JavaScript tests:       226 passed
  automation:                119
  Bluey Browser:              26
  cloud runner:               34
  workflows:                  23
  portal:                     24

Server unit tests:           332 passed
Server HTTP integration:      66 passed
Server clippy -D warnings:     passed
Jobs workspace typecheck:      passed
Portal production build:       passed
Jobs privacy gate:             passed (1,748 tracked paths; 1,531 text files)
SQLite/Postgres parity:        passed (5 tables; 8 indexes)
Provenance/license inventory:  passed (14 commit-pinned repositories)
Edge source-policy check:      passed
Client/server boundary check:  passed
git diff --check:              passed for scoped files
```

Focused security proofs include:

- signed worker success plus replay, expiry, scope, and body-tamper rejection;
- shared rate and replay state across two independent Redis-backed instances;
- authenticated read exhaustion returns `429` and `Retry-After`;
- local-run enumeration is rate-limited before secret lookup;
- result/resume capability swapping is rejected;
- account, application, run, profile, operation, and expiry binding;
- cross-account IDs are indistinguishable from missing IDs;
- crash-after-submit stays `side_effect_unknown` and cannot blindly retry;
- source and production bundles contain no source maps or protected prompt/
  worker-auth markers.

### Browser QA

- Desktop preview: `1440 x 900` class, dark theme, no overlap.
- Mobile preview: `390 x 844`, no body overflow; all five navigation items stay
  within the viewport. The Career Track strip is the only intentional
  horizontal scroller.
- Route checked: `/jobs/matches?preview=1`.

Assets:

- `ROUND-506-BLUEY-ANTI-SCRAPING-AND-AI-BOT-DEFENSE.assets/jobs-desktop.png`
- `ROUND-506-BLUEY-ANTI-SCRAPING-AND-AI-BOT-DEFENSE.assets/jobs-mobile.png`

## Production Go/No-Go Gate

| Acceptance check | Current result |
| --- | --- |
| AI Training/Search/Agent classes blocked by Cloudflare | **BLOCKED - external configuration absent** |
| Spoofed browser traversal challenged/rate-limited | **BLOCKED - edge rules not deployed** |
| Direct historical-origin HTTP/HTTPS bypass fails | **FAIL - 308/200** |
| `robots.txt` 200 text/plain with new source policy | Source pass; live policy not deployed |
| `/llms.txt` is 410 | **FAIL - live 200** |
| Jobs/API/account surfaces emit X-Robots-Tag | **FAIL - live header absent** |
| Missing Jobs assets/maps return 404 | **FAIL - live 200 SPA fallback** |
| Unauthenticated Jobs workspace returns 401 | **PASS - live 401** |
| Cross-account identifier non-disclosure | Source integration pass; authenticated live synthetic pending |
| Abusive authenticated reads return 429 + Retry-After | Source integration pass; live synthetic pending |
| Public worker paths are unreachable/404 | **FAIL - live 401 proves public routing** |
| Signed worker success; expired/replayed credential rejection | Source integration pass; private live worker synthetic pending |
| Square webhooks, OAuth callbacks, downloads, health, desktop clients, runners | Source regression pass; post-edge live smoke pending |
| Cloudflare/Caddy/firewall screenshots and rollback archive | **BLOCKED - rollout not performed** |

The correct release decision is **NO-GO for claiming live anti-scraping edge
completion**. Source hardening can proceed to review, but deployment requires
Cloudflare and origin-host access.

## Deployment And Rollback

Use `ops/CLOUDFLARE-BLUEY-EDGE-HARDENING.md` as the operator runbook.

Deployment order:

1. Back up DNS, Caddy, TLS, firewall, and service environments; keep SSH open.
2. Set independent worker and local-run signing keys plus strict Redis mode.
3. Deploy API and workers; prove signed private-worker and local-browser flows.
4. Deploy web/Caddy source; prove `/llms.txt`, asset misses, worker-route 404,
   noindex headers, webhooks, OAuth, downloads, and health.
5. Proxy DNS and enable Full (strict) plus AOP/Tunnel.
6. Restrict origin 80/443 to Cloudflare/trusted monitors and prove direct
   origin bypass fails.
7. Enable AI behavior blocks, bot controls, rate rules, and canary alerts in
   observation/challenge mode before tuned blocking.
8. Run `scripts/verify-bluey-edge-live.sh <historical-origin-ip>` and archive
   output plus control-plane screenshots.
9. Run authenticated synthetics and normal desktop/mobile browser QA.

Rollback order:

1. Disable newest WAF/rate rules in reverse order while keeping origin
   isolation intact.
2. Roll back Caddy/web/API and signing configuration if application regression
   is isolated to source deployment.
3. If Cloudflare itself must be bypassed, restore saved DNS, Caddy, certificate,
   and pre-change firewall snapshots together. Never expose the origin while
   DNS still implies WAF protection.
4. Re-run health, auth, Square webhook, OAuth, download, Jobs, worker, and
   browser smoke checks before closing the recovery SSH session.
5. Preserve the incident and rollback event timeline.

## Official References

- RFC 9309: <https://datatracker.ietf.org/doc/html/rfc9309>
- Cloudflare AI Crawl Control: <https://developers.cloudflare.com/ai-crawl-control/>
- Cloudflare AI bot blocking: <https://developers.cloudflare.com/bots/additional-configurations/block-ai-bots/>
- Cloudflare origin protection: <https://developers.cloudflare.com/fundamentals/concepts/cloudflare-ip-addresses/>
- Cloudflare Full (strict): <https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/full-strict/>
- Cloudflare rate-limit practices: <https://developers.cloudflare.com/waf/rate-limiting-rules/best-practices/>
- Cloudflare Turnstile: <https://developers.cloudflare.com/turnstile/>

## Handoff

The next operator should not add more client obfuscation or UA-only blocks.
They should execute the production runbook, preserve before/after evidence, and
stop immediately if direct-origin bypass remains possible. After the live gate
is green, the next source hardening task is cursor pagination and response
minimization for large Jobs collections, followed by authenticated production
synthetics and security dashboards keyed by bot class, IP/ASN, JA4, account,
device, path, response bytes, and 401/403/404/429 velocity.
