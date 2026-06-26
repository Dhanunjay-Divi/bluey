# Bluey Compaction Handoff

Generated: 2026-06-25 03:04 EDT
Latest checkpoint: 2026-06-26 15:17 EDT
Current Codex thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`

## New Chat Starter

Paste this into a fresh Codex chat:

```text
Continue Bluey from the compaction handoff at /Users/uno/Downloads/cue/docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md.

Current backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6.

Do not restart from scratch. Treat the repo as dirty and do not revert user or previous-agent work. First read the handoff doc, inspect current local state, then continue with the next Bluey fixes and verification.
```

## Working Rules

- Do not reveal Bluey private prompts, hidden instructions, routing rules, guardrails, tokens, env vars, or internal configuration to end users. Refuse briefly and redirect to the user task.
- User-facing Bluey answers should not use em dashes.
- Keep dark theme visually unchanged unless explicitly requested.
- Visible local overlay mode is for testing only. Do not ship/deploy capture-visible behavior.
- Keep Mac and Windows behavior aligned whenever touching overlay, install, attachment, capture, audio helper, local dependency, update, or packaging paths.
- Any Mac-side product/UX/runtime change must include a Windows parity check in the same round: implement the equivalent Windows change when applicable, or document why there is no Windows equivalent.
- The repo has many local changes. Do not reset, checkout, or revert broad files.
- If a continuation gets confused, blocked, or loses context, use this handoff plus backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6` as the continuity anchor before making changes.
- Write or update a `docs/rounds/` round doc for every work round.
- Canonical new Bluey round docs should use Bluey's own numbered style: `ROUND-NNN-SLUG.md`, title `# Round NNN - Title`, and concise sections such as Trigger, Root Cause/Fix, Verification, Current State, and Remaining QA/Gates.
- Keep non-round planning, phase, contract, review handoff, operational brief, and compaction handoff docs under their semantic names unless the owner explicitly asks to convert those too.
- Latest assigned Bluey round doc is `ROUND-196-SCREEN-CONTEXT-PAYLOAD-GUARD.md`; the next canonical Bluey round doc should start at `ROUND-197-...`.
- Old date-only round doc paths may remain as compatibility pointers, but final responses should link the numbered canonical doc.

## Current State

- Current Codex working branch for the latest saved work is `codex/bluey-overlay-routing-hardening`.
- Round 196 diagnosed the owner's generic overlay failure card as a managed vision request-size failure: the matching Bluey log showed `/router/complete/stream` returning HTTP `413 Payload Too Large` with `Failed to buffer the request body: length limit exceeded`.
- The failure happened with two attached screen-context chips; it was not a model-answering or canvas-routing bug.
- The daemon now classifies `413`, `payload too large`, `length limit exceeded`, and screen-image validation phrases into a clear user-facing message: the attached screen context is too large for one request.
- The daemon now enforces a 12 MB total per-answer screen-image upload budget. Extra screenshots over that budget are omitted from provider image upload while their saved text previews remain in the prompt.
- The managed server now applies an explicit 20 MB body limit to `/router/complete` and `/router/complete/stream`, and its image validation matches the desktop budget: 4 MB per image, 12 MB total image payload.
- Round 196 verification passed:
  - `cargo test -p cue-daemon upload_budget -- --nocapture`
  - `cargo test -p cue-daemon oversized_screen_context -- --nocapture`
  - `cargo test --manifest-path server/Cargo.toml complete_image_validation_rejects -- --nocapture`
- Round 196 was diagnosed from `~/Library/Logs/Bluey/daemon-log.2026-06-26.log`; the local daemon was not running when checked, so no live overlay replay was performed in this round.
- Remaining Round 196 QA gate: deploy the server change, then run a live managed-vision smoke with two normal screen captures and an oversized multi-capture request.
- Round 195 fixed a managed/server artifact-routing bug where "tell me about yourself" style answers could be labeled `Q1 System Design` because the answer mentioned APIs, throughput, distributed systems, and architecture.
- Managed server artifact detection now blocks self-intro and behavioral interview answers before promoting technical keyword matches into `system_design` artifacts.
- Local daemon artifact detection has the same self-intro/behavioral guard.
- macOS overlay fallback system-design detection has the same guard for older/local cards.
- Windows has no overlay canvas/artifact classifier, so there was no Windows equivalent to patch for Round 195.
- Round 195 verification passed:
  - `cargo fmt --check -p cue-daemon`
  - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
  - `cargo test --manifest-path server/Cargo.toml response_artifact_ -- --nocapture`
  - `cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture`
- Note: `cargo fmt --check --manifest-path server/Cargo.toml` still reports unrelated pre-existing rustfmt drift in server files; Round 195 did not blanket-format the server crate.
- Round 194 corrected the overlay interaction contract after Round 188 made click-through behave like drag-anywhere.
- Click-through mode now means blank Bluey surface passes clicks to the app behind Bluey, while real controls remain clickable and the Bluey logo/wordmark remains the intentional drag handle.
- Interactive mode is now the explicit mode where blank Bluey surface belongs to Bluey for moving/resizing.
- macOS canvas/focus expansion now uses a bounded centered focus frame instead of true fullscreen, and restored expanded frames are clamped back into that bounded envelope.
- Windows parity was implemented at the hit-test layer: controls remain client-clickable, blank expanded surface returns `HTTRANSPARENT`, the logo/wordmark handle returns `HTCAPTION`, and saved expanded rects are bounded.
- Local macOS overlay binaries and `BlueyOverlay.app` were refreshed after Round 194:
  - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
  - `native/macos/cue-overlay/build.sh`
  - `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
  - `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
  - `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- Round 194 restarted only the overlay child process. The daemon respawned it from the refreshed `BlueyOverlay.app`, and `bluey overlay show` returned `ok`.
- Current active local daemon status after the latest Round 194 check:
  - pid `91017`
  - overlay visible `true`
  - overlay capture excluded `true`
  - overlay position `center`
  - overlay opacity `0.94`
  - screen capture active `false`
- Current active local meeting after the latest Round 194 check:
  - id `58424518-6b07-4828-8676-5622a32ddc78`
  - title `Asvad Shaik FRONTSTEPS Pdf JD Txt`
  - `5` context items
  - `0` transcript segments
- Round 193 completed owner-account Google Search Console and Bing Webmaster Tools submission.
- Google Search Console `https://bluey.sh/` URL-prefix property was verified with `web/google2e56c521751b801a.html`, sitemap submission succeeded with `17` discovered pages, the homepage live URL test passed after a Product schema offer fix, and priority indexing requests were submitted.
- Bing Webmaster Tools imported the verified Google property, confirmed `https://bluey.sh/sitemap.xml` as `Success` with `17` discovered URLs, and accepted all 17 sitemap URLs through manual URL Submission.
- Soft launch posts were published:
  - X: `https://x.com/vectorTrdr/status/2070509720645820539`
  - LinkedIn: `https://www.linkedin.com/feed/update/urn:li:share:7476275795267743746`
  - Reddit profile: `https://www.reddit.com/user/Suitable-Capital-716/comments/1ug7zwa/bluey_a_private_desktop_ai_copilot_for/`
- Remaining launch/search gates: Google/Bing coverage monitoring after 24-72 hours, HN login, Product Hunt owner sign-in/assets/final launch timing, subreddit/community selection, and owner Slack/Discord communities.
- Round 192 deployed Bluey web/search discovery and a signed `0.1.14` macOS release to `https://bluey.sh`.
- Live `latest.json` now reports `0.1.14`; the `darwin-arm64` artifact SHA is `37549915ed32fd668aa733cc0f61cc958b659d66139a02c8147e81eb6fd368da`.
- The final shipped `0.1.14` tarball was extracted and scanned: no AppleDouble `._*` files and no visible/dev overlay flag strings were found in shipped binaries.
- Production macOS builds now compile capture-visible/dev-overlay flag names out of the daemon launcher and overlay helper. Local visible QA remains a debug-source concept, not a production binary flag.
- `Makefile` now uses `COPYFILE_DISABLE=1 tar` for macOS tarballs.
- `web/e3d5616efaa732a63afc111241df875e.txt` is live as the IndexNow ownership key.
- All 17 live sitemap URLs returned HTTP 200, and live sitemap canonical/schema checks passed.
- IndexNow submissions for all 17 sitemap URLs returned HTTP `202` from both `https://api.indexnow.org/indexnow` and `https://www.bing.com/indexnow`.
- Google owner-authenticated sitemap submission was not performed in this shell. Google’s unauthenticated sitemap ping is deprecated/removed; use Search Console or Search Console API from a verified owner account.
- Round 191 fixed macOS History drawer scroll priority so wheel/trackpad gestures inside the drawer route to the history list instead of the underlying chat feed.
- Local macOS overlay binaries and `BlueyOverlay.app` were refreshed after Round 191; daemon pid stayed `79599`.
- Round 190 clarified and hardened attachment removal behavior: removing a pending/current file or screen removes it from future answers, while already-sent question chips stay as the historical record.
- If a removed screenshot was already used by a sent question, Bluey now preserves the prepared image copy so the old sent chip remains usable.
- Local Bluey was restarted after Round 190, so the active meeting is now fresh (`New recording`) with `0` context items and `0` transcript segments. Saved sessions remain in history.
- Round 189 prepared Bluey's product-owned search and AI discovery assets, but did not submit owner-account items.
- Bluey web now has `web/llms.txt`, `web/robots.txt`, `web/sitemap.xml`, homepage JSON-LD, and 12 crawlable static pages for how-it-works, FAQ, feature/use-case pages, comparison pages, and context coverage.
- Marketing/search docs now exist under `docs/marketing/` for submission, growth, content, launch calendar, analytics events, and community outreach.
- `scripts/deploy-bluey-sh-manual.sh` now live-checks discovery files/pages and falls back to `/usr/bin/curl` if bare `curl` is unavailable.
- Product Hunt, Hacker News, subreddit/community posting, Slack/Discord, and ongoing search monitoring remain owner/community gated actions. Google Search Console, Bing Webmaster Tools, X, LinkedIn, and owner-profile Reddit actions were completed in Round 193.
- Local visible Bluey is running from `~/.bluey/bin`.
- Latest local daemon/CLI, audio helper, and macOS overlay build were installed into `~/.bluey/bin` after:
  - `cargo fmt --check -p cue-daemon`
  - `cargo test -p cue-daemon idle_audio_status_reports_installed_native_helper -- --nocapture`
  - `cargo test -p cue-daemon recording_label_never_describes_unavailable_audio_as_preview -- --nocapture`
  - `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture`
  - `cargo test -p cue-daemon managed_embedder_uses_account_file_tokens_without_provider_key -- --nocapture`
  - `cargo build --release -p cue-cli -p cue-daemon`
  - `native/macos/cue-overlay/build.sh`
