# Round 006 - Web Reload Credits Clarity

## Trigger

Owner showed the landing-page pricing cards and asked to make the `$30` reload clearer for inexperienced users across the landing page and web UI. Owner also clarified that the Pinky round-doc file was only a style reference, not a numbering source for Bluey.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-25 15:28 EDT

## Fix

- Corrected resumed Bluey round docs to use Bluey's own local sequence:
  - `ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`
  - `ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`
  - `ROUND-003-ROUND-DOC-NUMBERING-AND-BACKUP-THREAD.md`
  - `ROUND-004-MAC-WINDOWS-PARITY-RULE.md`
  - `ROUND-005-BACKEND-STREAMING-AND-NOVICE-UX-AUDIT.md`
- Updated the landing pricing cards so `$30` is described as `$30` in Bluey credits, with `$15` minimum reload and no subscription.
- Updated the signed-in credits panel to explain what credits cover:
  - AI answers
  - speech transcription
  - screen analysis
  - saved-session search
- Added clearer reload guidance:
  - `$30` is the recommended reload
  - `$15` is the minimum
  - checkout opens in a new tab
  - credits appear after payment succeeds
  - users can return or refresh the dashboard to see the new balance
- Clarified Auto Reload as optional and off by default until a card is saved.
- Updated dynamic account-page copy so edited manual and Auto Reload amounts keep the same explanation.

## Verification

Passed:

- `node --check web/assets/bluey-site.js`
- `curl -fsS http://127.0.0.1:8765/ | rg -n "\\$30 adds|\\$30 credits|no subscription|bluey-site\\.js"`
- `curl -fsS http://127.0.0.1:8765/assets/bluey-site.js | rg -n "Auto Reload is optional|return here or refresh|refresh this page"`
- Stale borrowed-number scan across the handoff, compatibility pointers, and earlier Bluey numbered docs returned no old `664` through `668` Bluey round references.

Expected local static-server limitation:

- `python3 -m http.server` returned `404` for `/reload` because it does not implement the production SPA route fallback. The account and reload UI still live in `web/index.html`, and production routing is expected to serve that file for `/reload`.

## Files Touched

- `web/index.html`
- `web/assets/bluey-site.css`
- `web/assets/bluey-site.js`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`
- `docs/rounds/ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`
- `docs/rounds/ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`
- `docs/rounds/ROUND-003-ROUND-DOC-NUMBERING-AND-BACKUP-THREAD.md`
- `docs/rounds/ROUND-004-MAC-WINDOWS-PARITY-RULE.md`
- `docs/rounds/ROUND-005-BACKEND-STREAMING-AND-NOVICE-UX-AUDIT.md`
- `docs/rounds/ROUND-006-WEB-RELOAD-CREDITS-CLARITY.md`
- `docs/rounds/AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP-2026-06-25.md`
- `docs/rounds/END-TO-END-AUDIT-AND-CLEANUP-2026-06-25.md`

## Mac Windows Parity

No native macOS or Windows code changed in this round. This was web UI and documentation only, so no platform-specific parity implementation was required.

## Remaining QA

- Visual browser QA on the production-routed `/reload` page is still useful after deploy, especially on narrow mobile widths.
