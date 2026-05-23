# Mac Overlay KB / Routing / Canvas Pass

Date: 2026-05-23
Owner: Codex
Branch: feat/phase-3-round-12

## Why

The overlay had the right primitives, but the user journey still felt opaque:

- attaching documents did not clearly become a "knowledge base" state,
- listening/transcribing looked like static text rather than live work,
- Auto Router classification was hidden from the user,
- canvas behavior needed an explicit product contract for follow-up code/design changes.

## What Changed

### Native macOS overlay

- Added a top `KB` status pill:
  - `KB empty`
  - `KB loading`
  - `KB N loaded`
- Added a top Auto Router status pill:
  - `Auto · ready`
  - `Auto · easy`
  - `Auto · medium`
  - `Auto · hard`
  - `Vision · deep`
  - canvas-aware labels such as `Code · canvas`.
- Attachment strip is now always meaningful:
  - empty state shows `Knowledge base empty · attach docs`,
  - attach click immediately shows `Indexing selected files...`,
  - loaded files render as compact chips with `LOADED · KIND`.
- Transcript strip now has a live state glyph:
  - `IDLE`
  - `LISTENING`
  - `TRANSCRIBING`
  - `CAPTURED`
  - `PAUSED`
- Listen state updates the composer placeholder so typing follow-ups during capture feels supported.

### Answer / canvas contract

- Provider prompt now says:
  - include a concise rationale when useful,
  - do not expose hidden chain-of-thought,
  - for code follow-ups, return the full updated implementation or replacement snippet in the artifact/canvas body,
  - for system-design follow-ups, return the updated whole architecture section so the canvas remains the current source of truth.

## UX Contract

Bluey should feel like:

1. Run `bluey on`.
2. A compact pill appears.
3. Click the pill.
4. Attach docs if needed; KB visibly moves from empty/loading/loaded.
5. Press Listen; transcript state becomes live without changing window size.
6. Ask or analyse; route classification becomes visible.
7. For simple answers, chat stays chat.
8. For code, system design, screen analysis, or long structured answers, canvas opens automatically.
9. Follow-up changes replace the canvas artifact with the current complete version, not fragmented line edits.

## Verification Added

`scripts/macos-overlay-visual-smoke.sh` now guards the source-level UI contract for:

- KB status pill,
- route classification pill,
- loading state on attach,
- listening/transcribing state,
- dynamic composer behavior,
- no return of `Full access` or `Start Bluey` labels.

## Remaining Follow-Up

The top route pill currently uses native overlay heuristics until daemon/server router metadata is pushed over the overlay IPC. The dashboard route already receives `router_meta`; the next backend follow-up is to add that metadata to `OverlayCommand::UpdateCard` or a dedicated `SetRouteStatus` command so the native pill reflects the exact server/router decision.
