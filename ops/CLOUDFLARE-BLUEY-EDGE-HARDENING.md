# Bluey Cloudflare Edge Hardening

This runbook is intentionally separate from application deployment. A public
site cannot be made impossible to copy; anything delivered to a browser can be
observed. Bluey keeps proprietary matching, policy, automation, and anti-fraud
logic server-side, then uses Cloudflare, origin isolation, authentication,
throttling, and detection to make bulk extraction materially harder.

## Required Deployment Order

1. Confirm out-of-band SSH access and take copies of DNS, Caddy, firewall, and
   certificates. Keep one existing SSH session open during the change.
2. Add `bluey.sh` to Cloudflare. Proxy both `bluey.sh` and `www.bluey.sh`.
3. Install an origin certificate that covers both hostnames. Set SSL/TLS to
   **Full (strict)**. Prefer Authenticated Origin Pulls or Cloudflare Tunnel.
4. Permit TCP 80/443 only from Cloudflare's published IPv4/IPv6 ranges and
   explicit trusted monitors. Do not change SSH rules. Then reject every other
   source to 80/443.
5. Verify direct requests to the historical origin IP fail even when the Host
   header and TLS SNI are `bluey.sh`. Only after that verification, enable WAF
   and bot policies in challenge/log mode.
6. In **Security Settings > Configure AI bot policies**, set Training, Agent,
   and Search to **Block on all pages**. Cloudflare's July 2026 taxonomy treats
   traditional and AI search as the same Search behavior. If Bluey later keeps
   Google/Bing indexing, create explicit verified-bot exceptions with product
   approval and accept that this cannot guarantee exclusion from every AI
   feature.
7. Enable AI Crawl Control, managed robots.txt, violation reporting, Bot Fight
   Mode/Super Bot Fight Mode as available, and AI Labyrinth for non-compliant
   crawlers. The origin `robots.txt` remains the fallback policy.
8. Create rate-limit rules in observation/managed-challenge mode first. Tune
   from normal traffic before switching abusive signatures to block.
   Add Turnstile to repeated signup/login recovery challenges rather than
   silently accepting a client-controlled success flag; validate every token
   server-side for the intended hostname and action.
9. Set `BLUEY_SCRAPE_CANARY_TOKEN` to a randomly generated, unadvertised value
   in the Caddy service environment. Alert whenever a matching access-log entry
   carries `X-Bluey-Scrape-Canary: hit`; first rule out an approved scanner or
   monitor before blocking the source.

## Initial Edge Rate-Limit Matrix

| Surface | Starting threshold | Key | Action |
| --- | ---: | --- | --- |
| Anonymous HTML traversal | 60 requests / minute | IP, then JA4 when available | Managed challenge |
| Static assets | 300 requests / minute | IP or JA4 | Managed challenge |
| Auth login/signup failures | 10 / 5 minutes | IP + session cookie | Managed challenge, then block |
| OTP/reset/device endpoints | 10 / 10 minutes | IP + session cookie | Block |
| Refresh | 60 / minute | IP + session cookie | Block |
| `/api/jobs/*` reads | 180 / minute | IP + authenticated session | Challenge; origin also enforces account+IP |
| `/api/jobs/*` writes | 60 / minute | IP + authenticated session | Block; origin also enforces account+IP |
| Repeated 401/403/404 | 10 / 3 minutes | IP or JA4 | Managed challenge |

With Bot Management, add a managed challenge for bot score below 30 and block
below 10. Use Cloudflare's rate analysis before tightening. Do not exempt a
client solely because it sends a browser User-Agent.

## Private Jobs Workers

The public hostname returns `404` for `/api/jobs/internal/*`. Same-host workers
call `127.0.0.1:8081` directly. Remote workers require a separate internal
hostname protected by Cloudflare Access service tokens or mTLS, with no public
DNS-only route to the Jobs listener. Every worker request is additionally
signed with `BLUEY_JOBS_WORKER_SIGNING_KEY` and carries a 90-second timestamp,
unique nonce, Jobs API audience, worker identity, method, path, body hash, and
operation scope. Redis rejects nonce replay across API replicas.

## Origin Firewall

Fetch current ranges immediately before the change:

```sh
curl -fsS https://www.cloudflare.com/ips-v4
curl -fsS https://www.cloudflare.com/ips-v6
```

Create explicit allow rules for those ranges on 80/443, keep established
connections and SSH recovery allowed, then add a final drop for all other
sources to 80/443. Store the pre-change rules with the host backup. Cloudflare
publishes range changes before use; automate a monitored refresh instead of
letting a stale allowlist silently break traffic.

## Detection And Response

Send Cloudflare security events and Caddy JSON access logs to the security log
sink. Dashboard crawler category, action, bot score, IP/ASN, JA4 when available,
path, response bytes, and 401/403/404/429 rates. Alert on rapid HTML traversal,
low asset-to-HTML ratios, account creation bursts, repeated tenant-ID probes,
canary hits, and reuse of a canary path from another ASN. Keep application
payloads, resumes, answers, cookies, and OTP values out of security telemetry.

For a canary hit: preserve the edge event and request ID, challenge the source,
look for authenticated account/device correlation, revoke sessions only when
account compromise is plausible, and record every block/unblock action. The
canary is detection evidence, not authentication and not a reason to expose a
sensitive internal route.

## Rollback

1. Keep the origin firewall in place while rolling back WAF/rate rules.
2. Disable the newest WAF and rate-limit rules in reverse order.
3. If Cloudflare itself is the failure, restore the saved DNS records and the
   prior Caddy file, then restore the pre-change firewall snapshot so direct
   traffic can reach the origin again.
4. Verify `/health`, login, Square webhooks, OAuth callbacks, downloads, Jobs,
   and normal browser access before closing the recovery SSH session.
5. Record every rollback action and preserve the Cloudflare event timeline.

## Required Evidence

Run `scripts/verify-bluey-edge-live.sh` with the historical origin IP after
each rollout. Archive its output plus screenshots of AI policies, WAF/rate
rules, DNS proxy state, Full (strict), origin firewall rules, and rollback
configuration. Do not mark the edge complete until every check passes.

Official references:

- RFC 9309: <https://datatracker.ietf.org/doc/html/rfc9309>
- AI Crawl Control: <https://developers.cloudflare.com/ai-crawl-control/>
- AI bot policies: <https://developers.cloudflare.com/bots/additional-configurations/block-ai-bots/>
- Origin protection: <https://developers.cloudflare.com/fundamentals/concepts/cloudflare-ip-addresses/>
- Full (strict): <https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/full-strict/>
- Rate limits: <https://developers.cloudflare.com/waf/rate-limiting-rules/best-practices/>
- Turnstile: <https://developers.cloudflare.com/turnstile/>
