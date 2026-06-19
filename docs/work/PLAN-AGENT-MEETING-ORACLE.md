# PLAN — Agent Meeting Oracle (vertical-slice, phased build)

> **Status:** active plan. Supersedes the ad-hoc "build all CLIs + cloud + GUI at once"
> approach that caused the "backend passes but it doesn't actually work end-to-end" mess.
> Branch: `agent/acp-migration`. Related: [[RESEARCH-SCOPE-ELEMENTS]] (E3–E6, E16, E17),
> [[PLAN-AGENT-BRIDGE]], the ACP work already landed in `crates/cue-agent-bridge/src/acp/`.

---

## 1. What we're building (the goal, in plain words)

A **meeting copilot** (invisible overlay you talk to during calls) whose intelligence comes
from **the user's own coding agent** — because that agent is the best, safest source of
project answers:

- **No data-retention risk** — it's the user's own agent, on their machine/account. Nothing
  leaves their environment.
- **Already seasoned on the project** — it has the repo, history, CLAUDE.md/AGENTS.md context.
- **Already has MCP connectors** — Jira, GitHub, Slack, etc. — so it can pull **live project
  data** to answer meeting questions ("what's the status of the auth ticket?", "did the
  payments fix ship?").

So Bluey does exactly two things with the user's agent (the visual app is irrelevant — we
never drive or watch it):

1. **READ their session data** — past conversations, titles, projects (from disk).
2. **TAP the agent to ANSWER** — drive it (headless) to answer project questions using its
   seasoning + MCP connectors, and **resume/continue** prior chats.

The overlay is just where the answer is shown.

---

## 1a. DEFINITION OF DONE — the 5-capability matrix (the real target)

The product is "done" when, for **every agent** and **on every surface** (CLI + GUI; cloud
later), Bluey delivers these **5 capabilities**:

1. **Read sessions** — list the agent's past conversations.
2. **Name** — the real chat title for each.
3. **Project / directory** — which repo/folder each session belongs to.
4. **Resume / Fork** — continue an existing conversation. **Both are required where the agent
   supports both** (resume = exact in-place continuation; fork = branch into a new chat with
   prior context, non-destructive). Where an agent supports only one, that one suffices —
   record the level per agent.
5. **Use their MCPs** — when the agent answers, it actually invokes its own MCP connectors
   (Jira/GitHub/Slack/…), so meeting answers are grounded in live project data.

**Tracking matrix** (every cell verified; resume/fork cell notes the level —
full-history fork vs context-only fork vs resume-by-id):

| Agent | 1 Read sessions | 2 Name | 3 Project/dir | 4 Resume / Fork | 5 Use MCPs |
|---|---|---|---|---|---|
| Claude Code | ✅ | ✅ | ✅ | ✅ **resume (session/load) + fork** — both proven | ✅ |
| Cursor | | | | | |
| Codex | | | | resume + fork (`/fork`) — *target* | |
| Gemini | ✅ | ✅ | ✅ | (resume = "latest"/index only) | |
| Copilot | | | | | |
| Antigravity | | | | | |
| (others) | | | | | |

> **Claude Code row is COMPLETE (2026-06-19)** — all 5 capabilities verified end-to-end via
> the CLI (read/name/project from disk; true in-place resume AND fork both proven with real
> runs + logs; MCP connector actually invoked). See [[STATUS-AGENT-CAPABILITIES]] for evidence.
> This closes Phase 0 and the Claude portion of Phase 1.

- Capabilities **1–5 are all CLI-verifiable by the agent** (they are data + driving, no GUI).
- The GUI's only job is to **render** these 5 — that part is user-verified (§2).
- A vertical slice / phase is "done" only when its target cells are GREEN with real evidence,
  not when a layer compiles.

---

## 2. Hard constraint that shapes EVERYTHING: we cannot verify the GUI

**Bluey's overlay is screen-capture-invisible — the agent (and the dev) CANNOT screenshot,
click, or watch it.** Verified the hard way (every screenshot path is blocked at the
compositor). Consequence:

| Layer | Who can verify it |
|---|---|
| Read sessions (files on disk) | ✅ **Agent** — `bluey agent sessions`, logs, tests |
| Tap-to-answer + resume (drive via ACP/CLI) | ✅ **Agent** — `bluey ask`, structured output, logs |
| Overlay **rendering** of the above | ⚠️ **User only** — by eye |

**This is why "backend works but frontend doesn't" kept happening:** layers were unit-tested
in isolation and called "done", but nobody drove the WHOLE chain, and the GUI exposed seams
no one crossed. The fix is in §3.

---

## 3. The build principle (how we avoid the mess)

**Build ONE shared spine + thin front-ends + agents-as-config. Prove each vertical slice
END-TO-END before adding the next. Never call a layer "done" — only call a user-journey
"done".**

- **Spine (build once):** one driver (ACP — already proven) `(agent, question) → structured
  answer stream`; one registry where **each agent is a data row** (binary, args, resume/fork
  support); one answer-record path (turns, session id) the front-ends share.
- **Agents are NOT builds — they are registry rows.** Adding Cursor vs Claude = add a row +
  one quick check. We do NOT build "Cursor" then "Claude" then "Codex" as separate features.
