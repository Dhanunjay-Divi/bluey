# Bluey Compaction Handoff

Generated: 2026-06-25 03:04 EDT
Latest checkpoint: 2026-07-02 04:18 EDT
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
- Latest completed Bluey round doc is `ROUND-301-FRESH-REVIEW-BILLING-GUARD-CLOSURE.md`; the next canonical Bluey round doc should start at `ROUND-302-...`.
- Old date-only round doc paths may remain as compatibility pointers, but final responses should link the numbered canonical doc.

## Current State

- Current Codex working branch for the latest saved work is `codex/bluey-overlay-spacing-20260626`.
- Round 301 is complete for the fresh review of Codex-owned billing guard work:
  - reviewed Round 300 against the Pinky billing lesson goal with fresh eyes
  - found one remaining edge: an internal/test account with legacy Auto Reload enabled and a saved payment method could still reach the background Auto Reload worker
  - moved the internal/admin/test billing-account policy into shared `server/src/billing/policy.rs`
  - checkout, saved-card setup, account settings, account payload, and the Auto Reload worker now use the shared policy
  - background Auto Reload now skips internal/admin/test accounts before reserving an in-flight top-up
  - `/account/me` now reports Auto Reload effectively off for internal/admin/test accounts so UI state matches the safety goal
  - added regression coverage for the legacy internal-account Auto Reload case
  - verification passed:
    - `cargo test --manifest-path server/Cargo.toml internal_and_test_accounts_cannot_enter_paid_billing_flows -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml skip_internal_test_account_even_if_auto_topup_was_already_enabled -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml billing -- --nocapture`
    - `cargo check --manifest-path server/Cargo.toml --quiet`
    - `git diff --check`
  - Round doc:
    `docs/rounds/ROUND-301-FRESH-REVIEW-BILLING-GUARD-CLOSURE.md`
  - Deploy note: server code is tested locally but not marked deployed unless the next operator runs the production server deploy path.
- Round 300 is complete for Pinky-style billing lesson guardrails:
  - audited Bluey's existing credit/reload billing posture against the Pinky billing lesson pack
  - confirmed Bluey already credits reloads only through verified Stripe/Square processor events and revokes/restricts on refund/dispute risk events
  - added an internal/admin/test account guard for paid checkout/card-save/Auto Reload setup
  - admin accounts, `internal-*`/`test-*`/`admin-test-*` `@bluey.sh` accounts, `+test` emails, and `@test.local` emails cannot enter real paid billing setup paths
  - `/account/me` now reports Auto Reload unavailable for those accounts so the UI does not invite a real payment method setup
  - added a regression test for the active internal account pattern:
    `internal-admin-20260606023943@bluey.sh`
  - verification passed:
    - `cargo test --manifest-path server/Cargo.toml internal_and_test_accounts_cannot_enter_paid_billing_flows -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml billing -- --nocapture`
    - `cargo check --manifest-path server/Cargo.toml --quiet`
    - `git diff --check`
  - Round doc:
    `docs/rounds/ROUND-300-BILLING-LESSON-GUARDS.md`
  - Deploy note: server code is tested locally but not marked deployed unless the next operator runs the production server deploy path.
- Round 299 is complete and deployed for live caption rail tail-follow and Enter send:
  - macOS caption rail now forces layout before scrolling to the newest caption tail
  - macOS repeats the tail-follow pass shortly after layout so partial caption updates stay visually live
  - macOS empty-composer Enter now sends usable non-placeholder caption text even when it does not pass the stricter question heuristic
  - short spoken captions still show directly as the visible Question card
  - long captions stay compact through the existing live-caption intent while transcript context is supplied by the daemon
  - Windows received the same simpler usable-transcript gate for empty-composer Enter sends
  - desktop workspace version bumped to `0.1.52`
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-core --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo test -p cue-core overlay --lib`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.52`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `cc6f5d6588b06867153e98757f1200f5640c9300af93b6f41b155c0fa4a55d0f`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
    - unpacked release reported `bluey 0.1.52` and `bluey-daemon 0.1.52`
  - live artifact:
    `https://bluey.sh/releases/v0.1.52/bluey-0.1.52-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-299-LIVE-CAPTION-RAIL-ENTER-SEND.md`
- Round 298 is complete and deployed for local letter shortcut removal:
  - removed local unmodified alphabet shortcuts from the Mac overlay
  - removed local unmodified alphabet shortcuts from the Windows overlay
  - kept local `Enter`, `Esc`, `Tab`, and `Shift+Tab`
  - kept global modified shortcuts:
    - macOS: `Ctrl+Option+...`
    - Windows: `Ctrl+Alt+...`
  - updated Mac and Windows shortcut help so it no longer lists `T/L/S/I/H/F`
  - help now states that letters type normally when Ask is focused
  - desktop workspace version bumped to `0.1.51`
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-core --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo test -p cue-core overlay --lib`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.51`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `f54d50e3939ab58dacdc4e36da60bb65846ed06558aea8042d44f2038ff788c2`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
    - unpacked release reported `bluey 0.1.51` and `bluey-daemon 0.1.51`
  - live artifact:
    `https://bluey.sh/releases/v0.1.51/bluey-0.1.51-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-298-LOCAL-LETTER-SHORTCUT-REMOVAL.md`
- Round 297 is complete for release discipline and ops runbooks:
  - copied the Pinky deployment discipline into Bluey-owned docs and scripts
  - updated `docs/RELEASE-RUNBOOK.md` with:
    - deployment execution policy
    - release id format `<version>-<commit12>`
    - preprod readiness gate
    - production promote gate
    - post-production verification gate
    - billing/credit/reload reconciliation gate
    - provider/search cooldown and 429 proof gate
  - added `scripts/bluey-release-live-verify.sh`
    - verifies live `latest.json.sig`
    - checks installer MIME types
    - checks artifact SHA
    - unpacks macOS release and checks binary versions
    - fails on configured capture-visible dev markers in the shipped daemon
  - updated `scripts/deploy-bluey-sh-manual.sh` to call the live verifier when a release public key or signing key is available
  - added `docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md`
  - added `docs/ops/DISPUTE-EVIDENCE-RUNBOOK.md`
  - updated `docs/OPERATIONS-RUNBOOK.md` to reflect current production beta state instead of stale not-provisioned language
  - updated `docs/PRODUCTION-DEPLOY-RUNBOOK.md` so manual API binary replacement is emergency-only and gated by storage, backup, billing, and reconciliation checks
  - no desktop/server binaries changed and no production deploy was required
  - verification passed:
    - `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.50`
    - `bash -n scripts/bluey-release-live-verify.sh scripts/deploy-bluey-sh-manual.sh scripts/publish-bluey-release.sh scripts/release-hygiene-scan.sh`
    - `scripts/release-hygiene-scan.sh docs/RELEASE-RUNBOOK.md docs/OPERATIONS-RUNBOOK.md docs/PRODUCTION-DEPLOY-RUNBOOK.md docs/ops scripts/bluey-release-live-verify.sh scripts/deploy-bluey-sh-manual.sh`
  - remaining gates:
    - add GitHub Actions production promotion workflow that promotes the already verified artifact instead of rebuilding
    - add preprod artifact metadata and exact artifact promotion enforcement
    - add server release bundle metadata for API deploys
  - Round doc:
    `docs/rounds/ROUND-297-RELEASE-DISCIPLINE-RUNBOOKS.md`
- Round 296 is complete and deployed for Keyboard-before-Theme focus order and final pass:
  - inspected `IMG_3704.MOV` by extracting frames
  - confirmed the focus ring reached Theme before the Keyboard Shortcuts icon despite the icon being visually left of Theme
  - macOS header keyboard traversal now sorts header controls by rendered row/x position
  - Windows header layout now places Keyboard Shortcuts before Theme
  - Windows keyboard focus and hit order now match the visual order
  - desktop workspace version bumped to `0.1.50`
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-core --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo test -p cue-core overlay --lib`
    - `cargo test -p cue-daemon deepgram --lib`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.50`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `58d949c727ac73381a84a363549c5d1562ce658519bcee32ff961ed2b6ef447e`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
    - unpacked release reported `bluey 0.1.50` and `bluey-daemon 0.1.50`
    - release artifact scan passed with no configured secrets/dev capture flags present
  - live artifact:
    `https://bluey.sh/releases/v0.1.50/bluey-0.1.50-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-296-KEYBOARD-THEME-TAB-ORDER-FINAL-PASS.md`
- Round 295 is complete and deployed for macOS click-through move-handle reliability:
  - enlarged the blue click-through move handle from `34px` to `42px`
  - enlarged the move-handle hit padding from `18px` to `26px`
  - changed click-through hit-testing so the handle returns the actual `HeaderMoveButton`
  - unified local button drag and global click-through monitor drag through the same panel drag methods
  - kept a guarded global frame fallback if macOS keeps the drag sequence outside Bluey
  - added `drag_started` / `drag_ended` lifecycle logs
  - Windows parity note:
    - no Windows source change needed; Windows already uses `HTCAPTION` for the click-through move handle
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-core --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.49`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `247a300a6a91362bf99752638f41117841173ed9400c216f96179bdb7650a542`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
    - unpacked release reported `bluey 0.1.49` and `bluey-daemon 0.1.49`
    - release artifact scan passed with no configured secrets/dev capture flags present
  - live artifact:
    `https://bluey.sh/releases/v0.1.49/bluey-0.1.49-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-295-CLICKTHROUGH-MOVE-HANDLE-RELIABILITY.md`
- Round 294 is complete and deployed for session ids and STT diagnostics:
  - active meeting/session is created when Listen starts preparing, before any transcript arrives
  - overlay receives `set_active_session` with a stable short support code
  - Mac overlay shows `ID XXXXXXXX` in the header/history and restores it after answer streaming finishes
  - Windows overlay accepts the same active-session command and shows the id in the header
  - web dashboard Saved Sessions shows `ID XXXXXXXX`, copy-id controls, and synced diagnostics in details
  - cloud sync uploads `session_code` plus sanitized diagnostics counters/last error metadata
  - Deepgram relay parser ignores control frames like `SpeechStarted` / `UtteranceEnd` instead of killing live captions on `channel: 0`
  - desktop workspace version bumped to `0.1.48`
  - local verification passed so far:
    - `cargo fmt --all`
    - `cargo test -p cue-core overlay --lib`
    - `cargo test -p cue-daemon deepgram --lib`
    - `cargo check -p cue-core --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `cargo check --manifest-path server/Cargo.toml --quiet`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `node --check web/assets/bluey-site.js`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.48`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `68eb1993e58d0485b9fa07c0c8d43c3842967a92870b73d75dc43ed86a3b4403`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
    - unpacked release reported `bluey 0.1.48` and `bluey-daemon 0.1.48`
    - release artifact scan passed with no configured secrets/dev flags present
  - live artifact:
    `https://bluey.sh/releases/v0.1.48/bluey-0.1.48-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-294-SESSION-ID-STT-DIAGNOSTICS.md`
- Round 293 is complete for click-through move-handle dragging:
  - fixed the macOS edge where the move handle could be swallowed by the manual button fallback instead of starting a drag
  - added explicit mouse-down/drag/mouse-up window movement for the handle
  - clamps movement to the visible display and persists the final frame
  - kept the global click-through drag fallback as a backup
  - desktop workspace version bumped to `0.1.47`
  - Windows parity note:
    - Windows already returns `HTCAPTION` for the click-through move handle
    - Windows source was syntax-checked again
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.47`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `069752aea6c8c4eeb79052cfb2e2e366208ba8de11c583e39e67adbd108123d4`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.47/bluey-0.1.47-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-293-CLICKTHROUGH-MOVE-HANDLE-DRAG.md`
- Round 292 is complete for stale Ask focus and reserved shortcut keys:
  - fixed the remaining macOS edge where an already-focused empty Ask box could still type reserved local shortcut keys
  - when Ask is empty and stale-focused, `L`, `S`, `I`, `H`, `F`, `T`, and `Enter` route to Bluey instead of being typed
  - added a short typing grace window after intentionally focusing Ask so normal prompt typing still works
  - desktop workspace version bumped to `0.1.46`
  - Windows parity note:
    - Windows does not have the same stale `NSTextView` routing
    - Windows source was syntax-checked again
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.46`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `6bd87be6218df20292907c203856101cecc937eefc10dda234dc5b58ec6d426f`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.46/bluey-0.1.46-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-292-STALE-ASK-FOCUS-SHORTCUT-KEYS.md`
- Round 291 is complete for local shortcut autofocus:
  - fixed the macOS key router fallback that focused Ask and inserted any printable key after shortcut handling
  - Ask now receives typed characters only when it is already focused
  - users can intentionally focus Ask by clicking it, pressing local `T`, or pressing global `Ctrl+Option+T`
  - local shortcuts such as `L`, `S`, `I`, `H`, `F`, and `Enter` remain available when click-through is off and Ask is not focused
  - desktop workspace version bumped to `0.1.45`
  - Windows parity note:
    - Windows already avoids this arbitrary printable-key autofocus behavior
    - Windows source was syntax-checked again
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.45`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `9f3457203262d9aaa03afbb4b86569a6af32a3608424d76337a038d98cc18034`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.45/bluey-0.1.45-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-291-LOCAL-SHORTCUT-AUTOFOCUS-FIX.md`
- Round 290 is complete for Tone editor Enter/save and readability:
  - macOS Tone field now saves on `Enter`
  - handled both `insertNewline:` and `insertNewlineIgnoringFieldEditor:`
  - Tone title copy is now `How should Bluey answer?`
  - Tone title is bright/visible instead of dim grey
  - Tone input text is left-aligned so it starts from the left
  - desktop workspace version bumped to `0.1.44`
  - Windows parity note:
    - Windows does not have the same native Tone input modal in `native/windows/cue-overlay/main.c`
    - Windows source was syntax-checked again
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.44`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `09fc42a591cd7279b11fc276c0c0058be6a100a528eac4f1b47e8b52e2217fc2`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.44/bluey-0.1.44-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-290-TONE-EDITOR-ENTER-READABILITY.md`
- Round 289 is complete for Tab order and composer Tab behavior:
  - fixed macOS header focus order so keyboard shortcuts is selected before theme, matching the visible header
  - added a composer-level `Tab` fallback so the Ask text box does not insert tab whitespace
  - anchored Tab navigation from Ask to the Ask input surface so Tab moves to the next control after Ask instead of restarting at History
  - `Shift+Tab` from Ask moves to the previous control before Ask
  - desktop workspace version bumped to `0.1.43`
  - Windows parity note:
    - this was macOS-specific `NSTextView` and Mac focus-order behavior
    - Windows source was syntax-checked again
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.43`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `4a69b176f028c1abbf3b140bcbf4f9b0067cfd277e08aac5d311df8515b81b16`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.43/bluey-0.1.43-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-289-TAB-ORDER-COMPOSER-TAB-FIX.md`
- Round 288 is complete for full Tab control coverage:
  - macOS keyboard focus is now control-based instead of button-only
  - `Tab` / `Shift+Tab` can reach header controls, drawer controls, transcript clear, Ask input, attach, Tone, Opacity, Auto-send, Auto/model, Listen, Answer, and Screen
  - `Enter` / `Space` activates the selected control
  - Ask input focuses the composer and then normal text editing takes priority
  - menus open when selected
  - opacity can be adjusted with arrow keys while selected
  - modal/drawer control discovery ignores static labels so fake Tab stops are avoided
  - shortcut guide copy now says controls instead of buttons
  - Windows parity:
    - Tab order includes Ask edit field and Auto-send combo box
    - Enter/Space opens Auto-send when selected
    - Windows shortcut guide copy says controls
  - desktop workspace version bumped to `0.1.42`
  - local verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.42`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `44918f0d5adbcf8b92ad8541a96dcc1e7d3e374bfc22d7e650b385786f63f82f`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.42/bluey-0.1.42-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-288-TAB-COMPLETE-CONTROL-COVERAGE.md`
- Round 287 is complete for Tab focus ring visibility:
  - fixed the reason Tab focus was hard to see on macOS
  - root cause was the new focus ring sitting at z-position `3000` while header/composer chrome is reasserted around `4000+`
  - raised the keyboard focus ring to z-position `4900`
  - reasserted that z-position during layout
  - increased ring border width, shadow, and added a subtle blue fill
  - desktop workspace version bumped to `0.1.41`
  - Windows parity note:
    - no Windows code change needed because Windows draws focus directly inside each owner-drawn button from Round 286
  - local verification passed:
    - `cargo check -p cue-daemon --quiet`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.41`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `23ca4c32d7781407616be9f9301b1ae38ef30f252ee45c0c73e96d6b417c7370`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.41/bluey-0.1.41-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-287-TAB-FOCUS-RING-VISIBILITY.md`
- Round 286 is complete for shortcut parity and button keyboard navigation:
  - shortcut guide now shows a stable command list in click-through on and off
  - the guide now separates mode guidance from available commands instead of hiding commands by mode
  - added global History/Files shortcuts:
    - macOS `Ctrl+Option+H` / `Ctrl+Option+F`
    - Windows `Ctrl+Alt+H` / `Ctrl+Alt+F`
  - macOS overlay now has a Bluey-managed keyboard focus ring:
    - `Tab` / `Shift+Tab` cycles visible enabled buttons when click-through is off
    - `Enter` / `Space` activates the selected button
    - modal help/confirm panels keep local keys inside the modal
    - shortcut help body is non-editable and non-selectable
  - Windows overlay parity:
    - owner-drawn buttons draw a focus outline
    - `Tab` / `Shift+Tab` cycles visible enabled buttons in interactive mode
    - `Enter` / `Space` activates the focused button
    - History/Files are registered/unregistered as global hotkeys
  - desktop workspace version bumped to `0.1.40`
  - local verification passed:
    - `cargo check -p cue-daemon --quiet`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.40`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `e1551312aaba717e8bac446995297ddc26a55ad11a135e5218bb145783fc97d8`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.40/bluey-0.1.40-darwin-arm64.tar.gz`
  - Windows source parity is implemented and syntax-checked, but the live public release manifest still advertises `darwin-arm64` only until a Windows build/package host publishes the Windows artifact.
  - Round doc:
    `docs/rounds/ROUND-286-SHORTCUT-PARITY-KEYBOARD-NAV.md`
- Round 285 is complete for code canvas line notes:
  - managed coding instructions now ask for separate `Line notes:` outside fenced code for non-trivial code answers
  - server code artifact formatting splits `Line notes:` into a dedicated `LINE NOTES` section
  - macOS code canvas renders `LINE NOTES` in muted grey
  - macOS code canvas tints existing inline code comments grey
  - macOS code canvas copy button now copies only the `CODE` section for code artifacts
  - non-code canvases still copy full canvas content
  - desktop workspace version bumped to `0.1.39`
  - Windows parity note:
    - Windows compact overlay does not yet have the Mac artifact canvas renderer
    - shared server line-note separation still improves Windows answer text
    - grey visual-only canvas annotations remain Mac-only until Windows gains artifact canvas support
  - local verification passed:
    - `cargo fmt --all`
    - `cargo test --manifest-path server/Cargo.toml response_artifact --quiet`
    - `cargo check --manifest-path server/Cargo.toml --quiet`
    - `swift build -c debug --package-path native/macos/cue-overlay`
  - production deploy completed:
    - built on droplet from source-only tree `/opt/bluey-build-codex-round285-line-notes/server`
    - installed `/usr/local/bin/bluey-server`
    - previous production binary backup:
      `/var/backups/bluey-api/bin/bluey-server.previous-20260701T213819Z`
    - installed binary SHA256:
      `f201d6f077033eb334e1f090fe09cabd1afa1d6e7b7e5ecc155fe32aa2a4472d`
    - `bluey-api.service` restarted active
    - `https://bluey.sh/health` returned `status=ok`
  - macOS release verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.39`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `d17f49268f1581815be41ccb5711a60354b6b50edaa3797256e7d10b9a635b92`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.39/bluey-0.1.39-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-285-CODE-CANVAS-LINE-NOTES.md`
