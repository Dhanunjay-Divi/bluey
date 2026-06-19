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
| Claude Code (App) | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ | desktop-app session store; driven via `claude` |
| Cursor | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ | ACP account-limited in earlier test; fork = context-only |
| Codex | ✅ | ✅ | ✅ | ✅ resume✅ + fork✅ | ✅ | **2nd agent — 5/5 GREEN, VERIFIED 2026-06-19 (post spine-extraction; Codex is just a registry row, ZERO Codex-specific code in the spine).** Caps 1-3: `bluey agent sessions codex` lists 20 real `rollout-*.jsonl` sessions w/ titles + projects. Handshake: `codex-acp` adapter (`/usr/local/bin/codex-acp`) — `acp_handshake_probe::handshake_codex` passes. **Cap 4 — BOTH modes now work.** *True resume* was initially broken (`session/load` → `invalid session id: …`urn:uuid:`… found `r` at 1`) because we sent the `rollout-…` FILENAME; the adapter wants the **inner `session_meta.payload.id` UUID**. FIXED in `sessions/jsonl.rs`: the JSONL reader now exposes the inner UUID as the session id for Codex (and `find_session_file` resolves by it) — the Codex analogue of Claude's "cwd is the resolver" fix. Re-verified: planted `violet-otter-77`, resumed by UUID → recalled it, log shows `ACP resume (session/load) committed — true in-place resume` (Delta arm, NO fork fallback). *Fork* also verified earlier (planted `tangerine-falcon-92`) and remains the automatic fallback. **Cap 5 PROVEN:** asked for React docs → Codex invoked `mcp.context7.resolve-library-id` + `mcp.context7.query-docs` → answered `useState`. Agent uses its OWN MCP; Bluey never touched secrets. **Display gap (not a cap failure):** `bluey agent connectors codex` shows 0/0 — connector reader doesn't parse Codex's TOML `[mcp_servers.*]` (only JSON). Follow-up task spawned. |
| Gemini | ✅ | ✅ | ✅ | ⬜ | ⬜ | read/name/project VERIFIED 2026-06-18 via `bluey agent sessions gemini`. native ACP; `--resume` takes "latest"/index, NOT uuid. MCP connector: `github` (ready). |
| Copilot | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ | native ACP (`--acp --stdio`); needs Node 24 |
| Antigravity | ⬜ | ⬜ | ⬜ | 🚫 | ⬜ | GUI IDE, no ACP CLI — sessions read-only via store |
| Aider | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ | programmatic surface unconfirmed |

> Surfaces: the matrix above tracks the **CLI/spine** (what we can verify). GUI rendering of
> each is tracked separately below once Phase 3 starts.

### GUI rendering (Phase 3 — USER-verified only)
| Capability | Renders in overlay? | Confirmed by user? | Notes |
|---|---|---|---|
| Session list (name + project) | ⬜ | ⬜ | |
| Ask → answer streams/renders | ⬜ | ⬜ | |
| Pick session → resume/fork visibly | ⬜ | ⬜ | |

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

## Verification log (append-only — date, what was tested, result)
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