- **CLI and GUI are NOT separate products — they are two thin shells on the same spine.**
  Build the spine + CLI first (the half the agent CAN verify); the GUI is a dumb renderer of
  the same proven `answer()`.
- **Cloud is a separate, LATER entry point** — explicitly deferred. Do not build it in
  parallel.
- **Verification rule:** the agent proves the spine via the CLI (full chain, real output).
  The user confirms only the thin GUI rendering. The agent NEVER claims the GUI works.

---

## 4. Phases (in order — each has a concrete "DONE =" gate)

### Phase 0 — Lock the spine on ONE agent: ALL 5 capabilities, CLI-verified
*The only expensive phase. Proves the full §1a capability set on one agent before scaling.*
- Pick the first agent (see Open Decisions). Wire it through the existing ACP spine.
- **DONE =** from the **CLI**, the agent (me) demonstrates **all 5 capabilities (§1a)** for
  that one agent, with real output:
  1. **Read sessions** — `bluey agent sessions <agent>` lists real past conversations.
  2. **Name** — each has its real title.
  3. **Project/dir** — each shows its repo/folder.
  4. **Resume / Fork** — pick a real session → continue it (resume in place AND/OR fork into a
     new chat) → the answer proves it has the prior context. (Robustness ladder = Phase 1;
     Phase 0 needs it working at the basic level.)
  5. **Use MCPs** — `bluey ask "<question needing Jira/GitHub>"` → the agent answers AND its
     MCP connector is actually invoked (the §5 linchpin).
- All verified by the agent via logs/output — **no GUI involved.** This fills the first row of
  the §1a matrix completely.

### Phase 1 — Resume / continue a chat (the part the user emphasized is REQUIRED)
*Make "continue an existing conversation" actually work, robustly.*
- Implement the continuation ladder on the spine:
  - **Native resume by id** where the agent supports it (`--resume <id>`), cwd set to the
    session's project dir.
  - **Fork** (`--fork-session` on Claude = full prior history into a new branch; non-destructive
    — doesn't disturb the user's live session) as the safer/primary primitive for the
    meeting-oracle. Codex `/fork`. (See [[RESEARCH-SCOPE-ELEMENTS]] E6/E16 findings.)
  - **Fresh-in-project fallback** when the project dir is gone/moved (seasoning still comes
    from repo + MCP).
- **DONE =** from the CLI: pick a real past session id → continue it → the agent's answer
  shows it has the prior context. Proven by the agent. Each agent's level documented
  (full-history vs context-only fork).

### Phase 2 — CLI front-end is the proven product surface
*The CLI is the proof the spine is real (not a unit-test illusion).*
- Ensure `bluey ask` / `bluey agent ...` exercise the FULL spine (read + answer + resume) as
  a real user would.
- **DONE =** the agent can demonstrate the entire meeting-oracle journey through the CLI,
  start to finish, with real output.

### Phase 3 — Overlay (GUI) as a thin renderer of the proven spine
*Small surface; the only phase the USER verifies.*
- The overlay calls the SAME `answer()` / session-read path as the CLI. Its only jobs: send
  the event, render the streamed answer, list sessions, trigger resume/fork.
- **DONE =** the **user** confirms by eye: sessions show with names, asking returns a visible
  answer, picking a session continues it visibly. (Agent confirms the data reached the
  overlay; user confirms it rendered.)

### Phase 4 — Scale to all agents (fill the §1a matrix, cheap)
- Add each remaining agent as a **registry row**; verify its row of the §1a matrix via the CLI
  (read sessions ✓ / name ✓ / project ✓ / resume+fork level / MCP-use ✓). No new build per
  agent — the spine already does the work.
- **DONE =** every agent's row in the §1a matrix is GREEN (with the resume/fork level noted).

### Phase 5 — Cloud (separate entry point, later)
- Only after 0–4 are solid. Its own phase, its own plan section. Not touched before then.

---

## 5. The "linchpin" to verify early (make-or-break, CLI-verifiable)

**When we drive the user's agent headlessly, does it actually USE its MCP connectors
(Jira/GitHub) to answer?** The whole meeting-oracle value depends on this. It is testable
without the GUI (ask a question that requires an MCP tool; confirm the connector is called).
If the agent ignores MCP tools in headless/ACP mode, that is the one real gap to solve — and
we want to know in Phase 0, not Phase 4.

---

## 6. What's already done (don't rebuild)
- ACP client + driver (`crates/cue-agent-bridge/src/acp/`) — handshake proven on Gemini,
  Claude, Codex, Copilot (Cursor account-limited).
- Dynamic discovery → real installed paths (`acp_spec_for_discovered`).
- `fix-path-env` at daemon startup (works when launched from Finder, not just terminal).
- Session readers (`sessions/`) — read titles/projects/history from disk.
- Maintained ACP adapters in use (not hand-rolled).

## 7. Open decisions (for the user)
- **First agent for Phase 0/1:** Gemini (cleanest/free, native ACP) vs Claude Code (most
  important + only TRUE full-history fork). Recommendation: prove the spine on Gemini, then do
  Claude first in Phase 1 because its resume/fork is the richest and most valuable.
- Anything in §1 (the goal) that's still not quite right.
