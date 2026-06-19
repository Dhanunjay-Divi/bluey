# Overlay Redesign — Plan (panel internals + pill polish, backend-mapped)

> **Status:** Plan for approval. Branch `integration/agent-ui`.
> **Scope (confirmed):** Redesign the EXPANDED panel's internal UI + UX, polish the
> pill to match, leave the overlay *concept* (pill ⇄ expand, transparency,
> screen-share invisibility) intact. Bring it to the dashboard's aurora-glass
> language. Make the sessions list work at scale (100s). Get the click/focus
> model right. Map everything to the backend and make it actually work.
> **Constraint:** native Swift (`native/macos/cue-overlay/Sources/cue-overlay/main.swift`,
> ~7400 lines). Verified via ONE capture-visible build (user lifted the no-binary
> rule for that single verification; the rule otherwise stands).

---

## 1. What we learned (read-only discovery, line-cited)

Three parallel readers mapped the overlay. The findings:

### A. The sessions problem is BACKEND-deep, not just UI
- The daemon **pushes** sessions via `OverlayCommand::SetSessions { sessions }` and
  **caps at 8** (`overlay_session_items()` → `.take(8)`, app.rs ~4093).
- There is **no command for the overlay to request** more, to **search**, to
  **paginate**, or to **group**. The overlay literally cannot see session #9.
- `OverlaySessionItem` today = `{ id, title, subtitle, is_active }` — no project,
  no timestamp, no pinned flag, no turn count.
- **Therefore the redesign REQUIRES a new IPC contract** (below), plus daemon
  query logic. This is the "proper mapping to backend + working first" piece.

### B. The click model (overlay-takes-click vs background-takes-click)
- Two windows, both borderless + `isOpaque=false` + `backgroundColor=.clear` +
  `level=.floating` + `sharingType=.none` (capture-excluded).
- **Pill:** always opaque to clicks within its 158×32 frame; everything outside
  passes through to the meeting app.
- **Expanded panel:** region-aware. `isInteractiveAtScreenPoint()` (main.swift
  ~3930) is THE gate — returns true (overlay eats the click) only for: open
  modals, the session drawer region, resize edges, explicit chrome (header,
  composer, buttons), and **enabled** feed buttons. Everything else (card text,
  empty feed, padding, outside bounds) → **passes through to the desktop behind.**
- **Focus:** activation policy is `.accessory` in production → the overlay can
  take key focus to type in the composer **without** stealing the meeting app's
  foreground. `canBecomeKey=true`. A 60ms timer keeps the expanded window's
  `ignoresMouseEvents=false` while visible (a load-bearing workaround — do not
  remove).
- **ModalBlockerView** (~1115): when a modal is open, it captures all panel
  clicks + Escape; `isInteractiveAtScreenPoint` short-circuits to true.

### C. Fragility flagged by the readers (must respect in redesign)
1. `isInteractiveAtScreenPoint()` is a hard-coded checklist — **every new
   interactive element must be added to it** or its clicks fall through. → Move to
   a **registry pattern** (interactive views register themselves).
2. `HeaderDragView.hitTest` whitelists draggable-vs-interactive — new header
   controls must be whitelisted or they trigger window-drag instead of clicks.
3. The `ignoresMouseEvents=false` invariant (timer) must hold for the panel.
4. The 5 merge reconciliations (PillView, makeCardView/kindLabel, subview
   z-order, isInteractiveAtScreenPoint) need a verify pass.
5. Capture-visible gate is AND(dev_gate, capture_requested) — keep it.

---

## 2. The new design (panel internals + pill)

### State model — keep pill ⇄ expand, declutter the inside
- **Pill** (unchanged concept, polished look): 5 run-states (ready/connecting/
  listening/paused/failed). Refined-blue accent, hairline, glass. Drag, click→expand.
- **Expanded panel** — replace the competing drawers (session-drawer + agent-drawer
  + canvas, all overlapping) with **one body region + a segmented switch** in the
  header: **Answers ⇄ Sessions ⇄ Agents**. One thing visible at a time. Composer +
  meta footer stay fixed (the always-one-keystroke ask-loop).

### Sessions at scale (the core fix) — per the mockup `bluey-overlay-panel-redesign.html`
- **Search field** (⌘F) filtering all sessions live.
- **Grouped + dated**: Pinned → Today → Yesterday → Earlier this week → Older,
  and/or by project. Section headers.
- **Recent-first + "show N more"** — never floods; pages on demand.
- **Pin/favorite** — hover pin; pinned rise to the top group.
- **Clean glass rows**: title, `project · turns`, timestamp, selected state, hover
  pin. Inline rename (pencil) + delete (trash, with confirm) — preserved.