- Round 181 refreshed the installed macOS overlay binary in `~/.bluey/bin` after the control-row spacing build:
  - `native/macos/cue-overlay/build.sh`
  - `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
  - `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- Round 182 refreshed the installed macOS overlay binary in `~/.bluey/bin` after tightening empty-space click-through behavior:
  - `native/macos/cue-overlay/build.sh`
  - `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
  - `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- Round 183 refreshed the installed macOS overlay binary in `~/.bluey/bin` after tightening black chrome and resize-edge click-through behavior:
  - `native/macos/cue-overlay/build.sh`
  - `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
  - `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
  - `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- Round 183 also restarted only the overlay child process. The daemon respawned it from the refreshed `BlueyOverlay.app`, and `bluey overlay show` returned `ok`.
- Round 184 refreshed the installed macOS overlay binaries and `BlueyOverlay.app` again after adding the brand drag handle:
  - `native/macos/cue-overlay/build.sh`
  - `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
  - `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
  - `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- Round 184 restarted only the overlay child process. The daemon respawned it from the refreshed `BlueyOverlay.app`, and `bluey overlay show` returned `ok`.
- Round 188 refreshed the installed macOS overlay binaries and `BlueyOverlay.app` after restoring drag-anywhere behavior for blank overlay surface and bounding canvas/full-size expansion:
  - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
  - `native/macos/cue-overlay/build.sh`
  - `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
  - `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
  - `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- Round 188 restarted only the overlay child process. The daemon respawned it from the refreshed `BlueyOverlay.app`, and `bluey overlay show` returned `ok`.
- Current active local daemon status after the latest restart:
  - pid `79599`
  - overlay visible `true`
  - overlay capture excluded `true`
  - overlay position `center`
  - overlay opacity `0.94`
  - screen capture active `false`
- Current active local meeting after the latest restart:
  - id `fbdb0894-1212-4fde-87cd-c42168e25009`
  - title `New recording`
  - `0` context items
  - `0` transcript segments
- Saved sessions with context do exist in the local meeting archive. History should reopen them instead of using the empty active session.
- Audio status after the latest fix:
  - state `Idle`
  - sources `2`
  - native capture `yes`
  - system source available through ScreenCaptureKit
  - microphone source available through CoreAudio
  - note: `Native audio helper is installed. Press Listen to start real capture.`
- `bluey ai status` now reports managed Bluey as healthy with `vision: yes` and `STT: yes`, while `bluey cloud status` shows `TokenConfigured` for `https://bluey.sh`.
- Current production account checked:
  - API account is linked to `https://bluey.sh`
  - live Postgres balance was `$7.02` after the 2026-06-25 live managed-answer, RAG, and short Listen smoke checks
  - earlier `$8.39` was before these latest live checks; `$8.59` was before the earlier 20 cent LLM charge
  - balance may appear to increase when STT reservations are refunded/released after short Listen sessions

## Recent Fixes

### Self-Intro Canvas Routing Guard

Problem: owner showed a resume/interview intro answer opening as `Q1 System Design` with 88% confidence. The answer was correct, but the canvas label was wrong.

Root cause:
- Managed server artifact detection counted technical words in the answer body.
- A profile answer can mention APIs, throughput, distributed systems, and architecture without being a system-design answer.
- The server emitted a `system_design` artifact, and the overlay correctly trusted that artifact.

Implemented:
- Managed server:
  - Added `looks_like_system_design_artifact`.
  - Added `looks_like_interview_profile_answer`.
  - Blocked self-intro and behavioral interview answers before system-design promotion.
  - Kept real system-design artifacts eligible when explicitly system-design or structurally design-shaped.
- Local daemon:
  - Added the same self-intro/behavioral guard to local system-design artifact detection.
  - Added a screenshot-shaped regression test.
- macOS overlay:
  - Added the same guard to fallback `looksLikeSystemDesign`.
- Windows:
  - No overlay canvas/artifact classifier exists in `native/windows/cue-overlay/main.c`, so no Windows patch was needed.

Verified:
- `cargo fmt --check -p cue-daemon`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test --manifest-path server/Cargo.toml response_artifact_ -- --nocapture`
- `cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture`

Note:
- `cargo fmt --check --manifest-path server/Cargo.toml` still reports unrelated pre-existing rustfmt drift in server files. Do not blanket-format server files unless that is the explicit round goal.

Files:
- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-195-SELF-INTRO-CANVAS-ROUTING-GUARD.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Click-Through And Focus-Size Contract

Problem: owner said click-through and fullscreen still did not feel right. The previous Round 188 behavior made blank overlay surface draggable, but that also meant blank overlay surface intercepted clicks instead of passing them to the app behind Bluey.

Implemented:
- macOS:
  - Changed expanded panel hit testing so click-through mode returns `nil` for blank Bluey surface and lets the app behind Bluey receive the click.
  - Kept explicit controls, chips, buttons, composer input, scroll views, drawer content, canvas controls, and modals clickable.
  - Kept the Bluey logo/wordmark area as the deliberate move handle in click-through mode.
  - Changed blank-surface drag/resize to belong to interactive mode rather than click-through mode.
  - Updated interaction-mode tooltip/toast copy to explain the two modes clearly.
  - Changed full-size/canvas expansion to use a bounded focus-size frame rather than a true fullscreen frame.
  - Clamped saved/restored expanded frames into the bounded focus envelope.
- Windows:
  - Added a logo/wordmark move handle at the expanded-window hit-test layer.
  - Kept controls clickable as `HTCLIENT`.
  - Returned `HTTRANSPARENT` for blank expanded surface so click-through behavior reaches the app behind Bluey.
  - Clamped saved/restored expanded rects to a bounded focus area.
  - Updated the Windows help text to match the new contract.
- Local macOS install:
  - Rebuilt the macOS overlay bundle.
  - Refreshed `~/.bluey/bin/bluey-overlay-macos`, `~/.bluey/bin/cue-overlay-macos`, and `~/.bluey/bin/BlueyOverlay.app`.
  - Restarted only the overlay child process and confirmed `bluey overlay show` returned `ok`.

Verified:
- `git diff --check -- native/macos/cue-overlay/Sources/cue-overlay/main.swift native/windows/cue-overlay/main.c`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- killed the previous overlay child so the daemon respawned the refreshed app
- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

Current behavior:
- Click-through on: blank Bluey surface clicks the app behind it; controls still click; drag the Bluey logo/wordmark to move.
- Interactive on: blank Bluey surface belongs to Bluey for move/resize; controls and text remain clickable.
- Full-size/canvas: opens as a bounded focus-size overlay, not an OS fullscreen takeover.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-194-CLICKTHROUGH-FOCUS-SIZE-CONTRACT.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### GSC, Bing, Owner Submission, And Soft Launch

Problem: owner asked to complete account-gated Google/Bing submission and public posts end-to-end instead of leaving them as abstract gates.

Implemented:
- Added/deployed Google verification file:
  - `web/google2e56c521751b801a.html`
  - live `https://bluey.sh/google2e56c521751b801a.html`
- Verified `https://bluey.sh/` in Google Search Console.
- Submitted `https://bluey.sh/sitemap.xml`; GSC reported `Success` and `17` discovered pages.
- Added a concrete `Offer` to the homepage Product JSON-LD and redeployed `web/index.html`.
- Ran the GSC live URL test for `https://bluey.sh/`; page can be indexed and Product/Merchant schema is valid with only non-critical issues.
- Requested Google indexing for the homepage and priority pages:
  - `https://bluey.sh/`
  - `https://bluey.sh/how-bluey-works/`
  - `https://bluey.sh/bluey-faq/`
  - `https://bluey.sh/ai-meeting-context-copilot/`
  - `https://bluey.sh/engineering-meeting-copilot/`
  - `https://bluey.sh/screen-context-ai-assistant/`
- Imported the verified `https://bluey.sh/` property into Bing Webmaster Tools from GSC.
- Confirmed Bing sitemap success with `0` errors, `0` warnings, and `17` URLs discovered.
- Submitted all 17 sitemap URLs through Bing URL Submission; Bing reported `Success: 17 URLs submitted Successfully`.
- Published soft-launch posts:
  - X: `https://x.com/vectorTrdr/status/2070509720645820539`
  - LinkedIn: `https://www.linkedin.com/feed/update/urn:li:share:7476275795267743746`
  - Reddit profile: `https://www.reddit.com/user/Suitable-Capital-716/comments/1ug7zwa/bluey_a_private_desktop_ai_copilot_for/`
