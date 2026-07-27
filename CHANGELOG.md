# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **App-owned conversation memory.** Every in-meeting Q&A turn is stored in
  Bluey's own `conversation_turns` table (migration 012) and re-supplied to the
  agent each turn as a token-bounded block (rolling summary + verbatim tail).
  "Follow up on that" now works WITHOUT depending on the agent's resumable
  session — the foundation for driving agents ephemerally.
- **Agent-history retrieval.** New `search_agent_history` MCP tool surfaces the
  driven agent's prior coding-session reasoning, scoped to the attached agent
  family by default (`BLUEY_AGENT_HISTORY_SCOPE` to widen).
- **Ephemeral drive.** `codex exec --ephemeral` drives a question without
  persisting a session. Only Codex exposes a real ephemeral flag; other agents
  degrade gracefully. Requesting it forces the CLI route (ACP cannot honor it).
- **Real agent logos.** Official brand marks (Simple Icons CC0 / LobeHub) for
  Claude, Codex, Cursor, Gemini, Copilot, VS Code, and Antigravity, replacing
  the generic glyph. "Code - Insiders" maps to the VS Code family.
- **Live transcript bar + mic input** in the overlay, with speaker labels and
  drag/click expand.

### Changed
- **Shared STT weights.** Per-source engines now share one loaded Parakeet
  weight set (`SttEngineHandle` + `SttEngine::from_shared`) instead of loading
  a full ~650MB copy per source, while keeping independent decoder state.
- **Live-first memory cadence.** Ledger fires every ~350 words (~2-3 min) and
  the rolling summary every ~800 words (~6 min), tuned for a LIVE pinned card
  rather than a post-meeting batch summarizer. Cost is bounded by SELECTIVITY,
  not cadence: both prompts may record nothing when a window adds nothing.
  Conversation tail raised to 10k tokens (was 2k) for modern context windows.