- Round 284 is complete for screen-code answer planning and first-token fallback:
  - production logs for the owner screenshot showed the generic screen prompt was planned as `missing_context`
  - root cause was the generic `documents` wording plus planner logic that did not use appended screen/session context strongly enough for coding signals
  - production logs also showed Gemini vision 429/503 and slow fallback could delay first answer
  - server planner now extracts planning context, detects code-shaped screen/session context, and plans generic screen-code requests as `coding` / `code_artifact`
  - generic screen-capture prompts no longer request docs unless real document context is present
  - image or planning context now counts as attached evidence and prevents false missing-context plans
  - answer-plan logs now include privacy-safe context character count, context hash, and `context_coding_signal`
  - streaming routes now have a connect deadline before falling through to the next provider
  - vision provider mix now prefers Gemini Flash and OpenAI accurate before Gemini Pro preview fallback
  - local verification passed:
    - `cargo fmt --all`
    - `git diff --check`
    - `cargo test --manifest-path server/Cargo.toml answer_plan --quiet`
    - `cargo test --manifest-path server/Cargo.toml provider_mix_keeps_vision_on_image_capable_routes --quiet`
    - `cargo test --manifest-path server/Cargo.toml stream_route_connect_deadline --quiet`
    - `cargo check --manifest-path server/Cargo.toml --quiet`
  - production deploy completed:
    - built on droplet from source-only tree `/opt/bluey-build-codex-round284-router`
    - installed `/usr/local/bin/bluey-server`
    - previous production binary backup:
      `/var/backups/bluey-api/bin/bluey-server.previous-20260701T210318Z`
    - installed binary SHA256:
      `87020cba27eabb004640920b1e2725322d7a2c377944fa807e2f11571a3c8192`
    - `bluey-api.service` restarted active
    - `https://bluey.sh/health` returned `status=ok`
  - Round doc:
    `docs/rounds/ROUND-284-SCREEN-CODE-ANSWER-PLAN-FIRST-TOKEN.md`
- Round 283 is complete for click-through move-handle reliability:
  - macOS click-through move handle is larger and has a larger practical hitbox
  - macOS window-level pass-through policy now checks the move handle directly
  - macOS has a global mouse fallback so a move-handle press can arm/manual-drag the window even if the window was still ignored at mouse-down time
  - blank Bluey space remains click-through while click-through is on
  - Windows parity:
    - cyan move-handle rect is larger
    - Windows `HTCAPTION` hitbox for the click-through handle is inflated
  - desktop workspace version bumped to `0.1.38`
  - local verification passed:
    - `cargo fmt --all`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo check -p cue-daemon --quiet`
    - `cargo test -p cue-daemon pcm16_i16le_stats_detect_silence_and_audible_samples --quiet`
    - `cargo test --manifest-path server/Cargo.toml stt::tests --quiet`
  - release/deploy verification passed:
    - `https://bluey.sh/latest.json` reported `0.1.38`
    - live `latest.json.sig` verified successfully against the release Ed25519 key
    - live artifact SHA256:
      `480722b769522f43051b1056fc2e3d63395e5545bb62b341c6b711e7b2bc7669`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - live artifact:
    `https://bluey.sh/releases/v0.1.38/bluey-0.1.38-darwin-arm64.tar.gz`
  - Round doc:
    `docs/rounds/ROUND-283-CLICKTHROUGH-MOVE-HANDLE-RELIABILITY.md`
- Round 282 is complete for STT audible gating and relay diagnostics:
  - desktop live STT now waits for audible PCM before opening a paid `/stt/session`
  - the daemon keeps a short prebuffer so first audible words are forwarded after the relay opens
  - quiet mic/system startup now produces user-visible waiting notices instead of looking broken
  - desktop logs now include privacy-safe PCM level fields: sample count, RMS dBFS, peak dBFS, nonzero percentage
  - server relay logs forwarded audio level fields without transcript text or raw audio
  - desktop logs no-transcript Deepgram frames by provider frame type and payload size only
  - server Deepgram live URL now mirrors direct tuning with `endpointing=300`, `utterance_end_ms=1000`, and `vad_events=true`
  - optional `BLUEY_DEEPGRAM_LANGUAGE` is supported server-side
  - desktop workspace version bumped to `0.1.37`
  - local verification passed:
    - `cargo fmt --all`
    - `cargo check -p cue-daemon --quiet`
    - `cargo test -p cue-daemon pcm16_i16le_stats_detect_silence_and_audible_samples --quiet`
    - `cargo test --manifest-path server/Cargo.toml stt::tests --quiet`
    - `cargo check --manifest-path server/Cargo.toml --quiet`
    - `bash native/macos/cue-audio/build.sh`
    - direct microphone helper smoke produced PCM bytes
  - deploy/release verification passed:
    - server relay was built on the droplet, installed to `/usr/local/bin/bluey-server`, and restarted active
    - previous server binary backup:
      `/var/backups/bluey-api/bin/bluey-server.previous-20260701T185425Z`
    - `https://bluey.sh/health` returned `status=ok`
    - `https://bluey.sh/latest.json` reported `0.1.37`
    - live artifact SHA256:
      `8c698f5faf664823bb304d790c743bc3f1b78baf67725d19b15d4ce4c6a0e942`
    - `/install.sh` returned `application/x-shellscript`
    - `/install.ps1` returned `application/x-powershell`
  - Round doc:
    `docs/rounds/ROUND-282-STT-AUDIBLE-GATE-RELAY-DIAGNOSTICS.md`
- Round 281 is complete for overlay connect-code visibility:
  - daemon login cards now use structured body copy with `Code: XXXX-XXXX` and hidden `login_url: ...`
  - macOS overlay sign-in cards extract the code and show a dedicated `Connect code` pill
  - macOS sign-in card hides raw `Code:` / `login_url:` metadata from the body
  - macOS sign-in CTA now says `Open browser`
  - Windows parity:
    - the daemon sends the structured code line to Windows too; current Windows body renderer shows the code plainly
  - desktop workspace version bumped to `0.1.36`
  - local verification passed:
    - `cargo fmt --all`
    - `cargo check -p cue-daemon --quiet`
    - `cargo check -p cue-cli --quiet`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
  - live deploy verification passed:
    - `https://bluey.sh/latest.json` reports `0.1.36`
    - live `latest.json.sig` verifies successfully against the release Ed25519 key
    - live artifact checksum matches `SHA256SUMS.txt`
    - live `/install.sh` returns `application/x-shellscript`
    - live `/install.ps1` returns `application/x-powershell`
    - temp-root installer smoke installed `bluey 0.1.36`
  - live artifact:
    `https://bluey.sh/releases/v0.1.36/bluey-0.1.36-darwin-arm64.tar.gz`
  - live SHA256:
    `4c0f90d748019ad05fb5a520e4775d77ab58e8350dde692cb93923347968e0b5`
  - Round doc:
    `docs/rounds/ROUND-281-OVERLAY-CONNECT-CODE.md`
- Round 280 is complete for production-facing updater/install/uninstall copy:
  - replaced old updater failure copy that mentioned `BLUEY_UPDATE_ALLOW_UNSIGNED` / local testing with production recovery language
  - old unverifiable builds now say to reinstall once from the production installer; after that `bluey on` keeps Bluey updated automatically
  - strict signed-update verification remains intact
  - `bluey uninstall --help` now says `Remove Bluey from this device`
  - `--purge-data` copy now refers to device account tokens and device data
  - macOS and Windows installers say `Bluey document tools`, not `Bluey-local document tools`
  - desktop workspace version bumped to `0.1.35`
  - local verification passed:
    - `cargo test -p cue-cli old_build_update_message_is_production_safe --quiet`
    - `cargo test -p cue-cli unverified_manifest_is_not_installable_by_default --quiet`
    - `cargo check -p cue-cli --quiet`
    - `cargo fmt --all`
    - `cargo run -p cue-cli --bin bluey --quiet -- uninstall --help`
  - live deploy verification passed:
    - `https://bluey.sh/latest.json` reports `0.1.35`
    - live `latest.json.sig` verifies successfully against the release Ed25519 key
    - live artifact checksum matches `SHA256SUMS.txt`
    - live `/install.sh` returns `application/x-shellscript`
    - live `/install.ps1` returns `application/x-powershell`
    - temp-root installer smoke installed `bluey 0.1.35` and exposed production-safe uninstall help
  - live artifact:
    `https://bluey.sh/releases/v0.1.35/bluey-0.1.35-darwin-arm64.tar.gz`
  - live SHA256:
    `ce9aaebf515f998efc533c81345f3e662035416a2df4c8ff7aeb667504e71190`
  - Important recovery note:
    - a customer already on a very old binary that cannot verify signed updates needs one production reinstall with `curl -fsSL https://bluey.sh/install.sh | bash`
    - after that, current builds have the embedded updater key and should update normally through `bluey on`
  - Round doc:
    `docs/rounds/ROUND-280-PRODUCTION-UPDATE-RECOVERY-COPY.md`
- Round 279 is complete for desktop login and uninstall:
  - daemon sign-in now stores the active desktop login URL/code and reopens it on repeated `Open login` clicks
  - duplicate close-together login requests reuse the active device-code flow
  - device login URLs now include `desktop=1&user_code=...`
  - `bluey on` no longer prints the plain `/login` fallback after starting the browser device flow
  - web `/login` remembers pending desktop codes for the 10-minute device-flow lifetime and keeps the Connect desktop banner visible
  - added `bluey uninstall` with `--yes` and `--purge-data`; default uninstall preserves local account data and saved sessions
  - macOS and Windows installer copy now mentions `bluey uninstall`
  - Caddy example and live Caddy config have a real-file handler for `/install.sh`, `/install.ps1`, `latest.json`, `latest.json.sig`, and `/releases/*`
  - desktop workspace version bumped to `0.1.33`
  - local verification passed:
    - `node --check web/assets/bluey-site.js`
    - `cargo test -p cue-cli device_login_url --quiet`
    - `cargo test -p cue-cli uninstall_root_detection --quiet`
    - `cargo check -p cue-cli --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `cargo check -p cue-dashboard --quiet`
    - `cargo run -p cue-cli --bin bluey --quiet -- uninstall --help`
    - `cargo test -p cue-daemon overlay_sign_in_event_is_accepted_by_production_validator --quiet`
  - live deploy verification passed:
    - `https://bluey.sh/latest.json` reports `0.1.33`
    - live `latest.json.sig` verifies successfully against the release Ed25519 key
    - live `/install.sh` returns `application/x-shellscript` and starts with `#!/usr/bin/env bash`
    - live `/install.ps1` returns `application/x-powershell`
    - live artifact checksum matches `SHA256SUMS.txt`
    - temp-root installer smoke installed `bluey 0.1.33` and exposed `bluey uninstall --help`
  - live artifact:
    `https://bluey.sh/releases/v0.1.33/bluey-0.1.33-darwin-arm64.tar.gz`
  - live SHA256:
    `d0d6ff6059eff0f32ee61ec93fb9777bc1d6a1a0c4b6e24a885d62916ef6d183`
  - remaining QA:
    - run a real signed-out desktop login on a user machine to confirm the browser account page reflects the device code after auth and the daemon receives the completed link
  - Round doc:
    `docs/rounds/ROUND-279-DESKTOP-LOGIN-UNINSTALL.md`
- Round 278 fixed the default click-through and movement model:
  - macOS now starts with click-through off by default
  - click-through-off mode lets blank Bluey space drag the window
  - blank history and keyboard/tone popup areas now drag Bluey while preserving real controls and editable/selectable text
  - click-through-on mode now shows a dedicated blue four-direction move handle
  - in click-through-on mode, blank space passes through and only the move handle drags the overlay
  - the old implicit logo/name move target is no longer used in click-through-on mode
  - keyboard shortcut help is more compact and explains the move behavior for both modes
  - Windows parity:
    - default mode is click-through off
    - click-through-on mode draws a cyan move handle
    - blank click-through space returns transparent hit testing; the cyan handle returns window-drag hit testing
    - Windows help/shortcut copy mirrors the new model
  - desktop workspace version bumped to `0.1.32`
  - verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo check -p cue-daemon --offline`
    - `cargo check -p cue-dashboard --locked`
    - `cargo test -p cue-daemon --lib --locked`
    - `cargo test -p cue-core sign_in_event_serializes --locked`
    - `git diff --check`
  - release artifact dev-flag/secret scan passed
  - live artifact:
    `https://bluey.sh/releases/v0.1.32/bluey-0.1.32-darwin-arm64.tar.gz`
  - live SHA256:
    `9883b0bff4be1e8967b463295011bd631fc0d7d6d43f3c86c32ebb3475e6de4f`
  - live `https://bluey.sh/latest.json` reports `0.1.32`
  - `latest.json.sig` verified successfully
  - `https://bluey.sh/install.sh` serves `application/x-shellscript`
  - local install updated to `bluey 0.1.32` and restarted with `overlay_capture_excluded: true`
  - Windows packaging was not produced on this Mac because the release artifact path still needs the Windows/MSVC runner; Windows source parity and syntax check passed
  - Round doc:
    `docs/rounds/ROUND-278-CLICKTHROUGH-DEFAULT-MOVE-HANDLE.md`
- Round 277 polished shortcut help, focus, and drag behavior:
  - macOS shortcut sheet now uses attributed text with shortcut keys in accent color and actions in primary text color
  - click-through on shows global shortcuts only
  - click-through off shows inside-Bluey shortcuts for when Ask is not focused, plus the global shortcuts
  - `H` is only an inside-Bluey History shortcut; there is no `Ctrl+Option+H` global History shortcut
  - `Ctrl+Option+Enter` is documented as global Answer
  - `Enter` is documented as inside-Bluey Answer, with `Shift+Enter` for a new Ask line
  - shortcut sheet Done button is centered and the panel is wider
  - clicking outside Ask clears Ask focus
  - feed answer text is no longer selectable, so blank/feed space can drag the overlay in click-through-off mode; copy buttons remain the answer-copy path
  - Windows shortcut help copy now matches the same mode split
  - Windows single-letter local shortcuts now require interactive/click-through-off mode
  - Windows clears Ask focus when clicking outside the Ask control
  - desktop workspace version bumped to `0.1.31`
  - verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `git diff --check`
    - `cargo check -p cue-daemon --offline`
    - `cargo check -p cue-dashboard --locked`
    - `cargo test -p cue-daemon --lib --locked`
    - `cargo test -p cue-core sign_in_event_serializes --locked`
  - release artifact dev-flag/secret scan passed
  - live artifact:
    `https://bluey.sh/releases/v0.1.31/bluey-0.1.31-darwin-arm64.tar.gz`
  - live SHA256:
    `698e885a66fc60e35c84f85053200cb45aa837af2f44b5d5962ca04ade133e8a`
  - live `https://bluey.sh/latest.json` reports `0.1.31`
  - `latest.json.sig` verified successfully
  - `https://bluey.sh/install.sh` serves `application/x-shellscript`
  - local install updated to `bluey 0.1.31` and restarted with `overlay_capture_excluded: true`
  - local shortcut smoke:
    - `Ctrl+Option+B` restored from collapsed state
    - `Ctrl+Option+H` did not change Bluey overlay state
  - Windows packaging attempt on this Mac did not produce a ZIP because the Makefile target expects `x86_64-pc-windows-msvc`, while this host only has `x86_64-pc-windows-gnu` installed; Windows source parity and syntax check are complete, but a fresh Windows artifact needs the Windows/MSVC release runner
  - Round doc:
    `docs/rounds/ROUND-277-SHORTCUTS-FOCUS-DRAG-POLISH.md`
- Round 276 removed the legacy `Ctrl+Option+H`/full-hide confusion:
  - dashboard no longer registers `Ctrl+Alt+H` / `Ctrl+Option+H` as a global overlay toggle
  - `H` remains only as an inside-overlay History shortcut when the native overlay is interactive and Ask is not focused
  - macOS native overlay source no longer contains the old `RestoreToast`, `hideAllBlueyChrome`, or fade-hide helper path
  - installer copy now says `Ctrl+Option+B to minimize/restore`
  - desktop workspace version bumped to `0.1.30`
  - screen-share answer: normal prod mode remains capture-excluded; `bluey status` should show `overlay_capture_excluded: true`
  - verification passed:
    - `cargo check -p cue-daemon --offline`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon --lib --locked`
    - `cargo check -p cue-dashboard --locked`
    - `cargo test -p cue-core sign_in_event_serializes --locked`
  - live artifact:
    `https://bluey.sh/releases/v0.1.30/bluey-0.1.30-darwin-arm64.tar.gz`
  - live SHA256:
    `5f39eb29f3f1f0c25e08ecc334508fa67a1d7c2983798ee5a5c5081d3b426eff`
  - live `https://bluey.sh/latest.json` reports `0.1.30`
  - `latest.json.sig` verified successfully
  - `https://bluey.sh/install.sh` serves `application/x-shellscript`
  - local install updated to `bluey 0.1.30` and restarted with `overlay_capture_excluded: true`
  - local shortcut smoke:
    - `Ctrl+Option+B` restored from collapsed state
    - `Ctrl+Option+H` did not change Bluey overlay state
  - Round doc:
    `docs/rounds/ROUND-276-REMOVE-LEGACY-H-HIDE-TOAST.md`
- Round 275 changed Hide back into a minimize-to-pill action:
  - macOS `Ctrl+Option+B` collapses expanded Bluey to the pill and restores from the pill
  - macOS IPC `hide` collapses to the pill instead of fully hiding every Bluey window
  - signed-out auth gate does not collapse to the pill; its hide control is disabled so users complete auth first
  - Windows `Ctrl+Alt+B`, IPC `hide`, and IPC `toggle` now use the existing `collapse_to_pill(...)` path
  - macOS and Windows shortcut/help copy now says `Minimize to pill / restore`
  - desktop workspace version bumped to `0.1.29`
  - verification passed:
    - `cargo check -p cue-daemon --offline`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon --lib --locked`
    - `cargo test -p cue-core sign_in_event_serializes --locked`
  - live artifact:
    `dist/bluey-0.1.29-darwin-arm64.tar.gz`
  - live SHA256:
    `17ad3711d96a16d3647cb6a0f16b454fc5ea5cf608cdc4ea0fa1f37c2457b016`
  - live `https://bluey.sh/latest.json` reports `0.1.29`
  - `latest.json.sig` verified successfully
  - local install updated to `bluey 0.1.29` and restarted with `overlay_capture_excluded: true`
  - local shortcut smoke:
    - `Ctrl+Option+B` collapsed overlay state to false
    - `Ctrl+Option+B` restored overlay state to true
  - Windows source parity is included, but Windows artifact was not republished yet
  - Round doc:
    `docs/rounds/ROUND-275-HIDE-COLLAPSES-TO-PILL.md`
- Round 274 fixed the signed-out gate and duplicate input caret:
  - removed the custom macOS composer caret so `NSTextView` owns the only blinking insertion point
  - added a signed-out gate state to the macOS expanded overlay
  - signed-out overlay now receives mouse events even when click-through is enabled, so Sign in remains clickable
  - disabled and dimmed unusable controls while signed out
  - Listen, Screen, Answer, Attach, and Text shortcuts now route to sign-in feedback instead of paid/local actions while signed out
  - successful account state unlock collapses the signed-out gate back to the pill
  - daemon `sign_in_requested` now pushes a visible `Bluey sign-in` status card instead of discarding helper text
  - desktop workspace version bumped to `0.1.28`
  - verification passed:
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `cargo check -p cue-daemon --offline`
    - `cargo test -p cue-daemon --lib --locked`
    - `cargo test -p cue-core sign_in_event_serializes --locked`
  - Windows artifact was not republished yet; daemon sign-in feedback is shared, but the visual gate/caret patch is macOS Swift overlay specific
  - Round doc:
    `docs/rounds/ROUND-274-SIGNIN-GATE-INPUT-CARET.md`