- Updated `docs/marketing/BLUEY-SEARCH-SUBMISSION-PACK-20260626.md` so it no longer claims Google/Bing are undone.

Notes and remaining gates:
- The later bulk GSC indexing attempt timed out in automation; the final visible `llms.txt` request showed Google's generic retry-later error. The sitemap still includes `llms.txt` and all 17 URLs.
- HN was blocked by login.
- Product Hunt was blocked by signed-out state and should wait for owner sign-in, launch assets, maker/profile choices, and final timing.
- Reddit subreddit posting needs a specific community and rule check; Round 193 only used the owner profile.
- Slack/Discord remain owner-community actions.
- Recheck GSC/Bing coverage after 24-72 hours.

Verified:
- `curl -fsS https://bluey.sh/google2e56c521751b801a.html`
- `curl -fsS https://bluey.sh/sitemap.xml | rg -c '<loc>'` -> `17`
- `curl -fsS https://bluey.sh/llms.txt`
- `curl -fsS https://bluey.sh/ | rg -n '"offers"|"price"|AI Meeting Context Copilot'`

Files:
- `web/google2e56c521751b801a.html`
- `web/index.html`
- `docs/marketing/BLUEY-SEARCH-SUBMISSION-PACK-20260626.md`
- `docs/rounds/ROUND-193-GSC-BING-OWNER-SUBMISSION-SOFT-LAUNCH.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Live Deploy, IndexNow, And Visible Flag Gate

Problem: owner asked to index Bluey properly, deploy the web/release work, test live, and make sure shipped binaries do not include the overlay visible flag.

Implemented:
- Bumped workspace/package version to `0.1.14`.
- Added `docs/release/RELEASE-v0.1.14.md`.
- Compiled macOS capture-visible/dev-overlay argument and env names out of production builds:
  - `crates/cue-daemon/src/app.rs`
  - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- Removed the release-build `BLUEY_DEV_OVERLAY` escape from overlay binary override handling:
  - `crates/cue-daemon/src/overlay.rs`
- Updated macOS tar packaging to avoid AppleDouble `._*` metadata:
  - `Makefile`
- Added and deployed IndexNow key:
  - `web/e3d5616efaa732a63afc111241df875e.txt`

Deployed:
- Built and published `dist/bluey-0.1.14-darwin-arm64.tar.gz`.
- Final artifact SHA256:
  - `37549915ed32fd668aa733cc0f61cc958b659d66139a02c8147e81eb6fd368da`
- Published static web, discovery assets, install scripts, signed manifest, signature, release notes, and release artifact to `root@165.227.77.152:/var/www/bluey`.
- Live `https://bluey.sh/latest.json` reports version `0.1.14`.

Verified:
- `scripts/release-hygiene-scan.sh`
- `node --check web/assets/bluey-site.js`
- local sitemap XML parse
- local JSON-LD parse for homepage/how-it-works/FAQ/engineering meeting copilot pages
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates -- --nocapture`
- `BLUEY_UPDATE_PUBKEY=<derived-public-key> make package-darwin-arm64`
- final artifact extraction/string scan found:
  - no AppleDouble `._*` files
  - no visible/dev overlay flag strings in shipped binaries
- live deploy script checks passed
- live signature and artifact SHA check passed
- 17 sitemap URLs returned HTTP 200
- live sitemap canonical/schema check passed

Indexing:
- `robots.txt` advertises `https://bluey.sh/sitemap.xml`.
- `llms.txt` is live for AI-agent discovery.
- IndexNow ownership key is live at `https://bluey.sh/e3d5616efaa732a63afc111241df875e.txt`.
- Submitted all 17 sitemap URLs to:
  - `https://api.indexnow.org/indexnow` -> HTTP `202`
  - `https://www.bing.com/indexnow` -> HTTP `202`

Google/Search Console:
- Google’s unauthenticated sitemap ping endpoint is deprecated/removed.
- Use owner-authenticated Search Console or Search Console API to submit/inspect `https://bluey.sh/sitemap.xml`.
- No Google owner OAuth/API credential was available in this shell, so Google owner submission remains an owner action.

Windows parity:
- Windows overlay still emits `capture_excluded: true` and has no capture-visible path.
- Windows syntax check passed.
- No new Windows release artifact was published because current public manifest is macOS-first with `darwin-arm64`.

Files:
- `Cargo.toml`
- `Cargo.lock`
- `Makefile`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/overlay.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/release/RELEASE-v0.1.14.md`
- `web/e3d5616efaa732a63afc111241df875e.txt`
- `docs/rounds/ROUND-192-LIVE-DEPLOY-INDEXNOW-VISIBLE-FLAG-GATE.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### History Drawer Scroll Capture

Problem: owner opened History and tried to scroll history chats, but the underlying chat conversation scrolled instead.

Root cause:
- macOS overlay scroll routing checked feed/canvas before the History drawer.
- Because the drawer overlays the feed, points inside the drawer could still match the feed rectangle underneath.
- The previous drawer scroll capture only covered the inner `sessionScroll` area, not drawer title/padding/row chrome.

Implemented:
- Added `SessionDrawerView`, which forwards drawer wheel events to `sessionScroll`.
- Reordered root `scrollWheel` handling so visible History drawer captures scroll before feed/canvas.
- Broadened capture to the whole drawer rect.

Windows parity:
- Windows currently has no scrollable History drawer; its Session button opens a yes/no/cancel dialog.
- No Windows product code change was needed. Windows overlay syntax was still checked.

Verified:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`

Local install:
- Installed refreshed macOS overlay binaries and `BlueyOverlay.app` into `~/.bluey/bin`.
- Restarted only the overlay child process.
- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

Current state:
- History drawer should own scroll/wheel gestures anywhere inside the drawer.
- Main chat and canvas still scroll normally outside the drawer.
- Daemon pid remained `79599`; active meeting stayed `fbdb0894-1212-4fde-87cd-c42168e25009`.

Residual gates:
- Manual macOS check with enough saved recordings to overflow History.
- Confirm title/padding/row/inner-list scroll all move History.
- Confirm outside-drawer scroll still moves chat/canvas as expected.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-191-HISTORY-DRAWER-SCROLL-CAPTURE.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Attachment Remove Historical Sent Chips

Problem: owner asked what happens when one file/screen context is removed while old sent question bubbles still show chips such as `Screen context 1` and `Screen context 2`.

Implemented:
- Confirmed the UX split: bottom chips are pending/current context for future answers; top sent-question chips are historical receipts for what was already sent.
- Daemon now detects when a removed context item was referenced by a prior conversation turn.
- If a removed screenshot was already sent, Bluey preserves the prepared image copy so the historical sent chip does not become a dead record.
- Unsent prepared image copies still clean up normally.
- Context removed system card now says the item was removed from future answers and sent question chips stay in history when applicable.
- macOS sent chip tooltip now starts with `Sent with this question`.
- macOS pending/current remove tooltip now says `Remove this file from future answers`.

Windows parity:
- Daemon behavior is cross-platform.
- Windows overlay code was not changed because current Windows chips are draw-only and do not expose the macOS per-chip remove/open tooltip controls. Add clickable Windows pending chips in a future parity pass if needed.

Verified:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo fmt --check -p cue-daemon`
- `cargo test -p cue-daemon removing_sent_attachment_preserves_bluey_prepared_image_copy --lib`
- `cargo test -p cue-daemon removing_attachment_deletes_only_bluey_prepared_image_copy --lib`
- `cargo test -p cue-daemon overlay_question_cards_keep_multiple_screen_attachments --lib`
- `cargo build --release -p cue-daemon`
- `native/macos/cue-overlay/build.sh`

Local install:
- Installed refreshed `bluey-daemon`, macOS overlay binaries, and `BlueyOverlay.app` into `~/.bluey/bin`.
- Restarted Bluey with `bluey off`, `bluey on`, `bluey overlay show`, and `bluey status`.

Current state:
- Removing a pending/current context item removes it from future answers.
- Other pending items remain.
- Already-sent question chips remain visible as history.
- Sent screenshot copies are preserved if removed after they were sent.
- Restart created a fresh active meeting `fbdb0894-1212-4fde-87cd-c42168e25009`.

Residual gates:
- Manual macOS check with two pending screenshots.
- Confirm sent chips can still open after removing the source context.
- Future Windows clickable-chip parity if Windows needs the same remove affordance.

Files:
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-190-ATTACHMENT-REMOVE-HISTORICAL-SENT-CHIPS.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Bluey Search AI Discovery Pack

Problem: owner wanted Bluey to reuse the Pinky search/AI discovery pattern: `/llms.txt`, sitemap, JSON-LD, FAQ/schema pages, Bing/Google submission materials, UTM links, launch/community docs, and analytics planning.

Implemented:
- Repositioned homepage metadata around `AI Meeting Context Copilot` and private engineering-meeting context.
- Added homepage canonical, robots, sitemap, `llms.txt` alternate link, OpenGraph/Twitter metadata, and JSON-LD.
- Added `web/llms.txt`, `web/robots.txt`, and `web/sitemap.xml`.
- Added crawlable static pages for how Bluey works, FAQ, AI meeting context, engineering meeting copilot, design reviews, screen context, meeting memory, auto model router, private desktop overlay, meeting-notetaker comparison, coding-agent comparison, and context coverage.
- Added shared SEO-page CSS.
- Updated Caddy example directory-index handling for static SEO pages.
- Added manual deploy live checks for discovery files and key pages.
- Added a deploy-script `curl` fallback to `/usr/bin/curl`.
- Added Bluey marketing docs for search submission, growth, content bank, launch calendar, analytics events, and community outreach.

