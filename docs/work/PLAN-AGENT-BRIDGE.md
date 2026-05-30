# PLAN — Agent Bridge (`cue-agent-bridge`)

> Status: DESIGN (no production code yet). Branch: `agent/agent-bridge`.
> Author: Claude. Date: 2026-05-30.
> Cross-refs: `ARCHITECTURE.md`, `PRODUCT-STRATEGY.md` (Meeting Agent Context
> Bridge), `ROADMAP.md` v0.5, `crates/cue-core/src/meeting.rs`,
> `crates/cue-core/src/cards.rs`, `crates/cue-router/`.

---

## 1. What this is

Bluey listens to a meeting and, when a real question surfaces, gets a grounded
answer using the user's **own already-installed coding agent** — its model, its
subscription, its connectors — rather than running Bluey's own AI on the user's
private work data.

This crate, `cue-agent-bridge`, is the single new surface that makes "any
installed coding agent" into a uniform source Bluey can:

- **discover** (which agents are installed, what they can do),
- **inherit connectors from** (read their MCP config, classify auth),
- **read context from** (their prior session history), and
- **drive** (ask a question, stream the answer back).

Everything else (audio, VAD, STT, cards, overlay, daemon, router) already
exists and is untouched by this work.

### Drive model (simple — nearly every agent now ships a CLI)

Almost all major agents expose a headless CLI, so the dominant case is
**drive directly**:

- **CLI agent** (Claude Code `claude -p`, Copilot `copilot -p`, Cursor
  `cursor-agent -p`, Gemini/Antigravity `gemini -p`, Codex `codex exec`) →
  run it; its model + its connectors answer. *(Copilot's GA CLI is the
  solution — no `vscode.lm` companion extension needed.)*
- **GUI app that has a same-vendor CLI** (Cursor, Antigravity, Copilot) →
  same drive-directly path, **plus** we read its local session history for
  context and prompt re-auth if a connector needs login.
- **CLI-less app** (e.g. the Claude *desktop* chat app) → rare fallback:
  inherit connectors (read MCP config + spawn) and answer via Bluey's
  zero-retention router.

So the build's drive layer is **one CLI runner + a per-agent command map**.
No per-vendor extension code.

---

## 2. Why this shape — the architectural decision

**Bluey is a conduit, not a data store.** The entire corporate value is that we
never hold the customer's work data or credentials. That is a hard constraint
that decides every design choice below, not a feature to add later.

- The customer's transcripts, tickets, code context, and connector credentials
  stay where they already live — inside the agent they already approved.
- For the strongest case (a CLI agent), Bluey never even sees the data: it
  hands a question to the agent and shows the answer. Their model, their key.
- For GUI apps, Bluey reads context locally, in-memory, and discards it. Never
  persisted, never uploaded.

This is the honest version of the product thesis in `PRODUCT-STRATEGY.md`:
use what the user can already see/attach/authorize; no hidden scraping, no
stolen tokens, no bypass flows.

---

## 3. What the spike proved (2026-05-30, real data on this machine)

Every risky assumption was validated before writing this doc. Facts:

### Drive a CLI agent — PROVEN
- `claude -p "<prompt>"` returns the answer on stdout. (`claude` 2.0.42 installed.)
- `--output-format json` yields a parseable record: `result`, `session_id`,
  `total_cost_usd`, `usage`, `is_error`, `permission_denials`.
- `--output-format stream-json --verbose` streams `system/init` →
  `assistant` → `result` events (for a live feed). The `init` event lists the
  session's `mcp_servers`, `tools`, and `model`.
- **Session continuation works**: capture `session_id`, then
  `claude -p --resume <id> "<follow-up>"` — the agent recalled prior context
  (the "continue their session" mechanism, end to end).
- `gemini` CLI also installed → second drivable agent available.

### Inherit connectors from GUI apps — PROVEN (17 MCP servers read across apps)
Read each app's MCP config without opening the app:

| App | Config path | Servers found |
|---|---|---|
| Cursor | `~/.cursor/mcp.json` | 7 |
| Antigravity | `~/.gemini/antigravity/mcp_config.json` | 8 |
| VS Code | `~/Library/Application Support/Code/User/mcp.json` | 2 |

Auth tier is derivable per server: `env`-auth stdio servers can be respawned
directly; hosted `http/sse` (e.g. Supabase) may carry OAuth the app
negotiated → flag `needs-reauth`, never extract tokens.

### Read session history from GUI apps — PROVEN (storage differs per app)
| App | Store | Format | Found |
|---|---|---|---|
| Cursor | `…/Cursor/User/globalStorage/state.vscdb` (3.1 GB) | SQLite blobs (`cursorDiskKV`, keys `composerData:*`, `bubbleId:*`) | 220 sessions / 45,872 messages |
| Antigravity | `~/.gemini/antigravity/conversations/*.pb` | protobuf | 100 conversations |
| VS Code / Copilot | `…/Code/User/workspaceStorage/*/chatSessions/*.json` | JSON files | 10 sessions |
| Claude Code | `~/.claude/projects/*/*.jsonl` | JSONL | (native, also drivable) |