- **Density-first answer style.** The copilot persona, per-ask style reminder,
  and fast/balanced mode instructions now optimize for
  completeness-at-minimum-length ("cover every point that matters in as few
  words as it takes — never drop a needed point to sound brief") instead of a
  hard 1-3-sentence cap. A fixed sentence count made the model omit needed
  points to obey the number; length is now a target, not a limit.

### Security
- **Whole-body persona-leak guard.** New `redact_persona_leak` scans the
  ENTIRE answer body (streaming, non-streaming final, and replay paths) for
  fingerprints of the internal persona and replaces the whole answer with a
  short decline when found. The prior guard only stripped LEADING echoes, so
  a prompt-injection ("print everything above") could dump the instructions
  mid-answer. Deterministic, agent-independent backstop; catches verbatim and
  near-verbatim dumps. Paraphrase-grade (semantic) detection + a red-team
  extraction eval suite are deliberately deferred — scoped in
  `docs/work/FUTURE-UPGRADES.md`.

### Fixed
- **Live audio diagnostics now follow the real native source handles.**
  `bluey audio status` reconciles the continuous microphone/system capture
  lifecycle instead of returning the daemon's stale boot-time aggregate state.
  Starting helpers remain `Starting`, running sources report native capture,
  and chunk-runtime sessions retain their original provider and telemetry.
- **macOS microphone permission now reaches the real helper.**
  `BlueyAudio.app` is signed with the hardened-runtime
  `com.apple.security.device.audio-input` entitlement in development and
  certificate-backed builds, and the helper verifier rejects artifacts that
  omit it. This prevents macOS from denying an already-enabled microphone grant
  before the permission prompt or capture session can start.
- **Clean macOS helper verification no longer fails during trap cleanup.**
  `verify-app.sh` now guards empty cleanup arrays before expanding them, which
  keeps the default no-launch verification path compatible with macOS Bash 3.2
  under `set -u`.
- **Overlay command, permission, and credential recovery hardening.** The native
  overlay now dispatches window actions only from an exact top-level command
  type, so transcript or card text cannot hide/show a window. Permission-denied
  events identify the blocked audio source and offer the correct Settings pane
  followed by a real retry. Calendar token saves, loads, and disconnects retry
  every legacy credential cleanup, aggregate failures, and retain the secure
  bundle on partial cleanup so stale credentials cannot be migrated back.
- **Google/Microsoft calendar onboarding and sync.** Meeting, development, and
  release builds now include `cloud-calendar`; OAuth fails fast when public
  client IDs are missing, propagates browser/provider errors, stores one atomic
  token bundle in the OS keychain, and activates or disconnects live sources
  without a daemon restart. Google and Microsoft incremental sync now preserve
  unchanged events, follow pagination, apply deletions, and periodically advance
  the rolling meeting window. The meeting-prep scheduler now observes refreshed
  cloud snapshots within 30 seconds instead of sleeping for up to five minutes.
  Provider operations are serialized, transient
  keychain initialization is retryable, and status distinguishes missing config
  or unusable authorization from a healthy connection. Calendar accounts remain
  manageable after onboarding, while public webhook ingress validates
  channel/client-state tokens and rejects placeholder credentials. Reconnect now
  stops the previous poller before publishing replacement tokens, and status,
  polling, and disconnect share one serialized provider store. Internal calendar
  warmup prompts never enter Bluey's meeting history, app-owned conversation
  memory, local RAG, or cloud sync; legacy warmup turns are filtered on read.
  Raw provider occurrence IDs remain distinct from Bluey's namespaced dedupe IDs.
- **Meeting overlay pill and shortcut controls.** Dragging the collapsed pill no
  longer triggers its expand click, so it can be placed freely on screen. The
  expanded window is clamped to the active display, the floating Ask shortcut
  now opens its input on one click, and system-audio/mic toggles change only the
  requested source instead of restarting or leaking the other native helper.
  Authoritative per-source state now survives collapse/expand, microphone-only
  capture creates a valid meeting session, and stopping the final source
  archives the meeting cleanly. Permission/audio failures remain visible while
  collapsed, and resize intents are serialized so a stale pill morph cannot
  shrink a newly expanded panel.
- **Meeting-prep banners are delivered once and stay private.** The banner
  webview now has its required event capability, ignores daemon retry duplicates
  after dismissal, queues simultaneous offers, and scopes responses to the exact
  calendar occurrence. It receives only banner commands, retains capture
  protection in normal mode, and positions itself inside the cursor display's
  work area.
- **macOS capture permissions no longer enter re-prompt/relaunch loops.**
  `BlueyAudio.app` and the Screen Recording helper now keep stable
  certificate-backed code requirements when available, development and archive
  installs preserve valid bundle signatures, and permission/setup failures stop
  audio-helper respawning until an explicit retry. Clean Make and GitHub release
  builds now require, stage, and verify the signed audio app beside the daemon
  and in dashboard resources, then sign and strictly verify the fully assembled
  outer macOS app. The restricted `persistent-content-capture` entitlement is
  now opt-in and requires a validated matching provisioning profile; a
  permission-free direct/LaunchServices probe catches AMFI launch failures
  before installation. Onboarding now explains the two distinct first-use
  audio grants.
- **On-device transcription starts warm without duplicate model loads.**
  Keyless local-STT builds now provision and prewarm Parakeet during startup,
  single-flight simultaneous microphone/system initialization, and keep
  transcript content out of synchronous inference-thread diagnostics. Configured
  cloud/LocalWhisper users retain the existing no-local-download behavior.
  Source-specific STT providers are created before capture begins, so an
  initialization failure is surfaced instead of showing a false `Listening`
  state while silently discarding PCM.
- **Multi-source capture shutdown and idle races.** Capture and task handles are
  removed as one generation before asynchronous shutdown, preventing an older
  terminal monitor or rapid off/on toggle from stealing a replacement task.
  The system-audio idle watchdog also defers while microphone capture is active,
  so it cannot archive a meeting based only on quiet system audio.
- **Cross-channel transcript retries no longer erase real replies.** Deduplication
  compares final events only and requires longer content before treating exact
  text from different speakers or capture sources as a retry, so a short reply
  such as "okay" remains visible while true same-source retries are suppressed.
- **Development reinstall no longer leaves an older daemon serving the overlay.**
  The reinstall script requests a full quit, waits for daemon exit, and fails
  before replacing binaries if a stale process remains. Visible dev-overlay
  startup also verifies the explicit overlay path without a blocking
  current-directory lookup before daemon IPC binds.
- **Live-memory concurrency fixes (adversarial review findings).** (1) The
  rolling-summary inflight guard now clears via a Drop guard, so a panic
  anywhere in the pass can no longer leave the flag stuck true and silently
  disable summaries for the session. (2) The summary task no longer calls
  `save_active` off the meeting lock — segment commits save the whole record
  UNDER the lock, so the off-lock save could clobber newer transcript segments
  on disk (lost update); persistence now piggybacks on the next segment commit
  or the meeting-end archive. (3) A slow ledger extraction that outlives its
  meeting no longer merges into the NEXT meeting's ledger (meeting-id guard);
  cross-meeting fact indexing is unaffected since facts carry their own
  meeting id.
- **Installer: "Apple could not verify" Gatekeeper popup on AirDropped/downloaded
  builds.** `libopenblas.0.dylib` (the diarization dylib the daemon loads at
  startup) ships read-only from Homebrew, and macOS `xattr -d` cannot remove the
  `com.apple.quarantine` flag from a read-only file — so the strip silently failed
  on exactly that dylib, leaving it quarantined and triggering the popup. install.sh
  now `chmod -R u+w` the install upfront so quarantine can be stripped from every
  file, verifies the strip took, adds `spctl --add` for the .app bundles, and prints
  the exact `xattr -dr` fallback command if anything is left. Ad-hoc signing alone
  does NOT satisfy Gatekeeper (no Apple Developer account here); removing quarantine
  is the actual cure for the daemon-spawned subprocesses.

### Added
- **Cloud OAuth calendar ("Connect Google / Connect Microsoft").** New
  cross-platform `cue-calendar-cloud` crate: native public-client OAuth
  (PKCE + loopback redirect per RFC 8252, client_id only — NO client secret),
  tokens on-device in the OS keychain (`bluey_calendar_google` /
  `bluey_calendar_microsoft`). Google Calendar (`events.list`) + Microsoft
  Graph (`calendarView`) clients map into the existing `CalendarSource` seam
  as background-snapshot sources — the warmup trigger loop is untouched.
  Gated behind the `cloud-calendar` daemon feature (cross-platform, unlike
  the macOS-only `calendar`/EventKit feature); `default_source()` prefers a
  connected cloud provider after the env-fake test hook, ahead of EventKit.
  New IPC `CalendarConnectStart/Status/Disconnect` + `CalendarStatus`
  response, overlay onboarding connect buttons, and CLI rendering. Shared
  calendar types moved to `cue-core::calendar` (cloud crate depends on core,
  never on the daemon). HARD PREREQUISITE: register OAuth apps (Google Cloud
  Console Desktop client; Azure mobile-and-desktop platform) and inject
  `BLUEY_GOOGLE_CLIENT_ID` / `BLUEY_MICROSOFT_CLIENT_ID` at build — until
  then the flow builds and tests green but cannot authenticate live. See
  `docs/work/IMPL-CLOUD-CALENDAR.md`.
- **The MCP-backend pivot: Bluey becomes the memory the agent PULLS from.**
  Bluey no longer only pushes context into agent prompts — the daemon now runs
  its OWN loopback MCP server (`cue-mcp`, hand-rolled over the already-in-graph
  hyper: zero new transitive deps) exposing four read-only tools
  (`get_recent_transcript`, `get_meeting_summary`, `search_meeting_decisions`,
  `search_past_meetings`) backed by the existing stores. Security per OWASP
  MCP: 127.0.0.1-only, Host allow-list, per-meeting rotating bearer token,
  read-only surface (a prompt injection spoken into a meeting has no exfil
  vector), strict schemas. Every design point was LIVE-verified first
  (Batch-0 spike across claude/gemini/copilot/cursor/codex: all fire
  loopback-HTTP MCP tools headlessly with zero prompts).
- **Warm meeting-backend session.** `WarmupStart` rotates the token, registers
  `bluey-memory` into the attached agent's own MCP config (write-side
  `mcp_register.rs`: scriptable `mcp add` for claude/copilot/codex, surgical
  project-JSON merge for gemini/cursor — Bluey never holds agent credentials),
  mints the meeting, and runs a warm-up drive whose session becomes THE
  session every in-meeting ask resumes. Live acceptance: a mid-meeting ask
  resumed the warmed session and answered the standup's reversals from live
  tool pulls.
