# PLAN — Production Vision: the complete roadmap

> Status: MASTER PLAN. Branch: `agent/agent-bridge`. Date: 2026-06-01.
> The single source of truth for: the vision, every current gap, the solution
> for each, and what production-grade + secure + scalable across all platforms
> actually requires. Cross-refs: PLAN-AGENT-BRIDGE, PLAN-FIX-BUTTON,
> PLAN-ADAPTIVE-RESOLVER, PRODUCT-STRATEGY.

---

## 1. The vision (restated, plainly)

Bluey is a live meeting overlay that answers using **the user's own coding
agent**, so answers are grounded in their real work and **Bluey never holds
their data**. The product must be **proactive**, not passive: it sets up what
it needs rather than degrading when something is missing.

The core loop:
1. User attaches an agent (Claude Code, Cursor, Copilot, Gemini, Codex,
   Antigravity, Windsurf, …).
2. Bluey **ensures that agent is drivable** — installing its CLI if needed,
   syncing its sessions if the vendor supports it, else building a context
   bridge from readable history.
3. During a meeting, Bluey listens; when a real question surfaces it routes to
   the agent. The agent answers with **its own model + its own MCP connectors**.
4. A **Fix button** turns a diagnosis into a review-gated change: propose →
   show diagnosis/reasoning/diff → user approves → apply through the agent.
   Never silent, never pushes.
5. Everything runs locally; Bluey is a conduit. Self-healing when agents change
   their on-disk formats.

**Non-negotiables (from PRODUCT-STRATEGY):** consent-based; use only what the
user can see/attach/authorize; no hidden scraping, no stolen cookies/keys, no
bypass flows; every proactive card explainable.

---

## 2. The architecture (corrected — proactive, three tiers)

The earlier build was **passive discovery** (find what's installed, degrade if
not). The correct model is **proactive provisioning**:

```
attach(agent):
  capability = assess(agent)
  ┌─ Drive        → ready, use it
  ├─ Installable  → install its CLI (consent-gated) → verify → Drive   ← THE GAP
  ├─ ReadOnly     → readable history but no install path
  └─ CloudBlocked → nothing local

  then ensure context:
  ┌─ vendor has GUI→CLI session sync (agy ↔ desktop) → sync → readable sessions
  ├─ else GUI history readable on disk (Cursor SQLite) → context bridge
  └─ else → drive fresh
```

Three tiers in priority order:
1. **Install the CLI** → native drive with the agent's own MCPs (best).
2. **Native sync** → vendor pulls GUI history into the CLI (Antigravity `agy`).
3. **Context bridge** → we read history + feed it as context (the fallback).

The passive layer already built (discovery, readers, drive, bridge) is the
**foundation**; the proactive provisioning layer sits on top.

---

## 3. What EXISTS today (committed, real)

| Capability | State | Verified |
|---|---|---|
| Agent discovery (10 agents, GUI+CLI, generic fork detector) | ✅ | live |
| Session readers: JSONL, Cursor SQLite, VS Code JSON-files, protobuf stub | ✅ | live on real data |
| Connector inheritance (read MCP config, auth tiers, **no secret leak**) | ✅ | live + audit test |
| Drive a CLI agent (stream, resume, cost) | ✅ | Claude + Gemini live |
| GUI→CLI bridge (read Cursor session → answer via Claude) | ✅ | live |
| MCP fires headless, **scoped/safe** (writes blocked) | ✅ | live canary |
| Fix button: propose→approve→apply, id-gated, never pushes | ✅ (UI compiles) | gate verified |
| Self-healing adaptive resolver (heuristics + cache + validate) | ✅ | adapts to unseen format |
| Streaming reads (no whole-file load), secret audit | ✅ | live |
| Command map corrected to official docs (Cursor/Codex/Copilot/agy) | ✅ | doc-confirmed |
| CLI: `bluey agent list/attach/...` | ✅ | wired |
| Native overlay UI (picker, Fix card) | ⚠️ compiles | NOT run/seen |

~7,000 lines, ~270 tests. This is a real system — the **passive foundation**.

---

## 4. CURRENT PROBLEMS (honest, complete)

### P1 — Passive, not proactive (the architecture gap)
When a CLI is missing, Bluey degrades to read-only instead of **installing it**.
Most users have GUI apps, not CLIs — so the product fails its promise for the
majority. **This is the #1 gap.**

### P2 — Never run end-to-end
The daemon + overlay + IPC have never run together (`bluey on`). Each part
works alone; the integrated app is unproven. The package build never finished
(cargo/target setup issues, now resolved).