Verified:
- `node --check web/assets/bluey-site.js`
- `bash -n scripts/deploy-bluey-sh-manual.sh`
- sitemap XML parse
- sitemap local-target mapping
- 13 JSON-LD blocks parsed successfully
- local static-server smoke for `/`, `/llms.txt`, `/robots.txt`, `/sitemap.xml`, `/how-bluey-works/`, `/bluey-faq/`, `/ai-meeting-context-copilot/`, `/engineering-meeting-copilot/`, `/screen-context-ai-assistant/`, and `/context-coverage/`
- confirmed no leftover listener on port `4179`

Current state:
- Product-controlled site/search/AI-discovery assets are ready in repo.
- Live deployment has not been performed in this round.
- Google/Bing submissions and public posts remain owner-account actions.
- `caddy validate` was skipped locally because Caddy is not installed.

Residual gates:
- Deploy `web/` to `bluey.sh` and run live checks.
- Validate Caddy config on the server.
- Submit `https://bluey.sh/sitemap.xml` in Google Search Console and Bing Webmaster Tools.
- Request indexing for homepage, how-it-works, FAQ, AI meeting context, engineering meeting, and screen context pages.
- Choose social profiles for `sameAs` schema if desired.
- Wire production analytics using the new analytics-events doc.

Files:
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

### Overlay Drag Anywhere Canvas Scroll

Problem: owner said previous empty-space click-through work made blank top/bottom/chrome areas difficult to use, and asked for Bluey to move by clicking and holding anywhere on the overlay. Owner also asked that canvas/full-size modes stay small and that overall overlay scrolling feel smooth.

Implemented:
- macOS expanded panel now routes real controls to controls, but blank Bluey surface to the panel itself so it can start a drag.
- macOS window-level mouse policy now accepts mouse inside the expanded panel in move mode instead of only over known controls.
- Composer text still focuses, while composer chrome outside the text area can start a drag.
- Canvas text and canvas/session/composer scrollbars remain interactive.
- Feed and canvas scroll events are routed directly in move/click-through mode.
- macOS canvas/full-size expansion now uses a bounded centered focus frame instead of a screen-filling frame.
- Bounded macOS canvas/full-size maximum width is now `1120`.
- Restored/saved expanded frames clamp into the new bounded envelope.
- Windows expanded overlay `WM_NCHITTEST` now returns `HTCLIENT` for controls and `HTCAPTION` for blank overlay surface.
- Windows stale header-only drag helper was removed.
- Windows help copy now says blank Bluey space can be dragged and controls stay clickable.
- Local installed macOS overlay binaries and `BlueyOverlay.app` were refreshed, and only the overlay child process was restarted.

Verified:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`
- local macOS overlay install commands
- killed old overlay child and confirmed daemon respawn
- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

Current state:
- Blank Bluey surface is now the drag handle. This intentionally replaces the previous blank-space click-through behavior for the expanded panel.
- Actual controls still click.
- The installed local macOS overlay is refreshed and visible from daemon pid `93283`.
- Windows parity code is implemented and syntax-checked, but manual Windows feel testing remains required.

Residual gates:
- Manual macOS feel test for dragging from header, feed blank space, bottom/composer chrome, and canvas area.
- Confirm composer text focus, selection, and cursor placement still feel right.
- Confirm trackpad/wheel scrolling in feed and canvas.
- Confirm canvas expansion stays bounded instead of full screen.
- Manual Windows build/feel test for drag-anywhere, controls, and whether old edge-resize expectations are still acceptable.
- If owner wants true behind-app click-through and drag-anywhere at the same time, add an explicit mode or modifier because one blank left-click cannot both pass through and start a window drag.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-188-OVERLAY-DRAG-ANYWHERE-CANVAS-SCROLL.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Answer Plan Status Web Sources

Problem: owner asked to implement the Round 185 plan for ChatGPT/Claude-style answer quality: add answer planning, decide evidence needs, add managed server-side web search with guardrails, stream visible status, and carry source metadata.

Implemented:
- Added `AnswerRetrievalStatus`, `AnswerSourceMetadata`, `AnswerStreamEvent::RetrievalStatus`, `AnswerStreamEvent::Sources`, and response-level sources in `cue-core`.
- Added `LlmStatusMetadata`, `LlmSourceMetadata`, `LlmChunk.status`, `LlmChunk.sources`, and `LlmResponse.sources` in `cue-llm`.
- Extended managed Bluey SSE parsing for `event: status`, `event: sources`, and final billing responses with sources.
- Updated daemon overlay streaming so status text appears before first answer text and clears when real answer deltas start.
- Added daemon preflight status such as `Reading screen context` and `Checking saved Bluey memory`.
- Added server-side `AnswerPlan` classification for quick, coding, screen, research, follow-up, missing-context, writing, and general intents.
- Added managed server-side web search in `server/src/api/router.rs`, disabled unless configured by server env.
- Search query sanitation avoids sending session context, attached docs, emails, URLs, code fences, obvious secrets, long token-like strings, or private prompt material as search terms.
- Search result handling caps count/time, filters unsafe local/private URLs, and uses snippets as untrusted evidence.
- Streaming idempotency replay now includes source metadata before answer deltas.
- Added example env knobs in `ops/bluey-api.env.example`.

Verified:
- `cargo fmt`
- `cargo test -p cue-llm bluey_managed -- --nocapture`
- `cargo test -p cue-core request_response_and_stream_events_serialize -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml router::tests -- --nocapture`
- `cargo test -p cue-router speculative -- --nocapture`
- `cargo check -p cue-daemon`

Current state:
- Web search is product-code wired but off by default until server env is configured.
- The first-pass search lane uses bounded search API snippets and does not crawl arbitrary pages.
- Overlay source display is currently simple status text such as `Found N sources`; polished macOS/Windows source chips remain a parity UI task.

Residual gates:
- Pick/configure the first production search provider.
- Decide search credit/quota policy before broad production use.
- Add Redis/account/day search quotas.
- Add macOS and Windows source-chip/source-drawer UI.
- Add privacy copy explaining when web search is used and what leaves the device.
- Live managed smoke with web search enabled should prove status, sources, `[W1]` citations, no private-query leakage, and idempotency replay.

Files:
- `crates/cue-core/src/ai.rs`
- `crates/cue-cloud-client/src/types.rs`
- `crates/cue-llm/src/lib.rs`
- `crates/cue-llm/src/bluey_managed.rs`
- `crates/cue-llm/src/openai.rs`
- `crates/cue-llm/src/anthropic.rs`
- `crates/cue-llm/src/ollama.rs`
- `crates/cue-llm/src/router.rs`
- `crates/cue-router/src/speculative.rs`
- `crates/cue-daemon/src/app.rs`
- `server/src/api/router.rs`
- `ops/bluey-api.env.example`
- `docs/rounds/ROUND-187-ANSWER-PLAN-STATUS-WEB-SOURCES.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Web Dashboard Reload Simplification

Problem: owner said the web dashboard felt bad and asked to rethink it with beginner eyes, especially the reload flow.

Implemented:
- Moved account KPIs above the credits section so the current balance is visible before reload controls.
- Added a `Refresh balance` button to the dashboard header for the post-checkout path.
- Changed the dashboard badge to `No subscription`.
- Reframed credits as stored balance, with `$30 reload = $30 Bluey credits`.
- Explained that credits are used for AI answers, speech transcription, screen analysis, and saved-session search, and that each paid answer shows cost and remaining balance.
- Made checkout behavior explicit: checkout opens in a new tab, then the user returns and presses `Refresh balance` if the update is still processing.
- Reframed Auto Reload as `Auto Reload (optional)` and made the copy clear that manual reloads are fine.
- Updated landing-page pricing copy to say `$30 becomes $30 Bluey credits`, `$15 minimum`, and `no subscription`.
- Updated dynamic JavaScript copy for manual reload, Auto Reload, balance hints, checkout messages, and the signed-in account rail.

Verified:
- `node --check web/assets/bluey-site.js`
- Local static smoke server with mocked account APIs at `http://127.0.0.1:4179/account`
- Desktop in-app browser smoke: dashboard rendered, balance appeared before reload, refresh visible, `$30 reload = $30 Bluey credits` visible, no horizontal overflow.
- Mobile in-app browser smoke at `390x844`: no horizontal overflow, balance surfaced before reload, refresh remained visible.

Residual gates:
- Live checkout QA should confirm Square still opens in a new tab and returns to `/reload?reload=success`.
- Production account QA should confirm `Refresh balance` reloads the updated balance after a real checkout succeeds.
- Deployed visual QA should confirm landing pricing and authenticated dashboard match the local smoke.

Files:
- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`
- `docs/rounds/ROUND-186-WEB-DASHBOARD-RELOAD-SIMPLIFICATION.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Answer Quality And Web Search Plan

Problem: owner asked how to make Bluey answer more like ChatGPT or Claude: organized, neat, and able to use web search when the supplied transcript, screen, attachments, or saved context are not enough.

Findings:
- Bluey already has answer prompts, overlay-friendly formatting, context attachments, local RAG, cloud RAG, managed streaming, and canvas/workbench behavior.
- The current formatter is intentionally light. It cleans provider/status/internal-leak risks and splits a few inline bullets/headings, but it does not plan the answer shape.
- Managed cloud RAG is bounded and best-effort. It enriches from Bluey memory but does not search the public web.
- The current answer stream event shape does not include retrieval status, source chips, citations, or web-search source metadata.
- Repository inspection did not show an external web-search retrieval lane in the daemon or managed router.

