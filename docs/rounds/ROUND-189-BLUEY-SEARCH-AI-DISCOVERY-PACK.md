# Round 189 - Bluey Search AI Discovery Pack

## Trigger

Owner asked to reuse the successful Pinky search and AI discovery pattern for Bluey: `/llms.txt`, sitemap, JSON-LD, FAQ/schema pages, Bing/Google submission materials, UTM links, launch/community docs, and analytics planning.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## What Changed

- Repositioned the homepage metadata away from "stay unseen" and toward "AI meeting context copilot" and "private desktop AI copilot for engineering meetings."
- Added canonical, robots, sitemap, `llms.txt`, OpenGraph/Twitter metadata, and JSON-LD on the homepage.
- Added crawlable static pages for:
  - How Bluey works
  - Bluey FAQ
  - AI meeting context copilot
  - Engineering meeting copilot
  - AI copilot for design reviews
  - Screen context AI assistant
  - Meeting memory and project context
  - Auto model router
  - Private desktop AI overlay
  - Bluey vs AI meeting notetakers
  - Bluey vs coding agents
  - Context coverage
- Added `web/robots.txt`, `web/sitemap.xml`, and `web/llms.txt`.
- Added shared SEO-page CSS for cards, notes, comparison rows, and FAQ blocks.
- Updated the Caddy example so directory-index SEO pages resolve cleanly with `try_files {path} {path}/index.html {path}/ /index.html`.
- Added discovery-page live checks to the manual deploy script.
- Made the manual deploy script tolerate environments where bare `curl` is not on PATH by falling back to `/usr/bin/curl`.
- Added Bluey marketing/search docs:
  - Search submission pack
  - Growth playbook
  - Content bank
  - Launch calendar
  - Analytics events
  - Community outreach

## Safety And Positioning

- Public copy uses approved positioning: live context, engineering meetings, screen context, source-aware answers, saved project memory, user-controlled capture, private desktop overlay.
- Public marketing docs explicitly avoid claims around bypassing rules, proctoring, undetectability, or hidden capture.
- The static pages do not claim that managed web search is broadly available; FAQ copy says availability depends on deployment and account settings.
- Search/Bing/Google submission is documented as owner-only and not marked complete.

## Verification

- `node --check web/assets/bluey-site.js`
- `bash -n scripts/deploy-bluey-sh-manual.sh`
- Parsed `web/sitemap.xml` with `xml.etree.ElementTree`.
- Mapped all sitemap URLs to local static targets or known SPA routes.
- Parsed all `application/ld+json` blocks from `web/**/*.html`; 13 blocks parsed successfully.
- Local static-server smoke on port `4179`:
  - `/`
  - `/llms.txt`
  - `/robots.txt`
  - `/sitemap.xml`
  - `/how-bluey-works/`
  - `/bluey-faq/`
  - `/ai-meeting-context-copilot/`
  - `/engineering-meeting-copilot/`
  - `/screen-context-ai-assistant/`
  - `/context-coverage/`
- Confirmed no process remained listening on port `4179` after the smoke test.

Skipped:

- `caddy validate --config ops/Caddyfile.example`, because `caddy` is not installed in the local environment.

## Current State

- Bluey now has the product-owned assets needed for search crawlers, AI discovery, and owner-led submissions.
- The new static pages deploy through the existing `web/` rsync path.
- The live site has not been updated by this round unless `scripts/deploy-bluey-sh-manual.sh` is run separately.
- Google Search Console, Bing Webmaster Tools, Product Hunt, X, LinkedIn, Reddit, HN, and ongoing monitoring are still owner-account actions.

## Remaining Gates

- Deploy `web/` to `bluey.sh` and run manual live checks.
- Validate the deployed Caddy config on the server.
- Submit `https://bluey.sh/sitemap.xml` in Google Search Console and Bing Webmaster Tools.
- Request indexing for homepage, how-it-works, FAQ, AI meeting context, engineering meeting, and screen context pages.
- Choose public social profiles to include in `sameAs` schema, if any.
- Choose and wire production analytics, following `BLUEY-ANALYTICS-EVENTS-20260626.md`.
- Monitor search coverage and revise pages using actual query data.

## Files

- `web/index.html`
- `web/assets/bluey-seo.css`
- `web/llms.txt`
- `web/robots.txt`
- `web/sitemap.xml`
- `web/how-bluey-works/index.html`
- `web/bluey-faq/index.html`
- `web/ai-meeting-context-copilot/index.html`
- `web/engineering-meeting-copilot/index.html`
- `web/ai-copilot-for-design-reviews/index.html`
- `web/screen-context-ai-assistant/index.html`
- `web/meeting-memory-and-project-context/index.html`
- `web/auto-model-router/index.html`
- `web/private-desktop-ai-overlay/index.html`
- `web/bluey-vs-ai-meeting-notetakers/index.html`
- `web/bluey-vs-coding-agents/index.html`
- `web/context-coverage/index.html`
- `ops/Caddyfile.example`
- `scripts/deploy-bluey-sh-manual.sh`
- `docs/marketing/BLUEY-SEARCH-SUBMISSION-PACK-20260626.md`
- `docs/marketing/BLUEY-GROWTH-PLAYBOOK-20260626.md`
- `docs/marketing/BLUEY-CONTENT-BANK-20260626.md`
- `docs/marketing/BLUEY-LAUNCH-CALENDAR-20260626.md`
- `docs/marketing/BLUEY-ANALYTICS-EVENTS-20260626.md`
- `docs/marketing/BLUEY-COMMUNITY-OUTREACH-20260626.md`
- `docs/rounds/ROUND-189-BLUEY-SEARCH-AI-DISCOVERY-PACK.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`