- Round 273 fixed incomplete answer finalization:
  - investigated the screenshot where the refrigeration / ML answer stopped in the middle of a Markdown table
  - confirmed the local saved answer itself ended at the table separator, so this was not only UI clipping
  - added final-answer integrity checks for unclosed code fences, unfinished Markdown tables, dangling headings, and bare list markers
  - incomplete final text now logs `answer_incomplete_reason` and returns the retryable incomplete-answer path instead of being saved as complete
  - updated overlay answer-shape instructions to avoid Markdown tables in streamed chat
  - desktop workspace version bumped to `0.1.27`
  - verification passed:
    - `cargo test -p cue-daemon --lib --locked`
    - `cargo test -p cue-llm --locked`
    - `cargo check -p cue-daemon --offline`
    - `cargo fmt`
  - live artifact:
    `dist/bluey-0.1.27-darwin-arm64.tar.gz`
  - live SHA256:
    `75651a30e1983f1183b3ee32297b0a6e075eba0af712ec4477a8031bd80237aa`
  - live `https://bluey.sh/latest.json` reports `0.1.27`
  - local `bluey update` updated from `0.1.26` to `0.1.27`, and `bluey status` showed the daemon running after restart
  - Windows artifact was not republished; daemon-side guard is shared Rust and should apply on the next Windows build
  - Round doc:
    `docs/rounds/ROUND-273-INCOMPLETE-ANSWER-GUARD.md`
- Round 272 fixed macOS keyboard editing and shortcut scope:
  - `Ctrl+Option+H` and other unassigned modified keys no longer route into old inside-Bluey shortcuts
  - official macOS global shortcuts are now limited to `Ctrl+Option+B/T/L/S/I/Enter`
  - local `Ctrl+<key>` no longer triggers optional inside-Bluey shortcuts
  - Ask no longer refocuses/re-arms the caret when already focused, so `Cmd+A` then Delete can clear selected text normally
  - Delete, Return, Home/End, Page Up/Down, and arrow keys are forwarded to the composer text view for normal editing behavior
  - Enter still submits while Ask is focused; from outside Ask the intended global answer shortcut is `Ctrl+Option+Enter`
  - Windows parity checked: Windows already only registers `Ctrl+Alt+B/T/L/S/I/Enter`, and Ask edit requests arrow handling with `DLGC_WANTARROWS`
  - desktop workspace version bumped to `0.1.26`
  - live artifact:
    `dist/bluey-0.1.26-darwin-arm64.tar.gz`
  - live SHA256:
    `697bde7408e8a6a06927bbe6f810b923f02d9eccce1f3fe420ac541a3aa63f67`
  - live `https://bluey.sh/latest.json` reports `0.1.26`
  - local `bluey update` updated from `0.1.25` to `0.1.26`
  - shortcut smoke passed before and after update:
    - `Ctrl+Option+H` -> `overlay_visible: true`
    - `Ctrl+Option+B` -> `overlay_visible: false`
    - `Ctrl+Option+B` again -> `overlay_visible: true`
  - Windows artifact was not republished; live manifest remains macOS arm64
  - Round doc:
    `docs/rounds/ROUND-272-KEYBOARD-EDITING-SHORTCUT-SCOPE.md`
- Round 271 refined shortcut guidance and fixed a release guard:
  - macOS shortcuts panel is mode-aware:
    - click-through on: tells users to use global shortcuts because blank Bluey space clicks through
    - interactive on: lists global shortcuts first and labels inside-Bluey shortcuts as optional
  - Windows shortcuts dialog mirrors the same mode-aware guidance
  - Windows global hotkey registration now checks every `RegisterHotKey` result and logs `global_shortcuts` as `ready` or `partial` with failed key/error detail
  - Release packaging now fails closed if `BLUEY_UPDATE_PUBKEY` is missing
  - desktop workspace version bumped to `0.1.25`
  - an intermediate `0.1.25` artifact was republished after discovering the local `0.1.24` build lacked `BLUEY_UPDATE_PUBKEY`; the final `0.1.25` artifact embeds the update public key
  - live artifact:
    `dist/bluey-0.1.25-darwin-arm64.tar.gz`
  - live SHA256:
    `cf6ab7072d61a78de166463240060ed6e4792cdd7328f93df5f10e573fd0cfab`
  - live `https://bluey.sh/latest.json` reports `0.1.25`
  - live `latest.json.sig` verified successfully
  - local reinstall from `https://bluey.sh/install.sh` installed `bluey 0.1.25`
  - local `bluey update` now reports up to date without unsigned-update warnings
  - shortcut smoke passed:
    - `Ctrl+Option+B` -> `overlay_visible: false`
    - `Ctrl+Option+B` again -> `overlay_visible: true`
  - Windows artifact was not republished; live manifest remains macOS arm64
  - Round doc:
    `docs/rounds/ROUND-271-SHORTCUT-MODE-GUIDANCE-RELEASE-GUARD.md`
- Round 270 fixed and deployed shortcut/hide/input polish:
  - macOS now registers OS-level Carbon hotkeys for `Ctrl+Option+B/T/L/S/I/Enter`, with the existing event monitor retained as fallback
  - local macOS key routing checks the global shortcut path first, so shortcuts work while Bluey itself has focus
  - `Ctrl+Option+B` now fully hides/restores Bluey chrome instead of collapsing to the pill
  - `Ctrl+Option+T` and local `T` are labeled/handled as Text input
  - the macOS shortcuts panel no longer exposes the Copy action
  - Windows source parity updates `Ctrl+Alt+B` to hide/restore fully and updates shortcut copy to Text input
  - macOS install copy now points to `Ctrl+Option+B` instead of the legacy F19 wording
  - desktop workspace version bumped to `0.1.24`
  - live artifact:
    `dist/bluey-0.1.24-darwin-arm64.tar.gz`
  - live SHA256:
    `b5a9cd1e65a0b6e45a29c4c8d8070e89361241cc61a8929363ee619135d0f2f7`
  - live `https://bluey.sh/latest.json` reports `0.1.24`
  - live `latest.json.sig` verified successfully
  - local machine updated from the published artifact and reports `bluey 0.1.24`
  - shortcut smoke passed:
    - `Ctrl+Option+B` -> `overlay_visible: false`
    - `Ctrl+Option+B` again -> `overlay_visible: true`
  - Windows artifact was not republished; live manifest remains macOS arm64
  - Round doc:
    `docs/rounds/ROUND-270-SHORTCUT-HIDE-INPUT-POLISH.md`
- Round 269 deployed the Round 268 login-state recovery fix:
  - bumped desktop workspace version to `0.1.23`
  - built `dist/bluey-0.1.23-darwin-arm64.tar.gz`
  - live SHA256:
    `105f3101e2a119e7a19bbee9a70efe8c61f7193556f79c570c527885d6301284`
  - published signed release files to
    `root@165.227.77.152:/var/www/bluey`
  - live `https://bluey.sh/latest.json` reports `0.1.23`
  - live `install.sh` serves as `application/x-shellscript`, not HTML
  - `latest.json.sig` verified successfully with OpenSSL
  - temp-home installer smoke installed `bluey 0.1.23`
  - local machine was installed from the `0.1.23` artifact, restarted,
    and reports `bluey 0.1.23`, `overlay_visible: true`, and
    `Balance: $14.50`
  - noninteractive smoke could not use `/dev/tty` for sudo, but correctly
    fell back to user-local `$HOME/.local/bin`
  - Windows artifact was not republished; live manifest remains macOS arm64
  - Round doc:
    `docs/rounds/ROUND-269-LOGIN-STATE-RELEASE-DEPLOY.md`
- Round 268 fixed the overlay mixed login state:
  - added shared overlay command `set_account_state`
  - daemon now sends signed-in state after successful balance refresh,
    background balance snapshots, and Listen auth verification
  - daemon now sends signed-out state on logout
  - standalone `bluey login` now best-effort pings daemon `CloudStatus`
    after saving cloud tokens, so a running overlay refreshes immediately
  - macOS overlay parses `set_account_state`, clears signed-out chrome
    without wiping attached docs/context state, hides stale sign-in toasts,
    and uses `Sign in` instead of `Login` for real signed-out state
  - local macOS binaries were rebuilt, installed into `~/.bluey/bin`,
    ad-hoc signed, and restarted
  - verification passed:
    - `cargo fmt`
    - `cargo test -p cue-core set_account_state_serializes_as_overlay_command -- --nocapture`
    - `cargo check -p cue-daemon -p cue-cli --quiet`
    - `swift build -c debug --package-path native/macos/cue-overlay`
    - `git diff --check`
    - `~/.bluey/bin/bluey status`
    - `~/.bluey/bin/bluey credits` -> `Balance: $14.50`
    - `~/.bluey/bin/bluey cloud status` -> `auth: TokenConfigured`
  - Round doc:
    `docs/rounds/ROUND-268-OVERLAY-LOGIN-STATE-RECOVERY.md`
- Round 267 added a visible keyboard/shortcuts entry point:
  - macOS header now has a keyboard icon beside the theme icon
  - macOS opens an in-overlay shortcuts panel with `Ctrl+Option`
    shortcuts and local `L/S/I/H/F/Esc` rules
  - macOS panel includes a Copy action
  - Windows header now has a `Keys` button beside Theme
  - Windows `Keys` dialog lists `Ctrl+Alt` shortcuts and local
    `L/S/I/H/F/Esc` rules
  - Windows Help copy now points to Keys for shortcuts
  - macOS overlay was rebuilt, hot-installed, signed, and restarted locally
  - live macOS check verified `Ctrl+Option+B` toggles `overlay_visible`
    false/true through `System Events`
  - safe macOS checks exercised `Ctrl+Option+I` twice and
    `Ctrl+Option+Enter` on an empty session without creating transcript or
    context rows
  - AX focus probe for `Ctrl+Option+T` was blocked with `AX` error `-25204`,
    so focus should still be manually confirmed in the visible overlay
  - Round doc:
    `docs/rounds/ROUND-267-KEYBOARD-SHORTCUTS-DISCLOSURE.md`
- Round 266 unified the shortcut model across macOS and Windows:
  - global shortcut family is now `Ctrl+Option+key` on macOS and
    `Ctrl+Alt+key` on Windows
  - `B` show/hide, `T` focus Ask, `L` Listen, `S` Screen, `I`
    click-through/interactive, `Enter` Answer
  - local direct `L/S/I/H/F/Esc` works only when the full overlay is
    interactive and a text editor is not focused
  - macOS now has global key routing plus local overlay shortcut routing
  - Windows now registers `Ctrl+Alt` hotkeys and has a real
    click-through/interactive toggle for blank-space hit testing
  - older dashboard shortcut registrations moved away from `Ctrl+Shift`
  - F19 remains only as a legacy fallback where it already exists
  - verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
    - `cargo check -p cue-dashboard --quiet`
    - `git diff --check`
    - release macOS overlay build
    - local `~/.bluey/bin` overlay hot-install, ad-hoc sign, and restart
  - Round doc:
    `docs/rounds/ROUND-266-SHORTCUTS-OVERLAY-PARITY.md`
- Round 265 fixed and deployed the balance poll recovery issue:
  - root cause was a stale in-memory token cache inside the long-running daemon
    balance poller while CLI/browser auth could refresh the secure token store
  - added `CloudClient::reload_tokens_from_store()`
  - balance polling now reloads stored tokens and retries immediately after a
    poll failure
  - desktop version bumped to `0.1.22`
  - live `https://bluey.sh/latest.json` reports `0.1.22`
  - `latest.json.sig` verifies and the live artifact checksum matches
  - temp-home install smoke installed `bluey 0.1.22`
  - local machine is installed/running `bluey 0.1.22`
  - local `bluey credits` returned `Balance: $14.56`
  - the server binary on the droplet was rolled forward to commit `0a3c67e`
    so Round 264 data-ops gates are live
  - `bluey-api.service` is active and `https://bluey.sh/health` reports
    `status=ok`, `commit=0a3c67e`
  - Round doc:
    `docs/rounds/ROUND-265-BALANCE-POLL-RECOVERY-DEPLOY.md`
- Round 264 implemented the production data operations gates in the repo:
  - `/account/export` keeps the existing JSON export
  - `/account/export?format=zip` now produces a structured zip with
    `account-export.json`, `manifest.json`, readable transcript/answer
    markdown, and original artifact bytes when object storage is configured
  - zip export fails closed if referenced object bytes cannot be fetched, object
    keys fall outside the account scope, or the configured export byte budget is
    exceeded
  - `/account/delete` now deletes account-scoped R2/S3 artifact objects before
    deleting account DB rows; if object storage is unavailable or object delete
    fails, the account delete fails closed
  - new redacted `ops_audit_events` table records export, delete, and admin
    support-bundle events without account foreign keys, so evidence survives
    hard-delete while avoiding transcript/document retention
  - new admin-only endpoints:
    - `/admin/storage/health`
    - `/admin/support/accounts/:account_id`
    - `/admin/ops/events`
  - new `ops/restore-drill-bluey-db.sh` restores latest local backup into an
    explicit non-production drill target, refuses live `BLUEY_DATABASE_URL`, and
    verifies core table counts
  - `docs/PRODUCTION-DEPLOY-RUNBOOK.md` now documents restore drills,
    zip exports, object-aware deletes, and admin ops endpoints
  - focused tests passed:
    - `account_export_zip_contains_readable_bundle`
    - `delete_account_deletes_artifact_objects_before_account_rows`
    - `admin_support_bundle_is_redacted`
    - `cargo check --manifest-path server/Cargo.toml`
    - `bash -n ops/backup-bluey-db.sh`
    - `bash -n ops/restore-drill-bluey-db.sh`
    - disposable SQLite restore drill
  - this round was deployed to the droplet during Round 265
  - Round doc:
    `docs/rounds/ROUND-264-PROD-DATA-OPS-GATES.md`
- Round 263 completed the production Postgres/R2 storage setup:
  - live `bluey-api.service` was confirmed running with
    `BLUEY_SERVER_DB_BACKEND=postgres`, `BLUEY_DATABASE_URL=<set>`, strict
    Redis/Valkey, and port `8080`
  - R2 object-byte sync was enabled live with:
    - `BLUEY_OBJECT_BUCKET=bluey-prod`
    - `BLUEY_OBJECT_ENDPOINT_URL=<set>`
    - `BLUEY_OBJECT_REGION=auto`
    - `BLUEY_OBJECT_KEY_PREFIX=bluey-cloud`
    - `BLUEY_OBJECT_RETENTION_DAYS=365`
    - `BLUEY_OBJECT_MAX_BYTES=26214400`
    - `BLUEY_REQUIRE_OBJECT_STORAGE=1`
  - previous live env file was backed up on the droplet at
    `/etc/bluey-api/bluey-api.env.bak.20260630T213447Z`
  - service restart and local health check passed
  - R2 object put/list/delete preflight passed under
    `bluey-cloud/preflight/object-storage-20260630T213605Z.txt`
  - unauthenticated object upload returned `401`
  - discovered the old backup script was SQLite-only while runtime was
    Postgres-backed
  - patched and installed `ops/backup-bluey-db.sh` so cron now creates
    backend-aware backups:
    - SQLite -> `.db`
    - Postgres -> `pg_dump --format=custom` `.pgdump`
  - installed `postgresql-client-18` on the droplet because managed Postgres is
    `18.4` and `pg_dump 16` failed with a server-version mismatch
  - forced Postgres backup passed:
    `/var/backups/bluey-api/hourly/bluey-postgres-20260630T214349Z.pgdump`
    (`11211157` bytes)
  - `pg_restore -l` could read the backup and showed server/dump version `18.4`
  - R2 now contains the `.pgdump` plus `.pgdump.sha256` under
    `s3://bluey-prod/backups/api/`
  - currently published signed release files were mirrored to:
    `s3://bluey-prod/releases/bluey-sh/`
  - `scripts/publish-bluey-release.sh` now supports optional future release
    mirroring via `BLUEY_RELEASE_MIRROR_DESTINATION`
  - runbook docs were updated for Postgres backups/restore and R2 release mirror
  - Round doc:
    `docs/rounds/ROUND-263-PROD-POSTGRES-R2-STORAGE-SETUP.md`
- Round 262 audited production R2 storage:
  - live droplet env confirms R2/S3-compatible off-host backups are configured:
    `OFFSITE_DESTINATION=s3://bluey-prod/backups/api/`
  - no R2 secrets were printed into docs
  - live R2 listing showed hourly `.db` backups plus `.sha256` checksum objects,
    with `470` objects and `246984702` bytes at audit time
  - backup cron is installed at `/etc/cron.d/bluey-api-backup` and runs hourly
  - live env did not show separate `BLUEY_OBJECT_*` settings, so raw user
    artifact object-byte sync is implemented in code but not observed enabled
    on the production droplet during this audit
  - source of truth remains database tables; R2 is blob/object storage only
  - Round doc:
    `docs/rounds/ROUND-262-PROD-R2-STORAGE-AUDIT.md`
- Round 261 restores strict click-through semantics and hides attached-file strips
  unless the user asks to inspect them:
  - macOS click-through mode now only receives mouse events for explicit
    controls, manual overlay controls, open history drawer, open canvas pane,
    resize edges, and the Bluey logo/name drag handle
  - blank expanded-panel space now returns no hit-test target in click-through
    mode, so clicks pass to the app behind Bluey
  - blank-space drag from pass-through mode was removed; movement in strict
    click-through mode is via the Bluey logo/name drag handle
  - window-level interactivity now returns false for blank click-through areas
  - Windows expanded blank space now returns `HTTRANSPARENT`; controls, resize
    edges, and the brand drag handle remain interactive
  - tooltips/toasts now say `Click-through on` instead of `Move-anywhere on`
  - uploaded/dropped files still prepare context immediately for the next answer
  - macOS attachment chips are collapsed by default behind `Show N files`
  - sending an answer collapses the attachment strip again
  - Windows context chips are hidden by default behind an explicit show state
    and collapse after sends/new attach flows
  - release version bumped to `0.1.21`
  - local checks passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `cargo check -p cue-cli -p cue-daemon --quiet`
    - `git diff --check`
  - local patched overlay installed into `~/.bluey/bin` for immediate testing
  - live `0.1.21` package installed locally from `https://bluey.sh/install.sh`
    and restarted with `BLUEY_SKIP_UPDATE=1`
  - local smoke returned `bluey 0.1.21` and daemon pid `26781`
  - verification/deploy passed:
    - release artifact dev-flag/secret scan passed
    - `latest.json` version `0.1.21`
    - `latest.json.sig` 88 bytes and OpenSSL verified
    - live darwin-arm64 SHA256
      `6c1ff0a731c63e10ca66d7d06aa0b936c7da6044960c2d13ca4a4f9cc20db0f4`
      matched `SHA256SUMS.txt`
    - temp-home installer smoke from `https://bluey.sh/install.sh` installed
      `bluey 0.1.21`
    - local install smoke moved this machine to `bluey 0.1.21`
  - Round doc:
    `docs/rounds/ROUND-261-STRICT-CLICKTHROUGH-ATTACHMENT-STRIP.md`
