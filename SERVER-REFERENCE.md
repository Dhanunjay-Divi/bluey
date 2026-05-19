# Bluey Server Reference

> **Exact live paths on each Bluey server.** Mirrors Pinky's
> `SERVER-REFERENCE.md` shape but Bluey-only.
>
> **State 2026-05-19:** No Bluey servers are stood up yet. This doc is
> populated as the canonical structure now and updated with real IPs +
> deployed hashes once R14.8 (distribution) + R14.9 (product server)
> servers are provisioned.

If you are looking for **why** the topology is the way it is, read
`ARCHITECTURE.md` first. This doc captures **where** things live.

---

## 1. Distribution server (Layer 2)

**Status:** ❌ not provisioned. Architecture decided in
`docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md`; awaiting user pick on
Path A / B / C + domain.

| Field | Value |
|---|---|
| Production IP / DNS | TBD |
| SSH user | TBD |
| Document root | `/var/www/bluey/` (Path C nginx) **OR** N/A (Path B CDN) |
| Web server | nginx (Path C) **OR** Caddy (Path A) **OR** CloudFront/Cloudflare (Path B) |
| TLS | LetsEncrypt (Path A/C) **OR** managed (Path B) |
| Public URL | `https://bluey.dev/...` (or `https://<host>/...` for raw-IP testing) |

### Layout (target, Path C / Path A)

```
/var/www/bluey/                     (or /opt/bluey-distribution/)
├── latest.json                     atomically swapped on promote
├── latest -> releases/v0.1.0       symlink
└── releases/
    ├── v0.1.0/
    │   ├── bluey-0.1.0-darwin-arm64.tar.gz
    │   ├── bluey-0.1.0-darwin-arm64.tar.gz.sha256
    │   ├── bluey-0.1.0-darwin-universal.tar.gz
    │   ├── bluey-0.1.0-darwin-universal.tar.gz.sha256
    │   ├── SHA256SUMS.txt
    │   └── RELEASE.md
    └── (future versions)
```

### URL contract (stable across paths)

```
GET https://<host>/install                  templated bash
GET https://<host>/install.sh               alias
GET https://<host>/install.ps1              templated PowerShell
GET https://<host>/latest.json              release manifest
GET https://<host>/downloads/v<ver>/<asset> immutable assets
GET https://<host>/admin/health             liveness check
```

### Promote flow (target)

```
dev box (uno)              preprod                       prod
make package-darwin-* →    publish.sh PUBLISH_DO=1   →   promote.sh
                           targets preprod path           swap latest
                                                          symlink
```

`scripts/publish.sh` (drafted) uploads the staged release dir + the
new `latest.json` to the configured host. Promotion to prod is an
explicit step (not auto).

---

## 2. Product server — preprod (Layer 3)

**Status:** ❌ not provisioned. Scaffolded as R14.9 in
`docs/rounds/PHASE-3-ROUND-14-PLAN.md`. Lives in a separate repo
(`bluey-server`) when stood up.

| Field | Value |
|---|---|
| Production IP / DNS | TBD |
| SSH user | TBD |
| Stack | Rust + SQLite + Caddy (Pinky operational shape, Bluey Rust stack) |
| Database | `/opt/bluey-api/bluey-preprod.db` |
| Stripe | test mode |
| Public URL | TBD (e.g. `https://api-preprod.bluey.dev`) |

### Layout (target)

```
/opt/bluey-api/
├── bluey-server                    Go binary
├── bluey-preprod.db                SQLite (sessions, accounts, billing state)
├── env                             environment file (BLUEY_JWT_SECRET, STRIPE_*, etc.)
└── logs/
```

### Endpoints (target)

```
POST /auth/{signup,login,refresh,reset}
POST /billing/{checkout,webhook}
GET  /account/me
POST /router/complete                 managed Auto Router endpoint
GET  /admin/customers                 Bluey-team only
GET  /admin/health
```

---

## 3. Product server — prod (Layer 3)

**Status:** ❌ not provisioned. Lights up after preprod settles + paid
alpha launch (Stage 4 in `ARCHITECTURE.md`).

Identical layout to preprod with these differences:

| Field | preprod | prod |
|---|---|---|
| DB | `bluey-preprod.db` | `bluey-prod.db` |
| Stripe | test keys | live keys |
| Logging | DEBUG | INFO |
| Backups | none | daily off-host snapshot |
| Monitoring | basic | uptime + alerting |

---

## 4. Source mirror on uno

The Bluey source-of-truth lives in this repo on uno at:

```
/Users/uno/Downloads/cue/
```

Server-side source mirrors (when servers exist) follow Pinky's pattern:

```
/tmp/bluey-distribution/        synced from this repo via deploy script
/tmp/bluey-server/              synced from the bluey-server repo (when it exists)
```

These mirrors exist to make ssh investigations on the live boxes easy
without needing the dev workstation handy. They are not the source of
truth; the GitHub repos are.

---

## 5. Update protocol

When a Bluey server is provisioned:

1. Fill in IP / SSH user / public URL in the relevant section above.
2. Document the actual deployed binary hash in a `Deployed:` block.
3. Document any environment-variable secrets the box needs (don't put
   the values here; document the names).
4. Reference the operations runbook (`docs/OPERATIONS-RUNBOOK.md`,
   when it exists) for restart / inspect / rollback procedures.
