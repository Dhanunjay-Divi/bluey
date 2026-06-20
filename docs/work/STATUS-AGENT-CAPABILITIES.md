# STATUS — Agent Capability Tracker

> **Living scoreboard.** Update this as we verify (or break) capabilities. The target +
> phase plan live in [[PLAN-AGENT-MEETING-ORACLE]] (§1a is the definition of done). This file
> is just the running status + notes so we always know what actually works vs. what's assumed.
>
> **Legend:** ✅ verified working (with evidence) · ⚠️ partial / works-but-caveat · ❌ broken /
> doesn't work · ⬜ not built/tested yet · 🚫 N/A (agent can't do this).
>
> **Rule:** a cell is ✅ ONLY if it was actually exercised (CLI output / logs / a real run) —
> never because code compiled or a unit test passed. GUI cells are ✅ only after the USER
> confirmed by eye (we cannot verify the overlay ourselves — see plan §2).

---

## ⏱️ SURFACE LEDGER — what's DONE vs TODO (read this FIRST; prevents re-loops)
> A "surface" = a distinct agent presentation (CLI binary vs desktop App vs in-editor) with its
> OWN session store. Testing one surface does NOT cover another (verified: 0 session-id overlap
> across all surface pairs). Before "doing the GUI/App version" of an agent, check the SURFACE
> MAP below — some agents have multiple surfaces (Claude, Copilot), some have only one.