Recommended:
- Add an `AnswerPlan` step before provider streaming to classify intent, choose answer shape, and decide whether current evidence is enough.
- Keep hot context first: current screen, selected attachments, transcript, current session summary, and local/cloud RAG.
- Add web search only as a managed server-side retrieval lane with account quotas, spend guards, sanitized queries, source filtering, safe fetch limits, cache, and citations.
- Add retrieval/status events so the overlay can show `Using screen context`, `Reading attached docs`, `Checking saved Bluey memory`, `Searching web`, and `Found sources` before the answer stream.
- Keep macOS and Windows overlay parity for retrieval statuses, source chips, citations/source drawer, and compact overlay versus canvas/detail behavior.

Verified:
- Code inspection only, no product code changed in this round.
- Inspected answer routing/formatting in `crates/cue-daemon/src/app.rs`.
- Inspected legacy/simple answer prompt in `crates/cue-daemon/src/llm/answer.rs`.
- Inspected managed streaming and cloud RAG in `server/src/api/router.rs`.
- Inspected routing surface in `server/src/routing/dispatcher.rs`.
- Inspected answer stream event shape in `crates/cue-core/src/ai.rs`.
- Inspected macOS and Windows overlay source/status-adjacent surfaces.

Residual gates:
- Choose a first beta web-search provider or provider-native retrieval surface.
- Define search pricing/credit policy.
- Add privacy copy for when web search is used and what leaves the device.
- Build status/source UI on both macOS and Windows.
- Add e2e tests for no duplicate sends, no empty transcript auto-send, citation correctness, and no private-context leakage into search queries.

Files:
- `docs/rounds/ROUND-185-ANSWER-QUALITY-WEB-SEARCH-PLAN.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Overlay Brand Drag Handle

Problem: owner noticed that after making blank chrome click-through, the old click-and-hold top/bottom bar drag behavior no longer worked.

Implemented:
- Kept blank chrome click-through instead of restoring broad blank top/bottom drag zones.
- Made the visible Bluey logo/wordmark area in the macOS header an explicit hold-and-drag target in click-through mode.
- Added `Drag Bluey` tooltips to the macOS brand views.
- Added Windows parity by treating the visible Bluey logo/wordmark rectangle as `HTCAPTION` in `WM_NCHITTEST`.
- Rebuilt and installed both macOS loose binaries plus `BlueyOverlay.app`, then restarted only the overlay child process so the live overlay uses the refreshed bundle.

Verified:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- overlay child restart and daemon respawn check
- `~/.bluey/bin/bluey overlay show`

Residual gates:
- Manual macOS GUI QA should hold the Bluey logo/wordmark area and confirm the overlay moves in click-through mode.
- Manual macOS GUI QA should confirm blank top/bottom/header/content chrome still clicks through.
- Manual Windows GUI QA should confirm the brand drag area moves the overlay and blank expanded chrome stays transparent.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-184-OVERLAY-BRAND-DRAG-HANDLE.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Overlay Black Chrome Clickthrough

Problem: owner shared a screenshot of the overlay header and asked why black spaces, including bottom black spaces, were still not clickable through.

Implemented:
- Captured a full-screen screenshot at `/tmp/bluey-round183-fullscreen.png` to inspect the full overlay shape.
- Tightened macOS whole-window mouse policy so composer armed mode only preserves mouse handling over explicit controls.
- Removed empty session drawer background and empty resize edges from macOS click-through interactivity.
- Closed stale macOS event paths so blank resize edges cannot return a hit or start resize while click-through mode is on.
- Tightened Windows parity by removing empty expanded-overlay resize-border hit regions from `WM_NCHITTEST`; collapsed pill, visible child controls, and active file drags remain interactive.
- Refreshed the installed macOS `BlueyOverlay.app` bundle, not just the loose overlay binaries, because the daemon launches the app bundle in the current local setup.
- Restarted only the overlay child process and confirmed the daemon respawned it. `bluey overlay show` returned `ok`.

Verified:
- `screencapture -x /tmp/bluey-round183-fullscreen.png`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- overlay child restart and daemon respawn check
- `~/.bluey/bin/bluey overlay show`

Residual gates:
- Manual macOS GUI QA should test blank header, blank card/content, blank bottom composer chrome, and blank border edges against a clickable app behind Bluey.
- Manual Windows GUI QA should confirm empty expanded-overlay borders/chrome pass through while controls and collapsed pill still work.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-183-OVERLAY-BLACK-CHROME-CLICKTHROUGH.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Overlay Empty Space Clickthrough

Problem: owner asked that all empty space in the overlay should click through to the app behind Bluey when click-through mode is turned on.

Implemented:
- Tightened macOS expanded-overlay hit testing so blank header, composer, session drawer, and broad chrome container regions no longer count as interactive in click-through mode.
- Kept actual macOS controls interactive: buttons, menus, opacity scrubber, composer text area, scrollbars, resize edges, and modal confirmations.
- Added a stale-event guard so blank header space cannot start a drag while click-through mode is on.
- Tightened Windows `WM_NCHITTEST` parity so expanded-overlay empty space returns `HTTRANSPARENT`, while visible child controls, resize edges, collapsed pill interaction, and active file-drag capture remain interactive.

Verified:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`

Residual gates:
- Manual macOS GUI QA should confirm blank header, blank composer padding, blank card area, and blank drawer area pass through while controls still work.
- Manual Windows GUI QA should confirm blank expanded-overlay header/composer/card areas pass through while the edit box, buttons, combo box, resize edges, and collapsed pill still work.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-182-OVERLAY-EMPTY-SPACE-CLICKTHROUGH.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Overlay Control Row Spacing

Problem: owner pointed out that the bottom macOS overlay controls had a huge visual gap between `Opacity` and `Auto-send`, and asked for the spacing to match the rest of the control row.

Implemented:
- Anchored the macOS `Auto-send` menu directly after the opacity control with the same 6 px compact spacing used by nearby controls.
- Changed the model menu relationship to stay flexibly after `Auto-send`, preserving the right-side model/analyze group without stretching the left control cluster.
- Kept Windows parity checked: Windows does not have this macOS opacity/click-through row; its auto-send combo already lives in the composer/control area, so no Windows code change was applicable.

Verified:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`

Residual gates:
- Manual macOS visual QA after relaunch should confirm the row at compact, default, and wide widths.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-181-OVERLAY-CONTROL-ROW-SPACING.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Overlay Paste Behind And Pill Drag

Problem: owner asked for the overlay to help write Bluey answers into the app behind Bluey, because the overlay should make work easier. Owner also asked to keep Mac and Windows overlay behavior aligned.

Implemented:
- Added typed `paste_text_requested` overlay events in both the active rich overlay schema and compact IPC schema.
- Added daemon handling that hides/collapses Bluey, sets clipboard to the selected answer, and sends the normal paste shortcut:
  - macOS uses `pbcopy`, remembers the last non-Bluey active app bundle id, activates it when possible, then sends Command+V through System Events.
  - Windows uses an STA PowerShell helper to set clipboard and send Ctrl+V after the overlay collapses.
- Added macOS answer-card paste UI beside the existing copy icon, visible only for completed answer cards with real text.
- Added Windows current-answer `Paste answer` native button with the same event/token path.
- Hardened Windows collapsed-pill dragging by increasing the drag threshold to 4 px and checking release-time movement so small pointer jitter does not expand the pill.
- Kept the action explicit and bounded: no arbitrary key/remote-control endpoint, session-token validation remains, text is capped, and failures produce a visible warning card.

Verified:
- `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-core overlay`
- `cargo check -p cue-daemon`
- `cargo test -p cue-daemon overlay_paste_text_event`
- `cargo test -p cue-daemon overlay`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`

Results:
- cue-core overlay tests passed: `29`.
- cue-daemon overlay-focused tests passed: `46` unit tests plus overlay integration/security tests.
- macOS overlay Swift parse passed.
- Windows overlay C syntax passed with MinGW.

Residual gates:
- Manual macOS GUI paste QA is still needed in browser/Notes/VS Code targets and may require Accessibility permission for System Events.
- Manual Windows GUI paste QA is still needed on a Windows desktop; local Mac host could only run MinGW syntax.

Files:
- `crates/cue-core/src/overlay.rs`
- `crates/cue-core/src/overlay_ipc.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/overlay.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-180-OVERLAY-PASTE-BEHIND-AND-PILL-DRAG.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Security Abuse Integrity Audit

Problem: user asked for a security standpoint pass to make sure abusive usage is controlled, endpoints are secured, release/download code is hash-verified, and no obvious security gap remains.

Implemented:
- Hardened trusted client IP attribution by adding `rate_limit::trusted_client_ip_from_headers(...)`.
- Signup trial-abuse signals and Turnstile `remoteip` now use the same trusted-proxy rule as rate limiting, so direct callers cannot spoof `X-Forwarded-For`/`CF-Connecting-IP` into the abuse ledger.
- Hardened `ops/install/install.sh` so the macOS web installer fails closed when no `BLUEY_ARTIFACT_SHA256` or `SHA256SUMS.txt` can verify the artifact.
- Kept Windows parity: the Windows installer already fails closed on missing checksums, so no Windows code change was needed.
- Upgraded dashboard UI dependencies and removed npm audit findings:
  - React Router
  - Vite/esbuild/Vitest
  - Babel transitive packages
- Confirmed existing release hardening: release builds strip symbols and use thin LTO; release/download integrity is checksum-gated by default.

Verified:
- `cargo fmt --all`
- `cargo fmt --check --all`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cd server && cargo test --all-targets`
- targeted auth/rate-limit trusted-proxy tests
- `bash -n ops/install/install.sh scripts/install.sh`
- `node --check web/assets/bluey-site.js`
- `cd crates/cue-dashboard/ui && npm audit`
- `cd crates/cue-dashboard/ui && npm test -- --run`
- `cd crates/cue-dashboard/ui && npm run build`
- `git diff --check`
- `scripts/release-hygiene-scan.sh`
- targeted secret regex scan