### Agents tab — fold the agent-drawer's picker/sessions/connectors into the same
segmented region (not a separate overlapping drawer), reusing the dashboard's
Agents UX (capability badges, attach/continue, connector sheet as a modal).

### Premium / one-product
- Aurora-glass: `#3B82F6` accent (retire neon cyan + the per-kind rainbow), white-
  alpha text, hairline borders, **no glow**, SF type scale matching the dashboard.

---

## 3. Backend contract changes (REQUIRED — the "mapping" work)

All in `crates/cue-core/src/overlay.rs` (+ `overlay_ipc.rs`), handled in
`crates/cue-daemon/src/app.rs`, mirrored in the Swift Decodables.

### New OverlayCommand (daemon → overlay)
- `SetSessionsPage { sessions: Vec<OverlaySessionItem>, total_count, has_more, query, offset }`
  — replaces the one-shot capped `SetSessions` (keep `SetSessions` as a thin alias
  for back-compat / initial paint).
- (optional) `SetSessionGroups { groups: Vec<SessionGroup> }` if we group daemon-side.

### New OverlayEvent (overlay → daemon)
- `SessionsRequested { offset, limit, search: Option<String>, sort: Option<String> }`
  — the overlay asks; daemon queries the store and replies with `SetSessionsPage`.
- `SessionPinRequested { id }` / `SessionUnpinRequested { id }`.

### DTO additions — `OverlaySessionItem`
- add `project: Option<String>`, `updated_at: String` (epoch/RFC3339),
  `turn_count: Option<usize>`, `pinned: bool`. (serde `#[serde(default)]` so old
  payloads decode.) Mirror in the Swift struct (main.swift ~446).

### Daemon query logic (`app.rs`)
- Replace `overlay_session_items()` `.take(8)` with a real query: filter by
  `search` (title/project), sort by `updated_at` desc (pinned first), window by
  `offset..offset+limit`, return `total_count` + `has_more`.
- Persist pins (a `pinned` set in settings or the store).
- Handle `SessionsRequested` → query → `SetSessionsPage`.

### Mapping discipline (matches the dashboard work)
- No `#[serde(rename_all)]` drift — keep snake_case verbatim so Swift Decodables
  match 1:1, exactly as we verified for the dashboard DTOs.
- A round-trip serde test per new command/event/DTO in `overlay.rs` (the file
  already has this pattern).

---

## 4. The click/focus model — explicit rules for the redesign

**Keep the model, fix the fragility.** The rule the redesign MUST preserve:

- **Pill visible:** overlay eats clicks in the pill frame; everything else passes
  through to the meeting.
- **Panel visible, no modal:** overlay eats clicks on interactive chrome + enabled
  controls; **card text / empty feed / padding pass through** so the user can still
  click their meeting behind the glass.
- **Modal open:** overlay eats all panel clicks (ModalBlockerView) + Escape.
- **Typing:** composer can take key focus WITHOUT activating the app to foreground
  (`.accessory` policy) — never steal the meeting's foreground.
- **Invisibility:** `sharingType=.none` always in production.

**Refactor (de-risk):** replace the hard-coded `isInteractiveAtScreenPoint()`
checklist with an **interactive-view registry** — any control the new panel adds
registers itself, so the new segmented tabs / search field / session rows / pins
are click-correct by construction, not by remembering to edit the gate. This
directly fixes the #1 fragility the readers flagged and is essential because the
redesign adds many new controls.

---

## 5. Build plan — phased, parallel where safe

**Reality:** `main.swift` is ONE file → Swift edits **cannot** be parallelized
(agents would collide). Parallelism is safe across the **Rust** side and the
**design/verification** tracks. So:

### Phase O0 — Backend contract (Rust) — CAN parallelize with O1 mockup
- O0a: add the new OverlayCommand/OverlayEvent variants + DTO fields in
  `overlay.rs` (+ serde round-trip tests).
- O0b: daemon query logic in `app.rs` (search/sort/paginate/pin, handle
  `SessionsRequested`).
- These two are different concerns but same-ish files → **one agent, sequential**
  (or me). Verify: `cargo build -p cue-core -p cue-daemon`, clippy, tests.

### Phase O1 — Finalize the mockup (design) — parallel with O0
- Extend `bluey-overlay-panel-redesign.html` to also show the **Answers** view and
  the **Agents** tab and the **polished pill**, so the full target is approved
  before Swift. (Me, fast, browser-previewable.)