### Real-world gotchas caught live (must handle)
- **JSONC, not JSON**: Cursor `mcp.json` has trailing commas; VS Code
  `settings.json` has control characters. Config reader must be tolerant.
- **Huge, possibly-live SQLite**: always open read-only / `immutable=1`; query
  bounded top-N; never load wholesale.
- **Per-app session formats**: one reader interface, per-app decoders
  (SQLite / protobuf / JSONL / JSON-files).

---

## 4. Crate layout

```
crates/cue-agent-bridge/
  Cargo.toml
  src/
    lib.rs           // AgentSource trait, AgentKind, Capability, public API
    discover.rs      // DYNAMICALLY scan the whole system for ALL agents (GUI+CLI),
                     // registry-driven + generic VS Code-fork detector (see §4.1)
    registry.rs      // data table: name, binary candidates, app-bundle names,
                     // data-dir globs, session format, drive command
    connectors.rs    // JSONC-tolerant MCP config reader + auth-tier classifier
    capability.rs    // drive | read-only | needs-trust | needs-reauth | cloud-blocked
    sessions/
      mod.rs         // SessionReader trait + SessionRef/Transcript types
      claude.rs      //   JSONL decoder
      cursor.rs      //   SQLite (immutable, read-only) decoder
      antigravity.rs //   protobuf decoder
      vscode.rs      //   JSON-files decoder
    drive/
      mod.rs         // Driver trait, Question, AnswerStream, AnswerChunk
      cli.rs         //   one CLI runner + per-agent command map (args array, no shell):
                     //     claude -p / copilot -p / cursor-agent -p / gemini -p / codex exec
  tests/
    discover_fixtures.rs   // fixture configs (synthetic, no real user data)
```

`cue-agent-bridge` depends on `cue-core` (for `CueCard`, shared types) and
nothing in the hot-path that runs AI. The daemon depends on it.

---

## 4.1 Discovery — dynamic, find ALL agents (GUI + CLI)

Discovery must find every coding agent actually installed on the machine — the
way the spike did manually — not match a hardcoded short list. It is
**registry-driven** plus a **generic detector** so new/unknown agents are still
found:

- **PATH scan** for agent CLI binaries: `claude`, `cursor-agent`, `copilot`,
  `gemini`, `codex`, `aider`, + registry candidates.
- **App-bundle scan** of `/Applications` and `~/Applications` for agent GUIs
  (Cursor, Claude, Antigravity, VS Code, Windsurf, Zed, …).
- **Data-dir scan** for footprints: `~/.<agent>`,
  `~/Library/Application Support/<App>/User/` (`mcp.json`, `settings.json`,
  `state.vscdb`, `chatSessions/`, `workspaceStorage/`),
  `~/.gemini/antigravity`, `~/.codex`, `~/.claude`, etc.
- **Generic VS Code-fork detector:** any app dir with
  `User/globalStorage/state.vscdb` + an `mcp.json` is treated as a VS Code-family
  agent automatically → new forks discovered with zero code change.
- Each result reports: `AgentKind` (incl. `Other`/`Unknown` for undetected
  agents), the install evidence (which path/binary proved it), `Capability`,
  connector-config path, and session-store path + format.
- **Registry as data** (`registry.rs`): adding a named agent = one table row
  (binary candidates, bundle names, data-dir globs, session format, drive
  command). No new code paths.

---

## 5. Public interface (the one abstraction)

```rust
pub enum AgentKind { ClaudeCode, Cursor, Antigravity, Copilot, Gemini, Codex, /* … */ }

pub enum Capability {
    Drive,        // can run a new question via its CLI (claude/copilot/cursor-agent/gemini/codex)
    ReadOnly,     // can read history/connectors but not drive
    NeedsTrust,   // drivable after a one-time interactive trust step (Cursor headless)
    NeedsReauth,  // a connector needs re-login before use
    CloudBlocked, // agent is cloud-only; nothing local to read or drive
}

pub struct Connector {
    pub name: String,
    pub transport: Transport,      // Stdio { command, args } | Http { url }
    pub auth_tier: AuthTier,       // EnvAuth | HostedOauth | None
}

pub struct SessionRef { pub id: String, pub title: Option<String>, pub updated_at: String }

pub struct Transcript { pub turns: Vec<Turn> }   // normalized across apps
pub struct Turn { pub role: Role, pub text: String }

pub struct Question {
    pub prompt: String,
    pub context: Option<Transcript>,   // bounded; meeting summary + optional prior session
    pub resume: Option<String>,        // native session_id to continue, when supported
}

pub enum AnswerChunk { Started { session_id: Option<String> }, Delta(String), Done { cost_usd: Option<f64> }, Error(String) }
pub type AnswerStream = /* stream of AnswerChunk */;

#[async_trait]
pub trait AgentSource: Send + Sync {
    fn kind(&self) -> AgentKind;
    fn capability(&self) -> Capability;
    fn connectors(&self) -> Vec<Connector>;
    fn recent_sessions(&self, limit: usize) -> anyhow::Result<Vec<SessionRef>>;
    fn read_session(&self, id: &str, max_turns: usize) -> anyhow::Result<Transcript>;
    async fn ask(&self, q: Question) -> anyhow::Result<AnswerStream>;
}
```

