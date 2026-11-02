# IMPL644 — Jobs workspace foundation

Baseba24cc3d; branch `feat/phase-644-jobs-workspace-ux`. Preflight `bluey-ops`,
shared agent toolkit, UI/UX Pro Max; React quality checklist after TSX changes.

## Scope and implementation

First slice of the owner's full Jobs website revamp. Current React/Vite/CSS and
brand assets reused; no package, external font, framework, telemetry, API,
authority, fact, pricing or production change. Desktop sidebar exposes all six
routes; compact header shows current section. Existing five mobile destinations,
account/settings access, query preview, sign-out, refresh, balance and intervention
links remain. Overview's primary link uses its existing first data-derived next
action, not a new execution path. Source details remain honest and read-only.

Owned source: AppShell.tsx, styles.css, CareerCommandCenterView.tsx, App.test.ts
and CareerCommandCenterView.test.tsx. The existing view CSS was inspected but
unchanged. Parent owns design notes, docs, generated output and qualification.

## Review corrections

Light/dark muted tokens made readable; actual paired-color arithmetic was
checked by the worker (not a browser-composition claim). Desktop header no longer
reserves a hidden brand gap. Mobile labels raised from8px to10px. Overview CTA
gets a direct preview-query/read-only regression. Account popup's misleading
ARIA menu contract was removed in favor of ordinary disclosure/Tab semantics,
retaining Escape and focus return. Sidebar now scrolls in short desktop heights.

## Verification

Final isolated gate71049 terminated0 on source832 manifest
`3cd6234c37d86cade69c70c9cfb6d779a354d904bcf5c4a7336f7ec11314f094`.
It passed2520 unique cases: Node355, Vitest2086, Python31, release11 and managed
cloud37. Focused shell/Overview40 cases repeat full-suite cases and are not added.
Both actual Chromium fixtures ran; every runner reported zero skipped cases.
All five Jobs workspaces passed typecheck/build, strict audit/provenance and the
modern archive round-trip passed, and repeated portal builds plus fresh
pre-export inventory matched. Source/index, temporary root and home cleanup
were verified. No Rust source changed; this is not a new full Rust qualification.

Private evidence `phase644-node-fYMiQF`: controller journal
`c44f22d9248d5d5730b7bf293215a459925c541e0578fd2292db652c5bc63a9e`,
inner journal `1e83200e86253f1ed5c76253722b41d208375458bc1b27802098ca41c401f4aa`,
generated manifest `506f0afbc80615e22046bea6e9d1e10f134d4f96b67beaeb16a6b97ceb22f273`.
All35 generated files (4,846,777 bytes) were read back after synchronization;
21 obsolete generated names were removed, recoverable through Git and the
private previous-tracked-bundle backup. Actual staged privacy passed3003 tracked
paths/2719 text files. Earlier successful59729 is superseded, not current-source
proof. Independent final source/bundle review accepted this bounded checkpoint.

Actual Chrome checks covered all six routes at375/768/1024/1440 CSS widths in
both themes (48 cases), asserting the observed viewport each time: no horizontal
overflow and Preview visible in every case. At1440x400, keyboard focus reached
Settings inside the scrolled sidebar (top265.945/bottom309.945; scrollTop118).
Account Tab reached Jobs settings; Escape closed the disclosure and returned
focus. At375, the skip link focused jobs-main; a read-only job detail opened and
Escape restored the original tile focus. Preview query/deep links survived.

An IAB viewport override silently left every final matrix case at1440; that
matrix is excluded. Chrome's independently asserted dimensions supply the
responsive evidence. Actual reduced-motion emulation was not available in this
pass: the existing CSS guard was inspected, and no new animation was added, but
physical reduced-motion interaction and composed contrast auditing remain
follow-ups. Token-pair contrast arithmetic is not labelled browser-composition
proof. The local server is fixture-only, not saved-account/production evidence.

## Follow-ups

This is not the entire website or production release. Onboarding, matches,
resume/application detail, inbox/automation and Jobs-specific entry receive
successive reviewed UX batches. Actual runner/ATS/OAuth/economics/release remain
separate gates. No deploy, merge, publication, real application/email or new paid
resource is authorized by this visual batch. Asana1220128420788075 remains open.