- Round 260 hardens auto-update restart and repeated Listen start latency:
  - root cause for `Bluey daemon did not become ready`: update restart could
    race daemon shutdown/port release, waited only `5s`, and did not report
    child early exit clearly
  - `bluey off` now waits for the daemon to stop before returning
  - `bluey on` waits up to `20s` and reports daemon child early exit
  - stale-daemon cleanup waits briefly after SIGTERM before removing state
  - auto-update relaunch prefers the current installed `bluey` executable path
    instead of relying on PATH order
  - Listen remains gated behind verified sign-in, but a successful account
    verification is cached for `30s` so repeated Listen toggles avoid the same
    `/account/me` round trip
  - logout and failed account checks clear the Listen verification cache
  - release version bumped to `0.1.20`
  - verification/deploy passed:
    - `cargo check -p cue-cli --quiet`
    - `cargo check -p cue-daemon --quiet`
    - `cargo test -p cue-cli relaunch_ --quiet`
    - `cargo test -p cue-daemon listen_auth_gate_requires_linked_cloud_account --quiet`
    - `latest.json` version `0.1.20`
    - `latest.json.sig` 88 bytes and OpenSSL verified
    - live darwin-arm64 SHA256
      `bfa16fc62e45eaf7ed613a162fd3547b97616392143c710b78ac3e22d7980f2e`
      matched `SHA256SUMS.txt`
    - temp-home installer smoke from `https://bluey.sh/install.sh` installed
      `bluey 0.1.20`
    - local install smoke moved this machine to `bluey 0.1.20` and
      `bluey-daemon 0.1.20`
    - local `BLUEY_SKIP_UPDATE=1 ~/.bluey/bin/bluey on` started daemon pid
      `70850`
  - Round doc:
    `docs/rounds/ROUND-260-UPDATE-RESTART-LISTEN-LATENCY.md`
- Round 259 prevents Listen/mic from starting before verified desktop sign-in:
  - added a daemon-side gate for both direct `AudioStart` IPC and overlay
    `recording_start_requested`
  - the gate verifies `/account/me` before audio capture starts
  - unsigned profiles keep Listen off and open browser sign-in
  - expired/deleted-account tokens (`401`, `403`, `404`) are cleared before
    sign-in is reopened
  - verified accounts with credit/cooldown/temporary verification issues keep
    Listen off without pretending sign-in is opening
  - macOS overlay no longer optimistically switches expanded or collapsed
    Listen UI into `Starting` before the daemon accepts the request
  - release version bumped to `0.1.19` for the downloadable package
  - live checks passed:
    - `latest.json` version `0.1.19`
    - `latest.json.sig` 88 bytes and OpenSSL verified
    - live darwin-arm64 SHA256
      `ebc2f52e07faf40ef7e09f167fcb804c60f00f7ebba8875d528567289b84c885`
      matched `SHA256SUMS.txt`
    - temp-home installer smoke from `https://bluey.sh/install.sh` installed
      `bluey 0.1.19`
  - Round doc:
    `docs/rounds/ROUND-259-LISTEN-SIGNIN-GATE-DEPLOY.md`
- Round 258 fixed the native desktop login gap where clicking the Bluey sign-in
  pill authenticated the browser but did not link the running desktop:
  - root cause: macOS overlay opened plain `https://bluey.sh/login`; no
    device-code flow was started, the daemon did not poll, and no local tokens
    were saved
  - added shared `DaemonRequest::CloudLogin` and `OverlayEvent::SignInRequested`
  - `bluey on` now automatically starts a browser device-code login when no
    local token is linked
  - macOS Sign in button emits `sign_in_requested` instead of opening a plain
    URL
  - daemon starts `/auth/device/start`, opens `verification_uri?user_code=...`,
    polls `/auth/device/poll`, saves tokens, refreshes cloud/balance state, and
    switches the overlay to `Bluey online`
  - duplicate guard avoids multiple simultaneous login flows
  - Windows parity: shared CLI/daemon behavior works on Windows package builds;
    `open_browser_from_daemon` includes a Windows `cmd /C start` branch; the
    current Windows overlay has no sign-in button to patch
  - bumped the desktop package to `0.1.18` so existing `0.1.17` installs can
    auto-update instead of seeing the same version
  - built macOS arm64 with embedded update pubkey and published signed
    `latest.json.sig`
  - live checks passed:
    - `latest.json` version `0.1.18`
    - `latest.json.sig` 88 bytes and OpenSSL verified
    - live darwin-arm64 SHA256
      `2272286b801c327e8c8f6913ce69ee76351367d6ea27aeef1341c6a03cf9eaab`
      matched `SHA256SUMS.txt`
    - temp-home installer smoke from `https://bluey.sh/install.sh` installed
      `bluey 0.1.18`
  - Round doc:
    `docs/rounds/ROUND-258-DESKTOP-LOGIN-AUTO-LINK-DEPLOY.md`
- Round 257 explained and fixed the still-low `$4.79` balance:
  - active local desktop account was `codex-smoke-20260608183100@bluey.sh`,
    not the earlier restored `internal-admin-20260606023943@bluey.sh`
  - live Postgres had `codex-smoke` at `balance_cents=479` and
    `internal-admin` at `balance_cents=1500`
  - recent ledger rows showed normal LLM/STT usage on `codex-smoke`
  - credited `codex-smoke` internally by `1021` cents:
    `479 -> 1500`
  - cleared stale `auto_topup_enabled=1` because the account had no saved
    Square/Stripe payment method; current backend already blocks enabling Auto
    Reload without a saved method
  - live verification showed `balance_cents=1500`, `reserved_cents=0`, and
    `auto_topup_enabled=0`
  - Round doc:
    `docs/rounds/ROUND-257-ACTIVE-TEST-ACCOUNT-BALANCE-RESET.md`
- Round 256 fixed the confusing `bluey login` browser handoff where login
  succeeded but the terminal kept waiting:
  - root cause: the pending desktop-link hint lived inside the login card, which
    became hidden after the browser switched to the dashboard
  - added a dashboard-level `Connect desktop` banner for pending `user_code`
    approvals
  - kept explicit approval so pasted/random codes cannot silently bind a signed
    in account to someone else's desktop
  - added clearer CLI waiting/timeout hints in source; installed users will see
    those after the next binary release
  - deployed static web assets live using release-safe rsync excludes
  - live checks confirmed the new dashboard hint/copy is served and
    `/install.sh` still serves the shell installer
  - verification passed:
    - `node --check web/assets/bluey-site.js`
    - `cargo check -p cue-cli`
    - `git diff --check`
  - Round doc:
    `docs/rounds/ROUND-256-DESKTOP-LOGIN-HANDOFF-VISIBILITY.md`
- Round 255 restored the public installer and release files after
  `https://bluey.sh/install.sh` started serving static website HTML:
  - restored `/install.sh`, `/install.ps1`, `/latest.json`,
    `/latest.json.sig`, and `/releases/v0.1.17/*` on the droplet
  - root cause was a web-only deploy/delete path that removed release files,
    causing Caddy to fall back to `index.html`
  - future Bluey.sh deploys should use `scripts/deploy-bluey-sh-manual.sh` or
    explicitly preserve installer, manifest, signature, and `/releases/**`
  - live checks passed:
    - `curl -fsSL https://bluey.sh/install.sh | sed -n '1p'`
    - `curl -fsSL https://bluey.sh/install.sh | bash -n`
    - `curl -fsSL https://bluey.sh/latest.json`
    - `curl -fsSL https://bluey.sh/latest.json.sig | wc -c`
    - release artifact `shasum -a 256 -c SHA256SUMS.txt`
    - temp-home installer smoke, ending with `bluey 0.1.17`
  - Round doc:
    `docs/rounds/ROUND-255-INSTALLER-RELEASE-FILES-RESTORE.md`
  - Current live install entrypoint is working again:
    `curl -fsSL https://bluey.sh/install.sh | bash`
- Round 249 added clearer low/critical balance visual states:
  - `>= $10.00` remains normal
  - `$5.00` through `$9.99` shows orange low-balance warning
  - `< $5.00` shows red critical balance warning
  - macOS collapsed pill dot now uses orange for low/sign-in-needed and red for
    critical balance under `$5`
  - macOS expanded header balance label now uses theme-aware orange/red text
  - dashboard floating balance pill now includes a matching colored dot
  - dashboard settings balance and web account balance KPI now use the same
    orange/red thresholds
  - Windows parity note: `native/windows/cue-overlay/main.c` has no matching
    overlay balance renderer in this round; shared dashboard/web account
    surfaces were updated
  - files changed:
    - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `crates/cue-dashboard/ui/src/components/BalanceIndicator.tsx`
    - `crates/cue-dashboard/ui/src/pages/Settings.tsx`
    - `web/assets/bluey-site.js`
    - `web/assets/bluey-site.css`
  - Round doc:
    `docs/rounds/ROUND-249-BALANCE-WARNING-COLOR-STATES.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `npm --prefix crates/cue-dashboard/ui run build`
    - `git diff --check`
  - Remaining gate: rebuild/hot-install the macOS overlay and deploy web assets
    before expecting live users to see this balance color update.
- Round 248 tightened the end-to-end AnswerPlan, transcript, STT, and smoke-test
  path:
  - added privacy-safe answer diagnostics with question/transcript hashes,
    lengths, transcript source counts, generic-live-prompt detection, plan source,
    provider/model, canvas artifact type/confidence, web-search status, and billing
    cents in server logs
  - added local eval coverage for the live tester prompts: LRU code, Fibonacci
    follow-up/topic reset, tell-me-about-yourself, Secret Passage Ranch, empty
    live-caption prompt, live-caption placeholder, and missing docs
  - fixed AnswerPlan priority so missing docs do not trigger web search and
    system design beats generic queue/cache/database code keywords
  - strengthened code prompt guidance so explicit code asks start with fenced
    working code and follow-ups preserve the existing artifact unless a new one
    is requested
  - macOS and Windows now require meaningful transcript text before Listen output
    can become a paid answer; one-word filler and placeholders are blocked
  - Windows no longer sends the generic live-caption prompt when transcript
    context exists but no usable question is present
  - Windows transcript-derived answer sends now preserve `Mic:`, `System:`, or
    `Audio:` source labels so server transcript diagnostics match macOS
  - daemon managed STT relay reservation default changed from 10 minutes to 120
    seconds, with `BLUEY_MANAGED_STT_RELAY_SECONDS` / `BLUEY_STT_RELAY_SECONDS`
    override and logs for requested/reserved seconds
  - added `scripts/bluey-e2e-staging-smoke.sh` for no-cost local/staging gates and
    opt-in paid provider smoke with `BLUEY_RUN_PAID_SMOKE=1`
  - live test account `codex-smoke-20260608183100@bluey.sh` was credited from
    `$12.68` to `$15.00`; `reserved_cents` remained `0`
  - files changed:
    - `server/src/api/router.rs`
    - `crates/cue-daemon/src/app.rs`
    - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `native/windows/cue-overlay/main.c`
    - `scripts/bluey-e2e-staging-smoke.sh`
  - Round doc:
    `docs/rounds/ROUND-248-ANSWERPLAN-E2E-STT-TRANSCRIPT-SMOKE.md`
  - Verification passed:
    - `bash -n scripts/bluey-e2e-staging-smoke.sh`
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo fmt`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `cargo test --manifest-path server/Cargo.toml answer_plan --quiet`
    - `cargo test --manifest-path server/Cargo.toml web_search --quiet`
    - `cargo test --manifest-path server/Cargo.toml answer_request_diagnostics --quiet`
    - `cargo test --manifest-path server/Cargo.toml generic_live_caption_prompt --quiet`
    - `cargo test -p cue-daemon managed_stt_relay_requested_seconds_defaults_and_clamps --quiet`
    - `cargo test -p cue-daemon stt_relay_websocket_url --quiet`
    - `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -Inative/windows/cue-overlay -DUNICODE -D_UNICODE`
    - `git diff --check`
  - Remaining gate: rebuild/hot-install local macOS binaries and deploy server/daemon
    changes to staging before expecting the live app to show these fixes.
- Round 247 added a hybrid AnswerPlan fallback classifier:
  - deterministic local rules remain the fast path for obvious coding,
    behavioral, system-design, screen, research, and missing-context requests
  - low-confidence or mixed-signal requests can now call a tiny managed
    `instant`-lane classifier before provider route selection
  - the classifier receives only a short sanitized routing prompt, not RAG
    chunks, document text, screenshots, private prompts, or secrets
  - hard overrides keep `tell me about yourself` behavioral, code prompts on
    code artifact/deep routing, and screen/image prompts on vision behavior
  - invalid classifier JSON or unsafe lane/output combinations fall back to the
    local rule plan
  - classifier usage is recorded as `answer_plan_classifier` with customer cost
    `0` so owner cost can be audited without charging users for hidden planning
  - new deploy knobs:
    - `BLUEY_ANSWER_PLAN_AI_FALLBACK=0` disables only the AI fallback
    - `BLUEY_ANSWER_PLAN_AI_CONFIDENCE_THRESHOLD`
    - `BLUEY_ANSWER_PLAN_AI_TIMEOUT_MS`
    - `BLUEY_ANSWER_PLAN_AI_MAX_TOKENS`
  - files changed:
    - `server/src/api/router.rs`
    - `docs/MODEL-ROUTING.md`
    - `ops/bluey-api.env.example`
    - `scripts/bluey-cloud-preflight.sh`
  - Round doc:
    `docs/rounds/ROUND-247-ANSWERPLAN-AI-FALLBACK.md`
  - Verification passed:
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml api::router::tests -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture`
    - `cargo check --manifest-path server/Cargo.toml`
  - Remaining gate: deploy to staging, confirm logs show
    `answer_plan_source`, `answer_plan_ai_attempted`, and
    `answer_plan_ai_reason`, then live-smoke ambiguous prompts before production.
- Round 246 restored the owner's preferred move-anywhere overlay behavior and
  fixed full-screen restore placement:
  - macOS click-through/default mode now keeps real controls clickable while
    blank Bluey surface acts as a drag handle again
  - macOS tooltip/toast copy now says `Move-anywhere on` instead of promising
    strict blank-space pass-through
  - macOS `restoreWindowFromFullSize()` now restores the saved
    `preWindowFullSizeFrame` instead of throwing it away and recalculating the
    default compact frame
  - Windows parity changed blank expanded hit testing from `HTTRANSPARENT` to
    `HTCAPTION`, while controls remain `HTCLIENT` and resize edges remain resize
    hits
  - local installed macOS overlay helpers were refreshed for live testing:
    `~/.bluey/bin/bluey-overlay-macos`,
    `~/.bluey/bin/cue-overlay-macos`, and
    `~/.bluey/bin/BlueyOverlay.app`
  - files changed:
    - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `native/windows/cue-overlay/main.c`
    - `scripts/macos-overlay-visual-smoke.sh`
  - Round doc:
    `docs/rounds/ROUND-246-OVERLAY-MOVE-ANYWHERE-FULLSCREEN-RESTORE.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `bash -n scripts/macos-overlay-visual-smoke.sh`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
  - Manual QA still needed after relaunch:
    - drag from blank center/feed/chrome space while the mode button is active
    - header/composer/control buttons still click
    - move Bluey, enter full screen, restore, and confirm it returns to the
      moved position
- Round 245 hardened provider cooldown and `429` behavior:
  - server-side `Retry-After` parsing now accepts both seconds and HTTP-date
    values
  - OpenAI-compatible, Gemini, and Anthropic stream error frames that signal
    rate-limit, quota, overload, capacity, or `RESOURCE_EXHAUSTED` now become
    typed upstream capacity errors
  - if the first provider stream event is typed capacity, Bluey cools the
    provider/model/key and keeps trying healthy keys/routes before committing
    the answer stream
  - if a selected stream later fails with typed capacity, the SSE error payload
    carries `reason: "provider_key_cooling_down"` and `retry_after_secs`
  - managed desktop stream parsing now maps those capacity payloads to
    `LlmError::CapacityBusy` instead of generic provider errors
  - cloud client `Retry-After` parsing now accepts seconds and HTTP-date values
  - docs updated:
    - `docs/PROVIDER-429-PLAYBOOK.md`
    - `docs/MODEL-ROUTING.md`
  - files changed:
    - `server/src/routing/dispatcher.rs`
    - `server/src/api/router.rs`
    - `crates/cue-llm/src/bluey_managed.rs`
    - `crates/cue-cloud-client/src/client.rs`
    - `crates/cue-cloud-client/Cargo.toml`
  - Round doc:
    `docs/rounds/ROUND-245-PROVIDER-429-COOLDOWN-HARDENING.md`
  - Verification passed:
    - `cargo fmt -p cue-cloud-client -p cue-llm`
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo test -p cue-cloud-client retry_after -- --nocapture`
    - `cargo test -p cue-llm capacity_busy -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml stream_error_frame -- --nocapture`
    - `cargo test -p cue-cloud-client parse_or_err_429_capacity_body_maps_to_capacity_busy -- --nocapture`
    - `cargo test -p cue-llm complete_stream_posts_to_managed_endpoint_and_parses_ndjson -- --nocapture`
    - `cargo check -p cue-cloud-client -p cue-llm`
    - `cargo check --manifest-path server/Cargo.toml`
- Round 244 audited why the current test account did not auto-reload even
  though the CLI said `auto top-up: ON, $30 at <$5`:
  - live `/account/me` for the smoke/test account returned balance `482`,
    threshold `500`, amount `3000`, `auto_topup_enabled: true`, provider
    `square`, environment `production`, and `auto_topup_available: false`
  - the account export showed no saved Stripe customer/payment method and no
    saved Square customer/card, so the account cannot auto-reload until a card
    is saved
  - the server already returned the correct readiness fields; the desktop
    client type and CLI were ignoring them
  - `cue-cloud-client::AccountMe` now deserializes billing readiness fields:
    provider, availability, unavailable reason, saved payment label, Square
    environment, billing restricted, and restriction reason
  - `bluey usage` now prints:
    `ON, setup needed: Save a card for Auto Reload before turning this on.`
    instead of a misleading plain `ON`
  - rebuilt and installed local CLI aliases:
    - `~/.bluey/bin/bluey`
    - `~/.bluey/bin/cue`
  - files changed:
    - `crates/cue-cloud-client/src/types.rs`
    - `crates/cue-cli/src/bluey_cmds.rs`
  - Round doc:
    `docs/rounds/ROUND-244-AUTO-RELOAD-READINESS-DIAGNOSTIC.md`
  - Verification passed:
    - `cargo fmt -p cue-cloud-client -p cue-cli`
    - `cargo test -p cue-cli auto_topup_label -- --nocapture`
    - `cargo check -p cue-cloud-client -p cue-cli`
    - `git diff --check`
    - `cargo build --release -p cue-cli`
    - install local CLI aliases and run `~/.bluey/bin/bluey usage`
  - Remaining gate: save a test Square card, verify
    `auto_topup_available: true`, then trigger a paid low-balance usage event
    and confirm exactly-one Square payment/credit.
- Round 243 fixed the composer/document/interaction defaults reported during
  live overlay testing:
  - macOS empty composer clicks now show an explicit focused blue blinking caret
    beside the placeholder
  - macOS composer surface now shows a blue focus border/shadow when it owns
    input
  - macOS click-through/interactive-off mode is now the default for new overlay
    windows, while controls, composer, resize edges, and header move handles
    remain interactive
  - newly added context files/screens now reveal the attachment strip
    automatically
  - sent attachments remain visible as saved session context instead of needing
    a manual `Show files` click
  - Windows parity: the existing ask box already forces arrow cursor and blank
    overlay space pass-through; Windows now keeps visible context chips after
    send instead of clearing them immediately
  - files changed:
    - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `native/windows/cue-overlay/main.c`
  - Round doc:
    `docs/rounds/ROUND-243-COMPOSER-DOCS-INTERACTION-DEFAULTS.md`
  - Verification passed:
    - `git diff --check`
    - `native/macos/cue-overlay/build.sh`
    - `x86_64-w64-mingw32-gcc -municode -D_WIN32_WINNT=0x0601 -o /tmp/bluey-win-check/bluey-overlay.exe native/windows/cue-overlay/main.c -luser32 -lgdi32 -ld2d1 -ldwrite -luuid -lshell32 -lcomctl32`
  - Local install/restart completed:
    - installed rebuilt `bluey-overlay-macos`, `cue-overlay-macos`, and
      `BlueyOverlay.app` into `~/.bluey/bin`
    - restarted with `~/.bluey/bin/bluey off && ~/.bluey/bin/bluey on`
    - post-restart status: daemon pid `59401`, meeting id
      `56431f96-80c0-438f-94c0-e650e9b17064`, `overlay_visible: true`,
      `overlay_capture_excluded: true`
  - Remaining gate: live-test the focus caret, auto-visible documents, send
    retention, and click-through controls before shipping.
- Round 242 made AnswerPlan routing default-on and improved deploy/preflight
  visibility:
  - `BLUEY_ANSWER_PLAN_ROUTING` now defaults to enabled in server code
  - `BLUEY_ANSWER_PLAN_ROUTING=0`/`false`/`no`/`off` is the rollback switch
  - `BLUEY_ROUTE_POLICY` still defaults to `provider_mix`, which is the safe
    anti-429 default across configured flagship providers
  - `BLUEY_ROUTE_POLICY=cost_optimized` remains available for owner-controlled
    GLM/DeepSeek-first testing
  - `ops/bluey-api.env.example` now makes the intended deploy posture explicit
  - `scripts/bluey-cloud-preflight.sh` reports AnswerPlan state and route
    policy, and fails if `cost_optimized` is selected without any Z.AI or
    DeepSeek key pool
  - `scripts/bluey-scalable-readiness.sh` reports optional Z.AI/DeepSeek route
    capacity readiness
  - deploy docs updated:
    - `docs/MODEL-ROUTING.md`
    - `docs/deploy/PHASE3-SERVER-DEPLOY.md`
    - `docs/deploy/BLUEY-SH-LAUNCH.md`
  - Round doc:
    `docs/rounds/ROUND-242-ANSWERPLAN-DEFAULT-AND-PREFLIGHT.md`
  - Remaining gates: deploy server binary, run cloud preflight on the droplet,
    then live-smoke Round 241 prompts.
- Round 241 implemented `BLUEY_ANSWER_PLAN_ROUTING=1` as a server-side
  AnswerPlan lane-promotion gate before managed provider routing:
  - the planner is deterministic local rules first, not an AI classifier call
  - it classifies quick, coding, coding follow-up, behavioral, system design,
    screen, research, missing context, writing, meeting, and general intents
  - when enabled, it can promote Auto/balanced traffic to `instant`, `deep`, or
    `vision` before the existing dispatcher applies `BLUEY_ROUTE_POLICY`
  - code/code-follow-up prompts now carry prompt guidance to include actual
    fenced code instead of vague summaries
  - self-intro/interview prompts route as behavioral and explicitly avoid
    system-design treatment
  - bare public lookup phrases such as `secret passage ranch` are eligible for
    research/web-search planning when no saved context applies
  - Mac/Windows parity: server-side managed routing benefits both clients once
    deployed to the shared `bluey-server`
  - `BLUEY_ROUTE_POLICY=cost_optimized` remains available and still applies
    after AnswerPlan chooses the lane
  - docs updated:
    - `docs/MODEL-ROUTING.md`
  - Round doc:
    `docs/rounds/ROUND-241-ANSWERPLAN-ROUTING-GATE.md`
  - Verification passed:
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo test --manifest-path server/Cargo.toml api::router::tests -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture`
    - `cargo check --manifest-path server/Cargo.toml`
    - secret-fragment scan over `server docs scripts`
  - Remaining gates: deploy with `BLUEY_ANSWER_PLAN_ROUTING=1` in staging,
    live-smoke the exact prompts listed in the round doc, compare answer-plan
    and provider logs, then canary production.
