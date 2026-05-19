# Bluey Distribution Architecture — v0.1

> **Decision (2026-05-19, user):** v0.1 ships terminal-only, BYOK, no SaaS.
> Distribution should follow a similar architecture to the existing Pinky
> daemon hosting on uno's Pinky API server, NOT a parallel reinvention.

This doc captures the architecture, not the implementation. Implementation
lands in a follow-up commit (publish.sh + install.sh changes + Pinky-side
config update).

## Reference: how Pinky distributes

```
Source of truth:    /tmp/pinky-full/cmd/pinky-server/main.go
                    /tmp/pinky-full/internal/api/server.go (Routes())
Production server:  161.35.177.238  (pinky.sh)
Relay server:       167.71.175.146  (relay.pinky.sh)

Endpoints relevant to distribution:
  GET /downloads/                  -> http.FileServer(http.Dir("downloads"))
  GET /install                     -> handleInstallScript     (templated bash)
  GET /install.sh                  -> same
  GET /install.ps1                 -> handleInstallPS1        (templated pwsh)
```

So Pinky's distribution is:

1. **Static file root** `downloads/` next to the running pinky-server binary.
   Files placed there are served unauthenticated at `/downloads/<file>`.
2. **Templated install scripts** at `/install`, `/install.sh`, `/install.ps1`
   that read the latest version + sha from server config and emit a
   shell/pwsh script the user can `curl | sh`.
3. **Promotion** is done by the existing CI/CD pipeline (preprod → prod via
   GitHub Actions). The `internal/api/deployments.go` admin page shows
   per-environment status and lets ops trigger promotions.

## Bluey v0.1 distribution: piggyback on Pinky's API server

**Why piggyback rather than stand up a new service:**

- Bluey v0.1 has no SaaS surface (no auth, no sync, no captions storage,
  no billing). The only server-side concern is binary distribution.
- Pinky's server already runs on `pinky.sh`, has TLS, has deploy automation,
  has admin UI, has the static-file route pattern.
- Adding `downloads/bluey/` and a `/install/bluey.sh` route is ~50 lines of
  Go + a new staging directory, vs ~3 days of work to provision/maintain
  a separate `bluey-api` service.
- If Bluey ever needs SaaS features (managed Auto Router endpoint, cloud
  RAG, billing), we either extend Pinky's server multi-product OR fork
  to `bluey-server` then. Don't pre-emptively complicate.

**Layout on the Pinky API host (`161.35.177.238`):**

```
/opt/pinky-api/
├── pinky-server                       (existing binary)
├── pinky.db                           (existing)
├── downloads/                         (existing static root)
│   ├── pinky-windows.exe
│   ├── pinky-mac/...
│   └── bluey/                         (NEW)
│       ├── latest.json                (manifest: version + asset URLs + sha)
│       ├── latest -> v0.1.0           (symlink)
│       └── v0.1.0/
│           ├── bluey-0.1.0-darwin-arm64.tar.gz
│           ├── bluey-0.1.0-darwin-arm64.tar.gz.sha256
│           ├── bluey-0.1.0-darwin-universal.tar.gz
│           ├── bluey-0.1.0-darwin-universal.tar.gz.sha256
│           ├── SHA256SUMS.txt
│           └── RELEASE.md
```

**Server-side route additions (Pinky-side, ~50 lines of Go):**

```go
// internal/api/server.go — add inside Routes()
mux.HandleFunc("GET /install/bluey", s.handleBlueyInstallScript)
mux.HandleFunc("GET /install/bluey.sh", s.handleBlueyInstallScript)
mux.HandleFunc("GET /install/bluey.ps1", s.handleBlueyInstallPS1)
mux.HandleFunc("GET /downloads/bluey/latest.json", s.handleBlueyLatestJSON)
// /downloads/bluey/v<ver>/* served by the existing /downloads/ handler.
```

`handleBlueyInstallScript` reads `downloads/bluey/latest.json`, templates
the version + URL into the standard installer body, returns it.

`handleBlueyLatestJSON` is a thin pass-through with `Content-Type:
application/json` and short cache.

**Client-side discovery (Bluey-side):**

User runs:

```
curl https://pinky.sh/install/bluey.sh | sh
```

The templated script:

1. Fetches `https://pinky.sh/downloads/bluey/latest.json`.
2. Reads the `darwin-universal` (or `darwin-arm64`) asset URL + sha256.
3. Downloads the tarball.
4. Verifies sha256 against the manifest.
5. Extracts to `~/.local/bluey/<version>/`, symlinks
   `~/.local/bin/bluey` and `~/.local/bin/bluey-daemon`.
6. Prints next steps.

This is the same shape as the existing `scripts/install.sh` but with the
URL discovery driven by `latest.json` instead of hard-coded.

**Promotion flow (mirrors Pinky's):**

```
dev box (uno)            preprod                  prod
make package-darwin*  →  /downloads/bluey/   →  /downloads/bluey/
                         (preprod env)            (prod env)
    publish.sh              promote.sh
                            via admin UI
                            or workflow_dispatch
```

`scripts/publish.sh` (already drafted in /tmp/) targets the preprod env
first; promotion to prod is an explicit step (not auto).

## What changes on the Bluey side

1. `scripts/install.sh` — add a discovery path that reads `latest.json`
   from a base URL controlled by `BLUEY_RELEASE_BASE_URL`
   (default: `https://pinky.sh/downloads/bluey/`).
2. `scripts/publish.sh` — finish the staging + rsync logic targeting
   the Pinky API host.
3. `Makefile` — `publish-darwin` target chains
   `package-darwin-universal` → `publish.sh`.

## What changes on the Pinky side

> NOTE: Pinky source on uno is at `/tmp/pinky-full/`. Changes there must
> follow the existing Pinky preprod → prod pipeline. **I do not push
> Pinky changes from this Bluey repo without explicit permission.**

1. Add the four route handlers above (`internal/api/server.go` +
   new `internal/api/bluey_install.go`).
2. `internal/api/bluey_install.go` reads
   `downloads/bluey/latest.json` and templates the installer body.
3. Add admin-page entry showing the latest Bluey version per environment
   (mirrors `adminDeploymentEnv` for Pinky).
4. Deploy via the existing Pinky pipeline.

## Open questions

1. **Server-side ownership.** Who owns the Pinky-side route additions —
   Kiro (me, this repo), the Pinky codex agent, or the user? My
   recommendation: write the Bluey-side install script + publish script,
   stub the Pinky-side change as a PR description for whichever agent
   touches Pinky next. Do NOT cross repos without permission.
2. **Naming.** Is the customer-facing install URL `pinky.sh/install/bluey`
   or do we want a separate domain like `bluey.dev`? If separate, that's
   nginx + DNS work on top of this design.
3. **Channel split.** Pinky has preprod + prod environments. Bluey for
   v0.1 is dev → prod (no real preprod consumers). Worth preserving the
   `preprod/` staging directory anyway for safety, or drop it for v0.1?

## What I'm doing next (Bluey-side only)

Bluey-side scaffolding lands in this repo without touching Pinky:

- finalise `scripts/publish.sh` to stage + manifest-generate + rsync
  to a configurable host:path
- finalise `scripts/install.sh` with `latest.json` discovery and a
  fallback to `BLUEY_ARCHIVE` for offline install
- add `Makefile` targets `publish-preprod` and `publish-prod`
- add `docs/INSTALL-FROM-SERVER.md` user-facing guide

When the Pinky-side route additions are ready, the URL `pinky.sh/install/bluey`
just works. Until then, `BLUEY_RELEASE_BASE_URL` can point at any static
file host (e.g. an S3 bucket) for early testing.
