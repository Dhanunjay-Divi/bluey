# Agent Model & Speed Control — What Bluey Can Control, Per Surface

> Grounded in `crates/cue-agent-bridge/src/registry.rs` + web research on each
> agent CLI/ACP (2026-07-03). This is the honest map of where Bluey can control
> the MODEL and SPEED/EFFORT of an attached coding agent — and where it can't.

## The key distinction: HOW an agent is driven

Every discovered agent row has a `drive_command`. There are three cases, and they
are the whole story:

| Drive type | `drive_command` | Model controllable? |
|---|---|---|
| **CLI-driven** | a real command (`claude -p {prompt}`, `codex exec {prompt}`, …) | ✅ Yes — via the CLI's `--model`/`-m` flag |
| **Empty / read-only** | `&[]` | ❌ No — Bluey can't drive it at all, only read its sessions |
| **ACP (in-place resume)** | via ACP protocol | ⚠️ No — ACP has no model field (bridge-dependent) |

**Critically: the "App" and "Agent" rows are NOT separate GUI apps we can't
control.** For Claude Code, `ClaudeCode` / `ClaudeCodeApp` / `ClaudeCodeAgent` are
three DISCOVERY sources (found via `/usr/local/bin/claude`, `/Applications/
Claude.app`, `~/.claude`) that ALL drive the **same `claude` binary** —
`drive_command: ["claude", "-p", "{prompt}"]` on every one, `model_flag: "--model"`
on every one. So driving the "App" = driving the CLI = **model IS controllable.**

## The Claude-Code "shared-engine CLI" pattern applies to ALL the major agents (verified 2026-07-03)

Every major coding agent ships an official CLI that runs the SAME engine and shares
the SAME account/subscription as its GUI/IDE — so Bluey drives the app via its CLI,
exactly like Claude Code. Confirmed against each vendor's docs:

| Agent | Shared-engine CLI | Same account as the app? | Per-run model flag |
|---|---|---|---|
| **Claude Code** | `claude` | ✅ App+CLI share one engine/store | `--model` (reference case) |
| **Cursor** | `cursor-agent` | ✅ `cursor-agent login` = same Cursor subscription; "works with any model as part of your Cursor subscription" | `--model` (`cursor-agent models` lists) |
| **GitHub Copilot** | `copilot` (GA 2026-02-25) | ✅ same GitHub Copilot account + premium-request/credit pool as VS Code Copilot | `--model=` (also `copilot --acp` for external drive) |
| **Antigravity** | `agy` | ✅ desktop app + CLI share one runtime, account, quota, with bidirectional session sync | `--model` (v1.0.5+; fragile: display-string value, `-p` non-TTY stdout bugs) |
| **Codex** | `codex exec` | ✅ CLI + IDE ext + Codex app share one cached OAuth login (`~/.codex/auth.json`) | `-m` (+ `-c model_reasoning_effort=`) |

**The running IDE window is never driven directly for any of them** — there's no
external "poke the editor" API. You drive the sibling CLI, which shares the account.
Bluey already encodes this for VS Code (`drive_command: &[]`, `continuation_via:
Copilot`); Antigravity IDE could bridge to `agy` the same way.

### Registry bugs this surfaced (3 rows understate what Bluey can do)
- **Cursor** `model_flag: None` → should be `Some("--model")`.
- **Copilot** `model_flag: None` → should be `Some("--model")`.
- **Antigravity** `model_flag: None` → `Some("--model")` (gate on `agy ≥ 1.0.5`; fragile).
- **Codex** already `Some("-m")` — correct.
- **Antigravity IDE** read-only row could bridge to `agy` via `continuation_via`.

## HOW TO DRIVE EACH AGENT (exact headless commands, verified from vendor docs)

The canonical one-shot headless invocation, auth, output, and session-resume per
agent. `{prompt}` = the question. These are what `drive_command` maps to.

### Claude Code — `claude`
```bash
claude -p "{prompt}" --model opus --effort high --output-format json
```
- **Headless:** `-p`/`--print`; `--bare` for scripts (skips discovery). All flags work in `-p`.
- **Model:** `--model <opus|sonnet|haiku|fable|best|opusplan|…|full-id>` (session-only).
- **Effort:** `--effort <low|medium|high|xhigh|max>`; in-prompt `ultrathink` keyword (in-context only).
- **Output:** `--output-format text|json|stream-json` (+ `--json-schema`). `--fallback-model sonnet,haiku`.
- **Auth:** shares the desktop app's engine/store; `ANTHROPIC_API_KEY`/`apiKeyHelper` for `--bare`.