- Round 240 added a release artifact secret guard after the owner asked whether
  deployed API keys can stay server-side and out of downloadable binaries:
  - confirmed intended architecture: provider keys live in `bluey-server`
    runtime env/secrets, not Mac/Windows/Linux release artifacts
  - `scripts/publish-bluey-release.sh` now scans release artifacts for actual
    configured provider secret values from the publish environment
  - the scan covers AI/STT/web-search/object-storage/billing secret env vars,
    including OpenAI, Anthropic, Gemini/Google, DeepSeek, Z.AI/Zhipu, Deepgram,
    Tavily/Brave, R2/S3, Square, and Stripe
  - scan failures name only the env var label, not the secret value
  - verified with a clean fake artifact and a fake `DEEPSEEK_API_KEY` leak
  - Round doc:
    `docs/rounds/ROUND-240-RELEASE-SECRET-ARTIFACT-GUARD.md`
- Round 239 made managed answer routing default to `provider_mix` so Bluey uses
  all configured flagship providers as needed without concentrating the first
  burst on one upstream:
  - streaming and non-streaming answer paths now pass `request_id` into route
    candidate resolution
  - the dispatcher rotates the top-tier route list deterministically by request
    id
  - text lanes can now start across Anthropic, DeepSeek, Gemini, OpenAI, and
    Z.AI GLM when keys are configured
  - vision rotates only across image-capable OpenAI/Gemini routes
  - existing provider/model buckets, key health cooldowns, 429/529 retry-after
    handling, balance checks, and upstream spend guards remain in place
  - operators can force the older static order with
    `BLUEY_ROUTE_POLICY=quality_first`
  - operators can still smoke GLM/DeepSeek-first behavior with
    `BLUEY_ROUTE_POLICY=cost_optimized`
  - docs updated:
    - `docs/MODEL-ROUTING.md`
    - `docs/PRICING-MODEL.md`
  - Round doc:
    `docs/rounds/ROUND-239-PROVIDER-MIX-ANTI-429-ROUTING.md`
- Round 238 added an explicit server route policy to use cheaper GLM/DeepSeek
  text routes first. Its "default remains quality-first" note is superseded by
  Round 239, where the new default became provider-mix:
  - root cause: GLM-5.2 and DeepSeek were wired/priced but placed after
    Anthropic/OpenAI in the quality-first candidate order, so they mostly acted
    as fallbacks
  - previous default remained quality-first at the time of Round 238
  - `BLUEY_ROUTE_POLICY=cost_optimized` or
    `BLUEY_ROUTE_ORDER=cost_optimized` makes:
    - `instant` start with DeepSeek Flash
    - `balanced` start with Z.AI GLM-5.2, then DeepSeek Flash
    - `deep` start with Z.AI GLM-5.2, then DeepSeek V4 Pro
    - `vision` remain unchanged on OpenAI/Gemini because GLM/DeepSeek are only
      wired for text/chat in this codebase
  - docs updated:
    - `docs/MODEL-ROUTING.md`
    - `docs/PRICING-MODEL.md`
  - security note: pasted provider keys must be rotated before production and
    stored only in server env/secrets, not repo/docs
  - Round doc:
    `docs/rounds/ROUND-238-COST-OPTIMIZED-GLM-DEEPSEEK-ROUTING.md`
  - Verification passed:
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml pricing -- --nocapture`
    - `cargo check --manifest-path server/Cargo.toml`
    - `git diff --check`
  - Remaining gates: rotate keys, configure rotated `ZAI_API_KEY(S)` and
    `DEEPSEEK_API_KEY(S)` on staging/prod, live-smoke instant/balanced/deep and
    vision, compare quality/latency, then canary before broad rollout
- Round 237 calculated per-question/per-row billing averages from a fresh
  account export:
  - `bluey usage` at the time showed `474` billable rows/cues and `$7.52`
    spent over the last 7 days
  - average across every billable row: `1.59c` customer charge and `0.42c`
    estimated provider cost
  - average for actual LLM answer rows only: `7.33c` customer charge and
    `2.57c` estimated provider cost
  - normal balanced text answers: `42` rows, average `4.07c` charged and
    `1.20c` provider estimate
  - screen/vision answers: `16` rows, average `15.88c` charged and `6.16c`
    provider estimate
  - Listen/STT rows: `74` rows, average `1.96c` per lane row; with mic+system
    active, a typical Listen window is roughly `3.92c` before the answer
  - embeddings/indexing: `342` rows, average `0.53c` charged while estimated
    upstream cost is tiny, about `$0.0046` total for those rows
  - practical examples:
    - normal typed answer: about `4c`
    - screen/image answer: about `16c`
    - spoken Listen + normal answer: about `8c`
    - spoken Listen + screen answer: about `20c`
  - temporary export `bluey-export-20260629-231921.json` was removed and not
    committed
  - Round doc: `docs/rounds/ROUND-237-BILLING-PER-QUESTION-AVERAGES.md`
- Round 236 fixed the live partial caption Enter path and the bottom transcript
  rail tailing behavior:
  - owner pressed Enter while the bottom rail showed a live partial caption, but
    the sent Question card used the generic live-caption instruction and then
    hit the provider-error fallback
  - daemon status showed `transcript_segments: 0` while the overlay still had
    `Mic: ...` in its live rail, so the caption existed only in overlay partial
    memory and not yet in finalized daemon transcript state
  - macOS now recovers answer text from latest live line, live lines by source,
    merged preview bodies, and the rail text before falling back
  - manual Enter/Answer now sends the actual bounded caption text when
    available instead of the generic live-caption instruction
  - auto-send-after-stop also sends bounded caption text for longer captions
  - duplicate suppression, "has transcript context", and current-session
    emptiness now include preview-only caption memory
  - `ask_answer_sent` / duplicate-suppressed lifecycle logs include
    `preview_transcript_context` so partial-caption sends can be diagnosed
    without logging personal transcript text
  - the rail now follows the newest caption immediately, after the next run
    loop, and after layout/resizing; it also measures attributed text width and
    allows horizontal elasticity
  - Windows parity: longer live transcript sends now use the bounded transcript
    text instead of the generic live-caption prompt when the short visible
    question is unavailable
  - Round doc: `docs/rounds/ROUND-236-LIVE-PARTIAL-ENTER-RAIL-TICKER.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
    - `git diff --check`
  - Current local visible QA status after restart: daemon pid `33583`, active
    meeting id `ea11ad65-014f-419c-adbd-0b4a4ed64e7b`, overlay visible `true`,
    overlay capture excluded `false`, transcript segments `0`, context items
    `0`
  - Important: before release/upload, return to normal mode and verify
    `overlay_capture_excluded: true`
  - Separate remaining backend/cloud gate: logs are still showing many cloud
    object-sync failures such as `object sync is not configured`, `503 Service
    Unavailable`, `cloud object upload failed; continuing with text sync`, and
    occasional `sync failed` / `500 Internal Server Error`
- Round 235 audited the `$5.28` to `$4.82` balance-drop concern and provider
  cost versus customer charge:
  - active account is linked to `https://bluey.sh`, user
    `codex-smoke-20260608183100@bluey.sh`, balance `$4.82`
  - `bluey usage` reported last 7 days: `484` cues and `$7.72` customer spend
  - account export includes customer usage rows but does not include
    `cost_cents_to_bluey`, so provider actuals were estimated from exported
    provider/model/token/audio rows using `server/src/pricing/mod.rs`
  - newest usage rows adding to `46` customer cents were mostly dual-source
    Deepgram STT rows plus recent Anthropic balanced LLM calls, so the visible
    drop was accumulated settled usage rather than one single 50-cent call
  - last 7 days estimated provider actual: about `$2.09` versus `$7.72`
    customer charge
  - all exported usage estimated provider actual: about `$2.90` versus `$15.80`
    customer charge
  - product gap: add an admin-only usage-cost report that exposes customer
    charge, stored `cost_cents_to_bluey`, raw provider actual estimate,
    provider/model breakdown, balance ledger, and STT reservation/settlement
    rows without requiring account exports
  - Auto Reload note: balance is below `$5` and CLI says Auto Reload is ON at
    `$30` under `$5`; if it does not run, audit reload worker/idempotency next
  - Round doc: `docs/rounds/ROUND-235-BILLING-USAGE-MARGIN-AUDIT.md`
- Round 234 fixed the visible Question card for short spoken Listen asks and
  added privacy-safe stream timing diagnostics:
  - owner spoke "Explain LRU cache", but the visible Question card showed the
    generic "Answer the latest live captions..." instruction even though the
    model used the transcript correctly
  - root cause was the Round 230 safety change that stopped raw long
    transcripts from becoming giant Question bubbles, but it also hid short
    clear spoken asks behind the generic prompt
  - macOS now shows the actual short live-caption question when it is safe:
    `<= 220` characters, `<= 2` non-empty lines, and not a placeholder
  - long or messy live transcript sends still use the compact generic prompt so
    the chat is not flooded by raw transcript blocks
  - auto-send-after-stop uses the same visible-question rule
  - Windows has parity for empty-input transcript sends
  - ask lifecycle logs now include `generic_live_prompt`
  - macOS answer-card logs now include privacy-safe
    `answer_stream_first_update` and `answer_stream_finished` timing events
  - server managed streaming logs now include provider first-event latency and
    first-event kind
  - Round doc: `docs/rounds/ROUND-234-LIVE-CAPTION-VISIBLE-QUESTION-TIMING.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo check --manifest-path server/Cargo.toml`
    - `native/macos/cue-overlay/build.sh`
    - `cargo test --manifest-path server/Cargo.toml first_token_deadline -- --nocapture`
  - Current local visible QA status after restart: daemon pid `89067`, active
    meeting id `3a84f982-4c7d-4df5-8a3c-0062bec9a8cb`, overlay visible `true`,
    overlay capture excluded `false`, transcript segments `3`, context items
    `0`
  - Remaining QA: speak a short ask and confirm the visible Question card uses
    the short spoken text; speak a long rambling ask and confirm Bluey keeps the
    card compact; before release/upload return to capture-excluded mode and
    verify `overlay_capture_excluded: true`
- Round 233 added optional managed flagship model routes for Z.AI GLM-5.2 and DeepSeek:
  - official provider docs checked:
    - Z.AI pricing/model docs at `https://docs.z.ai/guides/overview/pricing`
    - Z.AI GLM-5.2 docs at `https://docs.z.ai/guides/llm/glm-5.2`
    - DeepSeek pricing docs at `https://api-docs.deepseek.com/quick_start/pricing`
    - DeepSeek chat completion docs at `https://api-docs.deepseek.com/api/create-chat-completion`
  - added server-side key pools:
    - `DEEPSEEK_API_KEYS` / `DEEPSEEK_API_KEY`
    - `ZAI_API_KEYS` / `ZAI_API_KEY`
    - `ZHIPU_API_KEYS` / `ZHIPU_API_KEY` as GLM/Z.AI aliases
  - added first-class managed provider names `deepseek` and `zai`, not mislabeled as OpenAI in logs/billing
  - added OpenAI-compatible dispatch to:
    - DeepSeek `deepseek-v4-pro`
    - DeepSeek `deepseek-v4-flash`
    - Z.AI `glm-5.2`
  - added route candidates:
    - `instant`: DeepSeek flash after OpenAI fast
    - `balanced`: DeepSeek flash, then Z.AI GLM-5.2
    - `deep`: Z.AI GLM-5.2, then DeepSeek V4 Pro, plus DeepSeek flash as later fallback
    - `vision`: unchanged because these are text/chat routes in this pass
  - added conservative cache-miss pricing rows:
    - Z.AI `glm-5.2`: `$1.40/1M` input, `$4.40/1M` output, `150%` markup
    - DeepSeek `deepseek-v4-pro`: `$0.435/1M` input, `$0.87/1M` output, `150%` markup
    - DeepSeek `deepseek-v4-flash`: `$0.14/1M` input, `$0.28/1M` output, `200%` markup
  - added provider capacity buckets:
    - `BLUEY_LIMIT_PROVIDER_DEEPSEEK_LLM_PER_MIN`
    - `BLUEY_LIMIT_PROVIDER_ZAI_LLM_PER_MIN`
  - non-deep DeepSeek/Z.AI lanes send thinking disabled; deep can enable provider reasoning, and reasoning text is not surfaced in the overlay
  - if an OpenAI-compatible stream omits final usage, Bluey now uses the server input estimate plus a conservative output character estimate instead of charging zero
  - source-of-truth docs updated:
    - `docs/PRICING-MODEL.md`
    - `docs/MODEL-ROUTING.md`
  - Round doc: `docs/rounds/ROUND-233-FLAGSHIP-MODEL-ROUTES.md`
  - Verification passed:
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo fmt --all`
    - `cargo check --manifest-path server/Cargo.toml`
    - `cargo test --manifest-path server/Cargo.toml pricing -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml routing::dispatcher -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml rate_limit -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml config::tests -- --nocapture`
    - `cargo test --manifest-path server/Cargo.toml` (`179` unit tests, `41` integration tests, and doc-tests passed)
  - Remaining QA/gates: configure real `DEEPSEEK_API_KEY(S)` and `ZAI_API_KEY(S)`, run live smoke tests on staging/production, then compare provider dashboard usage with Bluey's usage ledger. Future improvement: add cache-hit/cache-miss token split accounting for DeepSeek/Z.AI cached inputs.
- Round 232 fixed rapid Listen on/off STT reservation churn and misleading balance drops:
  - owner clicked Listen on/off repeatedly and saw balance move from about `$5.28` to `$4.68` within a few seconds
  - root cause was layered: one dual-source Listen can reserve about `56` cents for 10 minutes of Deepgram relay capacity, macOS could emit repeated start events while still `Starting`, daemon start was not idempotent, relay sources reserved before first audio bytes, zero-audio relay settlements rounded up to one second, and balance could refresh before relay settlement finished
  - daemon audio runtime now has `starting` plus `start_generation`; duplicate starts while starting/active are ignored and stop invalidates a pending start generation
  - live relay sources now start the native helper first and reserve a server STT session only after the first PCM bytes arrive
  - if the user stops before audio bytes arrive, no cloud STT session is reserved
  - added authenticated `/stt/session/cancel` plus a cloud-client method so the daemon can release a just-created reservation if websocket open fails before streaming
  - server relay settlement now tracks forwarded audio bytes/chunks and settles zero-audio sessions with `0` billable seconds and full reservation refund
  - daemon refreshes balance after relay source settlement and also after late relay settlement
  - macOS Listen button and pill toggle now debounce/guard in-flight start/stop states
  - Windows record button now has bounce protection and a short restart guard after Stop
  - privacy-safe logs were added for hashed account id, source, bytes/chunks, reserved/settled/refunded cents, and close reason, without transcript text
  - Round doc: `docs/rounds/ROUND-232-LISTEN-STT-RESERVATION-GUARDS.md`
  - Verification passed:
    - `cargo fmt --all`
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo check -p cue-daemon`
    - `cargo check --manifest-path server/Cargo.toml`
    - `cargo test --manifest-path server/Cargo.toml db::stt_accounting::tests -- --nocapture`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo build -p cue-cli -p cue-daemon`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
  - Current local visible QA status after restart: daemon pid `54661`, active meeting id `3cce79e2-6031-406a-a7bc-2d9eb3c83e96`, overlay visible `true`, overlay capture excluded `false`, transcript segments `0`, context items `0`
  - Remaining QA: rapidly click Listen on/off and confirm no repeated start sessions; normal short speech should settle to the actual small STT charge, not the 10-minute reservation; before release/upload return to capture-excluded mode and verify `overlay_capture_excluded: true`
- Round 231 fixed document attach attempts getting stuck on `Indexing...` and improved attachment visibility:
  - owner attached a document, saw `Indexing...`, and could not see the document afterward
  - active local status showed `context_items: 0`, and `active-meeting.json` had `context: []`, so the current session did not retain an attached document
  - `handle_attach_paths` returned early when the picker returned no paths, which left the macOS overlay in the optimistic indexing placeholder because no fresh `set_context_items` was sent
  - all-skipped/all-failed attach attempts also had an early return that did not refresh the context list
  - added a daemon helper to always refresh the current overlay context list
  - empty/canceled attach attempts and all-skipped/all-failed attempts now send a fresh context list and refresh sessions before returning
  - macOS now shows saved attached files by default when there are no pending attachment chips, and after sending pending attachments it shows saved conversation files instead of clearing the strip completely
  - daemon refresh fix applies to both macOS and Windows; Windows does not have the same macOS pending/saved attachment strip UI
  - Round doc: `docs/rounds/ROUND-231-ATTACH-INDEXING-VISIBLE-RESET.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon overlay_context_items --lib`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
  - Current local visible QA status after restart: daemon pid `16450`, active meeting id `93e731e4-a161-4f83-8f98-6019c86bfb92`, overlay visible `true`, overlay capture excluded `false`, transcript segments `0`, context items `0`
  - Remaining QA: attach a known small file and confirm `Indexing...` becomes a visible chip and `context_items` increments; cancel picker and confirm overlay returns to `Docs empty`; before release/upload return to capture-excluded mode and verify `overlay_capture_excluded: true`
- Round 230 bounded the live-caption rail and stopped raw live captions from becoming giant visible Question bubbles:
  - owner showed a long Listen transcript becoming a bulky repeated Question card after pressing Enter
  - macOS already had a transcript `NSScrollView`, but it felt like a clipped one-line caption because scroll-wheel events were not forwarded to it and the scrollbar auto-hidden
  - macOS live caption rail now forwards scroll-wheel events to the transcript scroll view, keeps the horizontal scroller available, caps rail display to the latest 520 characters, and caps preview memory to 1400 characters
  - macOS `composedQuestionForAnswer` no longer pastes live transcript text into the visible question; empty live-caption sends now use a short "Answer the latest live captions from the current session transcript..." intent
  - typed asks stay as typed asks while the daemon supplies recent transcript through the existing `active meeting transcript` answer-context path
  - duplicate ask suppression now includes a private compact transcript fingerprint so different spoken asks are not suppressed as the same short prompt
  - transcript merge overlap was widened and near-identical revisions are coalesced before appending
  - Windows empty-input transcript sends now use the same live-caption intent when transcript context exists; Windows already uses a capped local transcript preview buffer and does not have the macOS horizontal rail
  - transcript text is not uploaded through the R2/artifact object endpoint; artifacts/screenshots use `/sync/artifacts/:artifact_id/object`, while transcripts sync via `/sync/batch` into cloud transcript tables/RAG
  - Round doc: `docs/rounds/ROUND-230-LIVE-CAPTION-RAIL-SEND-BOUNDS.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `native/macos/cue-overlay/build.sh`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
  - Current local QA status after the change: daemon pid `77359`, active meeting id `259b7fee-a3a2-478a-9061-325aedf31dcd`, transcript segments `12`, overlay visible `false`, overlay capture excluded `false`, screen capture active `false`
  - Remaining gate: restart the local visible QA overlay from the rebuilt macOS overlay binary before live-testing the rail, and before any release/upload return to capture-excluded mode and verify `overlay_capture_excluded: true`