### P3 — UI never visually verified
`swift build` compiles, but no one has seen the picker, the Fix card, the
chips. UI bugs are invisible to `cargo test`.

### P4 — Half the drivable agents unverified
Only Claude + Gemini were driven live. Cursor/Copilot/Codex CLIs aren't
installed; their commands match docs but are unproven. Resolved by P1 (install
them) + a live run.

### P5 — Self-healing incomplete (no AI tier)
The heuristic + cache + validation tiers exist; the **AI fallback (A3)** — for
when heuristics fail on a truly novel format — is designed, not built.

### P6 — Antigravity GUI conversations unreadable
The `.pb` store is encrypted (Keychain-bound). Correctly NOT decoded (would
violate non-negotiables). Path forward = `agy` CLI + its sync (needs P1).

### P7 — No cross-platform reality
Windows discovery paths are written but **cfg-gated, never compiled/run on
Windows**. Linux untouched. The overlay is macOS-only. "All platforms" is
~30% real.

### P8 — Reliability/stress untested
Hung agents, 3GB DBs under load, oversized prompts (arg limits), concurrent
reads, daemon restart recovery — bounded in theory, not stress-tested.

### P9 — The meeting layer itself
The agent bridge is the *answer source*; the actual **meeting pipeline** (audio
→ VAD → STT → "is this a real question?" detection → route to agent) is partly
pre-existing but the **question-gate + context-summarizer** that decide *when*
and *what* to ask the agent are not built for the bridge.