### Cursor — `cursor-agent`
```bash
cursor-agent -p "{prompt}" --model sonnet-4-thinking --output-format json
```
- **Headless:** `-p`/`--print` (read-only unless `--force`/yolo).
- **Model:** `--model <id>` (or `-m`); enumerate with `cursor-agent models` / `--list-models` (per-account).
- **Effort:** no flag — pick a `-thinking` model variant via `--model`.
- **Output:** `--output-format text|json|stream-json` (+ `--stream-partial-output`).
- **Auth:** `cursor-agent login` = same Cursor account/subscription as the IDE; `CURSOR_API_KEY`/`--api-key` for CI.
- **Sessions:** `cursor-agent ls`, `--resume "<chat-id>"`, `--continue` (CLI-to-CLI resume confirmed; CLI↔IDE sync unconfirmed).

### GitHub Copilot — `copilot`
```bash
copilot -p "{prompt}" -s --model claude-sonnet-4.5 --allow-all-tools
```
- **Headless:** `-p` runs non-interactively and exits; `-s` suppresses stats (response-only). `--allow-all-tools`/`--yolo` for unattended.
- **Model:** `--model=<slug>` (also `-m`); env `COPILOT_MODEL`; `/model` interactive. Precedence: agent-def > `--model` > env > settings > default.
- **Effort:** `--effort=<low|medium|high|xhigh|max>` (a.k.a. `--reasoning-effort`).
- **Auth:** GitHub account (same Copilot subscription + AI-credit pool as VS Code Copilot); inherits org policies. `/usage` shows premium requests.
- **External drive:** `copilot --acp` (stdio or `--port`) exposes ACP for a daemon to drive it. VS Code Copilot continues through this CLI.

### Antigravity — `agy` (v1.0.5+)
```bash
agy -p "{prompt}" --model 'Gemini 3.1 Pro (High)' --headless --approve <policy>
```
- **Headless:** `agy -p`/`--prompt`; `--print-timeout` (default 5m). **GOTCHA:** early `agy -p` silently DROPS stdout under a non-TTY (pipe/subprocess) — PTY-wrap with `script -qec` or pin ≥ a fixed version. `--output-format` was rejected in old builds.
- **Model:** `--model` (or `-m`) as of **v1.0.5** (older binaries reject `-model` → `agy update`). Value is a DISPLAY STRING, not a slug; list via `agy models`.
- **Effort:** no separate flag (effort is baked into the model display string, e.g. "(High)").
- **Auth:** shares the desktop app's runtime + account + quota; `GEMINI_API_KEY`/`ANTIGRAVITY_API_KEY` for headless; bidirectional session sync with the GUI.

### Gemini — `gemini`
```bash
gemini -p "{prompt}" -m gemini-2.5-pro --output-format json
```
- **Headless:** `-p`/`--prompt` (or positional prompt) runs headless; `--yolo`/`--approval-mode` for unattended.
- **Model:** `-m`/`--model <id>` — `gemini-2.5-pro` / `-flash`, `gemini-3-pro-preview` / `-flash-preview`; `/model` interactive.
- **Effort:** no flag — speed = pick the model (`-flash` = fast, `-pro` = deep). Gemini 3 "thinking level" is model/API-side, not a CLI flag.
- **Output:** `--output-format text|json` (also `stream-json` JSONL).
- **External drive:** `gemini --acp` (ACP over stdio JSON-RPC); model set client-side per session via ACP `unstable_setSessionModel`.
- **Auth:** `GEMINI_API_KEY`, or Google login (Gemini Code Assist). Note: `agy` REPLACED `gemini` for Antigravity on 2026-06-18 — the two are separate tools now.