- Round 229 fixed the strongest live-transcript duplication path and added better safe diagnostics:
  - active visible QA status had `transcript_segments: 0`, so current no-transcript cases are before answer generation
  - local settings have both system and microphone audio enabled, which can capture the same speech twice when the mic hears speaker audio
  - backend duplicate detection previously caught exact duplicates but not near-identical mic/system echoes
  - backend final transcript handling sent both `TranscriptFinal` and a second transcript `PushCard`, which macOS could display as duplicated/stiched transcript UI
  - final transcript display now uses the `TranscriptFinal` event path only; daemon no longer pushes an extra transcript feed card for the same final text
  - `add_audio_transcript_segment` now returns whether a final was actually stored, so skipped duplicates do not inflate emitted transcript metrics
  - added fuzzy near-duplicate detection that catches high-overlap echoes while keeping real longer continuations
  - added privacy-safe live STT relay diagnostics for source start, periodic audio chunks, and transcript event shape without raw transcript text
  - shared daemon behavior applies to both macOS and Windows overlays; no native overlay code changed
  - Round doc: `docs/rounds/ROUND-229-LIVE-STT-DUPLICATE-DIAGNOSTICS.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-daemon duplicate_transcript_detection --lib`
    - `cargo test -p cue-daemon --lib` (`277 passed`, `2 ignored`)
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status` reports daemon pid `77359`, overlay visible `true`, overlay capture excluded `false`, screen capture active `false`, and transcript segments `0` until Listen receives final STT
- Round 228 fixed stale-context anchoring for standalone new-topic questions:
  - owner showed Fibonacci follow-up context bleeding into a new LRO/LRU cache question
  - root cause was that daemon answer context always added the last 10 Bluey Q&A turns
  - `answer_context_from_meeting` now receives the latest question and conditionally includes recent Bluey Q&A
  - recent Q&A is kept for true follow-ups with explicit references such as "this", "that", "the code", "previous", or "what about"
  - recent Q&A is skipped for standalone named-topic requests like "explain LRU cache" when the user did not explicitly ask to compare or continue
  - topic matching ignores filler/ASR noise and normalizes `lro` to `lru`
  - provider instructions now say standalone new topics should be answered directly and not connected to prior session context unless requested
  - shared daemon behavior applies to both macOS and Windows overlays; no native overlay code changed
  - Round doc: `docs/rounds/ROUND-228-TOPIC-SHIFT-CONTEXT-GUARD.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-daemon meeting_context_ --lib`
    - `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
    - `cargo test -p cue-daemon follow_up_context_ --lib`
    - `cargo test -p cue-daemon --lib` (`275 passed`, `2 ignored`)
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status` reports daemon pid `59487`, overlay visible `true`, overlay capture excluded `false`, and screen capture active `false`
- Round 227 fixed accidental canvas/window expansion from rapid double clicks on the sidebar/canvas icon:
  - added a short rapid-repeat guard for the History toggle
  - added a short rapid-repeat guard for the canvas/sidebar toggle
  - when canvas opens from the header button, canvas full-window expansion is suppressed for 450ms so a second physical click cannot land on newly exposed canvas expand chrome
  - normal single-click behavior remains unchanged
  - Windows does not have the same macOS sidebar/canvas header interaction path; Windows syntax check passed for parity coverage
  - Round doc: `docs/rounds/ROUND-227-CANVAS-SIDEBAR-DOUBLE-CLICK-GUARD.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status` reports daemon pid `41825`, overlay visible `true`, overlay capture excluded `false`, and screen capture active `false`
- Round 226 removed the answer-card keyboard/paste action:
  - macOS no longer renders the answer-card paste action button
  - removed the macOS `PasteCardButton` class, keyboard icon setup, paste-click handler, paste-success flash, and parent callback chain
  - kept normal copy and canvas/code artifact actions
  - left daemon `paste_text_requested` protocol handling intact for compatibility with older clients/tests
  - Windows now hides/disables the native `Paste answer` button and no longer mentions it in help text
  - Round doc: `docs/rounds/ROUND-226-REMOVE-ANSWER-PASTE-KEYBOARD-ACTION.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status` reports daemon pid `34112`, overlay visible `true`, overlay capture excluded `false`, and screen capture active `false`
- Round 225 fixed I-beam cursor leakage inside the overlay:
  - macOS text-view and text-field subclasses now force arrow cursor on cursor updates, mouse move, and mouse drag after AppKit runs
  - macOS answer body labels now use the arrow-cursor text field subclass while remaining selectable/copyable
  - macOS overlay window now reapplies a top-level cursor policy after mouse/cursor dispatch: resize cursor on resize edges, arrow inside interactive Bluey surfaces, and no forced cursor when passthrough leaves the pointer to the app behind Bluey
  - macOS composer caret now stays on the themed blue accent
  - macOS Tone/rename field editors also receive the themed blue insertion point
  - Windows edit control now returns an arrow cursor instead of an I-beam
  - Windows main client-area cursor handling now explicitly returns arrow while preserving resize cursors
  - Windows custom blue caret was not added; the current Win32 edit control uses the platform caret and a custom colored caret would require a larger owner-drawn input change
  - Round doc: `docs/rounds/ROUND-225-OVERLAY-ARROW-CURSOR-INPUT-CARET.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status` reports daemon pid `8782`, overlay visible `true`, overlay capture excluded `false`, and screen capture active `false`
- Round 224 added privacy-safe diagnostics for the current live QA issues:
  - daemon now logs `session_list_requested`, session refresh elapsed time, session counts, context/image totals, active count, and overlay send failures
  - history replay now logs rebuilt card count, question/answer card counts, restored artifact count, inferred artifact count, attachment-chip count, transcript fallback, and empty-session fallback
  - history hydration now logs pushed versus failed card delivery
  - answer persistence now logs saved answer shape, code-fence count, visible context count, attachment count, artifact type, confidence, and artifact body length
  - direct transcript add and audio/STT transcript paths now log duplicate skips and stored-segment metadata without raw text
  - transcript clear now logs cleared transcript/action/decision counts or already-clear state
  - macOS overlay now emits safe lifecycle events for History drawer opened, sessions rendered, transcript buffer consumed/skipped/cleared, auto-send sent/skipped, and manual Ask sent/skipped
  - Windows overlay now emits matching safe lifecycle events for transcript clear and manual Ask send
  - daemon only prints lifecycle detail for the new safe diagnostic stages; raw question, answer, transcript, code, filenames, URLs, and source titles remain out of logs
  - this round is diagnostics-only; it does not change answer quality, billing, web search, click-through, or STT accuracy directly
  - Round doc: `docs/rounds/ROUND-224-PRIVACY-SAFE-LIVE-QA-DIAGNOSTICS.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon overlay_history_cards_restore_code_artifact_button --lib`
    - `cargo test -p cue-daemon overlay_history_cards_replay_saved_conversation --lib`
    - `cargo test -p cue-daemon duplicate_transcript_detection --lib`
    - `cargo test -p cue-daemon answer_overlay_artifact --lib`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `cargo build -p cue-cli --bin bluey`
    - `git diff --check`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status` reports daemon pid `96701`, overlay visible `true`, overlay capture excluded `false`, and screen capture active `false`
    - `cargo test -p cue-daemon --lib` (`273 passed`, `2 ignored`)
- Round 223 fixed the History drawer getting stuck on `Loading...`:
  - expected behavior is local history should usually render in under a second; more than a couple seconds means the overlay missed a session refresh or daemon reply
  - root cause was that opening the macOS History drawer showed `Loading...` but did not explicitly request a fresh session list
  - it depended on daemon startup `set_sessions`, so a missed/delayed startup push could leave the drawer waiting
  - added shared protocol event `session_list_requested`
  - macOS now emits `session_list_requested` every time the History drawer opens
  - daemon handles `SessionListRequested` by refreshing overlay sessions and sending `set_sessions`
  - Windows currently uses a session prompt rather than this drawer; shared protocol supports the event and Windows syntax check passed
  - local Bluey is currently running in visible QA mode after verification; final status reports daemon pid `75048`, `overlay_capture_excluded: false`, and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-223-HISTORY-LOAD-REFRESH-EVENT.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-core session_list_event_serializes --lib`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `cargo test -p cue-daemon overlay_history_cards_replay_saved_conversation --lib`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `git diff --check`
    - `cargo build -p cue-cli --bin bluey`
    - `cargo test -p cue-core --lib`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
- Round 222 fixed restored-chat code artifacts disappearing after switching chats:
  - root cause was that live answer cards could carry `CueCardArtifact` metadata, but saved `ConversationTurn` records only stored question/answer/source/provider/attachments/timestamp
  - when a user switched to another chat and returned, history replay rebuilt the answer card without the `{}` code/canvas artifact action
  - `ConversationTurn` now has optional `artifact: CueCardArtifact` with serde defaults for old sessions
  - answer completion now persists the same visible answer body plus the inferred or managed artifact
  - history replay restores `turn.artifact` onto answer cards and can infer code artifacts from fenced code for older saved turns
  - cloud sync fallback now preserves conversation artifact fields when turns are uploaded as cue responses and restores them during hydration
  - code detection now accepts small explicit assignment snippets like Python tuple swap so tiny interview answers remain code artifacts
  - this is shared core/daemon/history/sync behavior for Mac and Windows; no native UI fork was needed
  - local Bluey is currently running in visible QA mode after verification; final status reports daemon pid `60912`, `overlay_capture_excluded: false`, and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-222-HISTORY-CODE-ARTIFACT-RESTORE.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-daemon overlay_history_cards_restore_code_artifact_button --lib`
    - `cargo test -p cue-daemon overlay_history_cards_infer_code_artifact_for_old_saved_turns --lib`
    - `cargo test -p cue-daemon conversation_sync_preserves_code_artifact_fields --lib`
    - `cargo test -p cue-daemon answer_overlay_artifact --lib`
    - `cargo test -p cue-daemon overlay_history_cards_replay_saved_conversation --lib`
    - `cargo test -p cue-core --lib`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `cargo test -p cue-daemon --lib`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `git diff --check`
- Round 221 fixed explicit code requests that produced prose-only or vague chat answers:
  - owner showed a live QA case where "I want the code, in Python" produced prose in chat while only a small code snippet appeared in canvas
  - daemon output-format prompt now says explicit requests for code, a program, an implementation, "I want the code", or "write code in language" must include a complete fenced code block with a language tag
  - small standalone coding tasks must include the full runnable snippet directly in chat, not only prose or a canvas artifact
  - Code mode and General mode now repeat the explicit-code rule
  - added a runtime fallback for managed code artifacts: if a code artifact exists but visible chat has no fenced code, extract the first real code section from the artifact, infer a simple language tag, and append a compact fenced preview to the visible answer
  - this is shared daemon/backend behavior for Mac and Windows; no native UI fork was needed
  - local Bluey is currently running in visible QA mode after verification; final status reports daemon pid `40009`, `overlay_capture_excluded: false`, and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-221-CODE-REQUEST-VISIBLE-SNIPPET.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-daemon code_artifact_adds_preview_when_chat_body_is_vague --lib`
    - `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
    - `cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes --lib`
    - `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions --lib`
    - `cargo test -p cue-daemon answer_overlay_artifact --lib`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
- Round 220 fixed web-search unavailable behavior, added stronger search safety rails, and polished overlay actions/loading/cursors:
  - root cause for "secret passage ranch" not using web search was that the managed server has a web-search lane but still requires real provider configuration; when not configured, search skipped and the final prompt did not explicitly tell the model web search was unavailable
  - AnswerPlan prompt now tells the model when web search was attempted but returned no usable sources, so it must not imply search succeeded and should say web search was unavailable when public/current info is required
  - paid web search now preflights credits before calling the search provider
  - trial web search keeps a durable daily cap
  - paid accounts remain credit-metered instead of a low daily count, but now have a configurable durable hourly safety rail: `BLUEY_WEB_SEARCH_ACCOUNT_HOURLY_LIMIT`, default `120`, `0` disables
  - added a configurable short-window burst guard before provider calls: `BLUEY_WEB_SEARCH_BURST_LIMIT`, default `12`, and `BLUEY_WEB_SEARCH_BURST_WINDOW_SECS`, default `60`
  - repeated-identical-query guard remains in place
  - customer-facing skipped labels remain neutral and avoid internal abuse/fraud/scraping wording
  - macOS answer-card paste icon changed from confusing `A|`/text-cursor symbol to a keyboard-style icon
  - macOS History drawer now renders `Loading...` until the first session list arrives
  - macOS composer/canvas/answer-style/rename text areas now use arrow cursor rects so the overlay does not show an I-beam cursor while hovering
  - local Bluey is currently running in visible QA mode after verification; final status reports daemon pid `26805`, `overlay_capture_excluded: false`, and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-220-WEB-SEARCH-GUARDS-OVERLAY-POLISH.md`
  - Verification passed:
    - `cargo fmt --manifest-path server/Cargo.toml`
    - `cargo test --manifest-path server/Cargo.toml web_search --lib`
    - `cargo test --manifest-path server/Cargo.toml answer_plan_prompt_explains_unavailable_web_search --lib`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `git diff --check`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
- Round 219 fixed local history visibility, answer-card action clarity, repeated code-request behavior, and STT clear-copy clarity:
  - root cause for an empty History drawer was that current Bluey data existed under `~/Library/Application Support/bluey`, while older local recordings still lived under legacy `~/Library/Application Support/cue`
  - `MeetingStore` now bridges current Bluey and legacy Cue local active/archive meeting records for `all_meetings`, `last_meeting`, `load_by_id`, `rename`, and `delete`
  - history results are newest-first and deduped by meeting id; new writes still stay in the current Bluey store
  - macOS answer cards now expose a distinct open-canvas button for answer artifacts, alongside copy and paste-into-behind-app actions
  - the paste action now uses a text-cursor style icon when available instead of the confusing download-like icon
  - backend answer-shape rules now tell repeated build/implement/write requests to show or regenerate the implementation instead of saying it is already above
  - transcript-clear tooltip now clarifies that clearing captions affects the next answer context, while already transcribed cloud audio may still count as used
  - local Bluey is currently running in visible QA mode after verification; final status reports daemon pid `2855`, `overlay_capture_excluded: false`, and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-219-HISTORY-CANVAS-STT-CLARITY.md`
  - Verification passed:
    - `git diff --check`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-daemon meeting_store_reads_legacy_cue_history_from_bluey_store --lib`
    - `cargo test -p cue-daemon storage::security_tests::meeting_store --lib`
    - `cargo test -p cue-daemon mode_instructions --lib`
    - `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
    - `cargo test -p cue-daemon answer_overlay_artifact --lib`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
- Round 218 fixed visible-mode startup and saved-session dashboard boot:
  - root cause for `overlay_capture_excluded: true` after visible helper was that the prior release pass built the macOS overlay in release mode, which compiles out the debug visible-capture gate
  - `scripts/bluey-visible-local.sh` now prefers local debug Bluey, rebuilds the macOS overlay in debug mode, pins the matching local daemon/overlay paths when possible, forces the raw helper path, sets both capture-visible request env names, and only prints success after `bluey status` reports `overlay_capture_excluded: false`
  - web dashboard saved sessions now render a `Loading saved sessions...` placeholder and load asynchronously instead of blocking account/usage boot
  - local Bluey is currently running in visible QA mode after verification; final status reports daemon pid `78144`, `overlay_capture_excluded: false`, and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-218-VISIBLE-MODE-HISTORY-BOOT.md`
  - Verification passed:
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
    - `bash -n scripts/bluey-visible-local.sh`
    - `node --check web/assets/bluey-site.js`
    - `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates --lib`
    - `scripts/release-hygiene-scan.sh`
- Round 217 hardened the upload/release path after visible-overlay QA:
  - confirmed live `latest.json` points to current `v0.1.16` macOS arm64 artifact
  - downloaded the live active artifact and verified its hash matched `latest.json`: `c88e6d7faa32c0242f249076d4bf39c43ba61a557460368702d9a0ef62f3a5b1`
  - scanned the live active artifact inside the archive; no visible-overlay/dev flag strings were found
  - scanned live `install.sh` and `install.ps1`; no visible-overlay/dev flag strings were found
  - confirmed live `latest.json.sig` exists
  - scanned fresh local release `bluey`, `bluey-daemon`, `cue`, and `cue-daemon` binaries; no forbidden visible-overlay/dev strings were found
  - built and scanned fresh macOS overlay release binaries; no forbidden visible-overlay/dev strings were found
  - added an archive-level guard to `scripts/publish-bluey-release.sh` so current `.tar.gz`, `.zip`, and raw release artifacts are refused before manifest/checksum/sign/upload if they contain production-forbidden visible-overlay or dev markers
  - verified the guard passes on the current `v0.1.16` artifact and fails on a poisoned fake Windows zip containing `BLUEY_DEV_OVERLAY`
  - stopped the local visible QA daemon after verification and restarted Bluey normally; final local status reports `overlay_capture_excluded: true` and `screen_capture_active: false`
  - Round doc: `docs/rounds/ROUND-217-UPLOAD-SECURITY-RELEASE-GUARD.md`
  - Verification passed:
    - `cargo test --manifest-path server/Cargo.toml --test integration_e2e -- --test-threads=1` (`41 passed`)
    - `cargo test --manifest-path server/Cargo.toml --test gdpr_webhook_cleanup` (`2 passed`)
    - `cargo test --manifest-path server/Cargo.toml --test connectinfo_real_serve` (`1 passed`)
    - `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates --lib`
    - `cargo test -p cue-cli redact --lib`
    - `cargo test -p cue-cloud-client tokens --lib`
    - `cargo test -p cue-cli update --lib`
    - `cargo test --manifest-path server/Cargo.toml --lib` (`172 passed`)
    - `bash -n scripts/publish-bluey-release.sh`
    - `scripts/release-hygiene-scan.sh dist`
    - `cargo build --release -p cue-daemon -p cue-cli`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=release native/macos/cue-overlay/build.sh`