| Surface | Type | Status |
|---|---|---|
| `claude_code` | CLI | ✅ 5/5 |
| `codex` | CLI (= App; shares `~/.codex`, App ignores CLI sessions but Bluey reads the shared store) | ✅ 5/5 |
| `copilot` | CLI | ✅ 5/5 |
| `cursor` | CLI/App — ONE Bluey row reading the IDE store (the `cursor-agent` CLI's own `~/.cursor/chats` is a deliberate non-target) | ✅ 5/5 |
| `gemini` | CLI only (no separate App) | ✅ 5/5 |
| `antigravity` | CLI (`agy`) — reads the same store the App writes | ✅ 5/5 |
| `claude_code_app` | Claude **desktop App** (separate index, bridged to CLI JSONL via cliSessionId) | ✅ 5/5 |
| `vs_code_fork` | **VS Code** Copilot Chat (in-editor; bridges to Copilot CLI for drive) | ✅ 5/5 |
| `claude_code_agent` | Claude App agent-mode | 🚫 0 sessions — nothing to test |
| `Code - Insiders` | VS Code Insiders fork | ⚠️ reads 5 sessions after the cursorDiskKV fix; NOT run through the 5-cap harness |
| **Aider** | CLI | ⬜ programmatic surface unconfirmed |

**TODO — genuinely separate App stores NOT yet 5-cap tested:** *(per the App↔CLI research — these
Apps keep their own stores, so the CLI pass does NOT cover them)* — **Cursor App** (the editor's
own composer store IS what `cursor` reads, so likely already covered — confirm), **Antigravity App**
(`…/antigravity/brain` vs CLI — but `antigravity` row reads the shared store), **GitHub Copilot App**
(standalone, distinct from `vs_code_fork`), **Codex App** (shares `~/.codex` — likely covered).
⚠️ Several of these "App" surfaces may already be covered because the Bluey row reads the shared/IDE
store — VERIFY against the SURFACE MAP before assuming a separate test is needed; do NOT hunt for a
store that doesn't exist.

---

## The 5 capabilities (per agent) — see plan §1a
1. **Read sessions** — list past conversations
2. **Name** — real chat title
3. **Project/dir** — repo/folder the session belongs to
4. **Resume / Fork** — continue a chat (resume in-place AND fork into a new branch where the
   agent supports both; note the level)
5. **Use MCPs** — answering actually invokes the agent's own MCP connectors

---

## Capability matrix (CLI-verified unless noted)

| Agent | 1 Read | 2 Name | 3 Project | 4 Resume/Fork | 5 MCPs | Notes |
|---|---|---|---|---|---|---|
| Claude Code (CLI) | ✅ | ✅ | ✅ | ✅ resume✅ + fork✅ | ✅ | **ALL 5 GREEN 2026-06-19.** Cap 4 now BOTH modes: **TRUE in-place resume by id PROVEN** — `bluey agent attach claude_code --session 0d2907a8…` + `bluey ask` → resumed agent answered *"The event **I reported** was the Obama Presidential Center…"* (its own prior turn, not "NO CONTEXT"); daemon logged `TRUE resume (session/load) committed — true in-place resume` (3 runs, deterministic). Root cause of the old failure was **CWD** (resume resolves `~/.claude/projects/<encoded-cwd>/<id>.jsonl`; the cwd is the path key — NOT an id-space mismatch) + my earlier arm forcing replay so session/load was never attempted. **Fork still works** as the automatic fallback (drive_acp tries session/load → on early error replays context; 5 unit tests cover the adapter) and via the cwd-unusable branch (logged `cwd unusable → FORK`). **Cap 5 PROVEN:** driven Claude called `mcp__perplexity__perplexity_ask` — agent uses its OWN connectors; Bluey never touches secrets (adapter loads user MCP via settingSources; we auto-allow). CAVEATS: (a) resume can mutate/corrupt the original JSONL (anthropics/claude-code#36583) → fork stays the safe default for the meeting-oracle; (b) one Claude-**Desktop** session (`6e59107c`, inner id ≠ filename id, deleted project dir) forked to empty context — pre-existing reader edge case, not the resume path. minor: double-render. |
| Claude Code (App) | ✅ | ✅ | ✅ | ✅ true resume + fork | ✅* | **GUI-agent #1 — 5/5 VERIFIED 2026-06-19, NO code fix needed.** The Claude DESKTOP APP surface. Caps 1-3: `bluey agent sessions claude_code_app` lists 19 real app sessions w/ titles + projects ("Bluey repository setup" `[/…/Bluey]`, "Surface Gemini settings.json plaintext secret risk", etc.). **Key finding (investigation):** app sessions are backed by the SAME CLI JSONL files — the `ClaudeAppIndex` reader (`claude_app.rs`) follows each index row's `cliSessionId` into `~/.claude/projects/<enc-cwd>/<cliSessionId>.jsonl`, and ALREADY exposes that `cliSessionId` as the session id (NOT the app filename) — so true resume works exactly like CLI Claude, no Codex-style fix. **Cap 4 PROVEN:** resumed `5d579420` → recalled its real content (*"Gemini settings.json stored a GitHub PAT in plaintext… readers never surface credential files"*) + `8732530c` → recalled the Bluey-spine planning convo; log `ACP resume (session/load) committed — true in-place resume`. Same `claude-agent-acp` adapter, NativeResume tier. **Cap 5 (*):** uses the SAME adapter + `~/.claude.json` as CLI Claude → loads + invokes `mcp__perplexity__perplexity_ask` (proven: tool found + called, returned 401 insufficient-quota — the key is exhausted, NOT a Bluey gap; identical to the CLI Claude/Copilot MCP-key caveat). Connector DISPLAY shows 0/0 (app-local `.claude.json` has empty mcpServers) — cosmetic, MCP-use works via the user `~/.claude.json`. **NOTE (separate daemon bug found):** "Prompt is too long" appeared across asks until a daemon restart — context accumulates across asks in long daemon sessions (stale-context bug, not claude_code_app-specific; flagged for later). |
| Cursor | ✅ | ✅ | ✅ | ✅ fork (replay) | ✅ | **5/5 VERIFIED 2026-06-19** (account `knackkit@gmail.com`, quota OK). **Required a code fix first:** `spec_map.rs` had the stale argv `cursor-agent agent acp`; current builds want top-level `cursor-agent acp` (the old form parses "acp" as a prompt arg and hangs → daemon timed out). Fixed → fresh ask returns `CURSOROK`. Caps 1-3 read from `state.vscdb`. **Cap 4 = fork/replay (its correct mode):** attached existing composer `d1f3741b` ("Sudo permissions…") → recalled *"Jetson passwordless sudo SSH setup"* (replayed context); NO `session/load` line (correct — Replay-tier; its id is a SQLite composer UUID, not an ACP handle). **Cap 5:** listed its own MCP servers (context7, supabase_fairhire, supabase-cookwise, perplexity, retellai-mcp-server) + invoked `[tool: List MCP Resources]`; reader parses `~/.cursor/mcp.json` (5/7 ready). **Replay is the CORRECT choice, confirmed by research (do NOT promote to true resume):** the current cursor-agent advertises `agentCapabilities.loadSession:true`, but that capability is **a known, unfixed Cursor bug** — `session/load` returns *"Session not found"* even for a sessionId the server itself just minted via `session/new` (Cursor forum bug thread; no ETA as of Apr 2026; protocol-inconsistency confirmed). Wiring Cursor to `session/load` would BREAK it. So fork/replay stays — not a Bluey limitation, a Cursor-side defect. Refs: forum.cursor.com (session/load "Session not found" bug), cursor.com/docs/cli/acp. |
| Codex | ✅ | ✅ | ✅ | ✅ resume✅ + fork✅ | ✅ | **2nd agent — 5/5 GREEN, VERIFIED 2026-06-19 (post spine-extraction; Codex is just a registry row, ZERO Codex-specific code in the spine).** Caps 1-3: `bluey agent sessions codex` lists 20 real `rollout-*.jsonl` sessions w/ titles + projects. Handshake: `codex-acp` adapter (`/usr/local/bin/codex-acp`) — `acp_handshake_probe::handshake_codex` passes. **Cap 4 — BOTH modes now work.** *True resume* was initially broken (`session/load` → `invalid session id: …`urn:uuid:`… found `r` at 1`) because we sent the `rollout-…` FILENAME; the adapter wants the **inner `session_meta.payload.id` UUID**. FIXED in `sessions/jsonl.rs`: the JSONL reader now exposes the inner UUID as the session id for Codex (and `find_session_file` resolves by it) — the Codex analogue of Claude's "cwd is the resolver" fix. Re-verified: planted `violet-otter-77`, resumed by UUID → recalled it, log shows `ACP resume (session/load) committed — true in-place resume` (Delta arm, NO fork fallback). *Fork* also verified earlier (planted `tangerine-falcon-92`) and remains the automatic fallback. **Cap 5 PROVEN:** asked for React docs → Codex invoked `mcp.context7.resolve-library-id` + `mcp.context7.query-docs` → answered `useState`. Agent uses its OWN MCP; Bluey never touched secrets. **Display gap (not a cap failure):** `bluey agent connectors codex` shows 0/0 — connector reader doesn't parse Codex's TOML `[mcp_servers.*]` (only JSON). Follow-up task spawned. |
| Gemini | ✅ | ✅ | ✅ | ✅ fork (replay) | ✅ | **5/5 GREEN, VERIFIED 2026-06-19** (after a refreshed `GEMINI_API_KEY` was set where the daemon's `zsh -lic` env recovery picks it up). The earlier "no longer supported for individuals" gate + the free-tier daily-quota wall are both gone on the API-key path. **Verified fresh:** drive → `GEMOK` (no quota error); **Cap 4 replay** → attached the planted `session-2026-06-15T16-19-7944d2a5`, recalled `MARIGOLD-3982`; **Cap 5 MCP** → listed its `github` connector. Caps 1-3 read from `~/.gemini/tmp/<token>/chats`. **Cap 4 = fork/replay by design** (registry `Replay`; `--resume` is latest/index not by-uuid, so the spine replays the transcript and sends no resume id — correctly NO `session/load` line). MCP `mcpServers.github` (env-auth) parsed. |
| Copilot | ✅ | ✅ | ✅ | ✅ resume✅ + fork✅ | ✅ | **3rd agent — 5/5 GREEN, VERIFIED 2026-06-19.** Native ACP (`copilot --acp --stdio`). **Cap 4 TRUE resume works with NO id fix needed** (unlike Codex): Copilot's session id is the `session-state/<UUID>/` dir name, which == the inner `data.sessionId` — already the right `session/load` key. Planted `COPILOT-AZURE-4471`, resumed by UUID → recalled it; log shows `ACP resume (session/load) committed — true in-place resume` (NO fork). Fork remains the auto-fallback. **Cap 5:** Copilot exposes its own MCP tools — listed `perplexity-perplexity_*` + `github-mcp-server-*`; the `perplexity_ask` call fired (`[tool: perplexity-perplexity_ask]`) though it hit the same perplexity 401/quota seen w/ Claude (agent mis-summarized as "NO MCP" once, but the tool list + invocation prove MCP works). Connector reader parses `~/.copilot/mcp-config.json` (`perplexity env_auth ready`). **REQUIRED FIX (landed this session):** Copilot hard-requires Node ≥24; the ACP spawn was NOT applying the runtime resolution the CLI driver uses, so it picked v23 on PATH and failed with *"requires Node.js v24… Currently using v23.7.0"*. Fixed in `acp/client.rs` (see log). |
| Antigravity | ✅ | ✅ | ✅ | ✅ resume (agy) + fork | ✅ | **5/5 GREEN, VERIFIED 2026-06-19 — drives on its OWN tier (no quota wall).** Two corrections landed: (1) stale "GUI-only, no CLI" → Antigravity ships `agy` (real Go CLI v1.0.10). (2) **`agy` has no ACP mode AND `gemini --acp` is the WRONG path** — as of 2026-06-18 the `gemini` CLI stopped serving AI Pro/Ultra/free tiers (moved to `agy`), so routing via gemini hit the dead/quota-capped path (that was the "exhausted daily quota" I wrongly attributed to a shared account). **FIX:** drive via Antigravity's own `agy` CLI through the CLI-driver path — added a `KindTag::Antigravity` + `DriveSpec` in `drive/cli.rs` (`agy -p {prompt}`, resume `agy --conversation {id}`), split it off Gemini's spec, reverted spec_map to `bail!` (no ACP for agy), set registry `drive_command: agy -p`, fixed Fix profile (`--dangerously-skip-permissions` on apply; agy has no `--approval-mode`), and removed the TTY-gated `mcp list` probe. **Verified live:** fresh ask → `AGYOK` (no quota error); **Cap 4 resume** session `28a959e7` → recalled *"GBaMS search and report adjustments"*; **Cap 5 MCP** → `agy` listed its own servers (github, perplexity-ask, supabase×5). Caps 1-3 read from `~/.gemini/antigravity` store. Continuation tier `Replay` in the spine, but `agy --conversation <uuid>` resume works at the CLI level (the exposed UUID IS agy's resume key). |
| Copilot — VS Code (`vs_code_fork`) | ✅ | ✅ | ✅ | ✅ fork/replay via Copilot-CLI bridge | ✅ | **GUI-agent #2 — 5/5 VERIFIED 2026-06-19, NO code fix needed (runtime-pin already handled).** The in-editor GitHub Copilot Chat surface (read_only — no CLI of its own). Caps 1-3: 8 real VS Code chat sessions from `~/Library/Application Support/Code/User/workspaceStorage/<hash>/chatSessions/*.json` (`JsonFiles` reader) w/ real `customTitle`s + projects ("Automated Job Posting System Overview" `[/…/Automated posting]`, etc.). **Cap 4 = cross-surface bridge (its correct mode):** registry `continuation_via: Copilot` → `continuation_bridge_kind` returns Copilot → the VS Code transcript is replayed as context through the **Copilot CLI** (same GitHub account). PROVEN: attached `8f2616a0` → recalled real content (*"event-driven email + scheduled-report ingestion pipeline, Outlook webhook → n8n → Gemini/COLA…"*); log shows the bridge spawned `copilot` AND `runtime_resolve` pinned **Node 24** (`found_version 24.13.0` — the one risk the investigation flagged is handled, because the bridged drive goes through the CLI driver's runtime resolution). **Cap 5:** through the bridge, listed its MCP tools (`github-mcp-server-*`). Note: the bridged Copilot CLI uses its OWN `~/.copilot/mcp-config.json`, not VS Code's `mcp.json` (documented cross-surface seam; MCP-use proven regardless). Connector display reads VS Code's `mcp.json` (playwright ready, context7 needs-reauth = 1/2). |
| Aider | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ | programmatic surface unconfirmed |

> Surfaces: the matrix rows ARE per-surface (e.g. `Claude Code (CLI)` and `Claude Code (App)`
> are separate rows = separate surfaces). See the **SURFACE MAP** below for which agents have
> multiple surfaces (Claude: CLI/App/Agent; Copilot: CLI/VS Code) vs a single one (Codex,
> Cursor, Gemini, Antigravity). All rows here are **backend/spine-verified via the CLI harness**
> (`bluey agent`/`ask` + log-grep); GUI *rendering* of these in the overlay is the separate
> user-verified Phase-3 table above.

### GUI rendering (Phase 3 — USER-verified only)
| Capability | Renders in overlay? | Confirmed by user? | Notes |
|---|---|---|---|
| Session list (name + project) | ⬜ | ⬜ | |
| Ask → answer streams/renders | ⬜ | ⬜ | |
| Pick session → resume/fork visibly | ⬜ | ⬜ | |

---

## SURFACE MAP — how many surfaces each agent has, and how each is SUPPOSED to work
> **Read this FIRST before "testing the GUI/app version" of an agent.** The confusion
> that caused re-loops: "CLI vs GUI" is a real distinction for SOME agents (separate
> discovery rows + separate stores) but NOT others (one surface, or one row reading the
> IDE store). This table is the source of truth for *which* surfaces exist and what each
> reads/drives. A "surface" = a distinct `KindTag` registry row OR a distinct on-disk store.

### Do the CLI and the App SHARE sessions? (web-researched 2026-06-19 — NOT assumed)
**NONE of them functionally share session history between CLI and App.** Each App keeps its
own session store; only *config* (MCP servers, auth, rules) is shared. So every App is a
genuinely separate surface needing its own 5-cap pass.
| Agent | CLI store | App store | App share sessions w/ CLI? |
|---|---|---|---|
| **Claude** | `~/.claude/projects/*.jsonl` | App's own `~/Library/Application Support/Claude/` index | ❌ **NO** — "each maintains their own independent session history"; only config (`~/.claude.json`, MCP, CLAUDE.md) shared. Bluey's `ClaudeAppIndex` reader BRIDGES them by following the index's `cliSessionId` (which is itself buggy upstream — #28791/#63082/#58670). |
| **Codex** | `~/.codex/sessions/` | same `~/.codex` (`CODEX_HOME`) | ⚠️ **Same dir on disk, but NOT functionally shared** — the Codex Desktop App only surfaces its OWN most-recent session and ignores CLI-created ones (openai/codex #21079, #14389). So the App is still a distinct surface in practice. |
| **Cursor** | `~/.cursor/chats/` | App: `state.vscdb` composers | ❌ **NO** — different stores. CLI shares MCP/auth/rules with the app, NOT chat history. |
| **GitHub Copilot** | `~/.copilot/session-state/` | App: VS Code workspaceStorage `chatSessions` | ❌ **NO** — separate. Asymmetric: the App *surfaces* CLI sessions, but the CLI's `/chronicle` can't see App chats (github/copilot-cli #3816). |
| **Antigravity** | CLI `brain/` | App: `…/antigravity/brain`, IDE: `…/antigravity-ide/brain` | ❌ **NO** — separate `brain/` dirs (same agent core; bidirectional sync exists, stores separate). |
| **Gemini** | `~/.gemini/tmp/<token>/chats` | (no separate desktop app) | — CLI only |
Refs: openai/codex #21079/#14389; anthropics/claude-code #28791/#49775/#63082/#58670;
github/copilot-cli #3816; discuss.ai.google.dev "Antigravity 2.0 IDE/CLI shared brain";
deployhq.com Cursor 2026 guide.

**Did the CLI tests accidentally show App sessions? NO — verified 2026-06-19.**
`bluey agent sessions claude_code` (36 ids) vs `claude_code_app` (19 ids) = **0 overlap**.
So the CLI test exercised CLI sessions only; the App test exercised the 19 App sessions only —
neither contaminated the other (which is what makes both valid as separate-surface tests).
Nuance for Claude specifically: the App sessions' underlying JSONL files DO live in the CLI
store (18/19 are the same `~/.claude/projects/*.jsonl`), BUT (a) the CLI list is capped at ~40
recent of 870 files, and (b) a dedup (`summaries.rs`: `claimed_by_app` filter) explicitly
removes app-claimed sessions from the CLI list — so each Claude conversation appears in EXACTLY
ONE surface's list, never both. For Codex/Cursor/Antigravity/Copilot the App stores are
physically separate from the CLI, so there's no file overlap at all.

**Implication for Bluey:** because surfaces DON'T natively share, each GUI surface is a
genuinely distinct store that needs its own 5-cap pass — which is exactly why
`claude_code_app` and `vs_code_fork` were tested separately from their CLIs (both 5/5).

| Agent family | Surfaces (distinct rows) | Reads sessions from | Drives (answers) via | Continuation mechanism | Covered? |
|---|---|---|---|---|---|
| **Claude** | **3 separate rows**: `claude_code` (CLI), `claude_code_app` (desktop App), `claude_code_agent` (App agent-mode) — SEPARATE stores (don't natively share, see above) | CLI: `~/.claude/projects/<enc-cwd>/<id>.jsonl`. App/Agent: own `~/Library/Application Support/Claude/` index whose `cliSessionId` Bluey FOLLOWS back into the CLI JSONL files (Bluey bridges them — the apps don't natively share) | All three: `claude-agent-acp` adapter (same engine) | NativeResume — true `session/load` by `(id, cwd)`; fork fallback | CLI ✅, App ✅, Agent ⬜ (0 sessions) |
| **Codex** | CLI + Desktop App both use `~/.codex` (`CODEX_HOME`). Same dir on disk, but the App ignores CLI sessions (openai/codex #21079) — so the App is a distinct *surface* even though the *store path* is shared. Bluey discovers ONE `codex` row pointing at `~/.codex/sessions`. | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `codex-acp` adapter | NativeResume — true `session/load` by the **inner `session_meta.payload.id` UUID** (NOT the rollout filename) | ✅ (Bluey reads the shared `~/.codex` store — covers both CLI- and App-created rollouts on disk) |
| **Cursor** | **1 row** (`cursor`) reading the IDE store. NOTE: the `cursor-agent` CLI ALSO has its own store (`~/.cursor/chats/<ws>/<uuid>/`) that Bluey does **NOT** read — but that's the CLI's *own* sessions, a different id-space. | IDE composers: `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb` (`cursorDiskKV` table) | `cursor-agent acp` (ACP) / `cursor-agent -p` (CLI) | **Replay/fork** — its composer id is a SQLite db key, NOT an ACP handle; `session/load` is a known-broken Cursor bug. Replay the transcript. | ✅ (IDE store; the CLI's own `~/.cursor/chats` store is intentionally NOT surfaced) |
| **Gemini** | **1 surface** — `gemini` | `~/.gemini/tmp/<token>/chats/*.jsonl` | `gemini --acp` (API-key tier) | **Replay/fork** — `--resume` is latest/index, not by-uuid | ✅ |
| **Antigravity** | **1 real row** (`antigravity`). The `Antigravity` / `Antigravity IDE` read_only rows are DUPLICATE footprints of the same install (App-bundle detected separately) — NOT separate surfaces. | `~/.gemini/antigravity/` (index `agyhub_summaries_proto.pb` + `conversations/<uuid>` + `brain/<uuid>`). The IDE App-Support `state.vscdb` holds only UI state, no transcripts. | `agy -p` / `agy --conversation <uuid>` (its OWN tier — NOT `gemini`, which dropped the Pro tier 2026-06-18) | Replay tier in the spine; `agy --conversation <uuid>` resumes natively at the CLI level (exposed UUID IS agy's key) | ✅ (`antigravity` row; the IDE/dup rows fixed to not break — see `cursorDiskKV` fix) |
| **GitHub Copilot** | **2 separate surfaces**: `copilot` (the standalone CLI) AND `vs_code_fork` (the in-editor VS Code Copilot Chat) — DIFFERENT stores, same GitHub account | CLI: `~/.copilot/session-state/<UUID>/events.jsonl`. VS Code: `~/Library/Application Support/Code/User/workspaceStorage/<hash>/chatSessions/*.json` (`JsonFiles`) | CLI: `copilot --acp` (true resume). VS Code: NO CLI → **bridges to the `copilot` CLI** (`continuation_via: Copilot`) | CLI: NativeResume (dir-UUID). VS Code: replay the transcript THROUGH the Copilot CLI bridge | CLI ✅, VS Code ✅ |
| **VS Code Insiders** | footprint row `Code - Insiders` (read_only) — a VS Code-family fork, like `vs_code_fork` but the Insiders channel | `~/Library/Application Support/Code - Insiders/User/workspaceStorage/.../chatSessions/*.json` (`JsonFiles`, after the cursorDiskKV-shape fix) | (read_only; would bridge like vs_code_fork) | replay | reads 5 sessions after the fix; not run through the full 5-cap harness |
| **Aider** | unconfirmed | unconfirmed | unconfirmed | unconfirmed | ⬜ |

**The rule that prevents re-looping:** before "doing the GUI version" of agent X, check this table.
- If X has **multiple rows** (Claude: CLI+App+Agent; Copilot: CLI+VS Code) → each row is a real
  separate surface that needs its own 5-cap pass (different store, maybe different drive/continuation).
- If X has **one row / one store** (Codex, Cursor, Gemini, Antigravity) → there is no separate
  "GUI version" to test; the single surface is already it. Cursor's row reads the IDE store; the
  `cursor-agent` CLI's own `~/.cursor/chats` store is a deliberate non-target (different id-space).

---

## Known facts / gotchas (carry forward — don't relearn)
- **Cannot verify the overlay GUI** — capture-invisible; only the user can confirm rendering.
- **Agent kind string** is the snake_case `agent_model_label`, e.g. `vs_code_fork` (NOT
  `vscode_fork`) — wrong kind → empty session list (looks like a consent bug, isn't).
- **Claude resume is cwd-scoped — and that's the KEY, not a blocker.** The on-disk session id
  IS the SDK/ACP `session/load` resume key; it resolves `~/.claude/projects/<encoded-cwd>/
  <id>.jsonl`, where the **cwd is part of the path**. So true resume works iff we launch the
  ACP agent with cwd = the session's recorded project dir (the daemon now does this — see the
  `NativeResume if via_acp` arm). If the dir moved/gone → resume can't resolve → we fall back
  to fork. (This corrected the earlier wrong belief that it was an "id-space mismatch".)
  Ref: platform.claude.com/docs/en/agent-sdk/sessions.
- **True resume can corrupt the original transcript** (anthropics/claude-code#36583: messageId
  collision on resume) and a CLI subprocess death can break a session (claude-agent-acp#338).
  → Fork (fresh session + replayed context) stays the SAFE DEFAULT for the meeting-oracle;
  true resume is used when cwd is usable, with fork as the automatic fallback.
- **Gemini `--resume`** takes `"latest"`/index, not a session UUID — can't resume a specific
  past session by id; fork/replay instead.
- **ACP handshake** already PASSED for Gemini / Claude / Codex / Copilot (real session ids).
  Cursor reached-but-account-limited. (Handshake ≠ the 5 capabilities — those are still ⬜.)
- **MCP-use linchpin** (cap 5) is UNVERIFIED for everyone — does a headless-driven agent
  actually call its Jira/GitHub connectors? Test early (plan §5).
- **Production env**: `fix-path-env` at daemon startup recovers the user's shell PATH so
  agents are found when launched from Finder (not just terminal).
- **Cursor advertises `loadSession:true` but it is a KNOWN-BROKEN Cursor bug — do NOT wire
  `session/load` for Cursor.** cursor-agent reports `agentCapabilities.loadSession:true` at
  initialize, yet `session/load` returns *"Session not found"* even for a sessionId the server
  itself just minted via `session/new` (forum.cursor.com; unfixed as of Apr 2026). Cursor stays
  Replay/fork tier ON PURPOSE — its id is a SQLite composer UUID, not an ACP handle. Promoting it
  off Replay because the capability flag looks green will BREAK it. Cursor-side defect, not a
  Bluey gap. (This re-loop already happened once — don't repeat it.)
- **Pick the `.vscdb` session reader by TABLE SHAPE, not by the extension.** Cursor's store has a
  `cursorDiskKV` table; plain VS Code-family forks (Antigravity IDE, VS Code Insiders) ship a
  `state.vscdb` with only `ItemTable`. Assigning Cursor's `SqliteVscdb` reader to every `.vscdb`
  throws `no such table: cursorDiskKV` → broken `-` session counts that LOOK like an empty
  list / consent bug but aren't. `discover.rs::sqlite_has_table()` probes: `cursorDiskKV` →
  `SqliteVscdb`; else `User/workspaceStorage` exists → `JsonFiles`; else no store. Fixed in
  commit 9eb6c1c — do NOT regress when adding a new fork.
- **MCP "NO MCP" can be a FALSE NEGATIVE.** The user's `perplexity` MCP key is 401/quota-exhausted,
  so a driven agent that DOES load + call the tool may report "NO MCP" after the 401. Verify Cap 5
  by checking the agent INVOKED the tool (`[tool: mcp__…]` in output), not by the final wording.
- **Daemon context-accumulation bug (open):** in a long-running daemon, context accumulates across
  asks → eventually `Prompt is too long` even for a tiny fresh ask, until a daemon restart clears
  it. Workaround during testing: restart the daemon between heavy resume tests. (Follow-up task spawned.)

## Verification log (append-only — date, what was tested, result)
- 2026-06-20 — **✅ BACKEND COMPLETION RUN — adversarial gap-audit → fixed all 9 items.**
  A 5-auditor workflow found the real gaps (not assumed); fixed each with a
  machine-checkable proof + a regression gate (581→589 bridge tests, 0 regressions,
  clippy clean throughout). Items:
  - **C4** (resume integrity): an empty resume (Done with no Delta) was silently
    accepted as success — the relocated fake-success trap. Now forks instead, so
    the user never gets a silent empty answer. (acp/drive.rs)
  - **C1** (USP): Codex MCP servers live in `~/.codex/config.toml` (TOML) — were
    unread → 0/0 connectors. Now parsed secret-free → 5 connectors live. (connectors.rs,
    discover.rs, toml dep)
  - **C2** (USP anti-fake): Cap-5 verifier proved tool *availability*, not *firing*
    (a 401'd connector read as "MCP works"). Added `McpStep::Fired` — proven only
    when a real `[tool: …]` ToolCall is observed mid-answer. (prove_drive.rs)
  - **C3** (USP): the `claude-agent-acp` adapter ALREADY hardcodes
    `settingSources:["user","project","local"]`, so it loads user MCP — verified
    live (perplexity fired over ACP). Documented; do NOT add settingSources flags.
  - **C5** (security): IPC `AgentAttach` bypassed the BYOT disclosure gate the
    overlay enforces — now rejects un-disclosed BYOT cloud attach. (app.rs)
  - **C6** (read/name/project): deduped the 3 Antigravity rows → 1; percent-decode
    project paths (no more `%20` mojibake). (discover.rs, antigravity.rs)
  - **C7**: Bluey's own Replay banner + summary prompts leaked as session titles —
    now classified as boilerplate. (sessions/mod.rs)
  - **C9**: deduped the ACP-route gate (`should_use_acp` is now the single source;
    daemon delegates) so route + continuation can't desync. (lib.rs, app.rs)
  - **C8**: regression coverage for the (already-fixed) detach-clears + recoverable-
    retry was found ALREADY present (`normalize_resume_session` + `is_resume_recoverable_error`
    tests) — no new work needed.
  Production decision recorded: ACP stays opt-in (`BLUEY_USE_ACP=1`) for v1 — the
  spine has no CLI fallback if `drive_acp` errors before any Delta, so flipping the
  default would turn adapter/handshake failures into hard answer failures. Sequenced
  path to eventually flip it is in the gap-audit (add a spine-level no-Delta CLI
  fallback first). Commits: e983997, d72465b, 8aa4d82, 7f5860e, 7f6e52d, a9cd6a9,
  3305296, 3f3a45e.


- 2026-06-19 — **✅ FORK-READER MISASSIGNMENT FIXED (the 5th fix — non-Cursor VS Code forks).**
  SYMPTOM: every fork with a `state.vscdb` got Cursor's `SqliteVscdb` reader, so non-Cursor forks
  (Antigravity IDE, VS Code Insiders, the Antigravity App-Support footprint) threw `no such table:
  cursorDiskKV` on every session read → broken `-` counts. ROOT CAUSE: reader picked by extension
  ("it's a .vscdb"), not by table shape — only Cursor's store has `cursorDiskKV`; plain VS Code
  forks have only `ItemTable` and keep chats in `User/workspaceStorage/<hash>/chatSessions/*.json`.
  FIX (`crates/cue-agent-bridge/src/discover.rs`): added `sqlite_has_table(path, table)` and gated
  selection — `cursorDiskKV` → `SqliteVscdb`; else `User/workspaceStorage` exists → `JsonFiles`;
  else no store. VERIFIED: unit test `non_cursor_fork_gets_jsonfiles_not_cursor_reader` (+a 2nd for
  no-store); live, the `cursorDiskKV` errors are GONE and `Code - Insiders` now reads 5 sessions.
  Commit 9eb6c1c. (This is the fix the three "see the cursorDiskKV fix" pointers above refer to.)
- 2026-06-18 — Plan + this tracker created. Nothing in the 5-capability matrix verified yet
  (ACP *handshakes* passed earlier, but that's not the same as the capabilities above).
- 2026-06-18 — **Phase 0 start (Gemini).** Caps 1/2/3 (read sessions / name / project) ✅
  VERIFIED via `bluey agent sessions gemini` — real sessions, titles, project dirs. No GUI.
- 2026-06-18 — **KEY FINDING:** the daemon's actual ask path does NOT use the ACP spine. The
  ACP work (`acp::drive_acp`, `BLUEY_USE_ACP` gate) is real + handshake-proven, but
  `bluey ask` → `answer_with_agent` → `drive_answer_attempt` calls the OLD CLI driver
  (`cue_agent_bridge::drive::drive_with_options`, app.rs:6360), bypassing ACP entirely. This is
  the "spine built but not wired into the real chain" gap. → Caps 4 (resume/fork) + 5 (MCP) go
  through ACP, so they can't work via the real ask until the daemon's agent-answer is routed
  through the spine `drive()`. **First Phase-0 build task: wire answer_with_agent → spine.**
- 2026-06-19 — **✅ Gemini 5/5 — ALL 6 AGENTS NOW 5/5.** A refreshed `GEMINI_API_KEY` (set
  where the daemon's `zsh -lic` env recovery sees it) cleared BOTH the "no longer supported for
  individuals" gate AND the free-tier daily-quota wall — the API-key path bypasses the
  personal-OAuth free tier entirely. Verified fresh: `GEMOK` (no quota), replay recalled
  `MARIGOLD-3982`, MCP listed `github`. No code change needed — purely the auth/key. **Full
  matrix now GREEN:** Claude / Codex / Copilot = true-resume + fork + MCP; Cursor / Gemini =
  fork/replay (their correct mode — id isn't an ACP handle) + MCP; Antigravity = agy resume +
  fork + MCP. The meeting-oracle's agent layer is proven end-to-end on every installed agent.
- 2026-06-19 — **✅ Antigravity NOW 5/5 on its OWN tier — fixed the gemini-routing mistake.**
  Web research surfaced the key fact: **as of 2026-06-18 the `gemini` CLI stopped serving the
  Google AI Pro/Ultra/free tiers — they migrated to Antigravity's `agy` CLI.** So my earlier
  `gemini --acp` wiring for Antigravity was wrong: it routed to the dead/quota-capped gemini
  path (the "exhausted daily quota" I wrongly blamed on a shared account — Antigravity's own
  `agy` tier is NOT quota-capped). `agy` itself has no ACP stdio mode (antigravity-cli#31
  pending), so the fix is the CLI-driver path, not ACP. Changes: new `KindTag::Antigravity`
  + `DriveSpec` in `drive/cli.rs` (`agy -p {prompt}`; resume `agy --conversation {id}`), split
  from Gemini's spec; reverted `spec_map.rs` Antigravity → `bail!` (forces CLI path); registry
  `drive_command` `gemini -p`→`agy -p`; Fix profile `--approval-mode`(gemini)→
  `--dangerously-skip-permissions`(agy, apply only); dropped the TTY-gated `agy mcp list`
  probe (connectors still read from config). Verified live on `agy`'s tier: `AGYOK` (no
  quota), resume recalled "GBaMS search and report adjustments", MCP listed 7 own servers.
  Updated 2 tests. **Lesson:** don't assume sibling CLIs share a tier/auth — `gemini` and
  `agy` diverged on a hard date; route each agent through its OWN CLI.
- 2026-06-19 — **Research correction: Cursor `loadSession:true` is ADVERTISED-BUT-BROKEN → Replay is correct, NOT a stopgap.**
  Followed up the earlier "Cursor may now support true resume" flag with a web search instead
  of assuming. Finding: cursor-agent reports `agentCapabilities.loadSession:true` at
  initialize, but `session/load` is a **known unfixed Cursor bug** — it returns *"Session not
  found"* even for a sessionId the server JUST returned from `session/new` (Cursor community
  forum bug thread; no ETA as of Apr 2026; the server's own docs/capabilities are
  inconsistent with behavior). So promoting Cursor off Replay would BREAK it. The Replay/fork
  default is the *correct* engineering choice, not a placeholder — confirmed external defect,
  not a Bluey gap. (Also confirmed for Gemini: the "exhausted daily quota" is the free
  personal-OAuth tier's daily cap, not our code; the CLI isn't using the user's AI Pro
  entitlement — auth fix deferred per user. There's even a Gemini-CLI bug where quota-exhausted
  is falsely reported while /stats shows quota left.) Refs: forum.cursor.com session/load bug,
  cursor.com/docs/cli/acp, github.com/google-gemini/gemini-cli issues #13222/#17081.
- 2026-06-19 — **✅ Cursor 5/5; Antigravity + Gemini wired & read-verified (drive quota-blocked). 2 more code fixes.**
  Parallel investigate → serial verify again. Two stale-config code bugs found & fixed in
  `spec_map.rs`: (1) **Cursor** argv was `cursor-agent agent acp`; current builds want
  top-level `cursor-agent acp` (old form hangs on TTY → daemon timeout). (2) **Antigravity**
  was `bail!("GUI IDE, no ACP CLI")` — WRONG: it ships `agy` (real Go CLI v1.0.10) but agy
  has no ACP mode, so route via the sibling `gemini --acp` (which its row already drives);
  changed the arm + tests. **Cursor 5/5 GREEN** (account knackkit@gmail.com): fresh ask
  `CURSOROK`, Cap 4 fork recalled "Jetson passwordless sudo SSH setup" from composer
  d1f3741b (NO session/load — correct for Replay), Cap 5 listed 5 MCP servers + invoked
  List MCP Resources. **Gemini gate CLEARED** (OAuth re-login, not an API key) — handshake
  succeeds, reads fine — but Cap 4/5 now hit the **free-tier daily quota** (*"exhausted your
  daily quota"*), the input/output limit the user flagged. **Antigravity** reads its own
  store (titles+projects), 8 MCP connectors parsed, and the drive PROVED the `gemini --acp`
  wiring (reached the model → same Gemini quota error, not a spawn/entrypoint failure).
  Net: 4 agents fully 5/5 (Claude, Codex, Copilot, Cursor); Gemini+Antigravity wired &
  read-verified, live answer gated only by the shared Gemini daily quota (account-side, not
  code). FOLLOW-UP flagged: current cursor-agent advertises `loadSession:true` → may now
  support TRUE resume (promote off Replay later).
- 2026-06-19 — **✅ 3rd AGENT (Copilot) PROVEN 5/5 + a real spine bug found & fixed; Cursor/Gemini diagnosed (externally blocked).**
  Parallel read-only investigation of Copilot/Cursor/Gemini, then serial live verify.
  **Copilot 5/5 incl. TRUE resume** (its dir-UUID == inner `data.sessionId`, so no
  Codex-style id fix needed — `session/load committed`). **Spine bug fixed:** the ACP
  spawn (`acp/client.rs::to_acp_agent`) did NOT apply the runtime resolution the legacy
  CLI driver uses (`runtime_resolve::runtime_path_for_program`), so Copilot — which
  hard-requires Node ≥24 — picked an older `node` on PATH and died with *"requires
  Node.js v24… Currently using v23.7.0"* before the handshake. Fix: probe the program and
  pass a corrected `PATH=…` as a leading argv env entry (consumed by `AcpAgent::from_args`).
  Verified: daemon started from a v23-active shell now self-heals → Copilot answers. This
  was a genuine gap that would bite ANY Node-pinned agent over ACP, not just Copilot.
  **Cursor — true resume architecturally N/A (honest):** Replay-tier; its id is a SQLite
  composer UUID (local IDE db key, not an ACP handle), no `session/load`-by-composer path.
  Fork/replay is correct-and-only. LIVE BLOCKED: free-tier usage limit hit (account-side).
  **Gemini — Cap 4/5 BLOCKED by Google:** handshake gated with *"no longer supported for
  Gemini Code Assist for individuals → migrate to Antigravity"* (oauth-personal tier).
  Replay-tier (resume correctly N/A by id). Needs `GEMINI_API_KEY`/migration — not a code
  fix. **Lesson reinforced:** "both resume AND fork" means per-agent — NativeResume agents
  (Claude/Codex/Copilot) get true `session/load` (find the real id: cwd / inner-UUID /
  dir-UUID respectively); Replay agents (Cursor/Gemini) correctly do fork-only because
  their id genuinely isn't an ACP handle. Forcing session/load on them would be wrong.
- 2026-06-19 — **✅ 2nd AGENT (Codex) PROVEN 5/5 with BOTH resume AND fork — spine-as-rows validated.**
  After the spine extraction, lighting up Codex required NO Codex-specific code in the
  spine — it was already a registry row + the `codex-acp` spec. Verified via CLI:
  read/name/project (20 `rollout-*` sessions), handshake, Cap 5 MCP (`mcp.context7.*`
  invoked → `useState`), Cap 4 **both modes**.
  **Codex true-resume — diagnosed AND fixed in one pass.** First attempt: `session/load`
  rejected our id with `invalid session id: …`urn:uuid:`… found `r` at 1` → auto fork
  fallback (which DID answer correctly, proving `resume_with_fork_fallback`). Root cause:
  we sent the rollout FILENAME (`rollout-<ts>-<uuid>`); the adapter wants the bare UUID,
  which Codex records as the inner `session_meta.payload.id`. This is the Codex analogue
  of Claude's "the cwd is the resolver, not the filename" finding — a per-agent id-source
  quirk, not an unfixable id-space gap. FIX (`sessions/jsonl.rs`): expose the inner
  `session_meta.payload.id` UUID as the session id for Codex rollouts; `find_session_file`
  also resolves by it (+1 unit test). Re-verified: planted `violet-otter-77`, resumed by
  UUID → recalled it; log shows `ACP resume (session/load) committed — true in-place
  resume` (Delta arm, NO fork). So Codex now has TRUE resume ✅ AND fork ✅. (Also surfaced
  a display-only gap: connector reader doesn't parse Codex's TOML `[mcp_servers.*]` —
  MCP-use works regardless; follow-up spawned.)
- 2026-06-19 — **✅ TRUE RESUME INDEPENDENTLY RE-VERIFIED (2nd session, parallel audit).**
  Fresh end-to-end run on a *different* real session (`0241c147…`, project `/Users/ms/Developer/
  Staffing Desk`): pinned via `bluey agent attach claude_code --session <id>`, then `bluey ask`
  with `BLUEY_USE_ACP=1` → resumed agent answered with the session's exact prior facts (renamed
  env var `CEIPAL_STORAGE_STATE`→`CEIPAL_STORAGE_STATE_PATH`, file `ceipal_sourcing.skill.md`
  line 40, service `ceipal_dice`) — replayed its own original tool calls (Read→grep→Edit). Log:
  `ACP resume (session/load) committed — true in-place resume` (Delta arm only → fork never
  fired). Build clean. Robustness probe: synthetic session with unusable cwd → logged
  `cwd unusable → FORK`, recovered context via replay, did NOT mutate the probe file (fork is
  non-destructive). **CONFIRMED DESTRUCTIVE:** resuming `0241c147…` appended the test turn back
  into the SAME `.jsonl` (60494→70710 bytes, 30→42 lines, same inode/session-id) — empirical
  proof of anthropics/claude-code#36583. → keep **fork as the meeting-oracle default**, resume
  opt-in. (Adapter `claude-agent-acp@0.46.0`, Claude CLI v2.0.42.)
- 2026-06-19 — **Cap 4 true-resume RE-DIAGNOSED (my earlier "id mismatch" was wrong).**
  Research: SDK `resume="<id>"` DOES use the on-disk session id directly — it loads from
  `~/.claude/projects/<encoded-cwd>/<id>.jsonl`. The key resolver is the **CWD**: resume finds
  the session only if the launch cwd matches the session's original project dir (the cwd is
  part of the path). `session/load` IS supported by Claude Code + Codex. So true resume IS
  achievable — the failure was **wrong/missing cwd**, not an id-space mismatch. ALSO: my Cap-4
  "fix" routed ACP to REPLAY (dropped `resume`) when via_acp, so native session/load wasn't even
  being attempted in the last tests. **Plan to get true resume:** in the ACP path, when
  resuming, pass the session's real project dir as cwd + call `client.resume(id)` (session/load)
  — don't force replay. We already discover the project dir (Cap 3). CAVEATS (real, from
  research): resume can CORRUPT the original JSONL (anthropics/claude-code#36583), and CLI death
  can break a session (claude-agent-acp#338) — args FOR keeping fork as the safe default and
  resume as opt-in. Refs: platform.claude.com/docs/en/agent-sdk/sessions , #36583, #338.
- 2026-06-18/19 — **✅✅ CAP 5 (use MCPs) FIXED + PROVEN.** Changed our ACP permission handler
  from auto-DENY → auto-ALLOW (pick AllowOnce, else AllowAlways, else first option;
  client.rs). Re-tested: driven Claude **actually invoked `mcp__perplexity__perplexity_ask`**
  (the agent used its OWN MCP connector). Bluey never touched secrets — the adapter loads the
  user's MCP via its `settingSources:["user","project","local"]`; we just stopped blocking the
  tool call. (The user's perplexity API key was quota-exhausted → agent gracefully fell back to
  WebSearch and returned a real current event — proves the tool path works; key is user-side.)
  → **Phase 0 = 5/5 capabilities working for Claude** (true in-place resume is the lone ⚠️;
  fork covers continuation). Still flag-gated (BLUEY_USE_ACP=1), reversible.
- 2026-06-18 — **✅ Cap 5 TRUE ROOT CAUSE found — it's OUR permission handler, not MCP config.**
  The `claude-agent-acp` adapter ALREADY sets `settingSources:["user","project","local"]`
  (acp-agent.js:2213) → it loads the user's OWN MCP servers (with secrets) from their config,
  and merges with ACP's mcpServers (.d.ts:160). So MCP servers ARE available to the driven
  agent — we do NOT need to forward configs/secrets (privacy stays intact). The "NO MCP" is
  because **our `AcpClient` AUTO-DENIES every `requestPermission`** (client.rs:266-275, returns
  `Cancelled` — a Phase-1 placeholder). When the agent tries to call an MCP tool it asks
  permission → we deny → it can't use the tool. **Fix = auto-ALLOW permission (or allow tool
  calls) so the agent can use its connectors.** Simple, and the right behavior for the
  read-only meeting-oracle. **Next: change auto-deny → auto-allow, re-test perplexity.**
- 2026-06-18 — **Cap 5 — BETTER approach found (privacy-correct).** DON'T have Bluey read +
  forward MCP configs: our `connectors.rs` deliberately reads shape ONLY, never the secret
  `env` values (the no-retention principle) — and MCP servers (Jira/perplexity) need their
  secrets to run. So forwarding would violate the core privacy promise. Instead: the **Claude
  Agent SDK does NOT load filesystem MCP settings by default** (why we got "NO MCP"), but
  `settingSources: ["user"/"project"/"local"]` makes it **auto-load the user's OWN MCP servers
  from their OWN config with their OWN secrets** — Bluey never touches secrets. PERFECT fit for
  the meeting-oracle. **Catch:** `settingSources` is a Claude Agent SDK option; we drive via the
  `claude-agent-acp` ADAPTER → must confirm the adapter sets/exposes settingSources (env/flag),
  else the agent won't load MCP regardless. **Next: check if claude-agent-acp enables
  settingSources / loads user MCP config.** Ref: https://code.claude.com/docs/en/agent-sdk/mcp
- 2026-06-18 — **Cap 5 (use MCPs) NOT working yet — root cause found + fixable.** Driven
  Claude (fresh ACP session) asked a question requiring its `perplexity` web MCP → replied
  **"NO MCP"** = the agent's session has NO connectors. Root cause: our `session/new` passes
  NO mcp_servers — `client.rs:300` does `cx.build_session(&cwd).start_session()` (cwd only).
  Fix: the SDK's `SessionBuilder` explicitly "allows you to add MCP servers" (docs.rs;
  cookbook "Per-session MCP server with workspace context"). We ALSO already discover the
  connectors (`bluey agent connectors` shows them). So: pass the agent's discovered MCP
  servers into the session builder. NOT an upstream wall — a wiring gap. **Next: wire MCP
  servers into session/new.**
- 2026-06-18 — **✅ CAP 4 (resume/continue) FIXED + VERIFIED via read-transcript-replay.**
  Implemented: `apply_continuation_tier` now, when driving over ACP (NativeResume agent),
  REPLAYS the session transcript as `question.context` instead of relying on `session/load`
  (which can't target our read ids). `drive_acp` folds that context into the prompt via
  `Question::render_prompt()` (budget-capped). Proof: resumed Claude session c6eedf0c →
  "what was this conversation about?" → **accurate summary of the real prior conversation**
  (vs "NO CONTEXT" before). This is the user-required resume/continue, working over ACP. Note:
  this is effectively the FORK primitive too (fresh session + prior context, original
  untouched). Still flag-gated (BLUEY_USE_ACP=1), reversible.
- 2026-06-18 — **Cap 4 ROOT CAUSE found via research (not our wiring).** ACP `session/load`
  requires: (1) client checks the `loadSession` capability in the initialize response, (2) the
  agent REPLAYS prior messages via `session/update` before completing the load. TWO documented
  failure modes match our "NO CONTEXT": (a) some adapters' `loadSession` "create a fresh
  internal session mapping and return empty immediately, history never sent to the client"
  (agentclientprotocol divergence note); (b) **id-space mismatch** — anthropics/claude-code#8069
  "SDK resume gives a DIFFERENT session_id from the original" → the id we READ from the .jsonl
  store ≠ the id `claude-agent-acp` (Agent-SDK-based) can `session/load`. So loading "our" id
  finds nothing → fresh session → NO CONTEXT.
  **Robust fix (decision pending): option 2 = read the transcript ourselves (we already do,
  reliably) and REPLAY it as context into a fresh ACP session — sidesteps session/load + the
  id mismatch entirely, and IS the "fork/continue in a new chat" primitive the user wants.**
  Refs: https://agentclientprotocol.com/protocol/session-setup ,
  https://github.com/anthropics/claude-code/issues/8069
- 2026-06-18 — **Cap 4 (resume) NOT working yet for Claude — precise finding.** Attached
  `claude_code --session adced0d4...` (a real session in an EXISTING dir
  `…/crates/cue-agent-bridge`); attach said "Resuming on next answer". Asked "what was this
  conversation about?" → Claude replied **"NO CONTEXT"** = resume did NOT load history. Code
  trace: the wiring IS present — `apply_continuation_tier` sets `question.resume` for a
  NativeResume agent when cwd is usable (app.rs:6084-6088), and `drive_acp` calls
  `client.resume(id)` → ACP `session/load`. So the likely root cause is **ACP `session/load`
  on the claude-agent-acp adapter does not load the same on-disk `.jsonl` session the CLI/
  reader uses** (the adapter's session id space ≠ the reader's). Needs investigation:
  (a) confirm question.resume was actually set (log it), (b) check what `session/load` does in
  the adapter. **Cap 4 = the hard one; do NOT rabbit-hole — investigate deliberately.**
- 2026-06-18 — **✅ PHASE-0 BREAKTHROUGH: tap-to-answer works end-to-end via the ACP spine.**
  `bluey ask "Reply PONG"` with claude_code attached + BLUEY_USE_ACP=1 → routed through
  daemon → `acp::drive_acp` → `claude-agent-acp` → real answer **PONG** back on the CLI. The
  full chain (the thing that was NEVER actually working) is proven for Claude. No GUI. Claude
  caps 1/2/3 + answer ✅; caps 4 (resume/fork) + 5 (MCP) next.
- 2026-06-18 — **Spine wiring DONE + verified routing:** added `acp_answer_enabled()` +
  ACP branch in `drive_answer_attempt` (app.rs) so `bluey ask` routes through `acp::drive_acp`
  when `BLUEY_USE_ACP=1`. Confirmed it routes: with the flag on, the Gemini ask now fails with
  an **ACP-layer** error ("acp connection: This client is no longer supported for Gemini Code
  Assist…") instead of the old CLI's "256-color" error → proves the ask now goes through the
  spine. Reversible (flag-gated; default unchanged).
- 2026-06-18 — **Gemini is a BAD Phase-0 choice — gated on both paths.** Old CLI: TTY exit-1.
  ACP: "client no longer supported for Gemini Code Assist" (Gemini-side version/auth gate, not
  our code). → Switching the Phase-0 proof agent to **Claude** (adapter handshook cleanly, most
  important agent). Gemini caps 4/5 deferred until its auth/version gate is sorted.
- 2026-06-18 — **Baseline proof the OLD path is broken for Gemini:** attached gemini +
  `bluey ask "Reply with exactly: PONG"` → "Your gemini CLI couldn't answer (agent exited with
  status 1: Warning: 256-color support not detected…)". The old CLI driver spawns `gemini -p`,
  hits a TTY/terminal warning → non-zero exit → NO answer. Meanwhile the ACP handshake to
  `gemini --acp` worked cleanly earlier. → Routing the answer through ACP is the difference
  between "couldn't answer" and a real answer. Justifies the spine wiring. (Cap 4/5 still ⬜
  pending that wiring.)
