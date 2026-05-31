# PLAN — Fix Button (review-gated agent apply)

> Status: DESIGN. Branch: `agent/agent-bridge` (builds on the agent context bridge).
> Author: Claude. Date: 2026-05-31.
> Cross-refs: `docs/work/PLAN-AGENT-BRIDGE.md`, `crates/cue-agent-bridge/`,
> `crates/cue-core/src/cards.rs`, `crates/cue-daemon/src/app.rs`.

---

## 1. What it is

A **Fix** button on an agent answer. Instead of only *diagnosing* a problem, it
asks the attached agent to **propose a fix** — diagnosis, reasoning, and the
exact command/diff — then **stops and shows the user**. Nothing is applied or
pushed. Only after the user **approves** is the fix sent back to **the user's
own coding agent** to apply (their tools, their connectors, their data).

Decided behavior:
- **Apply target:** through the attached agent (not Bluey running commands).
- **Review surface:** diagnosis + reasoning + exact command/diff.
- **Never** apply or push without explicit approval.

It is a **review-gated apply lane** on top of the existing drive path — not a
new system.

---

## 2. Why three layers of safety (not one switch)

Research (2026) on per-agent "propose-and-wait" modes: the capability mostly
exists but is **not uniform or reliable**:

| Agent | Native propose-only | Reliable? |
|---|---|---|
| Claude Code | `--permission-mode plan` (works with `-p`) | solid |
| Codex | `--sandbox read-only --ask-for-approval never` | solid if forced |
| Aider | `--dry-run` | solid |
| Gemini | `--approval-mode plan` | likely (piped behavior unconfirmed) |
| Cursor | omit `--force` = propose | workable — but its `--plan` flag is a **known bug that writes files**; never use `--plan` |
| Copilot | propose-by-default | `-p` *stalls* without `--allow-all-tools` |
| Windsurf | Plan mode | **no CLI** — not drivable |

Also: **none emit a clean machine-readable pre-apply patch** — the proposed diff
must be parsed out of their text/JSON output regardless.

So safety is **defense-in-depth**, enforced three ways, any one of which alone
prevents an un-approved apply:
1. **Native propose-only flag** per agent (where solid) — agent-native guardrail.
2. **Prompt engineering** — instruct "diagnosis + reasoning + exact diff/commands,
   apply NOTHING, do not commit, do not push" — the portable guardrail that works
   even where the flag doesn't (Cursor/Copilot).
3. **Bluey's apply gate** — Bluey only ever sends the *apply* invocation (the
   `--force` / write-sandbox / `--yes` variant) AFTER the user clicks Approve.

---

## 3. Data-driven, no hardcoding

Extend the existing `AgentEntry` registry table (no per-agent `if` branches).
Each row gains a **fix profile** expressed as pure data:

```rust
// registry.rs — new field on AgentEntry
pub struct FixProfile {
    /// Extra args that force PROPOSE-ONLY (read-only / plan). Empty = rely on
    /// prompt + (for agents like Cursor) simply omitting the apply args.
    pub propose_args: &'static [&'static str],
    /// Extra args that ALLOW APPLY (write). Only ever appended after approval.
    pub apply_args: &'static [&'static str],
    /// True if this agent can be driven to apply at all (Windsurf = false).
    pub apply_supported: bool,
}
```

Examples (data, not code):
- Claude:  propose `["--permission-mode","plan"]`, apply `["--permission-mode","acceptEdits"]`
- Codex:   propose `["--sandbox","read-only","--ask-for-approval","never"]`, apply `["--sandbox","workspace-write","--ask-for-approval","never"]`
- Cursor:  propose `[]` (omit --force), apply `["--force"]`  // NEVER `--plan`
- Aider:   propose `["--dry-run"]`, apply `["--yes-always"]`
- Gemini:  propose `["--approval-mode","plan"]`, apply `["--approval-mode","yolo"]`
- Copilot: propose `[]` (prompt-only), apply `["--allow-all-tools"]`
- Windsurf/VsCode: `apply_supported = false`

Adding an agent = one row. The drive layer reads the profile; it never names an
agent.

---

## 4. The flow (reusing the bridge)

```
1. User clicks Fix on an agent answer (or on a diagnosis).
2. Daemon drives the agent in PROPOSE mode:
     drive_command + fix_profile.propose_args
     + the FIX-PROPOSAL PROMPT (structured: diagnosis / reasoning / diff).
3. Parse the streamed output into a FixProposal { diagnosis, reasoning,
   commands_or_diff }.  Nothing applied.
4. Push a PROPOSAL card to the overlay: the three sections + Approve / Reject.
5. On Reject → discard. On Approve →
6. Daemon drives the agent in APPLY mode:
     drive_command + fix_profile.apply_args + an APPLY PROMPT that references
     the approved plan ("apply exactly this, do not push").
7. Stream the apply result as a normal answer/status card.
   Never `git push` — apply ends at a local change; pushing stays the user's act.
```

---

## 5. The prompt engineering (load-bearing)

**Propose prompt** (enforces propose-and-wait, portable across agents):
- Role: "Propose a fix. Do NOT edit files, run commands, commit, or push."
- Output contract: three labeled sections — `DIAGNOSIS`, `REASONING`,
  `FIX` (exact commands or a unified diff). Machine-parseable delimiters.
- Explicit: "Stop after proposing. Apply nothing."

**Apply prompt** (only sent post-approval):
- "Apply exactly the following approved fix: <plan>. Make only these changes.
  Do NOT push, do NOT open a PR, do NOT make unrelated edits."

Both live as data/templates; the propose/apply *args* come from the registry.

---

## 6. Security / production rules

1. **Apply only after explicit approval.** The apply invocation is unreachable
   until an Approve event for that specific proposal id arrives.
2. **Never push.** Neither prompt nor args ever include push/PR; apply ends
   locally. Pushing remains a human action.
3. **Proposal/approval are id-matched.** Approve carries the proposal id; a stale
   or mismatched id is rejected (no replay applying an old/edited plan).
4. **`apply_supported = false` agents** can propose (read-only) but the Approve
   button is disabled with a clear reason (e.g. Windsurf has no CLI).
5. **Never `--plan` on Cursor** (known bug applies files) — propose = omit apply
   args + prompt.
6. **Args as arrays, no shell** (inherits the drive layer's injection-safe spawn).
7. **Reuse consent/data-residency** — same "their agent, their data"; Bluey runs
   no AI, holds no keys, stores nothing.

---

## 7. Build slices

| Slice | Scope |
|---|---|
| F1 | `FixProfile` on the registry (all rows) + propose/apply arg plumbing in the drive layer (data-driven). Tests: each row's propose args never contain apply args; Cursor never carries `--plan`. |
| F2 | Fix-proposal + apply prompt templates + `FixProposal` parser (structured sections out of agent output). Tests: parse well-formed + malformed output. |
| F3 | Daemon: Fix request → propose-drive → proposal card; Approve/Reject IPC → apply-drive. id-matched gate. |
| F4 | Overlay UI: Fix button on agent cards, proposal card (diagnosis/reasoning/diff + Approve/Reject), apply-result card. |

---

## 8. Out of scope (lean)

- No auto-push / PR creation (ever, by design).
- No multi-step fix orchestration (one propose → one apply).
- No Bluey-side command execution (apply goes through the agent only).
- No editing the proposed diff in-UI before approve (approve-as-is or reject) —
  could come later; v1 keeps the gate simple.
