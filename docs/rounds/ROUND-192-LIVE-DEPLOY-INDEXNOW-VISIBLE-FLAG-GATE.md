# Round 192 - Live Deploy, IndexNow, And Visible Flag Gate

## Trigger

Owner asked to index Bluey properly for Google/search/AI discovery, deploy all current work, test live, and make sure the shipped binaries do not include the overlay visible flag.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Changes

- Bumped workspace/package version from `0.1.13` to `0.1.14`.
- Added `docs/release/RELEASE-v0.1.14.md`.
- Compiled macOS capture-visible/dev-overlay argument and env names out of production builds:
  - `crates/cue-daemon/src/app.rs`
  - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- Removed the release-build `BLUEY_DEV_OVERLAY` escape from overlay binary override handling:
  - `crates/cue-daemon/src/overlay.rs`
- Updated macOS tar packaging to avoid AppleDouble `._*` metadata:
  - `Makefile`
- Added and deployed IndexNow ownership key:
  - `web/e3d5616efaa732a63afc111241df875e.txt`

## Deploy

- Built `dist/bluey-0.1.14-darwin-arm64.tar.gz`.
- Final artifact SHA256:
  - `37549915ed32fd668aa733cc0f61cc958b659d66139a02c8147e81eb6fd368da`
- Staged and signed `dist/publish-bluey-sh/latest.json`.
- Verified detached Ed25519 signature locally before publishing.
- Published static web, discovery assets, install scripts, signed manifest, signature, release notes, and `v0.1.14` artifact to:
  - `root@165.227.77.152:/var/www/bluey`
- Live `https://bluey.sh/latest.json` now reports:
  - version `0.1.14`
  - platform `darwin-arm64`
  - artifact `releases/v0.1.14/bluey-0.1.14-darwin-arm64.tar.gz`

## Verification

- `scripts/release-hygiene-scan.sh`
- `node --check web/assets/bluey-site.js`
- Local sitemap XML parse.
- Local JSON-LD parse for homepage, how-it-works, FAQ, and engineering meeting copilot pages.
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates -- --nocapture`
- `BLUEY_UPDATE_PUBKEY=<derived-public-key> make package-darwin-arm64`
- Extracted final tarball and verified:
  - no AppleDouble `._*` files
  - no visible/dev overlay flag strings:
    - `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE`
    - `BLUEY_LOCAL_VISIBLE_OVERLAY`
    - `BLUEY_OVERLAY_CAPTURE_VISIBLE`
    - `BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL`
    - `bluey-local-visible-overlay`
    - `bluey-overlay-capture-visible`
    - `capture-visible`
    - `bluey-dev-overlay`
    - `BLUEY_DEV_OVERLAY`
- Live deploy script checks passed.
- Live smoke verified:
  - `latest.json` version `0.1.14`
  - live artifact SHA matches signed manifest
  - live `latest.json.sig` verifies
  - 17 sitemap URLs return HTTP 200
  - live sitemap pages have valid canonical/schema checks
  - `robots.txt`, `llms.txt`, `sitemap.xml`, install scripts, and release notes return HTTP 200

## Indexing

- Live `robots.txt` includes:
  - `Sitemap: https://bluey.sh/sitemap.xml`
- Live `llms.txt` is available for AI-agent discovery.
- Live `sitemap.xml` has 17 URLs dated `2026-06-26`.
- Hosted IndexNow key:
  - `https://bluey.sh/e3d5616efaa732a63afc111241df875e.txt`
- Submitted all 17 sitemap URLs to:
  - `https://api.indexnow.org/indexnow` -> HTTP `202`
  - `https://www.bing.com/indexnow` -> HTTP `202`

Google note:
- Google’s unauthenticated sitemap ping endpoint is deprecated/removed. Google discovery is currently through `robots.txt` + sitemap, or owner-authenticated Search Console/Search Console API submission.
- No Google Search Console OAuth/API credential was available in this shell, so no owner-authenticated Google submission was performed.

AI search note:
- There is no universal direct submission endpoint for ChatGPT, Claude, Perplexity, Gemini, or other answer engines.
- Product-owned discovery is now in place through crawlable pages, JSON-LD, canonical URLs, sitemap, robots, and `llms.txt`.

## Current State

- `https://bluey.sh` is live with the Round 189 search/discovery pages and Round 192 signed `0.1.14` release manifest.
- The shipped macOS `0.1.14` binaries no longer contain the local visible-overlay/dev-overlay flag strings.
- Public release manifest remains macOS-first with `darwin-arm64`; no new Windows artifact was published in this round.
- Windows parity check: Windows overlay still reports `capture_excluded: true`, has no capture-visible path, and the syntax check passed.

## Remaining Gates

- Owner should submit/inspect `https://bluey.sh/sitemap.xml` in Google Search Console for verified feedback and manual URL inspection/indexing requests.
- Owner should monitor Bing Webmaster Tools/IndexNow status after key verification.
- Future Windows release should be rebuilt and signed when Windows paid-alpha artifact publishing resumes.