- Round 216 improved white/light theme blue contrast:
  - added dedicated macOS light-theme accent tokens: `accent`, `accentBorder`, and `accentSoft`
  - strengthened the macOS light-theme panel/feed/drop-target border contrast
  - routed the plus/attach icon, Answer accent button, History border/icon, active canvas icon, click-through icon, screen-ready route badge, and drop-highlight outline through the stronger light-theme accent
  - kept dark-theme cyan behavior unchanged
  - added matching Windows light-theme accent constants
  - Windows owner-drawn button borders, pressed light fill, Direct2D header/composer strokes, and GDI fallback header/composer strokes now use the stronger light accent
  - local visible/debug Bluey was rebuilt and relaunched
  - Round doc: `docs/rounds/ROUND-216-LIGHT-THEME-BLUE-CONTRAST.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
- Round 215 made saved-session sync automatic for signed-in desktop users:
  - `CueSettings.cloud_sync_enabled` now defaults to `true`
  - `bluey login` enables saved-session background sync for linked accounts, fixing older default-false settings files on newly linked machines
  - daemon auto-sync now uses a 20s debounced scheduler after durable local session changes such as final transcript saves, answers, attachments, context removal, transcript clear, instructions, and session open/rename/new/continue
  - pending debounced sync is aborted on daemon shutdown
  - `BLUEY_AUTO_CLOUD_SYNC=0` and `CUE_AUTO_CLOUD_SYNC=0` explicitly opt out
  - manual `bluey cloud sync` remains available for support/debug, but normal web/CLI copy no longer tells users to run it
  - web saved-session empty state now tells users to keep Bluey on while signed in because sessions sync automatically
  - download command grid now shows `bluey account` for account/sync status instead of `bluey cloud sync`
  - privacy/terms copy now describes signed-in saved-session sync and the ability to turn it off
  - local visible/debug Bluey was rebuilt and relaunched; local setting now reports `Cloud sync: automatic`
  - Round doc: `docs/rounds/ROUND-215-AUTOMATIC-SAVED-SESSION-SYNC.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo fmt --manifest-path crates/cue-cli/Cargo.toml`
    - `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
    - `git diff --check`
    - `cargo test -p cue-core default_settings_enable_cloud_sync_after_sign_in --lib`
    - `cargo test -p cue-daemon cloud --lib`
    - `cargo build -p cue-cli`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `cargo test -p cue-daemon --lib` (`268 passed; 2 ignored`)
    - `cargo test -p cue-core --lib` (`83 passed`)
    - `cargo test -p cue-cli --lib` (`53 passed`)
    - `/Users/uno/Downloads/cue/target/debug/bluey settings --cloud-sync true`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
    - `/Users/uno/Downloads/cue/target/debug/bluey cloud status`
- Round 214 improved coding follow-up behavior:
  - backend answer-shape rules now explicitly require complete code for first-time coding/build answers, but small changed blocks, `PATCH`, or unified diffs for code-changing follow-ups
  - backend code canvas normalization preserves `PATCH`, `DIFF`, `CHANGED BLOCK`, and `CHANGED LINES` sections instead of flattening them into generic full-code replacements
  - macOS code canvas follow-ups now merge into the existing code canvas:
    - patch/diff/change-block follow-ups append under `PATCH N`
    - full replacement follow-ups update the main `CODE` section in the same canvas and add a compact `CHANGED LINES N` summary
    - raw follow-up question text is no longer injected into code canvas update sections
  - macOS canvas rendering now highlights patch/update headers, added lines, removed lines, diff hunk headers, and changed sections
  - Windows receives the shared backend prompt/artifact behavior; no native Windows canvas change was made because this branch does not have the same rich canvas pane there
  - local visible/debug Bluey was rebuilt and relaunched
  - Round doc: `docs/rounds/ROUND-214-CODE-FOLLOWUP-INPLACE-UPDATES.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon mode_instructions --lib`
    - `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions --lib`
    - `cargo test -p cue-daemon llm_overlay_artifact_preserves_patch_canvas_header --lib`
    - `cargo test -p cue-daemon llm_overlay_artifact --lib`
    - `cargo test -p cue-daemon answer_diagnostics --lib`
    - `cargo test -p cue-daemon --lib` (`268 passed; 2 ignored`)
    - `cargo build -p cue-cli`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `git diff --check`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `/Users/uno/Downloads/cue/target/debug/bluey status`
- Round 213 added privacy-safe answer/canvas diagnostics:
  - daemon answer request logs now capture request id, source, route/provider, fallback count, streaming flag, question char/word counts, coarse intent, visible context count, pending context id count, and context counts by kind
  - daemon answer completion logs now capture provider, latency, token counts, source count, answer shape, code-fence shape, markdown/backtick flags, inferred artifact type, inferred artifact confidence percent, and inferred artifact body size
  - final overlay answer diagnostics now capture card/generation id, answer shape, artifact type, confidence percent, and artifact body size
  - provider stream truncation and managed stream incomplete cases now emit explicit metadata-only warnings
  - raw overlay event debug logging was replaced with event-kind-only logging
  - overlay error/stdout logs now record lengths instead of raw strings
  - daemon lifecycle logs now store `overlay_detail_chars` instead of raw lifecycle detail
  - macOS canvas lifecycle detail no longer includes question/title snippets; it records chars/words/intent/kind/body sizes instead
  - Windows did not have equivalent native canvas lifecycle snippet logs; shared daemon diagnostics cover Windows too
  - old pre-fix local logs are not retroactively scrubbed
  - local visible/debug Bluey was rebuilt and relaunched
  - Round doc: `docs/rounds/ROUND-213-PRIVACY-SAFE-ANSWER-DIAGNOSTICS.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon answer_diagnostics --lib`
    - `cargo test -p cue-daemon answer_context_diagnostics --lib`
    - `cargo test -p cue-daemon answer_overlay_artifact --lib`
    - `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
    - `cargo test -p cue-daemon mode_instructions --lib`
    - `cargo build -p cue-cli`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 212 fixed code-explanation answer quality after the owner showed an LRU answer that looked like raw notes and an old partial code canvas:
  - shared daemon prompt now separates implementation/patch requests from explanation-only coding questions
  - explanation-only coding questions now ask the model to teach: core idea, data structures, operation walkthrough, invariant, complexity, and edge cases
  - General mode avoids a Patch section for algorithm/code explanation requests unless the user asks for code changes
  - Code mode keeps Patch for implementation/change requests, but teaches step-by-step for explanation-only questions
  - chat prompt discourages Markdown emphasis in prose
  - macOS answer rendering strips Markdown headings/bold/underline decoration from prose and strips inline backticks only after code-fence/canvas handling
  - Windows answer rendering strips the same plain-text Markdown decoration for answer cards and preserves fenced-code identifiers such as `__init__`
  - local visible/debug Bluey was rebuilt and relaunched from the fresh daemon and macOS overlay
  - Round doc: `docs/rounds/ROUND-212-CODE-EXPLANATION-CHAT-QUALITY.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon mode_instructions --lib`
    - `cargo test -p cue-daemon provider_messages --lib`
    - `cargo test -p cue-daemon sanitize_answer_text --lib`
    - `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions --lib`
    - `cargo build -p cue-cli`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 211 fixed broken-looking coding canvases from partial streams:
  - fallback answer artifact inference now runs only on final overlay updates, not every `done: false` stream tick
  - unclosed fenced code blocks are no longer extracted as complete code canvases
  - OpenAI-compatible streams now treat truncation finish reasons such as `length` and `max_tokens` as incomplete-stream errors
  - explicit managed/provider final artifacts still pass through normally
  - this is a shared daemon/backend fix, so Mac and Windows get the same behavior without native UI forks
  - local visible/debug Bluey was rebuilt and relaunched
  - Round doc: `docs/rounds/ROUND-211-CODE-CANVAS-PARTIAL-STREAM-GUARD.md`
  - Verification passed:
    - `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
    - `cargo test -p cue-daemon answer_overlay_artifact --lib`
    - `cargo test -p cue-daemon provider_length_finish_reason_is_incomplete_stream --lib`
    - `cargo test -p cue-daemon llm_overlay_artifact_keeps_sql_code_canvas --lib`
    - `cargo build -p cue-cli`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 210 fixed Listen transcript reuse after send:
  - macOS trims already-consumed transcript prefixes from late cumulative STT partial/final events before they enter the next answer buffer
  - macOS consumed-transcript matching now also compares compact alphanumeric fingerprints, so `Build me LRU cache` and `BuildMeLRUCache` are treated as the same phrase
  - macOS resets the caption strip after a successful send instead of visually holding old transcript text
  - daemon duplicate and partial-to-final transcript dedup now handle compact-spacing variants
  - Windows clears only its local caption preview/send buffer after send, without clearing daemon meeting transcript context
  - visible QA status now follows the capture-visible debug gate, and the final local visible run reported `overlay_capture_excluded: false`
  - rebuilt and relaunched local visible/debug Bluey
  - Round doc: `docs/rounds/ROUND-210-LISTEN-TRANSCRIPT-CONSUME-ON-SEND.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon duplicate_transcript_detection_skips_same_speaker_and_cross_source_echoes --lib`
    - `cargo test -p cue-daemon --test live_transcript_dedup`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `cargo build -p cue-cli`
    - `cargo build -p cue-daemon --bin bluey-daemon`
    - `BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 209 restored control clickability while macOS click-through mode is enabled:
  - added manual control hit-zone detection to the pass-through hit-test path
  - included manually routed buttons, opacity scrubber, and transcript clear hit zones in the interactive region
  - updated the window-level mouse policy so padded Bluey controls keep the window mouse-active while blank interior still passes through
  - changed remote-input passthrough so it does not override Bluey controls when the pointer is currently over an interactive region
  - kept mouse-up alive briefly after a control mouse-down so a click does not get interrupted by tiny pointer movement
  - Windows parity checked; no Windows change needed because the Windows overlay already routes controls through `HTCLIENT`, borders through resize handles, logo/wordmark through `HTCAPTION`, and blank space through `HTTRANSPARENT`
  - rebuilt and relaunched local visible/debug Bluey
  - Round doc: `docs/rounds/ROUND-209-CLICKTHROUGH-CONTROL-HITBOX-RESTORE.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 208 fixed overlay resize stability after the owner reported resizing still felt bad:
  - macOS resize edges now win before header dragging, so the top border does not accidentally move the window instead of resizing
  - click-through mode treats the visible border as interactive resize chrome while blank interior space still passes through
  - full-screen mode disables manual edge resize until restored
  - manual resize now anchors the opposite edge and clamps to the current screen before setting the frame
  - resize frames are snapped to the backing pixel grid
  - manual resize can shrink below the default compact width, while the default open size remains familiar
  - macOS max resize now uses the available screen instead of the older canvas/focus cap
  - Windows parity adds expanded edge hit-testing, minimum track size, monitor-work-area max tracking, and removes the old 1040x620 post-resize snapback
  - rebuilt and relaunched local visible/debug Bluey
  - Round doc: `docs/rounds/ROUND-208-OVERLAY-RESIZE-STABILITY.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 207 tightened the macOS expanded header spacing from the owner's visible-mode screenshot:
  - reduced the history button width and nearby left-cluster gaps
  - reduced the logo/wordmark spacing and wordmark frame cap
  - changed the `Ready` route badge and `Show N files` badge from broad middle-width slots to compact text-measured widths
  - kept Windows parity checked; Windows does not render the same header badge cluster in that position, so no equivalent Windows spacing change was needed
  - rebuilt and relaunched local visible/debug Bluey
  - Round doc: `docs/rounds/ROUND-207-MAC-HEADER-BADGE-SPACING.md`
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh`
    - `target/debug/bluey status`
- Round 206 fixed the owner's visible-QA report about click-through, full-screen, spacing, and Listen/doubled captions:
  - Restored true macOS mode split:
    - Interactive mode keeps blank Bluey space draggable/resizable.
    - Click-through mode lets blank Bluey space click the app behind it.
    - The Bluey logo/wordmark remains the explicit move handle in click-through mode.
  - Added immediate window-policy refresh when the interaction-mode button is toggled.
  - Kept History drawer and open canvas interactive in click-through mode so their scroll/clicks do not leak through.
  - Changed macOS full-screen to use the actual screen frame, flatten corners while full-screen, and restore to Bluey's compact default frame.
  - Reduced macOS expanded-panel screen inset from 32px to 12px and tightened fixed chrome gaps/insets.
  - Mirrored Windows blank-space behavior to match its help text: controls are clickable, logo/wordmark moves, blank expanded space passes through.
  - Widened cross-source live-caption echo dedupe from 2.5s to 6s.
  - Local visible/debug overlay was rebuilt and relaunched with the patched debug binaries.
  - Audio status after relaunch is idle/ready with native helper installed.
  - Round doc: `docs/rounds/ROUND-206-CLICKTHROUGH-FULLSCREEN-LISTEN-QA.md`.
  - Verification passed:
    - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
    - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
    - `cargo test -p cue-daemon duplicate_transcript_detection -- --nocapture`
    - `cargo build -p cue-cli -p cue-daemon`
    - `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
    - `BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh`
- Round 205 ran Bluey in actual local visible mode for QA:
  - The installed release `scripts/bluey-visible-local.sh` restart printed visible-mode success but release builds intentionally compile out the capture-visible path, so no visible args appeared.
  - Rebuilt debug desktop stack with `cargo build -p cue-cli -p cue-daemon` and `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`.
  - Restarted with `BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh`.
  - Verified running daemon is `/Users/uno/Downloads/cue/target/debug/bluey-daemon`.
  - Verified overlay command includes `--bluey-dev-overlay --bluey-local-visible-overlay --bluey-overlay-capture-visible`.
  - Current mode is QA visible/debug mode. Return to normal with `target/debug/bluey off` then `bluey on`.
- Round 204 immediately fixed the owner's post-deploy report that the overlay felt dead: buttons were not clickable and blank panel space could not be dragged.
  - Root cause: expanded macOS click-through default plus `expandedWindow.ignoresMouseEvents` point-gating could make the whole overlay stop receiving clicks; Windows blank surface returned `HTTRANSPARENT`.
  - macOS expanded overlay now defaults to interactive, keeps normal expanded windows mouse-active, and treats blank panel surface as draggable while preserving real controls as clickable.
  - Windows expanded overlay now returns `HTCLIENT` for controls and `HTCAPTION` for blank surface, so blank space moves the panel instead of passing through.
  - Local `~/.bluey/bin` macOS overlay binaries and `BlueyOverlay.app` were rebuilt/installed.
  - The old overlay child process was killed and relaunched with `bluey overlay show`; the refreshed child PID was `25160`.
  - Public desktop release was bumped and deployed to `0.1.16`.
  - Live artifact is `https://bluey.sh/releases/v0.1.16/bluey-0.1.16-darwin-arm64.tar.gz`.
  - Live artifact SHA256 is `c88e6d7faa32c0242f249076d4bf39c43ba61a557460368702d9a0ef62f3a5b1`.
  - Live `latest.json.sig`, artifact checksum, temp-home installer smoke, archive string scan, and release hygiene scan all passed.
  - No API server redeploy was needed for Round 204; live `/health` remained OK on the Round 203 server.
- Round 203 deployed the current Bluey branch snapshot to the droplet and release host:
  - workspace desktop artifact version bumped to `0.1.15`
  - `docs/release/RELEASE-v0.1.15.md` added
  - macOS arm64 artifact published to `https://bluey.sh/releases/v0.1.15/bluey-0.1.15-darwin-arm64.tar.gz`
  - live `latest.json` reports `0.1.15`
  - live artifact SHA256 is `71ab873543a173c95020ff21f1fd9a9b10a0c8b8b9c6efbca45f79d8522573c1`
  - live `latest.json.sig` verified against the release key
  - temp-home install smoke from `https://bluey.sh/install.sh` installed `bluey 0.1.15`
  - shipped macOS binaries were scanned and did not contain capture-visible/dev overlay flag strings
  - release hygiene scan passed with only allowed local-QA/docs warnings
  - API server was built on the droplet from `/opt/bluey-build-codex-0.1.15`
  - `/usr/local/bin/bluey-server` was swapped with rollback backup `/usr/local/bin/bluey-server.bak-20260626T205440Z`
  - `bluey-api.service` restarted successfully and live `/health` reports commit `3d6bc5f-v0.1.15`
  - Postgres check confirmed `balance_ledger_entries` plus `accounts` billing restriction columns exist
  - Windows remains preview-gated in `install.ps1`; `latest.json` does not advertise a public Windows artifact yet
- Round 201 fixed the macOS installer PATH gap shown in the owner's screenshot:
  - `ops/install/install.sh` now asks for sudo to create `/usr/local/bin/bluey` and `/usr/local/bin/bluey-daemon` when `/usr/local/bin` is not writable
  - sudo is only for command symlinks; the Bluey install root stays user-owned
  - users can opt out with `BLUEY_INSTALL_NO_SUDO=1`
  - if sudo is unavailable or declined, fallback `~/.local/bin` install now auto-adds PATH lines to zsh/bash profile files for new terminals
  - final installer output uses `bluey on` only when it should work in the current shell, otherwise it prints the full path and says new terminals can use `bluey on`
  - Windows already had PATH automation through `Ensure-UserPathEntry`, so no Windows change was needed
- Round 201 verification passed:
  - `bash -n ops/install/install.sh`
  - temp fake-release installer smoke with `BLUEY_INSTALL_NO_SUDO=1`, temp `HOME`, temp artifact, checksum/local-tools skipped, confirming `.zprofile`, `.zshrc`, symlink, and installed command
- Round 201 release gate: do not manually publish only `install.sh`; live production install must go through the signed release publish flow because `latest.json` pins the installer SHA.
- Round 200 cleaned paid web-search customer copy:
  - customer-facing/product language should say paid web search uses credits and can have spend controls
  - do not frame normal paid search as a fixed daily search count
  - do not expose internal enforcement/risk language in customer copy
  - repeated-query status now says `Web search paused briefly for this repeated question.`
  - added `web_search_skipped_labels_stay_customer_friendly`
- Round 200 verification passed:
  - `cargo fmt --manifest-path server/Cargo.toml`
  - `git diff --check`
  - `cargo test --manifest-path server/Cargo.toml web_search_skipped_labels_stay_customer_friendly -- --nocapture`
  - `cargo test --manifest-path server/Cargo.toml router_cost_label_includes_web_search_usage -- --nocapture`
- Round 199 implemented the first production-shaped managed web-search lane:
  - server-side search remains provider/API based with Brave, Tavily, or generic endpoint configuration
  - default web-search pricing is `2` customer cents and `1` Bluey cost cent per successful managed search call, overridable by env
  - trial accounts default to `5` searches/day via durable `usage_events` counting
  - paid accounts are credit-metered and not blocked by a small fixed daily search-count cap
  - repeated identical searches are blocked for a short in-memory hashed-query window, default `600` seconds
  - status SSE copy now includes `Checking saved context...`, `Searching web...`, `Reading N sources...`, and `Web search used: 1 search, N sources`
  - combined paid deduction happens once after completion, while answer and search are recorded as separate usage events
  - failed/unconfigured/timeout/privacy-blocked searches show status and do not charge the user
- Round 199 daemon changes carry managed source metadata through the answer route and emit a shared `Sources` context card with web attachments. This gives Mac and Windows a common source-chip foundation without native UI forks.
- Round 199 local install refreshed `~/.bluey/bin/bluey` and `~/.bluey/bin/bluey-daemon`, then restarted Bluey. Latest local status:
  - pid `69465`
  - active meeting id `89a72895-1931-4990-bc8e-8a6a18dccdba`
  - overlay visible `true`
  - overlay capture excluded `true`
  - overlay opacity `0.92`
  - screen capture active `false`