The daemon decides **when** to ask and **what** context to include. The bridge
is "dumb" — it discovers, reads, and drives. Separation of concerns: the
question-gate and summarizer live in the daemon/meeting pipeline, not here.

---

## 6. Security & production-grade rules (constraints, enforced in code)

1. **Read-only, always.** Disk reads use read-only / `immutable=1` handles. We
   never write to another app's store.
2. **No persistence of their data.** Sessions read into memory, used, dropped.
   Nothing of theirs written to Bluey disk or cloud. (Bluey's own meeting DB
   stays local as today.)
3. **No credentials touch us.** We read connector *config* (command/args/url),
   never secrets. `env` servers spawn with their own env; OAuth servers are
   flagged `NeedsReauth`, never token-scraped.
4. **Conduit for answers.** `ask()` prefers driving the user's CLI → their
   model/key answers. Bluey runs no AI in the hot path. Bluey's own router is a
   last-resort fallback only (offline / no drivable agent), and only on a
   zero-retention / their-provider route.
5. **Explicit capability, no silent failure.** Every agent reports its
   `Capability`. UI shows it. We never pretend an agent works when it doesn't.
6. **Bounded & lazy.** Top-N recent sessions; hard caps on transcript size sent
   to `ask()`; never load a multi-GB store wholesale.
7. **Consent-gated.** Reading another app's data only after the user explicitly
   attaches that agent. No background scraping, ever.
8. **Sandboxed subprocess.** CLI drive uses an args array (never shell string
   interpolation → no command injection), wall-clock timeout, output size cap,
   killed on cancel/supersede.
9. **JSONC-tolerant, fail-soft config parsing.** A malformed config for one
   agent degrades that agent to `ReadOnly`/skip, never crashes discovery.

---

## 7. Build order (each slice independently shippable, tree stays green)

| Slice | Scope | Risk | Output |
|---|---|---|---|
| **1** | `discover` + `connectors` + `capability` | none (pure RO reads) | "here's what you have and what it can do" — list of agents + connectors + auth tiers, fully unit-tested against fixtures |
| **2** | `drive::cli` + `ask()` | low | one question → streamed `CueCard` answer via `claude -p` |
| **3** | `sessions` readers (Claude JSONL, then Cursor SQLite) | low | answers grounded in a real prior session |
| **4** | daemon wiring | medium | meeting question-detect → `ask` → overlay card |
| **5** | attach UI + capability chips | medium | user-facing "pick your agent" surface |

Out of scope for now (lean): cloud sync, multi-user merge, post-meeting
push-back-to-Jira, any `vscode.lm` companion extension (the GA `copilot -p` CLI
covers Copilot — no extension needed),
and the question-gate/summarizer (daemon concern, separate slice).

---

## 8. How it maps onto existing code

| Need | Existing piece | File |
|---|---|---|
| Answer card | `CueCard` (`Answer`/`Question`/`Warning`) | `cue-core/src/cards.rs` |
| Conversation history | `MeetingRecord.conversation`, `ConversationTurn` | `cue-core/src/meeting.rs` |
| Streaming to overlay | NDJSON IPC pattern | `cue-core/src/overlay_ipc.rs` |
| Fallback router | `cue-router` policy/provider traits | `crates/cue-router/` |
| Daemon orchestration | supervisor + IPC | `crates/cue-daemon/` |

The bridge adds a crate; it does not rewrite any of the above.

---

## 9. UI design — "Attach Agent" flow

Grounded in the real overlay (`native/macos/cue-overlay/Sources/cue-overlay/main.swift`):
reuses `BlueyTheme` tokens (cyan / green ready / amber warning / panelDeep),
`makeSessionRow` (52pt rows), `makeAttachmentChip` (28pt pills), `styleHeaderBadge`,
the left `sessionDrawer` slide-out, and `makeCardView` (role badge + accent rail).
**No new window** — all surfaces are panes inside the existing expanded panel.
New IPC on `overlay_ipc.rs`: `SetAgents`, `SetAgentSessions`,
`AgentAttachRequested{kind,session_id}`, `AgentDetachRequested`,
`ConnectorReauthRequested{name}`.