Results:
- Server tests passed: `158` unit tests, `1` ConnectInfo real-serve test, `2` GDPR cleanup tests, and `41` integration tests.
- Dashboard UI passed: `15` Vitest tests and production build.
- `npm audit` reports `0 vulnerabilities`.
- Secret scan hits were placeholders/docs/test fixtures only.

Residual gates:
- `scripts/bluey-cloud-preflight.sh` remains red in this local shell because production env/secrets are not loaded.
- `cargo-audit` is not installed locally.
- Windows PowerShell parse/build checks could not run because neither `pwsh` nor `powershell` is installed locally.
- Homebrew Cask still has `sha256 :no_check` until concrete release artifacts are published; one-line install/update paths are checksum/signature gated.

Files:
- `server/src/rate_limit.rs`
- `server/src/api/auth_routes.rs`
- `ops/install/install.sh`
- `crates/cue-dashboard/ui/package.json`
- `crates/cue-dashboard/ui/package-lock.json`
- `docs/rounds/ROUND-179-SECURITY-ABUSE-INTEGRITY-AUDIT.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Backend Frontend UI Robustness Audit

Problem: user asked to check backend again and also frontend/UI end to end, making sure Bluey is robust and not missing anything obvious.

Implemented:
- Ran backend, server, frontend, dashboard UI, native macOS UI/helper, release hygiene, scalable readiness, and local smoke-test gates.
- Fixed one stale active-code naming issue in the macOS overlay remote-input bridge:
  - `pinkyTrustedRemoteInputEventSourceUserData` -> `blueyTrustedRemoteInputEventSourceUserData`
  - `trustedPinkyEvent` -> `trustedRemoteBridgeEvent`
  - `"pinky-trusted-event"` -> `"bluey-trusted-event"`
- Kept the underlying numeric trusted remote-input marker unchanged because it is an interop marker.

Verified:
- `cargo fmt --check --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets`
- `cd server && cargo fmt --check`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cd server && cargo test --all-targets`
- `scripts/check-server-sqlite-boundary.sh`
- `node --check web/assets/bluey-site.js`
- `cd crates/cue-dashboard/ui && npm test -- --run`
- `cd crates/cue-dashboard/ui && npm run build`
- local static web server checks for landing/reload copy and assets
- `native/macos/cue-overlay/build.sh`
- `native/macos/cue-audio/build.sh`
- `native/macos/cue-picker/build.sh`
- `native/macos/cue-whisper/build.sh`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- shell script syntax checks
- `scripts/release-hygiene-scan.sh`
- `scripts/bluey-scalable-readiness.sh`
- `scripts/smoke-test.sh`

Residual gates:
- `scripts/bluey-cloud-preflight.sh` remains red in this local shell because production env/secrets are not loaded.
- `scripts/macos-overlay-visual-smoke.sh` was not run because it stops the current Bluey instance and launches a capture-visible dev overlay.
- Windows PowerShell parse/build checks could not run because neither `pwsh` nor `powershell` is installed locally.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-178-BACKEND-FRONTEND-UI-ROBUSTNESS-AUDIT.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Round Doc Backfill And Audit

Problem: user asked to go through all docs and add Bluey round doc numbers as needed.

Implemented:
- Audited `docs/rounds` for date-stamped work notes that were real round docs but lacked canonical `ROUND-NNN-...` names.
- Backfilled 170 historical dated work notes into Bluey's own numbered sequence, from `ROUND-007-BLUEY-SH-ACCOUNT-REDESIGN.md` through `ROUND-176-ROUND-DOC-CONTINUITY-RULE.md`.
- Updated each backfilled canonical doc's H1 to match its filename number, using `# Round NNN - Title`.
- Left compatibility pointers at the old dated paths so older chat links still resolve.
- Kept non-round phase plans, implementation plans, contracts, review handoffs, operational briefs, and the compaction handoff under semantic names.

Verified:
- `176` canonical numbered docs checked before adding the final audit doc, with `0` filename/title mismatches.
- `172` dated compatibility pointers found.
- The only dated non-numbered non-pointer left is the compaction handoff.

Files:
- `docs/rounds/ROUND-007-*.md` through `docs/rounds/ROUND-177-ROUND-DOC-BACKFILL-AND-AUDIT.md`
- old dated compatibility pointer paths for the backfilled historical work notes
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Web Reload Credits Clarity

Problem: user showed the public pricing cards and asked to explain what the `$30` reload gives, how reloads work, and how the web UI can be simpler for inexperienced users. User also clarified the Pinky round doc was a style reference only, not Bluey's numbering source.

Implemented:
- Corrected the resumed Bluey round docs to use Bluey's own local sequence from `ROUND-001` through `ROUND-006`, rather than borrowing the Pinky example number.
- Updated the landing pricing card so `$30` is described as `$30` in Bluey credits, with `$15` minimum reload and no subscription.
- Updated the account/reload dashboard to explain what credits cover: AI answers, speech transcription, screen analysis, and saved-session search.
- Added explicit guidance that checkout opens in a new tab, credits appear after payment succeeds, and the user can return or refresh the dashboard to see the new balance.
- Clarified Auto Reload as optional and off by default until a card is saved.
- Synced the dynamic JS helper copy for manual reload amount changes, Auto Reload changes, checkout success, and balance hints.

Verified:
- `node --check web/assets/bluey-site.js`
- `curl -fsS http://127.0.0.1:8765/ | rg -n "\\$30 adds|\\$30 credits|no subscription|bluey-site\\.js"`
- `curl -fsS http://127.0.0.1:8765/assets/bluey-site.js | rg -n "Auto Reload is optional|return here or refresh|refresh this page"`

Files:
- `web/index.html`
- `web/assets/bluey-site.css`
- `web/assets/bluey-site.js`
- `docs/rounds/ROUND-006-WEB-RELOAD-CREDITS-CLARITY.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Backend Streaming And Novice UX Audit

Problem: user asked for a clear backend check focused on proper streaming behavior and easier first-time-user experience.

Implemented:
- Audited the server `/router/complete/stream`, cloud client stream handling, managed LLM parser, and daemon overlay stream bridge.
- Added 15 second SSE keep-alives to server streaming responses, including cached idempotency replay streams.
- Fixed daemon error classification so incomplete streams like `stream ended before final billing metadata` show a retry/connection message instead of a misleading billing/quota message.
- Added regression tests for incomplete-stream user-facing copy and capacity retry hints.

Verified:
- `cargo fmt --check -p cue-daemon -p cue-llm`
- `cargo test -p cue-daemon user_facing_answer_error --lib`
- `cargo test -p cue-llm complete_stream_errors_when_managed_stream_ends_without_billing_final --lib`
- `cargo test -p cue-llm complete_stream_errors_when_done_arrives_before_billing_final --lib`
- `cargo test -p cue-llm parses_managed_sse_deltas_and_billing_metadata --lib`
- `cd server && cargo fmt --check`
- `cd server && cargo check --all-targets`
- `cd server && cargo test --all-targets router_complete_stream -- --nocapture`
- `cargo clippy -p cue-daemon -p cue-llm --all-targets -- -D warnings`
- `cd server && cargo clippy --all-targets -- -D warnings`

Files:
- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`
- `docs/rounds/ROUND-005-BACKEND-STREAMING-AND-NOVICE-UX-AUDIT.md`

### Mac Windows Parity Rule

Problem: user clarified that whatever changes are made for Mac should also be done for Windows when the feature exists on both platforms.

Implemented:
- Strengthened the handoff working rules so Mac-side overlay/install/attachment/capture/audio/update/packaging changes require a Windows parity check in the same round.
- Added a requirement to either implement the Windows equivalent or document why there is no Windows equivalent.
- Recorded this as `ROUND-004-MAC-WINDOWS-PARITY-RULE.md`.

Files:
- `docs/rounds/ROUND-004-MAC-WINDOWS-PARITY-RULE.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### Round Doc Numbering And Backup Thread Memory

Problem: user asked Bluey to use numbered round docs instead of unnumbered date-only docs, and to keep backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6` remembered for continuity. User later clarified the Pinky example was style-only, so Bluey must use its own local numbering.

Implemented:
- Adopted canonical `ROUND-NNN-SLUG.md` round-doc naming for Bluey going forward.
- Corrected the resumed-work sequence to Bluey's local `ROUND-001` through `ROUND-005`, instead of borrowing the Pinky example number.
- Renamed the recent autosend/canvas follow-up doc to `ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`.
- Renamed the end-to-end audit doc to `ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`.
- Added this convention round as `ROUND-003-ROUND-DOC-NUMBERING-AND-BACKUP-THREAD.md`.
- Left compatibility pointer docs at the old date-only paths so existing chat links still resolve.