- Round 199 verification passed:
  - `cargo fmt --check --manifest-path server/Cargo.toml`
  - `cargo fmt --check -p cue-daemon`
  - `git diff --check`
  - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
  - `cargo test --manifest-path server/Cargo.toml router -- --nocapture`
  - `cargo test --manifest-path server/Cargo.toml usage -- --nocapture`
  - `cargo test --manifest-path server/Cargo.toml router_cost_label_includes_web_search_usage -- --nocapture`
  - `cargo test --manifest-path server/Cargo.toml web_search_usage_event_records_separate_search_cost -- --nocapture`
  - `cargo test --manifest-path server/Cargo.toml count_task_events_in_window_counts_recent_matching_task_type -- --nocapture`
  - `cargo test -p cue-daemon managed_sources_render_as_context_card_with_web_attachments -- --nocapture`
  - `cargo test -p cue-daemon response_artifact_does_not_route_self_intro_to_system_design -- --nocapture`
  - `cargo test -p cue-daemon answer_overlay_cost_label -- --nocapture`
  - `cargo build --manifest-path server/Cargo.toml`
  - `cargo build --release -p cue-cli -p cue-daemon`
- Remaining Round 199 gates:
  - configure real search provider env vars in staging/production
  - deploy server before expecting live cloud web search
  - run live provider smoke confirming statuses, citations, source card, separate `llm` and `web_search` usage events, and one combined balance deduction
  - build a polished native source drawer later; current source-card path is the cross-platform foundation
- Round 198 clarified web-search quota policy after the owner asked for paid search to be credit-based instead of framed as a fixed daily count.
- A low fixed daily paid-search cap is not currently enforced in product code and should not become the normal paid-user product model.
- Paid web search should be credit-metered with clear spend controls: charge/reserve credits for search provider cost, fetched-page processing, and answer tokens; keep short-window repeated-search controls; allow user/workspace daily search spend controls.
- Free/trial web search can have a small hard daily cap. Paid accounts should not feel blocked by a low fixed count when they have credits.
- Future implementation should add durable account/day search accounting, idempotency, paid daily spend guard, query/source caching, and clear UI copy.
- Round 197 fixed the likely doubled-caption path when Mic + System both hear the same utterance.
- The daemon now treats exact final-caption repeats from the same speaker within 8 seconds as duplicates, and exact Mic/System echo repeats within 2.5 seconds as duplicates.
- The macOS overlay now suppresses identical cross-source live-caption preview echoes; when Mic and System have the same caption body, Mic wins for preview and pending answer context.
- Windows overlay did not need a platform-specific patch because it has one global transcript final/partial buffer rather than separate per-source preview buffers; the daemon dedup applies to Windows too.
- Round 197 verification passed:
  - `cargo test -p cue-daemon duplicate_transcript_detection -- --nocapture`
  - `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
  - `cargo build --release -p cue-cli -p cue-daemon`
  - `native/macos/cue-overlay/build.sh`
- Local install was refreshed after Round 197, then `bluey on` succeeded:
  - daemon pid `44710`
  - overlay visible `true`
  - overlay capture excluded `true`
  - active meeting id `e5645fad-8644-4b7c-80e5-26587b488bb3`
- Remaining Round 197 QA gate: live Mic + System smoke with spoken audio to confirm the bottom caption strip and sent Answer question do not contain duplicate caption lines.
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

## Latest Round: Round 250

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`.

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-250-DROPLET-RELEASE-DEPLOY-0.1.17.md`

Round 250 deployed the current Bluey work to the live droplet download/API path:

- workspace release bumped to `0.1.17`
- release notes added at `docs/release/RELEASE-v0.1.17.md`
- macOS arm64 release artifact built with embedded updater public key
- signed `latest.json` and `latest.json.sig` published to `bluey.sh`
- live artifact:
  - `https://bluey.sh/releases/v0.1.17/bluey-0.1.17-darwin-arm64.tar.gz`
  - SHA256 `8a91bdc1bd34444fb31f76450461e5d2b287405f205a415611c82d069359451d`
- static web, install scripts, release notes, checksum file, and manifest published to `/var/www/bluey`
- API server rebuilt on the droplet from `/opt/bluey-build-codex-0.1.17` and restarted through `bluey-api.service`
- previous server binary backed up at `/var/backups/bluey-api/bin/bluey-server.previous-20260630T090432Z`

Verification passed:

```bash
BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64
cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
bash scripts/release-hygiene-scan.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=... scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=... PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey bash scripts/deploy-bluey-sh-manual.sh
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/latest.json
openssl pkeyutl -verify -rawin -pubin ...
curl download + shasum against live SHA256SUMS.txt
temp-home install smoke from https://bluey.sh/install.sh -> bluey 0.1.17
```

Caveat: live `latest.json` currently advertises only `darwin-arm64` for `0.1.17`. `install.ps1` remains published, but a fresh Windows `0.1.17` zip still needs the Windows build host before Windows can update to this exact version.

## Current Active Round: Round 254

Round doc:

- `docs/rounds/ROUND-254-DELETED-ACCOUNT-INFLIGHT-STREAM-GUARD.md`

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Fixed the deleted-account inflight streaming race:

- Managed chat streams now re-check account liveness before dispatch, before every provider SSE event, and immediately before the billing event.
- Non-streaming managed answers now re-check account liveness before dispatch and immediately before billing.
- If the account is deleted mid-stream, Bluey emits `reason: account_deleted` with: `This Bluey account was deleted. The answer was stopped and was not billed.`
- If the account is billing-restricted or cannot be verified, Bluey stops before continuing output/billing.
- STT relay now checks account liveness before forwarding client audio and before forwarding provider transcript messages.
- STT relay skips settlement when the account is deleted while the relay is open.

Verification passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo test --manifest-path server/Cargo.toml stt -- --nocapture
cargo test --manifest-path server/Cargo.toml account_delete_requires_typed_delete_and_credit_loss_consent -- --nocapture
cargo test --manifest-path server/Cargo.toml auth_device -- --nocapture
git diff --check
```

Deployed to `/usr/local/bin/bluey-server`, restarted `bluey-api.service`, and verified `https://bluey.sh/health`.

Live stream/delete smoke:

```text
email=codex-stream-delete-1782837633@bluey.local
request_id=codex-stream-delete-1782837633
delete_status=200
curl_code=0
exists_after=0
event: error
data: {"error":"This Bluey account was deleted. The answer was stopped and was not billed.","reason":"account_deleted"}
```

Journal confirmed the request was accepted, the route was selected, the account was deleted, then the stream stopped because the account was no longer active. There was no `managed chat completed and billed` line for that request.

## Current Active Round: Round 253

Round doc:

- `docs/rounds/ROUND-253-ACCOUNT-DELETE-LINK-RECOVERY-GUARD.md`

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Investigated the low-balance/account-delete/login-code issue:

- The `$4.82 low` screenshot matched the smoke account `codex-smoke-20260608183100@bluey.sh`, not the internal admin test account.
- `internal-admin-20260606023943@bluey.sh` had been hard-deleted from live Postgres.
- Recovered that internal admin account from `/var/backups/bluey-api/hourly/bluey-20260630T150001Z.db` into live Postgres with the same account id, password hash, admin flag, verification timestamp, and auto top-up settings.
- Restored its live balance to `$15.00` with an `internal_credit` ledger entry.
- Confirmed the device login code is an authenticated, single-use device-flow approval; the main issue was copy clarity, not that a random code alone grants access.
- CLI login now warns users to approve the code only in the account they want the desktop to use.
- Web auth copy now says the desktop link must be confirmed after signing in.
- Server `/account/delete` now requires typed `DELETE`, explicit data-loss consent, and explicit credit-loss consent.
- CLI, dashboard, and web account deletion all send the explicit consent payload.
- Dashboard and web account deletion now use in-app Bluey modals instead of native browser confirm/prompt dialogs.
- Terms and privacy copy now document that account deletion is permanent and unused Bluey credits are lost on deletion.

Local verification passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo check -p cue-cli -p cue-dashboard
cargo test --manifest-path server/Cargo.toml account_delete_requires_typed_delete_and_credit_loss_consent -- --nocapture
cargo test --manifest-path server/Cargo.toml auth_device -- --nocapture
cargo build --manifest-path server/Cargo.toml --bin bluey-server
cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
git diff --check
```

Deployment verified:

- Web static files synced to `/var/www/bluey/`.
- Linux x86_64 server binary built on the droplet from `/opt/bluey-build-codex-delete-guard`.
- Installed to `/usr/local/bin/bluey-server`.
- `bluey-api.service` restarted cleanly and `https://bluey.sh/health` returned `status=ok`.
- Live throwaway-account delete smoke passed:
  - empty payload rejected with `422`, account still existed
  - typed `DELETE` without credit-loss consent rejected with `400`, account still existed
  - full data-loss plus credit-loss consent returned `200`, account deleted
- Live static copy contains the delete modal, credit-loss consent, and desktop-link confirmation wording.

## Current Active Round: Round 252

Round doc:

- `docs/rounds/ROUND-252-BLUEY-LOGO-WORDING-PACK.md`

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Created a local brand pack for the owner at:

```text
/Users/uno/Downloads/bluey-logo-and-wording/
```

Included files:

- `bluey-logo.svg`
- `bluey-wordmark.svg`
- `bluey-checkout-logo.png`
- `bluey-social-preview.png`
- `bluey-wording.txt`

No product code changed.

## Current Active Round: Round 251

Round doc:

- `docs/rounds/ROUND-251-LIVE-QA-SHUTDOWN-ROUTING-SMOKE.md`

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Live QA and fixes completed:

- Fixed the production Postgres graceful-shutdown/core-dump issue by replacing the raw r2d2 Postgres pool connection with `SafePostgresClient`, which drops sync `postgres::Client` on a normal thread when route state is dropped from a Tokio worker.
- Corrected deploy path awareness: systemd uses `/usr/local/bin/bluey-server`, not `/opt/bluey-api/bluey-server`.
- Deployed the fixed server to `/usr/local/bin/bluey-server`.
- Verified clean `bluey-api.service` restart after deploy:
  - active PID after final restart: `890981`
  - journal showed `Deactivated successfully`
  - no Tokio runtime panic
  - no core dump
- Fixed provider request-shape issues found by live smoke:
  - OpenAI GPT-5-family routes now omit non-default `temperature`.
  - Anthropic old manual `thinking.type=enabled` is disabled until the newer adaptive schema is implemented.
  - Anthropic no-thinking paths keep requested output caps bounded.
- Adjusted AnswerPlan so simple code prompts route to `balanced` but still produce `code_artifact`; LRU/cache/backend/debug/system-design remains `deep`.

Final live smoke:

```text
request_id: codex-live-route-smoke-1782814794
effective_lane: balanced
provider/model: anthropic / claude-sonnet-4-6
artifact_type: code
was_fallback: false
server latency: 2228ms
wall-clock curl latency: about 2412ms
cost: 1c
```

Verification passed:

```bash
cargo test --manifest-path server/Cargo.toml answer_plan -- --nocapture
cargo test --manifest-path server/Cargo.toml temperature -- --nocapture
cargo test --manifest-path server/Cargo.toml anthropic_manual_thinking -- --nocapture
cargo test --manifest-path server/Cargo.toml web_search -- --nocapture
cargo test --manifest-path server/Cargo.toml provider_health -- --nocapture
cargo test --manifest-path server/Cargo.toml router_complete_falls_back_when_preferred_provider_429s -- --nocapture
cargo test -p cue-daemon --test live_transcript_dedup --test live_transcript_emit --test pipeline_integration -- --nocapture
scripts/release-hygiene-scan.sh
```

Remaining live gates:

- Production preflight still fails because Turnstile is not configured:
  - add `BLUEY_TURNSTILE_SITE_KEY`
  - add `BLUEY_TURNSTILE_SECRET_KEY`
  - `/auth/captcha/config` currently returns provider/site key as null
- Managed web-search code/guards exist, but no real search provider key is configured yet.
- Anthropic adaptive thinking schema still needs implementation before Claude manual thinking should be re-enabled.
- Windows `0.1.17` downloadable artifact still needs a Windows build host/package.

## Historical Carried Section: Round 202

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-routing-hardening
```

Round doc:

- `docs/rounds/ROUND-202-BILLING-RISK-LEDGER-SYNC-GUARDS.md`

Round 202 copied the important Pinky billing/dispute posture into Bluey code where gaps were found:

- Added `balance_ledger_entries` to SQLite and Postgres runtime schemas.
- Wrote balance movement evidence rows transactionally for:
  - processor credits
  - internal credits
  - paid usage deductions
  - request-specific LLM/web-search/embed/transcribe deductions
  - processor credit revocation
  - credit expiry
  - STT reserve and settle movement
- Routed router deductions through `deduct_for_request` so spend can be tied to the request id.
- Blocked billing-restricted accounts from cloud write/compute paths:
  - `POST /sync/batch`
  - `POST /rag/query`
  - `POST /sync/artifacts/:artifact_id/object`
  - `GET /sync/artifacts/:artifact_id/object`
  - `POST /usage/event`
- Added admin-only `GET /admin/billing-risk` with restricted accounts and latest balance-ledger evidence.

Verification:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml db::balance::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml db::stt_accounting::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml api::sync::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml db::accounts::create_dup_tests -- --nocapture
cargo test --manifest-path server/Cargo.toml billing_ -- --nocapture
cargo test --manifest-path server/Cargo.toml router_complete_rejects_billing_restricted_account -- --nocapture
cargo test --manifest-path server/Cargo.toml
```

Full server suite passed:

- `172` unit tests
- `41` integration tests
- doc-tests

Remaining billing/security follow-ups:

- build a polished admin abuse/dispute UI on top of `/admin/billing-risk`
- store explicit checkout/reload terms version, IP, user agent, threshold, selected amount, and consent snapshots
- add durable auto-reload attempt rows with idempotency, spend guard, and receipt state
- add manual unblock/reinstate path for won disputes or benign refunds
- include `balance_ledger_entries` in account export if owner wants customer-visible evidence

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

## Latest Round 302: Shortcut Guide And Click-through Move Handle

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-302-SHORTCUT-GUIDE-MOVE-HANDLE.md`

Round 302 cleaned up stale keyboard shortcut affordances after plain single-letter local shortcuts were removed, and hardened the click-through move handle:

- macOS shortcut guide button uses a help-style `?` icon instead of the keyboard icon.
- macOS shortcut guide title/copy now says `Controls and shortcuts`.
- macOS click-through handle drag now keeps the overlay mouse-active while the drag is in progress.
- Windows shortcut guide button says `Shortcuts` instead of `Keys`.
- Windows help/shortcut copy avoids stale `keyboard shortcuts` wording.
- Windows click-through move handle visual target and hit slop were enlarged.

Verification:

```bash
swift build -c debug --package-path native/macos/cue-overlay
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

## Latest Round 303: Auto-send Stop Cancels

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-303-AUTOSEND-STOP-CANCELS.md`

Round 303 fixed the live tester issue where Auto-send could answer late after the user clicked Listen Stop:

- macOS no longer schedules auto-send from explicit Stop.
- macOS schedules auto-send only after a final caption settles for 900ms while Listen is still active.
- macOS Stop/paused/pill Stop cancels pending auto-send work and clears the auto-send buffer only.
- macOS auto-send preference version bumped to reset old stop-triggered choices to off.
- Windows default auto-send mode changed from system-stop to off.
- Windows final captions schedule a 900ms settle timer while recording is active.
- Windows Stop, transcript clear, and session switch cancel pending auto-send timers.
- Both platforms now say captions settle; Stop cancels pending auto-send.

Verification:

```bash
swift build -c debug --package-path native/macos/cue-overlay
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

## Latest Round 304: STT Realtime Latency Audit

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-304-STT-REALTIME-LATENCY-AUDIT.md`

Round 304 investigated why live transcription can feel slower than Web Speech API:

- Managed Deepgram already requests partials with `interim_results=true`.
- Deepgram partials are parsed and forwarded to overlay as `TranscriptPartial`.
- macOS and Windows overlays both have partial-caption display paths.
- Live relay reads small chunks, about 128 ms of PCM per read in the relay path.
- The main latency tradeoff is startup gating: Bluey waits for audible audio before creating the paid STT session and opening the websocket, which avoids silence billing but can make first captions feel late.

Recommended next build:

- Add a low-latency STT mode that opens the relay earlier while preserving zero-audio settlement/refund.
- Add timing logs for first PCM, first audible audio, reservation, websocket open, first provider partial, first overlay partial, and first final transcript.

No product code was changed in this round.

## Latest Round 305: Pending Attachment Strip

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-305-PENDING-ATTACHMENT-STRIP.md`

Round 305 made the attachment flow explicit:

- Newly attached documents/screens show below the workspace near the composer as pending context.
- Pressing Enter or Answer sends those pending attachments with the question.
- After send, the bottom pending attachment strip collapses.
- Sent files/screens remain available from the top `Show files` control.
- macOS now renders pending context items when `Show files` is closed.
- Windows now has separate pending context chip state for attach, drag/drop, and screen capture.

Verification:

```bash
swift build -c debug --package-path native/macos/cue-overlay
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

## Latest Round 306: Self-Intro Interview Voice

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-306-SELF-INTRO-INTERVIEW-VOICE.md`

Round 306 fixed the answer-quality issue where `tell me about yourself` with an attached resume produced an accurate but too compressed resume paragraph:

- Managed/server behavioral AnswerPlan now explicitly teaches self-introduction flow.
- Local/direct daemon behavioral mode now triggers for self-intro prompts, not only STAR/story prompts.
- Self-intro answers now prefer a first-person `present-past-fit` arc:
  - current role and specialty,
  - relevant past experience,
  - strongest proof points,
  - why the background fits the role.
- Added regression tests for self-intro routing and prompt shaping.

Verification:

```bash
cargo test -p cue-daemon provider_messages_enable_self_intro_interview_mode_with_resume_context --lib
cargo test -p cue-daemon provider_messages_enable_behavioral_interview_mode_with_resume_context --lib
cargo test --manifest-path server/Cargo.toml answer_plan_self_intro_is_behavioral_not_system_design --lib
cargo fmt -p cue-daemon
cargo fmt --manifest-path server/Cargo.toml
git diff --check
```

## Latest Round 307: Stream And Context Robustness

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Current branch for Codex-owned local work:

```bash
codex/bluey-overlay-spacing-20260626
```

Round doc:

- `docs/rounds/ROUND-307-STREAM-CONTEXT-ROBUSTNESS.md`

Round 307 addressed the photo where a simple follow-up coding request failed with `Bluey's connection dropped before the answer finished`:

- OpenAI-compatible provider streams now complete with estimated usage when text was delivered but `[DONE]` is missing.
- Anthropic streams now complete with estimated usage when text was delivered but `message_stop` is missing.
- Empty streams still fail.
- The desktop daemon now preserves complete-looking streamed answers when only terminal metadata is missing.
- Incomplete answer shapes such as unclosed code fences still fail and are not saved.
- User-facing incomplete-stream copy no longer tells users to check server logs.
- User-facing/status/prompt labels now say `conversation context` instead of `saved Bluey memory`, `saved context`, or `local RAG`.
- The AnswerPlan eval suite now covers the screenshot-style Java palindrome code request.

Verification:

```bash
cargo test --manifest-path server/Cargo.toml stream_without --lib
cargo test --manifest-path server/Cargo.toml answer_plan --lib
cargo test -p cue-daemon incomplete_stream --lib
cargo test -p cue-daemon terminal_metadata --lib
cargo test -p cue-core --lib
cargo fmt -p cue-daemon -p cue-core -p cue-llm
cargo fmt --manifest-path server/Cargo.toml
git diff --check
```
