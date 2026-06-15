# Session Continuation Architecture — "continue any agent's conversation, seamlessly"

> **Status:** Design (for approval before build). Branch `agent/agent-bridge`.
> **Date:** 2026-06-15.
> **Goal:** Let Bluey **continue an existing conversation** with any of the user's
> agents — Claude, Codex, Cursor, Antigravity, Copilot, VS Code — including very
> long ones, **without hitting "prompt too long"**, by doing it *the same way the
> coding agents themselves do it*.
> **Constraint reaffirmed:** Bluey holds no AI of its own. Any summarization is
> done by **driving the user's own agent**, never a Bluey-hosted model.

---

## 1. What we learned (web-verified, not assumed)

How do the coding agents continue a conversation — and continue it *seamlessly*
even after the user switches models mid-way on a huge thread?

**The mechanism is message-list replay, not a privileged protocol.**

- **Cursor** ([verified](https://www.mindstudio.ai/blog/never-switch-models-mid-conversation-ai-agents), [Cursor Learn: Context](https://cursor.com/learn/context)): the conversation is stored as *"a long list"* of messages, **independent of any model**. Switching models *"resends the full message history to the new model, which recomputes from scratch — the history is visible to the new model, not cached."* The new model continues seamlessly **because it's handed the full text history.** There is no shared model state (KV cache is model-specific and is NOT what carries over). The cost of a switch is recomputation + tokens — which is why Cursor docs warn against gratuitous switching.
- **Claude** ([compaction docs](https://platform.claude.com/docs/en/build-with-claude/compaction), [context-window docs](https://code.claude.com/docs/en/context-window)): same replay, **plus server-side auto-compaction at ~80% of the window** — it summarizes older turns into a compressed state, keeps recent turns verbatim, and continues in the same session. Productized as a server-side API (Opus 4.6+).
- **Cursor again**: when the message list exceeds the window it calls `summarizeConversation()` — a fresh window seeded with a summary; it even writes history to a file the agent can re-read to reduce summary loss.
- **2026 industry standard** ([Agent Context Engineering](https://agentmarketcap.ai/blog/2026/04/11/agent-context-engineering-sliding-windows-memory-2026)): **hierarchical memory** — a *hot layer* (last ~10 turns, verbatim) + a *compressed* history, with summarization triggered at progressive pressure thresholds (70/80/85/90%).

**Two consequences for Bluey:**

1. **"Continuing a conversation" = replaying the stored message list to the model.**
   Bluey already *reads* every agent's message list (the session readers we built).
   So Bluey can continue *any* agent's conversation — including Cursor/VS Code,
   which expose **no** CLI resume — by replaying the transcript it already has.
   For Cursor specifically, this is **identical** to what Cursor does on a model
   switch; we are not approximating, we are doing the same operation.

2. **"Prompt too long" is a real wall that the apps hit too** — and they solve it
   by **compaction** (summarize old, keep recent). Bluey must do the same.

---

## 2. The architecture (mirrors the agents')

Two tiers, picked per agent from registry data — **never** an `if agent == …`.

### Tier 1 — Native resume (Claude, Codex, Antigravity)
Use the agent's own resume command. **The vendor's own auto-compaction then
handles "too long" for us, server-side, for free** — we inherit their exact
seamless-infinite behavior.

- **Claude**: `claude --resume <id> -p "<prompt>"` — **cwd-scoped**: must run from
  the session's project dir (resume resolves against `~/.claude/projects/<encoded-cwd>/`).
  Bluey now reads each session's project, so it can `cd` correctly.
- **Codex**: `codex resume <id> --skip-git-repo-check "<prompt>"` — sessions are
  cross-surface portable (CLI ↔ desktop share `~/.codex/sessions/`).
- **Antigravity**: `agy --conversation=<id>` — **NOT** `gemini --resume` (different
  binary, different store: Antigravity sessions live in `.gemini/antigravity/`,
  `gemini` looks in `~/.gemini/tmp/`). Requires the `agy` binary; if absent, fall
  back to Tier 2.

### Tier 2 — Replay-with-compaction (Cursor, VS Code, and any agent w/o resume)
Bluey replays the transcript it already read, **sized to fit the model window**,
doing the same hierarchical compaction the apps do:

```
if transcript fits the window budget:
    replay it whole            (exactly like a Cursor model-switch)
else:
    keep the last N turns verbatim (the "hot layer")
    + a SUMMARY of the older turns   (the "compressed layer")
    → replay [summary] + [recent N] + [new prompt]
```

**Who summarizes?** Bluey runs no AI. So the summary is produced by **driving the
user's own agent**: one drive — *"Summarize this earlier conversation in ≤K words,
preserving decisions, code, and open threads"* — then the real continuation drive
with `[summary] + [recent verbatim] + [prompt]`. This is exactly Cursor's
`summarizeConversation()` pattern, done through the user's agent. (Mechanical
truncation — just dropping old turns — was rejected: it's lossy in the dumb way
the apps specifically avoid. We match the apps: summarize, don't truncate.)

**Size budget:** a per-agent/model token budget (conservative default, e.g. ~50%
of the smallest likely window so the answer has room). Bluey estimates transcript
size (chars/4 ≈ tokens, bounded) and only compacts when over budget — the same
threshold logic the apps use (~80%). Under budget → replay whole, zero extra cost.

---

## 3. What already exists vs. what's new

The `Question` struct **already** carries both levers (good foundation):
- `resume: Option<String>` — Tier 1 native resume id.
- `context: Option<Transcript>` — Tier 2 replay history.
- `render_prompt()` **already replays** `context` ahead of the prompt — **but
  UNBOUNDED**, which is the current "too long" bug.

**New work:**
1. **Wire resume end-to-end.** Today `attach --session` says *"resume lands later"*
   — the daemon `ask` path doesn't pass `resume`/`context` to the drive. Wire it:
   `attach --session <id>` (or a new `ask --resume <id>`) → daemon loads the
   session → builds a `Question` with resume (Tier 1) or context (Tier 2) → drives.
2. **Tier router (data-driven).** A registry field per agent: `Native` (with the
   resume command shape) vs `Replay`. Antigravity = `Native` via `agy`; Cursor/VS
   Code = `Replay`; Claude/Codex = `Native`.
3. **cwd-aware native resume.** Drive from the session's project dir for Claude
   (and Codex). Bluey now has the project (just added).
4. **Size-aware compaction for Tier 2** (`render_prompt` becomes budget-aware):
   under budget → replay whole; over → summarize-old + keep-recent via a
   summary drive through the user's agent.
5. **Antigravity `agy --conversation=<id>` drive spec** (separate from gemini).

---

## 4. Per-agent outcome

| Agent | Tier | Continue mechanism | "Too long" handled by |
|---|---|---|---|
| **Claude** (CLI/App/Agent) | Native | `claude --resume <id>` (cwd-scoped) | vendor auto-compaction (server-side) |
| **Codex** | Native | `codex resume <id>` | vendor auto-compaction |
| **Antigravity** | Native | `agy --conversation=<id>` | vendor (Antigravity) |
| **Cursor** | Replay | resend transcript (= its own model-switch) | Bluey compaction (summary drive) |
| **VS Code (Copilot)** | Replay | resend transcript | Bluey compaction |
| **Copilot CLI** | Native* | `copilot -p … --resume=<id>` (verified) | vendor |

\* Copilot CLI *does* have `--resume=<id>` (verified live earlier), so it's Native
too; the VS Code Copilot *extension* (no CLI) is the Replay case.

---

## 5. Why this is the right design (not a compromise)

- It **is** the agents' architecture: replay the message list; summarize when over
  budget. Verified, not assumed.
- It **prefers native resume** so we inherit vendors' battle-tested server-side
  compaction for free where it exists.
- It **covers the agents that expose no resume at all** (Cursor, VS Code) — which
  no amount of "hooking into the app" could do legitimately (their internal channel
  is in-process Electron IPC, no external API; reverse-engineering it = the same
  off-limits territory as the keychain/app-impersonation line).
- It keeps Bluey **AI-less**: summarization is the *user's* agent summarizing the
  *user's* conversation.

---

## 6. Open decision (resolved per user)

Compaction method when over budget: **(b) summarize via a drive through the user's
agent** — the highest-quality option and the one the apps actually use — chosen
over (a) mechanical truncation. ("Whichever is best; same as the coding agents'
architecture.")

---

## 7. Build order (each step verified before the next; nothing committed until approved)

1. Tier router as registry data + the `agy --conversation` Antigravity drive spec.
2. Wire resume end-to-end through the daemon (`attach --session` / `ask --resume`).
3. cwd-aware native resume (drive from the session's project dir).
4. Size-aware Tier-2 replay: budget check → replay-whole or summarize-old+keep-recent.
5. Live tests: continue a real long session on Claude (native, cwd-scoped) and a
   real Cursor session (replay + compaction), proving no "too long" and that the
   continuation actually recalls prior context. (Codex resume works mechanically
   but its account is model-blocked; Antigravity needs `agy` installed.)

---

## Sources
- Cursor model-switch = full-history replay, not cached: https://www.mindstudio.ai/blog/never-switch-models-mid-conversation-ai-agents
- Cursor context as a message list / resend each request: https://cursor.com/learn/context
- Cursor `summarizeConversation()` at window limit: https://forum.cursor.com/t/max-context-window-summarized-when-switching-models/154504
- Claude server-side auto-compaction (~80%): https://platform.claude.com/docs/en/build-with-claude/compaction , https://code.claude.com/docs/en/context-window
- Hierarchical memory (hot + compressed, threshold-triggered): https://agentmarketcap.ai/blog/2026/04/11/agent-context-engineering-sliding-windows-memory-2026
- Codex cross-surface session portability: https://codex.danielvaughan.com/2026/04/08/cross-surface-session-sync/
- Antigravity `agy --conversation=<id>`: https://medium.com/google-cloud/antigravity-cli-tutorial-series-part-2-conversations-conversations-and-conversations-76f61756d5bb
- Claude resume cwd-scoping + separate desktop history: https://code.claude.com/docs/en/sessions , https://github.com/anthropics/claude-code/issues/56038