### Codex — `codex exec`
```bash
codex exec -m gpt-5.5 -c model_reasoning_effort="high" "{prompt}"
```
- **Headless:** `codex exec` (no TUI). Progress → stderr, FINAL message → stdout (pipes cleanly). `--json` (NDJSON events), `--output-schema <path>`, `-o <file>`, `--skip-git-repo-check`, `--sandbox …`, `--ephemeral`.
- **Model:** `-m`/`--model <model>`. **Effort:** `-c model_reasoning_effort=<minimal|low|medium|high|xhigh>` (default medium).
- **Auth:** one cached OAuth login shared across CLI + IDE extension + Codex app (`~/.codex/auth.json`, keyed to `CODEX_HOME`). `codex login --device-auth` for headless boxes.
- **CAVEAT:** a ChatGPT-plan login gets a **400** for newer `*-codex` models ("model not supported when using Codex with a ChatGPT account") — needs an API key or a supported fallback. This is what Bluey's `resolve-model` handles.
- **Sessions:** `codex exec resume --last` / `resume <SESSION_ID>`.

### Not drivable directly (IDE-owned)
- **VS Code fork, Windsurf, Antigravity IDE (in place):** no external drive API for the running editor. Drive the sibling CLI (`copilot` for VS Code, `agy` for Antigravity) which shares the account. Windsurf has no shared-engine headless CLI → stays read-only.

## Per-surface capability (verified)

### CLI-driven (model + speed CONTROLLABLE — the fix wires these)
| Agent (all its rows) | drive_command | model flag (real) | effort/speed (real) |
|---|---|---|---|
| **Claude Code** (CLI / App / Agent) | `claude -p` | `--model opus` ✅ | `--effort low/high/xhigh/max` ✅ |
| **Codex** | `codex exec` | `-m gpt-5.5` ✅ | `-c model_reasoning_effort=high` ✅ |
| **Cursor** (cursor-agent) | `cursor-agent -p` | `--model` ✅ *(registry wrongly says None — FIX)* | via model choice (`-thinking` variant) |
| **Gemini** | `gemini -p` | `-m gemini-2.5-pro` ✅ | via model (`flash`=fast/`pro`=deep) |
| **Copilot** | `copilot -p` | `--model` ✅ *(registry wrongly says None — FIX)* | `--effort=high` ✅ |
| **Aider** | `aider --message` | `--model` ✅ | `--reasoning-effort` + `--thinking-tokens` ✅ |

> **CORRECTION (live-verified 2026-07-03, during implementation on `agent/agent-bridge-fixes`).**
> The effort column above was help/doc-derived. Live smokes on the installed CLIs
> overturned two rows — only **Codex** effort was confirmed drivable:
>
> - **Claude `--effort` — REFUTED.** claude 2.0.42 `--help` has NO
>   effort/thinking/reasoning flag headless (only `--model`/`--fallback-model`).
>   Wiring `--effort` would fail every Claude drive with unknown-option. → Claude
>   `effort_args: None` (prose-only). Revisit version-gated if a future CLI ships it.
> - **Copilot `--effort` — REFUTED.** The flag parses, but under the default `auto`
>   model (which the answer path always uses) it hard-errors: `Model "auto" does not
>   support reasoning effort configuration`. Sending it would break every Copilot
>   fast/deep answer. → Copilot `effort_args: None` (prose-only).
> - **Codex `-c model_reasoning_effort` — CONFIRMED.** `codex exec -c
>   'model_reasoning_effort="low"'` echoed `reasoning effort: low` in the banner (key
>   applied); the exit-1 is the orthogonal ChatGPT-account model-block. → **the sole
>   live-verified effort row.** Cursor effort stays coupled inside the `--model`
>   bracket syntax (prose-only in v1); Gemini/Antigravity have none.
>
> Aider's effort flags were not live-smoked (no drivable-answer verification this
> round) — treat as help-derived, not confirmed. See
> [work/VERIFY-AGENT-BRIDGE-FIXES.md](work/VERIFY-AGENT-BRIDGE-FIXES.md) for the raw
> smoke evidence and [work/IMPL-AGENT-BRIDGE-FIXES.md](work/IMPL-AGENT-BRIDGE-FIXES.md)
> for what shipped.

