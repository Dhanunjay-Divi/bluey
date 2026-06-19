# FIX — Overlay runtime bugs (post real-daemon link)

> Status: diagnosis complete, fixes in progress (sequential, on `integration/agent-ui`).
> Context: after wiring the new Tauri overlay (`cue-overlay-tauri`) to the real daemon,
> live testing surfaced runtime/behavior bugs. The IPC plumbing is correct; these are
> behavior bugs on top of it. The "Ask" feature works (answers stream with latency via
> the managed route) — earlier claim that it was dead was wrong (RAG-embeddings being
> disabled is unrelated to the answer LLM).

## Confirmed bugs & root causes

### Group C — auto-ad-hoc-meeting lifecycle (root of #5, #6, #7)
- **#5 "Continuing Ad hoc meeting" / screen capture I didn't request**
  - `TranscriptAdd` auto-creates a meeting when none is active:
    `crates/cue-daemon/src/app.rs:1166` → `MeetingRecord::new(Some("Ad hoc meeting"))`,
    then `save_active` persists it (`:1179`).
  - `load_active` resumes ANY persisted `active.json` unconditionally on every boot:
    `crates/cue-daemon/src/storage.rs:24`. No staleness/ended check.
  - Net: a stray screen-analyze or audio segment creates + persists an ad-hoc meeting
    that the daemon then resumes forever on restart.
- **#6 History shows ad-hoc junk**
  - `overlay_session_items` lists `all_meetings()` (`app.rs:4168`); `all_meetings` always
    includes the active meeting (`storage.rs:78`). The auto-created ad-hoc therefore
    appears. Same root as #5.
- **#7 "In context: 0 turns"**
  - `turn_count = meeting.conversation.len()` (`app.rs:4210`). Overlay asks are not being
    appended to the active meeting's `conversation`, so it stays 0. Tied to meeting binding.

### Group A — agent discovery runaway loop (#1)
- `refresh_overlay_agents` is invoked on EVERY attach (`handle_agent_attach`, `app.rs:2117`)
  and detach (`:2243`), in addition to the Agents-tab click (`:1723`).
- `build_agent_summaries` discovery takes ~15s. Rapid "Use" clicks stack back-to-back
  rediscoveries → looks like a forever loop, burns CPU continuously.
- Fix direction: debounce/coalesce discovery; on attach/detach, update only the attached
  flag rather than re-running full discovery.

### Group B — agent attach has no feedback (#2)
- UI emits `agent_attach_requested` for ANY agent, including `cloud_blocked` /
  `needs_reauth` (`crates/cue-overlay-tauri/ui/index.html:595`).
- Backend `handle_agent_attach` runs but cannot succeed for those; UI shows nothing.
- Fix direction: gate the "Use" action on `capability`; push a status/warning card on
  refusal so the user gets feedback.

## Fix order (sequential, single coherent diff)
1. **C** — auto-meeting lifecycle (#5/#6/#7) — biggest, shared root.
2. **A** — discovery debounce (#1).
3. **B** — attach gating + feedback (#2).

## Implementation summary (done)

- **cue-core** `MeetingRecord::has_content()` — shared predicate (transcript/context/
  conversation/summary). Reused by boot-resume + History filter.
- **cue-daemon storage** `MeetingStore::discard_active()` — drop active file without
  archiving (for empty shells).
- **Fix C** (boot): `run()` now archives a content-ful leftover active meeting, discards
  an empty one, and discards an *unreadable/corrupt* one (fail-soft — corrupt active file
  no longer crashes boot). Always starts with no active meeting.
- **#6** (`overlay_session_items`): filters `!has_content()` meetings from History.
- **Fix A**: added `Daemon.agent_cache`; `refresh_overlay_agents` populates it;
  new `refresh_overlay_agents_attached_only` flips the `attached` flag from cache and
  re-sends WITHOUT rediscovery. `handle_agent_attach`/`handle_agent_detach` (the overlay
  paths) use it. Shared `is_agent_kind_attached()` keeps attached-flag computation
  identical to the discovery path (+unit test).
- **Fix B** (backend): `handle_agent_attach` always pushes an "Agent attached" card
  (capability-aware message) on success — no more silent attach.
- **Fix B** (UI, index.html): "Use this agent" / "Stop using" show optimistic
  "Attaching…/Stopping…" + disabled state on click.

## Verification

- **Fix C — VERIFIED end-to-end** (real daemon, planted active-meeting.json):
  - empty ad-hoc → discarded, not archived, not resumed ✓
  - content-ful → archived to `meetings/` (preserved), active cleared, not resumed ✓
  - corrupt file → discarded, daemon boots anyway (no crash) ✓
- **Fix A — VERIFIED**: idle daemon runs 0 discoveries (was looping every ~15s);
  each Agents-tab open = exactly one discovery; overlay attach/detach use the cache path
  (no `build_agent_summaries`). Unit test locks the cache flag-flip consistency.
  Real-user flow is safe: the Agents tab (which warms the cache) is the only place the
  "Use" buttons exist, so by the time the user mashes "Use", the cache is warm.
- **Fix B UI — VERIFIED** in browser preview: clicking "Use" flips the button to
  "Attaching…", disabled, dimmed.
- **Fix B backend card — verified by code+compile** (uses the same `push_system_card`
  proven elsewhere); the full real-overlay-click→card round-trip was NOT driven (overlay
  is screen-capture-invisible, can't be clicked in automation).
- fmt clean, clippy clean (warnings-as-errors), cue-core 103 + cue-daemon 231 tests pass.

## Related finding (NOT fixed — separate path)

- The **CLI** `DaemonRequest::AgentAttach` (`app.rs:~1503`) still calls
  `discover_agent_summaries` (full ~15s) on every attach. This is a different handler
  from the overlay's `handle_agent_attach`; it's a one-shot CLI command that returns the
  fresh list, so the latency is arguably acceptable there. Left as-is; noted for later.

## Not bugs (verified)
- "Ask" answering: works (managed route, delayed). No missing API key for answers.
- IPC bridge / socket: works (events flow both ways; verified in daemon log).
- View switching (Ask/History/Agents): works (earlier failed test used a lowercase
  tab name; real code uses capitalized names and switches correctly).
