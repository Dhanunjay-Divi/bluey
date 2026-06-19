# Overlay Build Spec — mockup → Swift → backend (nothing broken or empty)

> **Contract:** `docs/design/bluey-overlay-redesign-v2.html` is the exact visual target.
> **Rule:** every surface is mapped two ways — (1) the Swift that renders it, (2) the
> backend command/event that makes it actually work. No dead buttons, no empty lists.
> **File:** `native/macos/cue-overlay/Sources/cue-overlay/main.swift` (one file → core
> done by me; leaf views by parallel agents). Backend: `crates/cue-core/src/overlay.rs`
> (IPC), `crates/cue-daemon/src/app.rs` (handlers). Build: `swift build`; verify via a
> capture-visible test bundle (user looks at checkpoints).

## 0. What already exists (the agent's rewrite left scaffolding)
- `BodyTab` enum + `configureBodyTabControl()` + `setBodyTab()` — the segmented switch exists.
- `presentBillingDisclosure()` + `configureBillingModal()` + `push_billing_disclosure` parse + `emitBillingDisclosureResponded` — G4 wired.
- `SessionSearchField`, `ShowMoreButton`, `SessionActionButton`, date-grouping, pins — History-at-scale scaffolding.
- Backend G1/G2 (pagination/pins) + G4 (billing) DONE and verified (clippy+tests green).
**So this is NOT build-from-zero. It's: fix the VISUAL execution (real transparency, kill clutter, exact mockup layout) + wire the CONTINUOUS THREAD + verify every surface is populated.** The agent's failure was rendering/layout, not absent plumbing.

## 1. THE CORE STRUCTURAL FIXES (me — not delegated)

### 1a. Real frosted transparency (the #1 visual bug)
- **Problem:** `refreshBackgroundChrome()` / `materialAlpha(base)` uses base 0.92–0.98 → panels are near-opaque (the "black box" the user saw). The mockup is `rgba(17,20,26,.52)` over a real blur.
- **Fix:** the overlay DOES have an `NSVisualEffectView` (`.hudWindow`/`.behindWindow`) at the window level — keep it. Lower the content-fill alphas to ~0.50–0.58 so the blur shows through. Panel shell, header, composer, rows: translucent fills + hairline borders, NOT solid dark. Keep the opacity slider working (it multiplies, floor ~0.45 so it never goes fully clear).
- **Backend:** none. `SetOpacity { opacity }` already drives the slider.

### 1b. Action-bar pill (PillView) — the hero
- **Mockup:** Stage 1 — `● Bluey │ [⌥A Ask] [⌥S Screen] [⌥L Listen] [⌥H History] ⋯`. Each a glass segment, hotkey shown, hover state. 5 run-states (Stage 5).
- **Swift:** `PillView` — rebuild from a label-only pill to an action bar of tappable segments. Each segment → an action callback on `OverlayApp`.
- **Backend wiring per action:**
  - Ask → opens Ask bar / focuses composer → emits `AskRequested { question, … }` on submit. ✓ exists.
  - Screen → emits `AnalyzeScreenRequested`. ✓ exists (daemon captures+answers, app.rs verified WORKS).
  - Listen → toggles `RecordingStartRequested`/`RecordingStopRequested`. ✓ exists.
  - History → opens panel on History tab (UI-only; data via `SessionsRequested`).
  - ⋯ (more) → opens the panel.
  - Run-states ← `ListeningStateChanged { state }` (Idle/Connecting/Listening/Paused/Failed). ✓ exists.
- **Global hotkeys (⌥A/⌥S/⌥L/⌥H):** the daemon owns the global hotkey (today F19 toggles). NEW: register ⌥S etc. — **GAP: needs daemon hotkey registration** (today only F19). Decision: wire ⌥S→AnalyzeScreen, ⌥A→show+focus-composer, ⌥L→record-toggle, ⌥H→show+History as global shortcuts. If global registration is heavy, MVP = the on-pill buttons work (always visible) + F19; hotkeys are a follow-up. **Flag for user.**

### 1c. Ask bar (Spotlight fast path)
- **Mockup:** Stage 1/1b — a floating frosted composer that drops, you type, the answer floats in below as one card, then tucks.
- **Swift:** a NEW lightweight window/view (separate from the full panel) OR reuse the panel collapsed to just composer+one-answer. Simpler: a dedicated `AskBarWindow` (borderless, same glass). Emits `AskRequested`; renders the streamed answer card from `UpdateCard`.
- **Backend:** `AskRequested` → daemon answer pipeline → `PushCard`/`UpdateCard` stream back. ✓ all exist & WORK.