### Empty drive / read-only (model NOT controllable — Bluey can't drive them)
| Agent | Why |
|---|---|
| **Antigravity IDE** | `drive_command: &[]` — pure IDE, no headless CLI. Read sessions only. |
| **Windsurf** | `drive_command: &[]` — no headless CLI. |
| **VS Code fork** | `drive_command: &[]` — the IDE owns the model; can't set from outside. |
| **Antigravity (`agy`)** | Has a CLI but `-p` mode REJECTS `--model` (`flags provided but not defined: -model`); uses the last TUI-picked model. Effectively IDE-owned model. |

### ACP path (model advisory only)
The ACP schema has no `model`/`effort` field on `session/new` or `session/prompt`.
Model selection is delegated to bridge-specific `set_config_option` / `set_mode`,
which is fragile. `drive_with_overrides` correctly IGNORES `model_override` on the
ACP/cloud branch. So over ACP, model/effort is **not controllable** until a per-
bridge model param is wired.

## What Bluey does TODAY (the gap)

- **Model:** the user's pick reaches the agent ONLY via the fallback resolver
  (when the agent rejects its own model). On a normal question, `model_override`
  is empty → the agent uses its own configured model. So **the picker does not
  control the model today**, even for CLI agents that support it.
- **Speed:** the fast/balanced/deep choice becomes in-prompt PROSE (a system
  instruction "answer in Fast mode…"), which does reach CLI agents — but it never
  uses the agents' real `--effort` flags.
- **Registry bugs:** Cursor and Copilot are marked `model_flag: None` but both
  support `--model`. These also block the fallback resolver for those agents.

## The fix (to make it real)
1. `registry.rs`: Cursor & Copilot `model_flag: None → Some("--model")`; add
   `effort_flag` + `effort_style` fields (Claude `--effort` separate; Codex `-c
   model_reasoning_effort=` config-kv; Copilot `--effort=` equals; Aider
   `--reasoning-effort` separate; Cursor/Gemini/Antigravity None).
2. `drive/cli.rs`: add `effort_override: Vec<String>` to `DriveOptions`, render
   per `effort_style`, append after the model override.
3. `app.rs` `answer_with_agent`: seed `model_override` from the USER's picked
   model (keyed off `model_flag`) BEFORE the drive loop, not just on fallback;
   render effort args from the speed pick. Keep speed-as-prose for flagless agents.

## Honest UI states (so the picker never lies)
- **CLI agents** (Claude/Codex/Cursor/Gemini/Copilot/Aider): model + speed = **live control**.
- **Antigravity, Windsurf, VS Code, IDE surfaces**: model + speed = **locked (IDE-owned)**.
- **ACP-driven**: model + speed = **advisory**.

## SESSION LIFECYCLE — resume / fork / new (CLI + GUI), verified 2026-07-03

All six agents can resume, fork, and start new sessions on their CLI. Nuances are
(1) whether SPECIFIC-ID resume works in our headless drive, and (2) whether the CLI
session store shares with the GUI.

| Agent | Resume specific id (headless) | Fork/branch | New | Session id on disk | CLI ↔ GUI share |
|---|---|---|---|---|---|
| **Claude** | ✅ `claude -p --resume <id>` (cwd-scoped) | ✅ `--fork-session` / `/branch` | default (`-n` names) | `~/.claude/projects/<cwd>/<id>.jsonl` | ❌ separate per surface |
| **Codex** | ✅ `codex exec resume <id>` | ✅ `codex fork <id>` / `/fork` | default | `~/.codex/sessions/…/rollout-*.jsonl` | ✅ shared per machine (App Server) |
| **Copilot** | ✅ `--resume=<id>` / `--session-id <id>` | ✅ `/fork` `/branch` | `/new` | `~/.copilot/session-state/<id>/` | ⚠️ CLI store yes; editor LM chat no |
| **Cursor** | ✅ `cursor-agent --resume=<chat-id>` | ❌ no fork primitive | default | SQLite `~/.cursor/chats` | ❌ CLI↔IDE don't sync (bug) |
| **Gemini** | ⚠️ **interactive only** — headless `-p` = latest-only, NOT per-id | ✅ `/chat save`→`/chat resume <tag>` | default | `~/.gemini/tmp/<hash>/chats/*.jsonl` | ❌ separate |
| **Antigravity** | ✅ `agy --conversation=<id>` | ✅ `/fork` `/rewind` | default | `~/.gemini/antigravity-cli/brain/<id>/` | ✅ import/copy both ways |