### Surfaces
1. **Agent picker** — re-skinned `sessionDrawer` (`agentDrawer`, widen 190→230pt).
   One row per detected agent: icon + name, capability chip, `"{n} tools · {n} sessions"`.
   Chips: Drive=`live` (green), ReadOnly=`history only` (grey), NeedsTrust=`needs trust`
   (amber), NeedsReauth=`re-auth` (amber), CloudBlocked=`unavailable` (red, dimmed,
   non-tappable). States: skeleton loading, empty (`"No coding agents found"`),
   discovery-error retry row.
2. **Session picker** — push-nav in same drawer. Search field over title+snippet;
   two pinned quick-actions: **Continue most recent** (cyan) and **Fresh — no past
   context** (outline). Daemon sends top-40 (`SetAgentSessions`), `"Load 40 more"`
   footer if `has_more` (honors §6 bounded top-N). Read error degrades to "attach
   fresh instead", never blocks.
3. **Connector inheritance sheet** — bottom confirm sheet (reuses `closeConfirmOverlay`).
   Per-connector row: name + auth tag (`env · ready` green / `oauth · re-auth` amber + ↻).
   Summary `"5 of 7 connectors ready · 2 need re-auth"`. **Attach never blocked** by
   re-auth gaps. `Cancel` / `Attach`.
4. **Attached state** — header agent badge (`styleHeaderBadge`, cyan): `"cursor · 7 tools"`,
   tap to switch/detach. `statusLabel`: `"agent: cursor · session: auth-service"`.
   Pill: tiny cyan agent glyph (no text → no truncation). Detach = ✕ on active row.
   Crash/timeout → amber badge + `WARNING` card, Bluey fallback.
5. **Data-residency reassurance** — ambient, not modal. Drawer captions
   (`"Answers run on your machine — your agent replies."`,
   `"Connectors run from your agent. Bluey stores nothing."`), answer-card footer
   (`"answered by your Cursor agent · stays local"`), one-time `System` card on first
   attach. One-line, calm, no marketing slab.
6. **Agent-answer card** — reuses `makeCardView`, no new component. Role badge reads
   source (`CURSOR`/`CLAUDE`) not `BLUEY`; cyan rail kept (Bluey-mediated). Source label
   `"answered by your Cursor agent"` via `CueCard.source`; `cost_label` `"$0.012 · cursor"`
   (native = `"on your plan"`). Streaming via existing `PushCard`→`UpdateCard` path with
   `"via cursor"` tag. `AnswerChunk::Error` → `WARNING` card, never silent.

### Non-negotiable checks (from UX brief)
- No new window; pill stays uncluttered (glyph only). ✔
- Every surface has loading/empty/error/degraded paths. ✔
- No terminal dependency — discovery/attach/re-auth are in-overlay buttons. ✔
- ⚠ Verify widened drawer (230pt) + connector rows truncate-tail (not clip) at
  `minCompactWidth` 680 via `fitExpandedFrameToVisibleScreen`.

### Files to touch (UI)
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift` — drawer (~2125),
  header badges (~2050), card labels (~1100), composer button (~2231), pill (~617).
- `crates/cue-core/src/overlay_ipc.rs` + `overlay.rs` — new commands/events.
- `crates/cue-core/src/cards.rs` — `source`/`cost_label` already sufficient.

---

## 10. Open decisions for the user

**DECIDED (2026-05-30):**

| Decision | Resolution |
|---|---|
| Drive selection | **Dynamic / data-driven.** No hardcoded "first agent". A per-agent command map (agent → command + flags + parser) is data; the runner drives whichever agent is attached. New agent = new table row, not new code. Claude Code used only as the first *test target* since it's installed. |
| Fallback when no drivable agent | **Warn card only.** A `WARNING` card prompts the user to attach an agent. Bluey never answers from the user's context itself. Purest data residency. |
| Session-read consent | **Global setting.** One toggle enables/disables reading other apps' session history for all agents. |
| Daemon routing | **New provider route** (`AiProviderKind::Agent`). When an agent is attached, answers route through it via the existing `resolve_answer_route` / `OverlayAnswerStream` machinery — the agent is "just another provider", maximizing reuse. |
| Agent CLI not ready | **Guide, never silently fall back.** Bluey does NOT auto-install anything. If the attached agent's CLI is missing or not logged in, show a card guiding the user to install / sign in (command or button). Their agent stays the source of truth — no silent Bluey-AI answer on their meeting context. |

**Attach model:** Bluey detects which agent CLIs are present + authenticated. For
a GUI app whose CLI isn't installed/logged-in, the attach UI guides the user to
enable it (one-time, their choice). We never install or authenticate on their
behalf.