### 1d. The continuous thread (Panel · Ask) — the heart
- **Mockup:** Stage 2 — ONE timeline feed: HEARD (transcript) → SCREEN·⌥S (capture+answer) → YOU·ASKED → BLUEY, each a `.turn` with a rail icon, connected. A context bar: "transcript · 1 screen · 2 turns".
- **Swift:** `FeedView` — today it renders `RenderedCard`s by kind. Extend so the 9 card kinds map to the timeline turn-types:
  - `transcript` kind → **HEARD** turn (italic, "INTERVIEWER · HEARD", from `PushCard{kind:transcript}` / transcript_final). *(Today transcript is suppressed from feed — CHANGE: show it as a heard-turn.)*
  - screen answer → **SCREEN** turn (the answer card from an AnalyzeScreen, labeled with a capture thumbnail). Distinguish via the card's `source`/kind.
  - `question` kind → **YOU · ASKED** turn.
  - `answer` kind → **BLUEY** turn.
  - context/action_item/decision/warning/fix_proposal → their own turn styling (kept).
- **Backend wiring:** the feed is **populated by `PushCard`/`UpdateCard`** (daemon pushes every turn). Transcript turns ← `transcript_partial`/`transcript_final` (✓ daemon streams these, verified WORKS). Screen turns ← the AnalyzeScreen answer card. The context bar is derived UI (count of transcript/screen/turns in the feed) — no new backend. **All wired; the change is RENDERING the existing pushed cards as a connected thread instead of disconnected bubbles.**

### 1e. Header (decluttered) + composer + "+" menu
- **Mockup:** Stage 2/6 — header = status dot/waveform + "Bluey · listening" + segmented switch + close. NO badge row, NO opacity on the face. Composer = "+" · field · Listen · send. The "+" menu = Attach · Capture browser page · New session · Recap · Answer style · Opacity (secondary only).
- **Swift:** `configureHeader()` — remove `routeBadge`/`knowledgeBadge`/`balanceLabel`/`canvasToggleButton`/`fullSizeButton` from the always-visible header row; keep them as hidden/contextual. `configureComposer()` — the "+" opens a popover menu (NEW small menu view) instead of the 5-button row.
- **Backend wiring per "+" item:**
  - Attach → `AttachRequested` (daemon opens picker, ingests). ✓ WORKS.
  - Capture browser page → `ActivePageCaptureRequested`. ✓ WORKS.
  - New session → `SessionNewRequested`. ✓ WORKS.
  - Recap → `RecapRequested`. ✓ WORKS.
  - Answer style → `InstructionsRequested` → editor → `InstructionsUpdated{text}`. ✓ WORKS.
  - Opacity → opacity popover → `SetOpacity` (or local). ✓.
  - Attachment chips ← `SetContextItems{items}`; remove → `RemoveContextRequested{id}`. ✓ WORKS.
  - Knowledge/indexing chip ← knowledge state (shown only when attaching). ✓.
  - Balance chip (managed mode only, hidden in agent mode) ← `SetBalance{label}`. ✓.

## 2. THE TAB SURFACES

### 2a. History (sessions at scale)
- **Mockup:** Stage 3 — search field, groups Pinned→Today→Yesterday→older, glass rows (title, project·turns, timestamp, hover-pin), "Show N more".
- **Swift:** the scaffolding exists (`SessionSearchField`, grouping, `ShowMoreButton`, `SessionActionButton`, the constraint-crash already fixed via `.width` alignment). Polish to match mockup exactly (glass rows, rail icons, spacing).
- **Backend wiring:** opening History / typing / show-more → `SessionsRequested{offset,limit,search}` → daemon replies `SetSessionsPage{sessions,total,has_more,query}`. Pin → `SessionPinRequested`/`SessionUnpinRequested`. Row click → `SessionOpenRequested{id}`. Rename → `SessionRenameRequested{id,title}`. Delete → `SessionDeleteRequested{id}` (+confirm modal). **ALL built & verified (G1).** `OverlaySessionItem` carries project/updated_at/turn_count/pinned. ✓ populated.

