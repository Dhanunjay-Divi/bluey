# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