### Bluey classification corrections (registry.rs `continuation`)
Bluey's tier (NativeResume = resume by id / Replay = replay transcript, new session):
- **Cursor: Replay → NativeResume** (STALE). `--resume={id}` already wired in
  `drive/cli.rs COMMAND_MAP` but dead because tier=Replay. Two-word fix.
- **Antigravity 2.0: Replay → NativeResume** (STALE). `--conversation {id}` already
  wired. Two-word fix. (Antigravity IDE row stays Replay — GUI-only, no CLI.)
- **Gemini: keep Replay** — the "stale" hypothesis is FALSE here: per-id resume is
  interactive-only; the headless drive can only `--resume latest`. Honest limit.
- Claude / Codex / Copilot: already NativeResume ✓. Aider / Windsurf / VS Code
  (bridges via Copilot): keep Replay ✓.

### Fork-safety
"Fork = safe default" is genuinely required only for: **Gemini** (no headless per-id
resume), **Aider / Windsurf / VS Code / Antigravity-IDE** (no drivable CLI resume).
Native resume is SAFE (never touches the user's live IDE session) for Claude
(separate stores), Codex (designed for cross-surface resume), Copilot (editor chat
untouched), **Cursor** (CLI/IDE don't sync → CLI resume can't reach the IDE), and
**Antigravity 2.0** (resume operates on a CLI-side copy). Net: promote Cursor +
Antigravity 2.0 to native resume; keep the rest.

### The fix (net surface)
Exactly TWO one-line edits in `registry.rs`: Cursor row + Antigravity-2.0 row,
`ContinuationTier::Replay → NativeResume`. `resume_args` already correct in
`drive/cli.rs`. Also correct the stale doc-comments (registry.rs Replay enum doc,
continuation/tier.rs) and the memory note "Replay agents are correctly fork-only"
(right for Gemini/Aider/Windsurf/VS-Code/Antigravity-IDE; wrong for Cursor + Antigravity 2.0).

---

## Session discovery — how production systems find sessions (research + Bluey verdict)

Completes the resume research loop: *to resume, you first have to discover the session.*
Grounded in the real code in `crates/cue-agent-bridge/` + `crates/cue-daemon/src/app.rs`.
Sources: Vibe Kanban #2993, ACP session-list RFD, Cursor forum (session/list, Apr 2026),
Paxel data-handling page.

### The three production patterns
1. **File-scrape the vendor stores** — read `~/.claude/projects/*.jsonl`,
   `~/.codex/sessions`, Cursor's `state.vscdb`. **Paxel (YC) is the pure example**: a
   read-only *analytics* scraper that never resumes. This is the ONLY way to see
   sessions you did not spawn. Fragile: breaks whenever a vendor changes schema/path
   (3 layouts already across Claude/Codex/Cursor) and can't reach remote/containerized
   agents.
2. **Mint-and-track your own ids** — the orchestrators (Vibe Kanban, Claude Squad,
   Conductor) spawn the agent CLI themselves, capture the id it emits at spawn, store it
   in their OWN DB (VK: `execution_processes.agent_session_id`), and resume via native
   `--resume <id>`. The vendor store is only the substrate `--resume` reads, never the
   discovery source. **Most robust** — decoupled from transcript format; depends only on
   the CLI's stable `--resume` contract.
3. **ACP `session/list` + `session/load`** — the protocol standard. `session/list`
   returns `{sessionId, cwd, …}`; `session/load` restores. Shipped in Cursor CLI
   `2026.04.16` + Zed, explicitly to avoid filesystem coupling and support remote agents.
   But **not exposed by Claude Code or Codex** — the two biggest agents in our stack — so
   it's forward-compat, not a baseline yet.

**Production baseline (mid-2026): (2) as the spine, (1) as fallback for un-spawned
sessions, (3) as forward-compat.**

### What Bluey actually does — the hybrid (Paxel framing undersells it)
- **Mint-and-track (2) IS present.** The CLI drive parses the agent's emitted id into
  `AnswerChunk::Started { session_id }` (`drive/cli.rs`, `system/init`+`result` events);
  the daemon captures the last non-empty id per turn (`app.rs:7203`) and **persists it as
  the resume key for the next turn** (`app.rs:7499`), gated by `agent_chains_by_session_id`
  (`app.rs:349`, true only for NativeResume-tier agents). Exactly Vibe Kanban's pattern —
  we own the minted id for chaining and do NOT re-scrape to continue an active thread.
- **File-scrape (1) is only the DISCOVERY layer** — the `SessionReader` trait, one decoder
  per format (`sessions/mod.rs`), feeding `bluey agent sessions <agent>`, for enumerating
  sessions we did not spawn. Paxel's job, done more carefully.
- **Partial ACP (3).** For NativeResume agents over ACP we attempt real `session/load`
  with fork-fallback (`acp/drive.rs`, `continuation/tier.rs:146`). We consume
  `session/load` but NOT yet `session/list`.

### The drift risk + Bluey's mitigation (ahead of the field)
Format drift is an operational certainty (Claude v2.1.128 nulled legacy `messages`;
Cursor `ItemTable`→`cursorDiskKV`; Codex `RolloutLine`; Antigravity 2.0 dir split —
documented `sessions/mod.rs:34`). **No researched tool, Paxel included, has a drift
detector.** Bluey's health canary (`list_with_health_check`, `mod.rs:128`) emits one
structured `unrecognized_format` warning when a store holds raw records but parses ZERO —
using a parse-success *ratio* (`ReaderHealth::Parsed{parsed, raw_total}`), correctly
rejecting the naive "warn on empty" (false-positive on genuinely empty stores). The
`all_untitled` refinement (`mod.rs:141`) catches *body*-format drift even when id+mtime
rows still surface. Best-in-class at *detecting* drift.

But detection ≠ avoidance, and the production tools avoid it structurally: mint-and-track
means a schema change **can't** break resume of a session you spawned, because you never
re-read the JSONL to resume — you hold the id and call `--resume`. We already have this
for the hot path. Residual exposure is confined to enumerating un-spawned / cross-surface
sessions, where file-reading is intrinsic (even ACP `session/list` is a vendor read of the
same store).

### Verdict
**Substantially right — and in two places ahead of the field.** Not a naive Paxel
scraper: we run the production-standard mint-and-track spine for live chaining, use
file-scraping only where it's unavoidable, attempt true ACP `session/load`, and add a
drift canary no peer has. Honest read: *ahead on detection + multi-format coverage,
behind on protocol adoption and on making our OWN sessions drift-proof.*

Because our reason-to-resume is that the **meeting's context must carry across the resumed
session**, a silent drift-induced resume failure = agent starts fresh, meeting context
lost. Engineer that out, in this order:

1. **Spawn-time session ledger** *(highest value, self-contained)* — append-only
   `{agent, session_id, cwd, spawned_at}`, written where `app.rs:7499` already persists the
   chained id. Makes our OWN sessions resume without ever re-reading a vendor store — closes
   the "resume re-reads the store for cwd" gap (`continuation/tier.rs:70`) and the "we
   re-scrape to enumerate our own past sessions" gap together, directly protecting
   meeting-context continuity. This is the change that most raises robustness.
2. **Consume ACP `session/list` where advertised (Cursor/Zed first)** — gate on the
   `sessionCapabilities.list` flag in `initialize`; fall back to `SessionReader` when
   absent. Highest payoff on Cursor: most volatile store AND the one place the protocol path
   is live today. Keep `SessionReader` as the universal fallback (Claude/Codex have no
   `session/list` server).
3. **Wire the canary's `unrecognized_format` warning to user-visible telemetry** — so real
   vendor drift surfaces as an actionable signal the day it ships, not just a log line.
4. *(Lower)* **Cross-surface reconciliation** — dedup a session appearing in two stores
   (Cursor CLI ≠ IDE, confirmed real) so resume picks the resumable surface.

Net: not "rip out file-scraping" — "you already have the robust mint-and-track spine;
extend it to discovery/resume of your OWN sessions (ledger), then adopt `session/list` as
vendors ship it, keep the canary as the safety net for everything you didn't spawn."

**Key code refs:** mint-and-track spine `app.rs:7203`(capture)+`:7499`(persist)+`:349`;
discovery/canary `sessions/mod.rs:128`; resume cwd re-read `continuation/tier.rs:70`;
ACP true-resume `acp/drive.rs`.