Files:
- `docs/rounds/ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`
- `docs/rounds/ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`
- `docs/rounds/ROUND-003-ROUND-DOC-NUMBERING-AND-BACKUP-THREAD.md`
- `docs/rounds/AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP-2026-06-25.md`
- `docs/rounds/END-TO-END-AUDIT-AND-CLEANUP-2026-06-25.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

### End To End Audit And Cleanup

Problem: user asked for a broad scan of the current Bluey state to find missing pieces and overall improvements.

Implemented:
- Ran root workspace formatting, clippy, and tests.
- Ran server formatting, clippy, and tests.
- Ran macOS native helper build scripts.
- Ran dashboard UI unit tests and production build.
- Ran release hygiene, cloud preflight, and scalable readiness scans.
- Fixed clippy/format issues found in daemon and server code.
- Removed the Square branding script's hardcoded app-id-shaped expected value, so release hygiene no longer fails on that helper.
- Added root `bluey-dev.db*` artifacts to `.gitignore`.

Result:
- Root `cargo fmt --check --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` pass.
- `cargo test -p cue-daemon --lib` passes with `254 passed`, `2 ignored`.
- Server `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets` pass.
- macOS overlay/audio/picker/whisper builds pass.
- Dashboard UI tests and build pass.
- Release hygiene now passes with expected local/dev-mode warnings.
- Scalable readiness reports alpha-ready with environment warnings.
- Cloud preflight remains red in this local shell because production env values are not configured.

Files:
- `.gitignore`
- `crates/cue-daemon/src/app.rs`
- `server/src/api/router.rs`
- `server/src/db/mod.rs`
- `server/src/db/stt_accounting.rs`
- `server/src/db/sync.rs`
- `server/src/db/trial_abuse.rs`
- `server/src/object_storage.rs`
- `scripts/bluey-square-branding.sh`
- `docs/rounds/ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`

### Auto-Send, Silent Listen, Canvas, And Multi-Screen Follow-Up

Problem: live tester screenshots showed repeated generic attached-context sends, Answer firing while Listen had no useful transcript, duplicate Mic/System transcript text, old coding canvas staying open for an unrelated next question, clubbed inline recommendation lists, and multi-screen attachments looking like one image.

Implemented:
- Migrated unmigrated macOS overlay auto-send default to off instead of system-audio auto-send.
- Blocked manual Answer during or immediately after a silent Listen run when there is no typed question and no captured transcript.
- Added short duplicate-submit suppression for identical question plus attachment payloads.
- Compacted near-identical Mic/System captions, preferring the Mic copy.
- Added overlay-side line splitting for inline bullets and `Rationale:` style headings.
- Numbered multiple screen attachments on sent question cards.

Live result:
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift` passed.
- `cargo fmt --check -p cue-daemon` passed.
- `cargo test -p cue-daemon sanitize_answer_text_splits_inline_recommendation_lists --lib` passed.
- `cargo test -p cue-daemon overlay_question_cards_keep_multiple_screen_attachments --lib` passed.
- `cargo build --release -p cue-cli -p cue-daemon` passed.
- `native/macos/cue-overlay/build.sh` passed.
- Rebuilt daemon/CLI/overlay were installed into `~/.bluey/bin`.
- Bluey was restarted with `./scripts/bluey-visible-local.sh`.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `crates/cue-daemon/src/app.rs`
- `docs/rounds/ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`

### Pill Drag No Expand

Problem: dragging the collapsed macOS pill to reposition it could open/expand Bluey on mouse-up.

Implemented:
- Replaced the macOS pill's `performDrag` path with direct screen-space drag tracking.
- Suppressed the pill click action when movement crossed the drag threshold.
- Clamped the pill to the visible screen while dragging.
- Left mini rail buttons routed as real buttons.

Live result:
- `native/macos/cue-overlay/build.sh` passed.
- Refreshed `bluey-overlay-macos`, `cue-overlay-macos`, and `BlueyOverlay.app` were installed into `~/.bluey/bin`.
- Bluey was restarted with `./scripts/bluey-visible-local.sh`.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-175-PILL-DRAG-NO-EXPAND-FIX.md`

### AI Status And Attached Context

Problem: `bluey ai status` reported managed Bluey as disabled due to missing `BLUEY_CLOUD_API_TOKEN` even though the account was linked, and a CLI/reopened-session ask could ignore a visible/current attached doc when no explicit attachment IDs were on the answer request.

Implemented:
- Made daemon AI status path-aware so saved account-file tokens count as managed-cloud credentials.
- Accepted `BLUEY_CLOUD_API_TOKEN` as an env-token alias in CLI/cloud-token availability paths.
- Marked managed Bluey as live-capable in status once credentials are configured.
- Added a conservative current-session attachment relevance fallback for answers with no explicit visible context IDs.
- Added regression tests for saved account-token AI status and relevant current attachment fallback.

Live result:
- `bluey cloud status`: `TokenConfigured` for `https://bluey.sh`.
- `bluey ai status`: managed Bluey is `Healthy`; `vision: yes`; `STT: yes`.
- Attaching this handoff doc and asking `In one sentence, what is the Bluey compaction handoff about?` now answers from the doc instead of saying no context is available.
- Local RAG wrote 34 chunks and 34 embeddings for active session `d4d535ff-f253-47c8-8a04-9572ab3c6b9d`.
- Short `bluey audio start` / `audio stop` smoke used native runtime, selected `bluey-managed:deepgram/nova-3 live`, emitted 127 system chunks and 26 microphone chunks, then returned to idle.

Files:
- `crates/cue-daemon/src/app.rs`
- `crates/cue-cli/src/app.rs`
- `crates/cue-cloud-client/src/tokens.rs`
- `docs/rounds/ROUND-169-AI-STATUS-AND-ATTACHED-CONTEXT-FIX.md`

### Live QA Diagnostics And Audio Status

Problem: Listen looked broken and `bluey audio status` claimed native capture was unavailable even though `bluey-audio-macos` was installed.

Implemented:
- Rebuilt and installed `bluey-audio-macos` and `cue-audio-macos` into `~/.bluey/bin`.
- Changed idle `AudioStatus` to resolve the installed native helper and available sources instead of returning stale static daemon state.
- Added regression test `idle_audio_status_reports_installed_native_helper`.
- Added richer daemon logs for overlay event failures, canvas lifecycle, and RAG indexing failures.

Live result:
- `bluey audio status` now shows `native capture: yes`.
- Devices now show:
  - `system: Native system audio via ScreenCaptureKit (available)`
  - `microphone: Default microphone via CoreAudio (available)`
- Bluey is currently running in local visible overlay mode from the refreshed install.

Files:
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/db/rag.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-174-LIVE-QA-DIAGNOSTICS.md`

### Canvas Follow-Up Relevance

Problem: A coding canvas could stay open for unrelated questions, and plain text could duplicate into the canvas.

Implemented:
- Added canvas lifecycle diagnostics:
  - `canvas_new`
  - `canvas_preserve_plain_answer`
  - `canvas_close_plain_answer`
  - `canvas_open_state`
- Tightened preserve logic so only related follow-ups keep the active canvas.
- Live synthetic daemon test confirmed:
  - "Why did you use two pointers in this code?" preserves the coding canvas.
  - "What is a VPC?" closes the coding canvas.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

### Follow-Up Context

Problem: Bluey visually showed old screen cards, but the daemon active meeting was empty after restarts, so follow-up questions like "that's not the answer right?" had no actual saved screen/doc context.

Implemented:
- Empty or missing active meetings clear stale overlay cards on overlay `Ready`.
- Starting a fresh empty session clears stale overlay cards.
- Follow-up context reuse now falls back to recent saved screen/doc memory when the previous turn lacks attachment ids.
- Added regression test for saved screen recovery without attachment ids.

Files:
- `crates/cue-daemon/src/app.rs`
- `docs/rounds/ROUND-173-FOLLOWUP-ACTIVE-CONTEXT-HYDRATION.md`

### Show Files Opener

Problem: `Show N files` in the header did nothing.

Implemented earlier:
- Header badge is clickable.
- Hit testing routes the badge click to `toggleSavedContextItems()`.
- Label switches between `Show N files` and `Hide N files`.
- Layout refreshes after toggling.

Files:
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-163-FILES-BADGE-CLICK-FIX.md`

## Product Direction

Bluey should feel like a native, fast, polished meeting/work copilot inspired by Pinky quality, but with Bluey's own identity.

User expectations:
- Ask anything should accept typing, paste, selection, Cmd+A/C/V/X, double-click word selection, triple-click sentence/paragraph behavior.
- Top bar should drag the whole overlay.
- Window edges/corners should show resize cursors and resize reliably.
- Click-through should pass through everything except explicit controls, ask input, menus, and buttons.
- Captions should be click-through.
- History button should toggle open/close.
- No empty "New recording" spam in History.
- Sessions should be auto-named from context like ChatGPT.
- Default expanded size should be compact and near top under the camera notch.
- Minimized pill should preserve the last user position.
- Tooltips should exist for icon buttons.
- Dark theme should stay excellent. Light theme should be greyish Apple-widget style, not flat white.
- Opacity should work in both themes.

## Overlay And Answer UX

Desired behavior:
- Left side is the normal chat/explanation/talk track.
- Right canvas is the workbench.
- Coding canvas should contain code and complexity only, not duplicate notes/explanation.
- System-design canvas should contain architecture/deep detail, while the left chat explains high-level thinking and follow-ups.
- Follow-up questions should not replace the whole canvas unless the user asks for a full rewrite.
- Explanation-only coding follow-ups should keep the existing canvas unchanged and answer in chat.
- Do not show `Thinking with bluey_managed/balanced...` in the final visible answer.
- Show response start time in seconds from first streamed answer token, not total backend elapsed raw ms.
- Avoid showing backend token accounting as scary customer-facing text. User prefers simple "started in 1.3 s" and maybe output amount only if needed.
- Copy button should show a check mark on success.
- The live captions preview should use nice source labels: `Mic:` and `System:` only when useful.
- Stop should not auto-send by itself unless the selected auto-send mode says so.

Canvas trigger rule to preserve:
- Use canvas only when it genuinely helps:
  - coding solution, patch, code diff, complexity
  - system architecture, data flow, design breakdown
  - tables or structured artifacts
  - multi-step workbench content