### P10 — Cloud / multi-user / commercial (per PRODUCT-STRATEGY v0.4+)
Team mode (merge multiple participants' agents), cloud sync, billing,
post-meeting push-back to Jira/Notion — all future, none built.

---

## 5. SOLUTIONS — what to build, in order

### Phase A — Proactive provisioning (closes P1, the #1 gap)
- **`provision.rs`**: `InstallRecipe` per registry row (method + spec +
  verify-binary). New `Capability::Installable`. Recipes are vetted official
  sources only, never arbitrary strings. ✅ BUILT (plan_install/run_install,
  shell-injection guarded, verified).
- **Dynamic pre-flight + recovery (production-grade, env-resilient)** — real
  install found that messy real environments block installs (a broken symlink
  `/usr/local/bin/codex` → deleted brew cask blocked `npm install` with EEXIST).
  Production must handle the WHOLE CLASS of obstructions, not one case:
  ```
  provision(agent):
    1. pre-flight (read-only): prereq present? binary already resolves?
       bin-path occupied (working binary vs BROKEN SYMLINK vs stale)? dir writable?
    2. obstructions → remedy plan (each labeled safe/destructive), consent for
       destructive ones
    3. run recipe → 4. on failure diagnose (EEXIST/EACCES/ENOTFOUND/network) →
       map to remedy → apply if safe-or-consented → retry once
    5. verify on PATH → 6. honest outcome with real diagnosis
  ```
  A **data-driven diagnosis→remedy table** (new symptom = a row): already-installed
  → none; prereq missing → report; broken symlink → remove dangling link + retry
  (destructive, consent); EACCES → suggest sudo/user-prefix; network → retry-later;
  installed-not-on-PATH → "open new shell". Destructive remedies remove ONLY
  provably-stale things (a symlink whose target doesn't exist), never a working
  binary, never a blanket rm.
- **Session sync hook**: after install, if the vendor has GUI→CLI sync (`agy`
  import), trigger it; else fall through to the context bridge.
- Recipes (verified June 2026): claude→curl `https://claude.ai/install.sh`;
  cursor→curl `https://cursor.com/install`; gemini→npm `@google/gemini-cli`;
  codex→npm `@openai/codex`; copilot→npm `@github/copilot`; agy→curl
  `https://antigravity.google/cli/install.sh`.

### Phase B — Run it end-to-end (closes P2, P3)
- Finish `make package-darwin-arm64`; `bluey on`; drive the real flow:
  attach → ask → answer card → Fix → approve. Fix what breaks.
- Visual QA the overlay on a real display; iterate on the UI.

### Phase C — Verify the agent matrix (closes P4, P6)
- With Phase A installing them, drive Cursor/Copilot/Codex/agy live; confirm
  output parsing, resume, MCP, not-logged-in handling. Turn "doc-confirmed" →
  "verified". Confirm Antigravity `agy` sync surfaces GUI chats.

### Phase D — Complete self-healing (closes P5)
- **A3 AI fallback**: when heuristics fail, sample structure (never content/
  secrets) → ask the user's own agent "where are the messages?" → recipe →
  validate gate → cache. Extension point already marked in `adaptive.rs`.
- Apply the resolver as the fallback inside the session readers (when a pinned
  reader returns 0, try the resolver before giving up).

### Phase E — The meeting layer (closes P9)
- Question-gate (when is there a real question worth asking the agent?) +
  rolling context-summarizer (bounded — solves the arg-limit/cost risk too).
  Wire into the existing audio/STT/daemon pipeline.

### Phase F — Cross-platform (closes P7)
- Windows: real build + run; verify discovery paths; Windows overlay parity.
- Linux: build decision + audio/capture feasibility.
- Per ROADMAP, Windows is v0.3 — sequence accordingly.

### Phase G — Reliability hardening (closes P8)
- Stress: hung-agent timeout under load, multi-GB DB, oversized-prompt cap,
  concurrent reads, daemon-restart recovery. Add the arg-length cap (deterministic).

### Phase H — Commercial (closes P10, per PRODUCT-STRATEGY)
- Cloud auth/sync, team multi-agent merge, billing, post-meeting push-back.
  Explicitly v0.4+; out of scope until A–G are solid.

---

## 6. PRODUCTION-GRADE BAR (what "done" means per area)

| Area | Production bar |
|---|---|
| Security | no secret ever leaves; no unsafe writes; consent for every install/read; all verified by tests + canaries |
| Reliability | bounded memory/CPU; survives hung agents, huge stores, restarts; no panics |
| Scalability | data-driven registry (new agent = a row); self-healing on format drift; no per-version code |
| Cross-platform | builds + runs + verified on macOS, Windows; Linux decided |
| UX | no terminal dependency; every state has loading/empty/error; visually QA'd |
| Honesty | capability shown truthfully per agent (drive/installable/read-only/blocked); never fakes an answer |

---

## 7. SEQUENCING & EFFORT (honest)

| Phase | Closes | Rough effort | Needs |
|---|---|---|---|
| A — provisioning | P1 | 1 session | consent decision |
| B — run end-to-end | P2,P3 | 1 session | the build to run |
| C — agent matrix | P4,P6 | 0.5 session | A done (installs them) |
| D — AI self-heal | P5 | 1 session | code only |
| E — meeting layer | P9 | 1–2 sessions | code |
| F — cross-platform | P7 | 2–3 sessions | a Windows machine |
| G — reliability | P8 | 1 session | code |
| H — commercial | P10 | weeks | separate track |

**Critical path to "real product on macOS": A → B → C → E.** That's the loop a
real user lives. F (Windows) and H (cloud) are expansions after the core is
proven. D and G harden it.

---

## 7.5 REAL-WORLD PROOF (mandatory — no mocks, no simulations)

Every phase must be proven by **actually doing it on the real machine**, not by
unit tests or mocks. Unit tests stay (they catch regressions), but they are NOT
proof a capability works — the synthetic fixtures already lied to us once
(readers passed tests, failed on real data). Each capability has a **live proof**
that must be run and its real output shown:

| Capability | Real-world proof (must actually run, show real output) |
|---|---|
| Install a CLI | Actually run the install recipe for an agent whose CLI is missing → verify the binary appears on PATH → drive it. Real install, real binary. |
| Native sync | Actually trigger the vendor sync (`agy` import) → confirm a GUI conversation now appears in the CLI's readable store. |
| Context-bridge fallback | Actually read a real Cursor session → feed it to a CLI → get a grounded answer about THAT session. |
| MCP tool fires | Actually drive an agent to call a real MCP tool (perplexity/github/supabase) → show the real data it returned (not "TOOL_RAN" — the actual content). |
| "What is this conversation about?" | Actually read a real session → ask the agent to summarize it → show the real summary → eyeball that it matches the real chat. |
| Fix button | Actually click/drive Fix on a real diagnosis → show the real proposal → approve → confirm the real change applied (in a scratch dir). |
| End-to-end | Actually `bluey on` → attach → ask in the overlay → see the real answer card. |

**Rule:** a capability is not "done" until its live proof has been run and the
real output shown. "It compiles" and "tests pass" are necessary, not sufficient.
A proof harness (`examples/` throwaway or a `bluey agent prove` command) runs
these against the real machine on demand.

## 8. The honest one-liner

The **passive foundation is built and proven**. The gap to your full vision is:
**(1) make it proactive** (install CLIs — Phase A), **(2) actually run it**
(Phase B), **(3) verify across agents** (Phase C), **(4) build the meeting
question-gate** (Phase E), then **(5) cross-platform + commercial**. Phases
A→B→C→E get a real, secure, scalable product on macOS; F+H scale it out.