### Phase O2 — Swift rebuild (ONE agent or me, sequential — NO parallel on this file)
- O2a: the interactive-view **registry** refactor (de-fragilize the click gate).
- O2b: header **segmented switch** (Answers/Sessions/Agents) replacing drawers.
- O2c: **Sessions-at-scale** view (search, groups, show-more, pins) wired to the
  new IPC (`SessionsRequested` ⇄ `SetSessionsPage`, pin events).
- O2d: **Answers** feed + composer re-skin (cards, fix-proposals, canvas) to glass.
- O2e: **Agents** tab (picker/sessions/connectors) to glass.
- O2f: **pill** polish + retire `BlueyTheme` neon-cyan → refined tokens.
- Each sub-step compiles before the next.

### Phase O3 — Verify (the one authorized capture-visible run)
- Build + run capture-visible, screenshot each state, confirm: sessions-at-scale
  works with many rows, search filters, pins persist, click passthrough still
  lets the desktop behind take clicks, composer typing doesn't steal foreground,
  invisibility holds (then delete the build).
- Verify the 5 flagged merge reconciliations live.

### What MUST survive (from the inventory — full checklist lives in the agent report)
Pill 5 run-states · expand/collapse · composer (Enter/⌘↵/Shift+Enter) · 9 card
kinds · streaming · copy-per-card · sign-in card · system/restore toasts · canvas
(5 kinds, open/expand/collapse/copy) · fix-proposals (diagnosis/reasoning/diff,
approve/reject, states) · sessions (rename/delete/new/continue-latest) · agents
(picker/sessions/connectors, attach/detach, capability chips) · knowledge/attach ·
opacity slider · hide/close/turn-off-confirm · Escape-dismiss · F19 · all IPC
commands/events. **Nothing dropped.**

---

## 5b. END-TO-END AUDIT — all 54 features (which actually work)

Two parallel auditors checked BOTH ends (overlay emits + daemon handles) of every
feature. Verdict: the backend is in good shape — **50 of 54 work end-to-end**
(audio/STT, screen analysis, ask→LLM→answer, context/RAG attach, recap,
instructions, session CRUD persists, agent discovery+attach routes answers, fix
generate+apply). **4 gaps** need backend/contract work beyond visual redesign:

| # | Feature | Status | Gap | Fix |
|---|---------|--------|-----|-----|
| G1 | Sessions list | ⚠️ capped | daemon sends `.take(8)` (app.rs ~4093); no search/paginate | new `SessionsRequested⇄SetSessionsPage` + pins (§3) |
| G2 | Agent sessions | ⚠️ capped | cap 40 (`AGENT_SESSION_LIST_CAP`); consent-off → silent empty list, no in-overlay "enable" affordance | same pagination + an in-panel "enable history" prompt instead of silence |
| G3 | Connector re-auth | ⚠️ stub | daemon logs + "not available yet" card (`TODO(slice-reauth)`, app.rs ~2423) | **RESOLVED (web-verified):** the agents do re-auth IN-AGENT (Claude `/mcp` or `claude mcp auth <name>`, own browser flow). Bluey does NOT reimplement OAuth. Overlay shows an expired connector as a quiet status + the exact agent command to reconnect (e.g. "login expired — run `/mcp` in Claude"). Overlay-side only; map agent kind→its re-auth command. No new Rust. |
| G4 | Billing disclosure (BYOT) | ❌ BROKEN | daemon REFUSES BYOT-agent attach until disclosure accepted, but overlay has NO `push_billing_disclosure` parser case AND no `BillingDisclosureResponded` emitter → user can never accept → BYOT attach fully blocked from overlay | BUILD the disclosure modal + response event in the Swift overlay (the daemon side already works, app.rs ~2149) |

**Consequence:** the redesign's backend scope = G1 (sessions page contract) + G2
(agent-sessions page + enable affordance) + **G4 (build the missing billing
disclosure UI + event — a real broken feature, blocks "borrow your own cloud
agent" from the overlay)**. G3 stays an honest deferred message (no big OAuth build
now). Everything else just needs the visual+UX redesign, not backend work.

Add G4 to the contract (§3): the Swift overlay must (a) parse
`OverlayCommand::PushBillingDisclosure { billing_model, vendor_short, ... }` and
render a premium consent modal, (b) emit `OverlayEvent::BillingDisclosureResponded
{ vendor_short, accepted, pending_kind, pending_session_id }` on accept/decline.
No Rust change needed for G4 (daemon handler exists) — it's overlay-side only.

## 6. Open decisions for the user
1. Group sessions by **date**, **project**, or **both** (date primary, project as a
   filter chip)? → default: date groups + a project filter.
2. Pins persisted in **settings** vs **store**? → default: store (travels with the
   session list).
3. Do O2 as **me** (sequential, careful) or **one delegated agent**? (Can't be
   multi-agent — one file.)