### 2b. Agents
- **Mockup:** Stage 4 — agent cards, capability badges (Active/Needs re-auth/Read-only/Cloud blocked), "X/Y connectors ready · N sessions", Use/Stop + View sessions.
- **Swift:** the agent drawer exists; restyle as cards in the body region (not an overlapping drawer). Capability→badge mapping exists.
- **Backend wiring:** list ← `AgentListRequested`→`SetAgents{agents}` (✓ real discovery via cue-agent-bridge, WORKS). Attach → `AgentAttachRequested{kind,session_id}` (✓ routes answers to agent, WORKS). Detach → `AgentDetachRequested`. View sessions → `AgentSessionsRequested{kind,offset,limit,search}`→`SetAgentSessions` (✓ G2; empty→show "enable history in Settings" prompt). Connectors → `AgentConnectorsRequested`→`SetAgentConnectors`. Capability/connector data is real (auth_tier/ready). **All wired.**

## 3. LEAF VIEWS (parallel agents — self-contained, exact mockup spec)

Each agent gets: the mockup HTML for its surface + the exact tokens + which existing Swift method to replace + the backend wiring. I verify each against the mockup + compile.

- **Agent L1 — Fix-proposal card** (`makeFixProposalView`): mockup Stage 7. Diagnosis/Reasoning/Fix + unified diff (+green/−red/@@accent) + Approve&apply/Reject + states (awaiting/applying/discarded). Backend: rendered from `PushFixProposal{proposal_id,diagnosis,reasoning,fix,diff,apply_supported}`; buttons emit `FixApprovalResponded{proposal_id,approved}`. ✓ exists & WORKS (verified) — this is a RESTYLE to glass.
- **Agent L2 — Canvas pane** (`CanvasPaneView`): mockup Stage 8. 5 kinds (code/system_design/screen/document/structured), split layout, copy/expand-full-window/collapse, mono syntax-tint. Backend: canvas content arrives in card artifacts (`UpdateCard{artifact}` / `CueCardArtifact`); copy=local, expand/collapse=local. RESTYLE to glass + match split layout.
- **Agent L3 — The 3 modals** (`configureCloseConfirm`, connector sheet, `configureBillingModal`): mockup Stage 9. Connector sheet (auth-tier rows, expired→"run /mcp in Claude" quiet status — G3, NO reauth event), BYOT billing (G4, already wired — RESTYLE), turn-off confirm. Backend: connector sheet ← `SetAgentConnectors`; billing ← `push_billing_disclosure`→`billing_disclosure_responded` (✓); turn-off → `CloseRequested`. RESTYLE to glass + the G3 reauth labels.
- **Agent L4 — Capability badges + agent cards** (the Agents tab card view): mockup Stage 4. Pure view styling against `AgentSummary` fields.

## 4. BACKEND GAPS TO CLOSE (so nothing is broken/empty)
| Gap | Status | Action |
|---|---|---|
| G1 sessions pagination/pins | ✅ built+tested | wire Swift History to it (§2a) |
| G2 agent-sessions paging + enable-prompt | ✅ built | wire Swift Agents (§2b) |
| G4 billing modal | ✅ Rust+Swift wired | restyle to glass (L3) |
| G3 connector re-auth | ✅ resolved (quiet "/mcp" label, no OAuth) | render label (L3) |
| **Global hotkeys ⌥A/⌥S/⌥L/⌥H** | ❌ only F19 today | daemon-side global shortcut registration — **decide: MVP (pill buttons + F19) vs full hotkeys**. Flag to user. |
| Ask bar window | ⚠️ new view | build (§1c) |
| Transcript-as-heard-turn | ⚠️ today suppressed | show transcript cards in feed (§1d) |

**Everything else (50 of 54 features) is already wired & verified end-to-end** (audio/STT, screen analysis, ask→answer, context/RAG, recap, instructions, session CRUD, agent attach+route, fix gen+apply). The redesign is overwhelmingly a **rendering/layout** job on working plumbing — plus the few items above.

## 5. SEQUENCE
1. **Spec approved** (this doc) + user decides the hotkey scope (MVP vs full).
2. **Me — core:** 1a transparency → 1e header/composer/+menu declutter → 1d continuous thread → 1b pill action-bar → 1c Ask bar. Compile each. **Checkpoint: launch capture-visible, user looks.**
3. **Parallel agents — leaf views** L1–L4 against this spec + mockup. I verify each vs mockup + compile.
4. **Wire-through pass:** confirm every surface populated by its real command/event (the table above) — no dead control, no empty list. A manual IPC drive (the probe) pushing each command to see it render.
5. **Final checkpoint:** user reviews the whole thing capture-visible; diff against mockup.

## 6. Open decision
- **Hotkey scope:** full global ⌥A/⌥S/⌥L/⌥H (needs daemon shortcut registration) vs MVP (always-visible pill buttons + existing F19)? Default proposal: ship pill buttons now (they're the visible UX win), add global hotkeys as a fast-follow so we don't block the redesign on hotkey plumbing.
