# Round 536 - Jobs Module Split and Mainline Integrity

Date: 2026-07-17

## Objective

Reduce oversized Bluey Jobs portal files without changing behavior, keep the
production bundle reproducible from source, and verify that reviewed Codex-owned
work is represented on the canonical mainline without touching Sashreek-owned
branches.

## Mainline baseline

This round started from clean `origin/main` at
`e55310174d834b348220562527c79696c19c34bf`.

Before the final commit, main advanced to
`f67dceb14ea3928212f86161e7fbd90970895e4a` with interview-answer hardening in
unrelated server and evaluation files. This refactor was replayed on top without
conflict; none of that concurrent work was overwritten.

The branch audit was repeated after fetching `origin/main`:

- every remote Codex branch except the historical stream-attachments branch has
  zero patch-unique commits missing from main;
- the local interrupted-asks branch has three commits that `git cherry` marks as
  patch-equivalent to main;
- `origin/codex/bluey-jobs-20260710` and
  `origin/codex/bluey-branch-reconciliation-20260712` are also patch-equivalent;
- `origin/codex/bluey-stream-attachments-20260704` still reports the same 11
  historical patch-unique commits documented in Round 521.

Round 521 remains authoritative for that historical branch. Its required product
behavior was reimplemented or superseded on main, while its `0.1.91` release
metadata and older trial, device, provider, and runtime code are unsafe to merge
over the current implementation. It remains audit history, not missing launch
work.

The following Sashreek-owned refs were not opened, edited, merged, reset, or
deleted:

- `origin/agent/agent-bridge`
- `origin/agent/agent-bridge-fixes`
- `origin/agent/meeting-frontend`
- `origin/agent/parakeet-stt`
- `origin/meeting-main`

## Resume document boundary

Before this round, `jobs/portal/src/lib/documents.ts` was 1,126 lines and owned
four separate responsibilities: file import, resume parsing, profile merge policy,
and document export.

It is now a four-line compatibility facade. Existing imports continue to use
`lib/documents`, while implementation lives in focused modules:

| Module | Responsibility | Lines |
| --- | --- | ---: |
| `documents/import.ts` | PDF, DOCX, and TXT extraction plus file validation | 175 |
| `documents/parser.ts` | Resume section parsing and profile inference | 652 |
| `documents/profile.ts` | Import previews, replace/merge policy, and deduplication | 235 |
| `documents/export.ts` | PDF and DOCX generation and downloads | 73 |

The existing exported names and call sites are unchanged. The production build now
emits document export as its own lazy chunk instead of keeping that implementation
inside the original mixed module graph.

## Portal shell boundary

`jobs/portal/src/App.tsx` previously mixed the authenticated application controller
with the signed-out marketing screen, preview packet construction, and generic
loading/error UI. It fell from 894 lines to 703 lines by extracting:

- `components/AuthGate.tsx` - signed-out Jobs entry experience;
- `components/PageState.tsx` - loading and load-error states;
- `lib/preview-application.ts` - deterministic preview job and resume builders.

Routes, API calls, rendered copy, preview behavior, and component props remain
unchanged.

## Production asset parity

The checked-in `web/jobs` bundle was rebuilt from the refactored source. Old hashed
assets were removed by Vite and the generated `index.html` references the new
source-derived entry and existing stylesheet. This keeps a manual production deploy
reproducible without relying on stale asset names.

## Verification

Run from `jobs/portal` with the existing locked dependency tree:

```text
npm run typecheck
  passed

npm test -- --run
  8 test files passed
  37 tests passed

npm run build
  2,278 modules transformed
  production bundle completed

git diff --check
  passed
```

Rendered smoke checks used the rebuilt static output:

- desktop Matches preview rendered with the complete navigation, metrics, source
  health, and match list;
- 390 x 844 mobile preview rendered the compact header, two-column metrics,
  responsive source-health list, and bottom navigation without overlap;
- browser console errors: zero.

The temporary static file server does not implement the production Caddy SPA
fallback, so a direct reload of `/jobs/matches` returned the expected static-server
404 during the test. Navigating through `/jobs/?preview=1` rendered correctly; this
is not a portal runtime regression.

## Deliberate non-scope

This round does not mechanically split every large file. In particular,
`styles.css`, daemon `app.rs`, native overlay entry points, the server router, and
the Jobs database module each require a domain-specific extraction plan and their
own behavior or visual regression gate. Bundling those unrelated high-risk changes
into this source-only portal refactor would make review and rollback worse.

## Outcome

The two mixed Jobs portal files now have explicit ownership boundaries, all portal
tests and responsive checks pass, generated assets match source, and no reviewed
Codex-owned launch work remains stranded on a mergeable branch. Historical and
Sashreek-owned branches remain untouched as required.