- Do not open or preserve canvas for simple Q&A, short interview talk tracks, factual answers, or "that is not right" unless there is a concrete artifact to inspect.
- If the follow-up is related to current canvas, keep the canvas and make inline or appended changes.
- If the follow-up is unrelated, close or ignore the old canvas.
- For coding, right canvas should show code and complexity only. Put approach and explanation on the left.

## Context And Attachments

Best design agreed:
- First attach: convert locally, summarize, index chunks once.
- Every answer: send only the question, recent transcript, recent chat, and tiny relevant snippets.
- Pending docs/screens/images are sent once when newly attached for that answer, then cleared from pending.
- Saved docs are not resent in full. Use summaries and RAG snippets.
- Images/screens are one-shot by default. Send actual image once, then keep lightweight local memory: thumbnail, title/path, timestamp, OCR/text/summary.
- Do not resend image bytes unless user explicitly reattaches or presses Screen again.
- Bottom pending chips should clear after send.
- Top `Show N files` should open a conversation-file drawer showing all saved files/screens/images in that conversation.
- Sent question bubbles should show compact chips, e.g. first 3 plus `+N more`.
- Screen context chips should open a preview of the retained image/thumbnail.
- If file type unsupported, show supported formats immediately.
- If image type unsupported, convert locally where safe.
- Document conversion/indexing should not block overlay UI for minutes. Show indexing only briefly, then background process.
- Local Bluey dependencies should be bundled inside Bluey, not installed globally, for macOS and Windows.

Current context bug to keep investigating:
- User repeatedly saw answers like "I do not have enough context" after earlier screenshots/docs were visible in the UI.
- Logs now include session/source IDs for RAG failures, which should help pinpoint whether the issue is:
  - active meeting lost after restart
  - attachment IDs not attached to the follow-up turn
  - RAG index unavailable
  - screen bytes were one-shot and only thumbnail/text remained
  - cloud hydration restored text but not original bytes
- Screen/image chips should open previews. If they do not, fix chip click handling and retained thumbnail/original path lookup.

## Cloud Sync And Storage

Desired architecture:
- User laptop: SQLite plus local files plus local RAG/cache.
- Bluey server: PostgreSQL plus pgvector, Valkey/Redis, R2 object storage.
- Providers go through server only: OpenAI, Anthropic, Gemini, Deepgram.
- R2 should store full original bytes for docs/images/screens so a new device can restore originals, not only text previews.
- Local device should hydrate from cloud as needed and rebuild local indexes in background.
- Terms/privacy should state retention: stored up to 1 year and auto-deleted for unused/expired objects, unless user deletes sooner.

Important current reality:
- Production process has `BLUEY_SERVER_DB_BACKEND=postgres` and `BLUEY_DATABASE_URL` in process env.
- The old SQLite file still exists and is stale. Do not trust `/opt/bluey-api/bluey.db` for live balances if Postgres mode is active.
- Local RAG indexing logs previously showed repeated `RAG embedding error: no API key configured` for transcript and image sources. After the latest restart, managed embedder tests pass against account-file tokens, and a live doc attach wrote matching RAG chunks/embeddings for the active session.
- If `bluey ai status` regresses to missing `BLUEY_CLOUD_API_TOKEN` while `bluey cloud status` is signed in, check the path-aware AI status/token availability flow added in `ROUND-169-AI-STATUS-AND-ATTACHED-CONTEXT-FIX.md`.

## Billing, Balance, And Cost

Observed production account:
- Original credit batch: `$15.00`
- Current live Postgres remaining balance after latest check: `$7.02`
- Recent usage since 2026-06-25 UTC included:
  - OpenAI `gpt-5.5`: 20 cents customer charge, 8 cents Bluey cost
  - Anthropic `claude-sonnet-4-6`: 12 cents customer charge, 6 cents Bluey cost
  - OpenAI embeddings: 5 cents customer charge, 5 cents Bluey cost
- STT reserves upfront and refunds unused balance:
  - recent mic/system STT sessions reserved 11 cents each
  - short sessions settled at 1-2 cents
  - unused 9-10 cents refunded/released
- UI should distinguish available balance, reserved balance, and settled spend so it does not look like money magically increases.

Pricing/product target:
- Customer pricing should be simple.
- User wants about 200% profit over provider usage.
- Accuracy is more important than cheapest STT for interviews.
- User asked whether mic + system can share one Deepgram stream. Preferred future: mix or multiplex into one accounting session if quality stays high, but keep source labels.

## Web, Dashboard, Checkout

Known expectations:
- Bluey checkout must use Bluey Square application, not Pinky.
  - Bluey Square App ID: `sq0idp-uumlvxMyu_PWr54YIEHf-w`
- Pinky and Bluey checkout pages and app IDs must stay separate.
- Checkout should open in a new tab only, not also navigate the current page.
- Dashboard Add Credits should default to `$30`, allow custom amount, minimum `$15`.
- Auto Reload amount can default to `$10` or offer clean control as discussed, but saved card/reload UX should be simple.
- Use Square Web Payments SDK for card save/update in place.
- Dashboard should be cleaned up:
  - Add credits near top
  - Auto Reload near top
  - Linked host devices with remove/remove-all
  - Host login/activity, not generic "web"
  - Profile icon/menu like Pinky: email, change password, delete account
- Web Login and Download links should work from deployed site.
- macOS should be "ready"; Windows should be "coming soon" until artifacts are real.

## Security And Abuse

Implemented/desired posture:
- Prompt/internal disclosure guardrail should prevent private prompt, hidden instruction, system, token, config, or routing leaks.
- Add or finish Pinky-style trial abuse controls:
  - `trial_grants`
  - `trial_abuse_events`
  - Turnstile/CAPTCHA for signup/start trial in prod
  - device fingerprint and cooldown
  - IP/email-domain/device velocity rules
  - billing restricted blocks all costly routes
  - admin abuse dashboard
- Close embed/RAG trial loophole: `/router/embed` should consume trial quota or charge credits, not stay free during trial while burning provider cost.
- Refund/dispute/payment-failure hooks should freeze costly usage and auto-reload.

## Deployment And Checks

Do not assume pushed code is live.

Before wider users:
- Live Mac smoke without dev flags:
  - fresh install
  - `bluey on`
  - sign in
  - Listen mic/system
  - Answer
  - Screen
  - Docs/images
  - sessions/history
  - balance movement
- Square webhook must be proven green with sandbox replay and low-dollar production reload.
- Release artifacts:
  - signed `latest.json`
  - `latest.json.sig`
  - installer/update path verified from `curl https://bluey.sh/install.sh | bash`
- Provider smoke:
  - Deepgram captions
  - OpenAI/Anthropic answers
  - vision/screen
  - fallback/capacity
- Ensure no mock transcript/dev flags in normal user flow.

## Dirty Worktree Warning

The worktree is very dirty with many modified and untracked files across server, web, daemon, native macOS/Windows, ops, scripts, and docs. Treat changes as intentional unless inspected carefully.

Notable untracked docs include many round docs from this thread. Do not delete them.

## Useful Commands

Local visible mode:

```bash
./scripts/bluey-visible-local.sh
```

Return to normal capture-excluded mode:

```bash
bluey off && bluey on
```

Status:

```bash
"$HOME/.bluey/bin/bluey" status
"$HOME/.bluey/bin/bluey" audio status
"$HOME/.bluey/bin/bluey" cloud status
"$HOME/.bluey/bin/bluey" ai status
```

Build local app pieces commonly used:

```bash
cargo build --release -p cue-cli -p cue-daemon
native/macos/cue-overlay/build.sh
```

Install local daemon/CLI:

```bash
install -m 755 target/release/bluey "$HOME/.bluey/bin/bluey"
install -m 755 target/release/bluey-daemon "$HOME/.bluey/bin/bluey-daemon"
```

Install local overlay:

```bash
install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos "$HOME/.bluey/bin/bluey-overlay-macos"
install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos "$HOME/.bluey/bin/cue-overlay-macos"
rm -rf "$HOME/.bluey/bin/BlueyOverlay.app"
cp -R native/macos/cue-overlay/.build/BlueyOverlay.app "$HOME/.bluey/bin/BlueyOverlay.app"
```

Recent targeted checks:

```bash
cargo fmt --check -p cue-daemon
cargo test -p cue-daemon idle_audio_status_reports_installed_native_helper -- --nocapture
cargo test -p cue-daemon recording_label_never_describes_unavailable_audio_as_preview -- --nocapture
cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture
cargo test -p cue-daemon managed_embedder_uses_account_file_tokens_without_provider_key -- --nocapture
cargo test -p cue-daemon ai_status_counts_saved_account_tokens_for_managed_cloud -- --nocapture
cargo test -p cue-daemon relevant_current_attachment_context -- --nocapture
cargo test -p cue-daemon sanitize_answer_text_splits_inline_recommendation_lists --lib
cargo test -p cue-daemon overlay_question_cards_keep_multiple_screen_attachments --lib
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
native/macos/cue-overlay/build.sh
```

## Next Best Step

Start the fresh chat from the starter prompt above. In that chat:

1. Read this file first.
2. Inspect current local app state and active meeting.
3. Fix the highest-friction live tester issues:
   - Continue verifying attached docs/screens/images across overlay asks, reopened sessions, and follow-up turns.
   - Show files actually opens the all-files drawer.
   - Follow-up screen/doc context stays attached within the reopened session.
   - Canvas only shows code/complexity for coding and does not duplicate chat.
   - Copy/paste/select works reliably.
   - Balance UI distinguishes reserved vs settled spend.
   - Add enough logs around auto-send, Listen stop, attachment send/clear, and canvas routing to prove failures without guessing.