- **Calendar trigger.** A deterministic poll fires the warm backend T-minus
  3 min, once per (event, occurrence), retrying while gates (agent not yet
  attached) refuse — proven by deliberately reproducing the trigger-before-
  attach race live. Env-fake source drives the full path headless
  (`BLUEY_CALENDAR_FAKE_EVENTS`); EventKit follows behind a `calendar`
  feature.
- **Context-coverage meter (onboarding).** `SourceCoverage` IPC classifies the
  attached agent's connectors against the meeting-relevant sources
  (calendar/slack/email/tickets + bluey-memory) and the overlay's Agents tab
  renders connected/missing chips with guided connect commands (the user
  authorizes in THEIR agent).
- **Hybrid cross-meeting retrieval — the mem0 v3 search pipeline in Rust.**
  Recall over the facts memory is no longer cosine-only: it now fuses
  semantic similarity + Okapi BM25 over stemmed fact text (sigmoid-normalized
  with mem0's query-length-adaptive params) + entity boosts from a linked
  entity store (facts' proper names / identifiers / quoted terms, extracted
  POS-free and linked at write time). The scoring math is pinned by parity
  tests against values produced by RUNNING mem0's own `scoring.py`; the
  relevance floor still gates the SEMANTIC score before fusion (mem0's
  threshold contract), so the verified precision behavior is unchanged and
  fusion only improves ranking. Measured on a meeting-shaped eval (real
  bge-small): hybrid ≥ cosine baseline with zero regressions; the eval also
  caught and fixed a real small-embedder failure mode (bge-small clusters
  short acronyms — "SLA"≈"SSO" cleared mem0's 0.5 entity floor and falsely
  boosted the wrong fact; non-exact short-entity matches now require 0.85).
  Store schema v2 migrates transparently (stemmed-text backfill, new
  `fact_entities` table).
- **Mem0-in-Rust: the full two-phase memory pipeline (PLAN-CONTEXT-WARMUP
  Appendix E).** Phase 1 (extraction via the attached agent's throwaway
  one-shot, quote-verified) now feeds phase 2 — an agent-decided
  ADD/UPDATE/DELETE/NONE consolidation pass ported near-verbatim from the Mem0
  paper's update prompt, with the source-verified hardening tricks from mem0
  v3 (integer display-id indirection so the agent can never hallucinate store
  ids, exact-hash dedup BEFORE the agent call, empty-neighborhood fast-path
  that skips the drive entirely). DELETE lands as a Zep-style SUPERSEDE
  (validity window closed, row kept queryable) because meeting decisions get
  REVERSED, not erased. Every mutating op is recorded in a `facts_history`
  audit table (mem0's history-log pattern). No agent attached / unusable
  output → the cosine-similarity heuristic, so memory works headless.
  Real-model + real-`claude -p` integration test verifies a reversed decision
  is superseded end-to-end.
- **Two-stage question detection (SET 1).** The lexical `is_question` check
  now has an ONNX stage 2: `shahrukhx01/question-vs-statement-classifier`
  (int8, ~11MB) runs via `ort` on regex REJECTS only, catching the disfluent
  questions real meeting speech is full of ("so um do we need the flag or
  not"). The model is exported once by `scripts/export-qdetect-onnx.sh` (with
  an int8-vs-pytorch parity gate) and ships in the AirDrop tarball next to the
  daemon (`bin/models/qdetect-en`); absent model → regex-only detection, never
  fatal. Speaker/name gating is unchanged and stays in cue-core
  (`detect_for_me_question_given` seam).
- **Manual "ask about what was just said" button (SET 3).** A one-tap button
  in the meeting overlay composer fires a canonical ask through the EXACT
  pipeline typed questions use — the daemon already attaches the rolling
  summary + decisions ledger + recent transcript to every ask, so the answer
  is grounded with no typing. The escape hatch for when detection misses.
- **One-click install for a missing agent CLI.** When the attached agent's CLI
  isn't on PATH (e.g. `copilot`) but has a vetted install recipe, Bluey now shows
  an Install / Not now card in the Ask screen instead of only telling you to
  install it manually. Approving runs the exact vetted command
  (`npm install -g @github/copilot`) via the existing `provision.rs` engine (which
  preflights, clears safe obstructions, installs, and verifies the binary actually
  appears + runs), then reports the outcome as a card. Bluey NEVER signs you in —
  after install you sign in and ask again. Wired via new
  `OverlayCommand::PushAgentInstall` / `OverlayEvent::AgentInstallResponded`, with
  a snake_case kind that round-trips through `parse_attached_agent` (tested).
- **Meeting-only macOS build (`scripts/build-meeting.sh`).** A distinct build that
  stages ONLY the meeting overlay (`cue-meeting-overlay`), never the legacy Swift
  interview overlay (`bluey-overlay-macos` / `cue-overlay-macos`). The shared
  `build-macos.sh` ships both, which let the daemon launch the WRONG UI (the
  interview overlay) in place of the meeting overlay; with the interview overlay
  simply absent from the distribution, that can no longer happen. Additive — it
  changes no shared code and does not remove the interview overlay from the repo
  (that is the other product's surface); it is purely a build that excludes it.
  Pairs with `scripts/package-airdrop.sh` for the AirDrop tarball.

### Fixed
- **Meeting lifecycle: no tail-final re-fragmentation after a meeting ends.**
  Follow-up to the one-meeting-per-session fix: when listening stopped,
  `stop_audio_capture` joined only the capture supervisor, not the detached
  STT/sink task — so trailing finals still draining from the queue committed
  *after* `auto_end_active_meeting` archived the meeting, found no active meeting,
  and spawned a fresh never-ended 1-line fragment (the exact bug the lifecycle fix
  set out to remove). Now `stop_audio_capture` awaits the outer STT/sink task
  (bounded: provider flush + finite queue drain) before any caller auto-ends, so
  the tail finals land in the still-active meeting first; the idle path takes its
  own JoinHandle before that join to avoid a self-join deadlock. As defense in
  depth, an audio segment can only *create* a meeting while capture is live — a
  straggling final after capture stopped is dropped, never re-fragments.
  `start_new_session` now also archives the previous meeting through the single
  shared end path (was inline, which skipped the ledger reset and bled the prior
  meeting's ledger into the new one).
- **Meeting lifecycle: one meeting per listening session (no more "Ad hoc
  meeting" fragments).** History filled with junk because a too-eager create path
  minted a fresh meeting per transcript line and titled each from its first line.
  Now a single session meeting is created at listening-start (create-iff-none,
  generic time-based "Meeting HH:MM" title) and every segment + Q&A coalesces into
  it; it auto-ends (archives — never discards) on stop (`AudioStop` /
  `RecordingStopRequested`), explicit `MeetingEnd`, and idle-silence, all through
  one shared `auto_end_active_meeting` path that runs only AFTER audio capture has
  stopped (so it can never archive a live recording). Titles are upgraded from the
  recap ONLY at end, and a user rename that happens to start with "Meeting " is
  preserved (strict "Meeting HH:MM" shape check, never a loose prefix match). The
  History list is filtered by a stricter `meeting_is_substantive()` (2+ committed
  units, a written summary, or attached context) so 1-line fragments and empty
  active shells no longer appear. Also repaired a pre-existing compile break in the
  `live_transcript_emit` integration test (drifted `LiveTranscriptEvent` fields).
- **Continue now restores the FULL meeting context, not just the transcript.**
  "Continue in Ask" re-seeded the transcript + Q&A but never re-attached the
  meeting's linked agent thread — so the agent conversation was missing. It now
  also re-attaches the meeting's agent session (on the agent kind the meeting
  actually used, falling back to the attached agent for legacy links) in the same
  action.
- **Opening a past meeting is instant on re-open.** The read-only viewer re-fetched
  `openMeeting` on every click (round-trip + "Opening…" spinner). Opened views are
  now cached by id (a past meeting's snapshot is immutable), so re-opening one is
  instant. (First open still fetches once; the daemon side was already fast —
  ~32ms to read all meetings.)

### Added
- **Continue a past meeting in the Ask screen.** Selecting a past meeting in
  History now offers "Continue in Ask →" — it makes that meeting the ACTIVE one,
  so the Ask screen shows its transcript + Q&A and new questions continue within
  it (was: past meetings could only be viewed read-only in the History tab, and
  the Ask screen was stuck on the live meeting). SAFETY: a live recording is never
  lost — the daemon detects an in-progress recording across BOTH audio paths
  (`audio_runtime` REST/cloud AND `system_audio` the default keyless streaming)
  and BLOCKS the switch with guidance ("stop listening first") when audio is
  capturing and the target differs; only an idle active meeting is archived (ended
  + recap + saved) before activating the selected one. The read-only viewer stays
  as an additive alternative. Backed by a pure `continue_decision` function with a
  machine-checked test proving a live recording can never enter the archive path.

### Fixed
- **Overlay History/Agents screens left dead space at the bottom** and
  double-scrolled: the list roots hardcoded `maxHeight: 480` and App's tab body
  scrolled on top of each screen's own scroll. The screens now `flex: 1` to fill
  the tab body and scroll internally (single scroll), so the panel uses the full
  window height.
- **Model picker lagged on every agent switch**: AskScreen fetched the model list
  in a per-mount effect with local state, so the picker stayed hidden until a
  fresh daemon round-trip. Models now live in the shared SWR `DataStore` (cached
  per kind, revalidated in the background) and are prefetched the moment an agent
  is attached — so the picker appears instantly on re-attach.

### Changed
- **Codex desktop app is now detected** (`/Applications/Codex.app`), so a machine
  with the Codex app but not the `codex` CLI still surfaces Codex. Its
  `Application Support/Codex` dir is an Electron browser profile (not a session
  store), so it is deliberately not scanned for sessions — the app's readable
  threads already come from `~/.codex`.

### Added
- Per-agent **model override** for the attached agent: a vendor model id
  (e.g. `composer-2.5`, `gpt-5.1-codex`) carried on `CueSettings.attached_model`
  and on the `AgentAttachRequested` overlay event, seeded onto the drive via the
  registry row's `model_flag` (a no-op for flagless agents), pushed into
  `tried_models` so the ModelBlocked resolver never re-proposes a blocked pick,
  and cleared on detach. Preserved across conversation-chaining, dropped on the
  BYOT re-attach round-trip (cloud rows have no model flag).
- **Model flags** enabled for Cursor / Copilot / Antigravity (`--model`,
  live-verified 2026-07-03); fallback model lists stay empty since vendor ids are
  release/plan-dependent, so a flag stays inert until the user picks a model.
- **Speed→effort** control: the overlay fast/balanced/deep picker now maps to
  per-run reasoning-effort argv on agents with a live-verified effort flag —
  Codex (`-c model_reasoning_effort=…`, live-verified). Prose speed instructions
  remain for every agent (effort args are additive, never a replacement).
  Copilot's `--effort` was live-refuted (the default `auto` model hard-errors on
  it) so Copilot stays prose-only.
- **Cursor native resume is now ledger-gated**: `cursor-agent --resume=<id>`
  resumes only Bluey-minted CLI sessions from their original cwd (a wrong cwd or
  unknown id silently mints an empty session), so Cursor resume by id requires
  provenance from the new spawn-time session ledger; unproven ids degrade to
  replay.
- **Spawn-time agent-session ledger** (`agent-session-ledger.jsonl` under the
  daemon data dir): records `{agent, session_id, cwd, spawned_at}` for
  Bluey-minted CLI sessions so cwd-scoped resume never depends on store drift.
  Resolved ledger-first (an undiscovered agent can still resolve its cwd),
  fail-soft (missing/corrupt lines skipped), no fsync/rotation (optimization
  layer; a miss falls back to the store re-scrape).

### Changed
- **Seamless, no-redundant-load overlay (stale-while-revalidate).** Agent
  discovery is ~14s (installs + connectors + session counts), and the overlay
  re-ran it on every mount / tab switch, so agent selection lagged and every
  screen threw away loaded data and refetched. Now:
  - **Daemon serves the agent cache instantly.** `AgentListRequested` pushes the
    cached agent list immediately (attached flag recomputed from live settings)
    then refreshes in the background and pushes the update — the ~14s discovery no
    longer blocks the paint. First-ever call (cold cache) still discovers once. A
    cache-epoch guard prevents a background refresh from clobbering a just-attached
    state, and an in-flight guard drops overlapping refreshes.
  - **Frontend shared data store** (`DataProvider`, modeled on `MeetingProvider`):
    agents, per-kind agent sessions, and meetings are held once and shared across
    tabs — cached reads render instantly with no spinner, and revalidate in the
    background. A single subscription to the daemon's pushed `set_agents` keeps it
    live (attach/detach/refresh). No more per-screen mount refetch.
- **History is one tab with a `[Meetings | Sessions]` sub-toggle** (was two
  separate "Meetings"/"Agents" history tabs). Meetings = past meetings; Sessions =
  the agent-session list. The "Agents" tab (attach/detach) stays its own tab.

### Added
- **Meeting history — two distinct lenses.** The overlay now has a **Meetings**
  tab (your past meetings, newest-first, with transcript/turn counts + a preview)
  — open one to VIEW its full transcript + Q&A, so "continue an existing meeting"
  shows its prior exchanges. Kept separate from the **Agents** tab's agent-session
  list (resume a Claude/Cursor thread). Backed by new `meetings_requested`/
  `set_meetings` and `meeting_open_requested` IPC over the store's existing
  `all_meetings()`/`load_by_id()`. SAFETY: opening a past meeting is a PURE READ —
  it never writes the active meeting, and the daemon marks the view read-only
  whenever a live/active meeting exists, so viewing history can never clobber an
  in-progress recording.
- **Meeting ↔ agent-session link.** A meeting records the agent session id AND
  kind it was chained to (`MeetingRecord.agent_session_id` + `agent_kind`, both
  `#[serde(default)]` for legacy meetings), stamped on every answered turn
  (including stable-id resumes). Opening a meeting offers "Resume agent thread →"
  which reattaches THAT agent + session (not whatever is currently attached);
  legacy links without a kind fall back to the attached agent.

### Fixed
- **Overlay lost the whole session on collapse-to-pill** (and on a full restart):
  the panel was UNMOUNTED when collapsed (`if (collapsed) return <Pill/>`), which
  destroyed the React state that owned the transcript, message history, and Q&A —
  reopening showed a blank session even though the daemon still had everything.
  The overlay is now a VIEW, not the owner: (A) the panel stays mounted and is
  hidden via `display:none` on collapse, so state survives instantly; (B) session
  state moved to a `MeetingProvider` above `<App/>` that, on mount, rehydrates from
  the daemon via a new `meeting_state_requested`/`set_meeting_state` IPC (reads the
  active `MeetingRecord`'s transcript + conversation) — so even a cold overlay
  restart repaints the meeting so far. Snapshot and the live `onTranscript` stream
  reconcile by STABLE per-segment id (the live push now carries the persisted
  segment id + channel source via a shared `speaker_channel`, so seed↔live can't
  duplicate or mislabel). Foundation for showing prior exchanges when a meeting
  continues. (See `docs/work/DESIGN-CONTEXT-REDUNDANCY.md` sibling design notes.)
- **Redundant per-turn context on a resumed session** (send-heavy-context-once):
  every ask re-sent the entire meeting package — pre-meeting brief, saved summary,
  back-history transcript, and Bluey's own prior Q&A (including internal
  scaffolding chatter) — even after the attached agent already had that history.
  Now, after the first answered turn of an attached session (tracked by a new
  `attached_context_primed` marker), later turns send only the always-pinned delta
  (decisions ledger + recent transcript) plus the new question; the heavy blob is
  sent ONCE. Research confirmed all six agent CLIs reload their own transcript on
  resume, so re-sending it was pure duplication. The marker resets whenever the
  attached session changes (re-attach / detach), so a new session re-primes; a
  model-only re-attach preserves it. The mid-meeting-decision contract holds — new
  pinned decisions still reach the agent every turn. (See
  `docs/work/DESIGN-CONTEXT-REDUNDANCY.md`.)
- **On-device STT silently absent from dev builds**: `scripts/run-local.sh` ran a
  plain `cargo build`, which omits the `cue-daemon/parakeet-stt` feature — so
  `bluey on` launched an STT-less daemon and `Listen` fell through to the cloud
  "sign in for transcription" gate, producing zero transcript. Now builds with the
  feature (matching the packaged installer, `scripts/build-macos.sh`).
- **Diarization model now auto-downloads on demand**: the first-run fetch pulled
  only the three STT files, so enabling `BLUEY_STT_DIARIZE=1` on a fresh install
  warned "sortformer model not present" and silently ran without speaker labels.
  The ~490MB Sortformer model is now fetched once, on first diarized run, from a
  verified-public source (`cgus/diar_streaming_sortformer_4spk-v2.1-onnx`) —
  overridable via `BLUEY_SORTFORMER_MODEL_URL`; a failed fetch degrades to
  transcription-only rather than erroring STT bring-up. It stays out of the default
  download so the 99% who don't enable speaker labels never pay the 490MB.
- Overlay discovery probed `target/{debug,release}/` but `--target` builds land in
  `target/<triple>/<profile>/`, so `bluey on` launched a stale overlay binary. Now
  probes the apple-darwin triple dirs (debug-first) before the plain fallback.
- Live STT lag + eaten/scrambled words (three distinct bugs, all diagnosed on
  real-time-streamed VoxConverse audio — see `docs/TRANSCRIBE-BUILD-PLAN.md`):
  - **Bursty/laggy transcript + lost audio** — the audio→engine feed VAD-gated
    (dropped "silence" frames → holes in the timeline) and sent variable-size
    chunks, desyncing Parakeet's cache-aware streaming (output batched into ~9s
    bursts). Fixed: feed a CONTINUOUS, UNIFORM 100ms stream (no VAD gate, fixed
    chunk size). Measured: bursts 86→9, max gap 9.0s→3.4s, steady ~593ms emit.
  - **Growing lag** — 20ms chunks forced ~50 full-buffer mel recomputes/sec in
    parakeet-rs (RTF 1.29×) → unbounded backlog. Fixed by the 100ms coalescing
    above (RTF 0.25×, backlog flat at 0).
  - **Scrambled/dropped words** — a detached `tokio::spawn` per segment committed
    out of order on the multi-thread runtime, corrupting the dedup tail. Fixed
    with a single ordered sink task (FIFO commit, non-blocking handoff).
- Diarization no longer stalls STT: `label_segments_by_overlap` did a ~50-200ms
  disk write (`save_active`) under the `meeting` lock, blocking the STT sink every
  tick. Now stamps ids under the lock, saves off-lock → STT + diarization run truly
  in parallel (verified: STT byte-complete with diarization firing every 5s).
- Stable live speaker ids: `LiveDiarizer` mapped windows by centroid, but speakrs
  per-run centroids aren't comparable across `diarize()` calls (~0 self-similarity)
  → id churn. Now maps by TIME OVERLAP with the previous window (append-only,
  arrival-ordered) → stable `{0}→{0,1}→{0,1,2}`, verified on VoxConverse.

### Added
- Verified decisions ledger (master doc §5): every N transcript turns
  (`BLUEY_LEDGER_INTERVAL_TURNS`, default 15) a **stateless, cheap-lane** LLM
  pass extracts decisions / constraints / owners from the recent window. Reuses
  the existing provider abstraction — **no bundled model, no new dependency** —
  and forces the Instant/cheap lane (managed-instant → `gpt-4o-mini` → skips if
  none configured) so the expensive answer model and the user's session are never
  touched. Every extracted item is gated by an anti-hallucination harness in
  `cue-core::ledger` (`parse_and_verify`): an item survives only if its `quote`
  is a verbatim substring of the transcript, and a `speaker` is kept only if it
  actually appears in the window. Verified items merge into a capped, deduped
  `LedgerState` rendered as a pinned context block inserted ahead of everything
  else on the answer path (survives context compaction). Off by default
  (`BLUEY_LEDGER=1`); the keyword-heuristic ledger remains the zero-cost floor.
  Fires on both the live-audio and `TranscriptAdd` IPC paths. See
  `docs/LEDGER-PLAN.md`.
- On-device English STT via Parakeet (Nemotron, `parakeet-rs`/ONNX): system-audio
  capture runs through the streaming `SttProvider` trait as the keyless local
  backstop (no cloud key / account required — nothing leaves the machine).
  First-run model auto-download (atomic, resumable) from a public int8 mirror,
  env-overridable via `BLUEY_PARAKEET_MODEL_URL` / `BLUEY_PARAKEET_MODEL_DIR`.
  Real inference verified end-to-end on native arm64 (`parakeet_real_inference`
  test). (The earlier chunk-file mic loop was superseded by the continuous
  streaming path — see Changed.)
- Speaker labels: transcript renders conversational `You` / `They` (mic vs
  system) for both the agent context and the recap (`Speaker::display_label`).
- Question→trigger (master doc §6): a final line from another speaker that is
  question-shaped and (if `my_names` is set) mentions the user is surfaced as a
  for-me question — suggest card by default, or auto-drive the attached agent
  (`auto_trigger_enabled`). Fires on both the live-audio and `TranscriptAdd` IPC
  paths. New `detect_for_me_question` in cue-core; `my_names` /
  `auto_trigger_enabled` settings.
- Pinned context blocks always sent ahead of the recency transcript: a decisions
  ledger (decisions + open commitments) and a pre-meeting brief (new
  `cue_core::prestage` module) so a minute-5 constraint still reaches the agent
  at minute 40.
- Overlay answer-speed picker (fast / balanced / deep) wired UI → daemon, now
  producing genuinely distinct answer instructions (was previously cosmetic).
- `BLUEY_OVERLAY_BIN` env override to point the daemon at a specific overlay
  binary / `.app`.
- `bluey agent prove` — a capability matrix that probes EVERY known agent on the
  machine and reports, per agent, what genuinely works at the highest honest
  level: LIVE (exercised against the real installed agent), FIXTURE (reserved
  for real captured samples), or SKIP (with the concrete reason — e.g. "CLI not
  on PATH (install it to drive)"). Read-only: it really lists sessions, reads
  connectors, and checks install/drive readiness, but never installs or drives.
  Turns "test against all applications" into one truthful report instead of
  per-agent guesswork — never a fake pass. New `prove` module in cue-agent-bridge.
  Now also reports GUI vs CLI as **distinct surfaces with real versions**: the
  GUI app version (from its bundle), the CLI version (by actually running
  `--version`), which binary it drives via (e.g. Antigravity → gemini), and a
  **present-but-broken** state (CLI on PATH but won't run — a dangling symlink /
  bad install). `drivable` now requires the CLI to actually run, not just exist.
- Proactive provisioning (Phase A) — Bluey now installs a missing agent CLI so a
  user with only the GUI becomes drivable, instead of degrading to read-only.
  New `provision` module: vetted per-agent `InstallRecipe` (official npm/curl
  sources only, shell-injection-guarded), a read-only **pre-flight diagnosis**,
  and a **dynamic recovery** flow that handles the *class* of real-world install
  obstructions (data-driven obstruction→remedy). First obstruction handled:
  a broken symlink blocking `npm install` (`EEXIST`) — removed (consent-gated,
  re-checked so a real file is never touched) then retried. Proven live on a real
  machine: detected a broken `/usr/local/bin/codex` → deleted Homebrew cask,
  cleared it, ran a real `npm install -g @openai/codex`, verified the binary, and
  re-discovered Codex as `Drive` (was un-installable + un-runnable). Master plan:
  docs/work/PLAN-PRODUCTION-VISION.md.
- Adaptive Resolver core (self-healing session decoding, Track A1+A2): a
  schema-agnostic layer that re-derives where a message's text/role/order live
  when an agent changes its on-disk shape, so readers aren't pinned to one
  version's field names. Deterministic-first — a persisted recipe cache keyed by
  a structure fingerprint, a heuristic shape-detector (longest-string field =
  text, small-domain field = role, named/int values mapped), and a validation
  gate that rejects incoherent recipes. Verified it adapts to Cursor, Claude,
  AND an unseen made-up format with zero hardcoding. No AI yet — there's a
  marked extension point (Track A3) where an AI fallback slots in, always behind
  the same validation gate. Design: docs/work/PLAN-ADAPTIVE-RESOLVER.md.

### Changed
- On-device capture is now **continuous-streaming, not chunk-file**. The overlay
  "Listen" button and the `bluey listen` CLI (when no cloud STT is configured)
  route to the proven streaming system-audio task — continuous PCM → one
  streaming `SttProvider`, no per-second WAV record/transcribe/delete cycle. This
  is the real-time path; the old chunk-file on-device Parakeet branch (record 1s
  `.wav` → transcribe → delete, with its fragment accumulator and dual-drain
  pipeline) is **removed** (net −400+ lines). On-device system-audio STT is now
  on by default (disable with `BLUEY_SYSTEM_AUDIO_STT=0`); `BLUEY_STT_PARAKEET`
  (the chunk-path switch) no longer applies. The cloud/REST transcription path
  (OpenAI / Bluey-managed / local-whisper) is unchanged. v1 streams **system
  audio only** (the other participants — the question trigger); mic streaming is
  a deliberate follow-up, and the overlay copy ("Listen (system audio)") no
  longer promises mic capture it doesn't yet do. `stop_audio_capture` now
  actually tears down the streaming handle, and the streaming start is idempotent
  (no double-spawn of the capture helper / STT model).
- ACP agent driving is now ON by default for ACP-capable agents (was opt-in
  `BLUEY_USE_ACP=1`), made safe by a CLI fallback: a pre-first-token ACP failure
  (spawn/handshake/adapter) transparently falls back to the CLI driver, so the
  default flip can't turn setup faults into hard answer failures. Disable with
  `BLUEY_USE_ACP=0`.
- The daemon now discovers and launches the real meeting overlay
  (`cue-meeting-overlay`) ahead of the older overlays, and the macOS package
  builds + ships it (UI + arm64 binary, staged into the release).
- Semantic transcript recall (embedding RAG) is now documented and logged as a
  cloud-optional enhancement, not a faux-"local" default — the local core loop
  (transcript buffer + decisions ledger + pre-meeting brief) needs no embeddings,
  and keyword `bluey memory search` is unaffected.
- Corrected the per-agent CLI drive command map against official 2025-2026 docs
  (we had guessed wrong before): Cursor now uses `--output-format json` (a new
  `CursorJson` parser captures the answer + session id) and `--resume=<id>`;
  Codex uses `exec --json` (a new `CodexJsonl` parser) with the read-safe
  `--sandbox read-only --ask-for-approval never`; Copilot's bogus `--continue`
  resume is removed (it has none) and `-s` added; `agy` (Antigravity CLI) is now
  a discovery candidate. Each change is annotated DOC-CONFIRMED vs
  NEEDS-LIVE-VERIFY (Cursor/Copilot/Codex aren't installed here, so the commands
  match docs but await a live run).

### Security
- Hardened the connector secret guarantee. The stored HTTP connector URL now has
  its query string and fragment stripped (some MCP endpoints embed tokens as
  `?token=…`), and a `headers` block (bearer tokens) is never read. A new audit
  test feeds a config carrying secrets in *every* hiding place — env values, a
  URL query token, and a bearer header — and asserts none ever appear in a
  serialized connector (only names, commands, hosts, and auth tiers do).
- Headless answer drives no longer use a blanket auto-approve flag. The first
  MCP-enable attempt used Gemini's `--approval-mode yolo`, which auto-approves
  *all* tools — a live canary test proved it let a read-intent **answer write a
  file**. Replaced with a scoped `mcp_allow_flag` (`--allowed-mcp-server-names`)
  that the drive layer fills with the agent's *own* configured MCP server names:
  the agent's MCP read-tools fire while file/shell writes stay gated (blocked
  headless). Verified live through the bridge: MCP fired (real-time data) AND a
  file-write attempt was blocked.

### Fixed
- Session reads no longer load whole files into memory. The JSONL reader (Claude
  /Codex) and the per-session title lookup now stream line-by-line and stop at
  the bound, instead of `read_to_string` on files that reach 10 MB+ (the title
  lookup ran once per session during a list, so it could load every session's
  file). A single oversized line is skipped via a 4 MiB cap. Verified live: 264
  Claude sessions list + read correctly while streaming.
- Agent MCP connectors now fire when Bluey drives a CLI in headless mode. Live
  testing showed `gemini -p` blocks tool calls on an approval prompt that never
  arrives non-interactively, so its MCP never ran. The drive layer now appends
  the scoped MCP allow-list (see Security above) for agents that need it;
  Claude Code loads MCP in `-p` with no flag.
- Agent session readers, corrected against real on-disk data (live testing
  found the synthetic-fixture tests had masked these):
  - **Cursor** (`vscdb`): listed sessions but read 0 turns/titles — now walks
    `fullConversationHeadersOnly` → `bubbleId:` rows to recover ordered messages
    and first-user titles (verified: full transcripts from a 220-session DB).
  - **Copilot / VS Code** (`json_files`): found 0 sessions — now descends into
    `workspaceStorage/<hash>/chatSessions/*.json`.
  - **Codex**: session store wasn't discovered — registry now declares the JSONL
    format + a per-agent `jsonl_subdir` (`sessions`), and the JSONL reader
    recurses the date-nested `YYYY/MM/DD` layout.
  - **Claude Code**: listed 98 of 259 sessions (one-level recursion) — now finds
    all; added `SessionRef.project` decoded from the `~/.claude/projects/<cwd>`
    dir name, resolving hyphenated folders against the real filesystem.
  - **Claude MCP config**: read the empty `~/.claude/settings.json` — connector
    discovery now also checks the sibling `~/.claude.json` and prefers whichever
    config actually declares servers.
  - **Antigravity** (`protobuf`): confirmed the `.pb` wire format is opaque with
    no public schema; left as an honest enumerate-only stub (read returns a clear
    deferral, never guesses).

### Added
- Fix button overlay UI (F4): a **Fix** button on agent answer cards (emits
  `FixRequested`), a proposal card rendering DIAGNOSIS / REASONING / FIX with a
  colorized diff block, and **Approve / Reject** buttons (id-matched, one-shot,
  Approve disabled when the agent can't apply). Approve/Reject emit
  `FixApprovalResponded`; states cover shown / approve-disabled / applying /
  discarded. Reuses the overlay theme + card builders; `swift build` clean.
- Fix button daemon flow (F3): the review-gated propose → approve → apply state
  machine. New `OverlayEvent::FixRequested` / `FixApprovalResponded` and
  `OverlayCommand::PushFixProposal`. On Fix, the daemon drives the attached agent
  in ProposeFix mode, parses the structured proposal, and pushes a proposal card
  (diagnosis / reasoning / diff + Approve-Reject) — applying nothing. Apply is
  reachable only via an `approved=true` response carrying a server-minted,
  unconsumed, unexpired proposal id for an apply-capable agent (remove-on-take +
  TTL prevent replay/stale-apply); never pushes. Pending proposals are bounded.
- Fix button foundation (F1+F2): a data-driven `FixProfile` on each registry row
  (per-agent propose-only vs apply args, `apply_supported`) plus a `DriveMode`
  (Answer / ProposeFix / ApplyFix) so the drive layer forces propose-only or
  apply purely from the table — no per-agent branches, apply args appended only
  in ApplyFix and only for apply-capable agents (Cursor never uses the broken
  `--plan`). Adds the fix-proposal / apply prompt templates (structured
  DIAGNOSIS/REASONING/FIX, "apply nothing", "never push") and a fail-soft
  `FixProposal` parser. Design: docs/work/PLAN-FIX-BUTTON.md.
- Agent bridge native overlay UI (Slice 5b): an "Attach Agent" drawer in the
  macOS overlay — agent picker with capability chips, session picker
  (continue-most-recent / fresh), connector sheet with per-connector readiness +
  re-auth, attached-state header badge + pill glyph, and agent-answer card
  relabeling (role badge shows the agent, "answered by your <agent>"). Decodes
  `set_agents`/`set_agent_sessions`/`set_agent_connectors`, emits the agent
  request events. Reuses existing theme/drawer/card builders; no new window.
- Agent bridge session resume + agent-labeled answers: a new `attached_session`
  setting persists the chosen session so an attached agent continues it
  (`Question.resume`), and agent answers now carry an agent-labeled card source so
  the overlay attributes them to the user's agent.
- Windows agent discovery: discovery now resolves Windows base dirs
  (`%APPDATA%`/`%LOCALAPPDATA%`/program dirs, `%USERPROFILE%`) and scans
  Windows app-install + VS Code-family data locations, so `agent list` finds GUI
  agents on Windows too (cfg-gated; macOS behavior unchanged).
- `scripts/test-agent-bridge.sh`: one-command end-to-end smoke test (build →
  start daemon → list/attach/connectors/sessions/ask/detach → stop), with safe
  daemon start/stop and graceful handling when no drivable agent is installed.
- `cue-agent-bridge` crate (Slice 1): read-only discovery of installed coding
  agents (GUI + CLI) via registry + generic VS Code-fork detector, JSONC-tolerant
  MCP connector reader with auth-tier classification, and the `AgentSource` trait
  foundation. Design: docs/work/PLAN-AGENT-BRIDGE.md.
- `cue-agent-bridge` drive layer (Slice 2): data-driven per-agent CLI command map
  (claude/copilot/cursor-agent/gemini/codex), safe subprocess execution (args array,
  no shell injection), wall-clock timeout, output-size cap, kill-on-drop, and
  stream-json + plain-text answer parsers streaming `AnswerChunk`s. `AgentSource::ask`
  wired to the runner.
- `cue-agent-bridge` session readers (Slice 3): per-format decoders behind a
  `SessionReader` trait — JSONL (Claude/Codex), SQLite `state.vscdb` (Cursor/VS Code,
  read-only/immutable, bounded), JSON-files (VS Code/Copilot), and a deferred
  Antigravity protobuf stub. Normalizes to `Transcript`/`SessionRef`.
- Agent bridge daemon wiring (Slice 4): new `AiProviderKind::Agent` provider route
  so an attached coding agent answers through the existing
  `resolve_answer_route`/`OverlayAnswerStream` machinery. Selection is driven by two
  new `CueSettings` fields (`attached_agent`, `allow_agent_session_history`); when an
  agent is attached, answers stream from it and the turn is recorded with an agent
  provider label. If the agent CLI is missing or not signed in, Bluey shows a
  guidance `Warning` card and never silently falls back to its own AI.
- Agent bridge IPC + discovery surface (Slice 5a): overlay-facing DTOs
  (`AgentSummary`/`AgentConnectorInfo`/`AgentSessionSummary`), new `OverlayCommand`
  (`SetAgents`/`SetAgentSessions`/`SetAgentConnectors`) and `OverlayEvent`
  (`AgentListRequested`/`AgentAttachRequested`/`AgentDetachRequested`/
  `AgentSessionsRequested`/`AgentConnectorsRequested`/`ConnectorReauthRequested`)
  variants, and daemon handlers that discover agents, map them to summaries,
  persist attach/detach to settings, and list sessions (gated on the
  `allow_agent_session_history` consent toggle). Discovery/IO runs off the async
  runtime; connector readiness reported without exposing secrets.
- `bluey agent` CLI subcommand (list / attach / detach / sessions / connectors /
  status) to discover, attach, and inspect coding agents from the terminal,
  driving the daemon over new `DaemonRequest`/`DaemonResponse` agent variants.
  `attach` persists the selection so `bluey ask` routes through the agent;
  `sessions` honors the session-history consent gate. Lets the agent bridge be
  tested end-to-end without the overlay UI.
- Master plan V3 (docs/reviews/CUE-BLUEY-V3-PLAN-COMPLETE.md)
- Phase 0 foundation: workspace structure, Cargo workspace, crate scaffolding
- Development workflow docs: CLAUDE.md, templates, CI, .codex agents + skills
