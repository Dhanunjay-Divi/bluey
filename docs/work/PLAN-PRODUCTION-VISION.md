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

## 2a. The product shape: an I/O sandwich with the spine in the middle

The two product flavors (interview copilot, meeting-assist) are **not
alternatives — they are the two ends of the same pipe.** Input on one side,
output on the other, the agent spine in the middle. Combine them and you have the
complete loop: *listen → understand → answer with the user's own agent → put the
answer where it's needed.*

```
   INPUT (B)                    SPINE                      OUTPUT (A)
   ─────────                    ─────                      ──────────
 system/call audio  ─┐                               ┌─►  invisible overlay  (interview)
 mic                 ─┤   cue-agent-bridge            ├─►  Slack DM / Teams    (meeting)
 typed question      ─┼─►  tap the user's agent  ────┼─►  back into call chat
 Slack slash cmd     ─┤   + its MCP → AnswerStream    ├─►  web dashboard
 meeting transcript  ─┘                               └─►  …
```

- **B (ingest)** — the system hears the call → live transcript → detect/extract
  the question + context.
- **Spine** — taps the user's seasoned agent + its MCP → `AnswerStream`. **Built
  and proven** (the agent matrix: all 6 agents 5/5; see [[STATUS-AGENT-CAPABILITIES]]).
- **A (output)** — the answer lands wherever's useful: invisibly in the overlay,
  in Slack/Teams, back into the meeting chat, the dashboard.

**Bluey already has the B side** — this is NOT greenfield. The daemon already has
the ingest machinery: system-audio capture (`build_system_audio_stt_provider`),
the STT pipeline (Deepgram / OpenAI Realtime / LocalWhisper via the factory),
live transcript segments, meeting records, and RAG indexing of what's said. So
Bluey already *listens and transcribes*. This session built the **spine** (middle)
and **one output** (the overlay — now with rich markdown + code cards).

**What's genuinely new to fully combine A+B:** more **output adapters**
(Slack/Teams) and the **question-detection trigger** (detect a question in the
live transcript → auto-answer, vs. the user tapping it). The spine in the middle
does NOT change — that's the entire payoff of decoupling it.

### The two products = same brain, different ends plugged in

| Product | B (input) | Spine | A (output) | Privacy |
|---|---|---|---|---|
| **Interview** | local system-audio (hears interviewer) | user's agent | invisible overlay | **fully local — nothing leaves** |
| **Meeting-assist** | call audio / Slack cmd | user's agent | Slack/Teams/visible | answer leaves to the tool |

The **interview case is the most private combination** (all-local in, invisible
out) — the "no data retention" principle at its strongest. It is also closest to
done (ingest exists + spine exists + invisible overlay exists).

### Two honest sub-decisions inside B (scope)

1. **System-audio capture vs. a meeting bot.** Bluey does **local system-audio
   today** (the user's machine hears the call) — perfect for interviews
   (invisible, no bot the interviewer sees). A bot that *joins* Zoom as a
   participant is a different, heavier thing (cloud infra, visible in-call,
   consent) — only needed for the "shared team assistant" flavor.
2. **Local-only vs. cloud output.** Answer → invisible overlay = stays on the
   machine. Answer → Slack = leaves to Slack. Be deliberate — output choice is the
   privacy moat.

**Status:** ~70% there — ingest exists, spine exists (proven this session), one
output exists (overlay). Remaining: output adapters + the question-detection
trigger. (This expands on Phase E below — "The meeting layer".)

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

### Phase E — The meeting layer: combine A+B over the spine (closes P9)
*The I/O sandwich (§2a). B already exists (audio→STT→transcript); the spine
exists (proven). This phase adds the **trigger** (B→spine) and **more outputs**
(spine→A). The spine itself does NOT change.*
- **B-side trigger — question-gate:** detect when the live transcript contains a
  real question worth asking the agent (vs. ambient chatter). Plus a rolling,
  bounded context-summarizer (also solves the arg-limit/cost risk). Wire into the
  existing audio/STT/daemon pipeline. Two modes: **auto-answer** (trigger fires →
  spine answers) vs. **user-tap** (the current overlay flow).
- **A-side outputs — adapters:** the answer is an `AnswerStream`; route it to more
  sinks beyond the overlay — **Slack DM / slash-command**, **Teams**, **back into
  the call chat**, the **web dashboard**. Each is a thin output adapter; none
  touch the spine.
- **Privacy gate per output:** overlay = stays local (interview, strongest
  privacy); Slack/Teams = answer leaves to the tool (meeting-assist). Make the
  local-vs-cloud output choice explicit and user-controlled.
- **DONE =** the full loop runs end-to-end for at least one product: a real call's
  audio → transcript → detected question → spine answers via the user's agent →
  answer lands in the chosen output. Interview loop (local audio → spine →
  invisible overlay) is closest to done and the natural first target.

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

## 7.6 BUGS FOUND IN LIVE UI RUN (2026-06-01) — fix later, not now

First real `bluey on` + visible-overlay run. The UI renders and every IPC round
trip works (daemon log confirms it received AgentListRequested,
AgentDetachRequested, AgentSessionsRequested{antigravity/claude_code/copilot}).
Bugs surfaced — logged here, NOT yet fixed:

- **B-UI-1 (perf):** agent picker sits on "Finding coding agents…" ~40s before
  populating. Discovery runs filesystem + version-probe synchronously per call.
  Fix: cache discovery / parallelize, show partial results, or a spinner with
  progress. Functional but bad UX.
- **B-UI-2 (silent consent gate):** clicking an agent fires
  `AgentSessionsRequested` but session history is consent-gated OFF by default,
  so the daemon returns an EMPTY list with no message. UI looks like "nothing
  happened." Fix: when `allow_agent_session_history` is off, show "Session
  history is off — enable it to see sessions" (or a consent prompt), not silence.
- **B-UI-3 (window drift):** the expanded panel drifted partially off the left
  screen edge during interaction. Fix: clamp the panel to visible screen bounds
  (the brief's `fitExpandedFrameToVisibleScreen` may not cover all cases).
- **B-UI-4 (no live-drive confirmation):** attaching Antigravity showed
  "ANTIGRAVITY · 8/8 tools" in the header (correct), but there's no end-to-end
  check that a *question* then actually routes through it in the running app
  (we proved drive separately, not through the live overlay→daemon→agent path).
- **B-UI-5 (session titles missing / wrong):** the Antigravity session picker
  lists ~100 rows all reading "Untitled session / 1779546050" — no real titles
  and an identical/wrong timestamp on every row. Antigravity protobuf is
  unreadable (known), so there's no title; the duplicated timestamp is a
  separate bug (mtime not read per-file, or a placeholder). Fix: real titles
  where possible, real per-session timestamps, and a meaningful label for
  unreadable-content stores ("Antigravity session · <date>") instead of a
  repeated "Untitled session".
- **B-UI-6 (long list breaks the UI):** ~100 session rows overflow the drawer —
  no scroll containment / virtualization, the list spills over the panel. Fix:
  scrollable, bounded, virtualized list (the design said top-40 + "load more";
  the overlay isn't enforcing it).
- **B-UI-7 (duplicate event spam):** selecting/hovering in the broken list fired
  `AgentConnectorsRequested{antigravity}` FOUR times in ~0.5s (daemon log,
  ts 1831328/1831500/1831668/1831834). A re-render loop or overlapping hit
  targets in the overflowing list. Fix: debounce + fix the list hit-testing.
- **B-ARCH-1 (dual code paths):** the CLI drives the daemon via
  `DaemonRequest::AgentList/AgentSessions/...` while the overlay drives it via
  `OverlayEvent::AgentListRequested/...` — TWO parallel handler paths for the
  same operations, which can (and did) diverge. Confirmed via instrumentation:
  CLI `bluey agent list/sessions` returns correct data through its path, but the
  new overlay-path logs (`sending SetAgents` etc.) didn't fire for CLI calls.
  Backend is CORRECT on both, but they should share one core to avoid drift +
  give unified logging. (Instrumentation now added to the overlay path;
  CLI path next.)
- **B-UI-9 (session-row click is dead — CONFIRMED via log):** clicking a session
  in the picker fires NO `agent_attach_requested` event — daemon log shows 0
  `AgentAttachRequested` across a full run where the user clicked sessions
  repeatedly. The Swift session-row click handler is not wired to emit attach
  (or the overflowing list's hit-targets swallow the click — see B-UI-6). This
  is why "click a session → goes blank, nothing happens." Backend is fine: the
  same run logged `SetAgentSessions count=40` and `SetAgentConnectors count=7`
  for Cursor — sessions ARE delivered; the click just never asks to attach.
  Pinpointed root cause of the user's "selected something, nothing happened."
- **B-UI-1 FIXED (perf):** root cause was NOT discovery (5ms) or version probes —
  it was the JSON-files reader (`json_files.rs`) reading + fully JSON-parsing
  multi-hundred-MB VS Code chat files (observed 141 MB) just to extract a title
  during `list`. VS Code listing alone took 24s. Fixed: skip titling files over
  2 MiB, and cap full-read at 25 MiB with an honest "too large to load here"
  turn. VS Code listing 24s → 0.9ms; full list now sub-second. Verified live.
- **B-UI-9 ROOT CAUSE FOUND (not a dead handler):** clicking a session row DOES
  fire — `quickAttachClicked` → `beginAttachFlow`, which opens the **connector
  confirmation sheet** and emits `AgentConnectorsRequested` (matches the log's
  4× connector requests). Attach only fires after confirming IN that sheet. So
  "click → nothing" = the **connector sheet isn't appearing/usable** (rendered
  off-screen or hidden under the overflowing list — see B-UI-6). The fix is the
  sheet's visibility/layout, NOT the click wiring. Needs the visible-overlay
  see-it-fix-it loop to fix AppKit layout reliably.
- **B-UI-8 (no action feedback — the big one):** selecting a session produced
  NO attach, NO ask, and NO visible feedback — the user cannot tell whether it
  worked, failed, or hung. Every agent action (select / attach / ask / drive /
  fix) MUST show a clear state: in-progress (spinner) → success / failure /
  empty, with a message. Silent no-ops are unacceptable. This is the #1 UX bug.
- Carry-overs still open from §4: P2 (full app run — now partially done), P3
  (UI visual QA — now started), P9 (meeting question-gate), P7 (cross-platform).

## 7.7 HYBRID FLOW PROOF (2026-06-01) — engine works; naming is the blocker

Ran a full real-machine hybrid proof. Results:
- **Select session → "what is this about" → grounded answer:** ✅ works (real
  Cursor session → accurate Claude summary).
- **Hybrid vs direct routing:** ✅ correct — Antigravity `Drive` (has gemini CLI)
  drives direct; Cursor `ReadOnly` (no cursor-agent) → "no CLI → would install:
  curl https://cursor.com/install" (proactive-install path).
- **Proactive install when a GUI agent lacks its CLI:** ✅ planned correctly.
- **MCP tools (direct):** ✅ fired, returned live data.

- **B-NAME-1 (THE usability blocker):** session "titles" are unhelpful, so a
  user can't tell what to select:
  - Claude: shows system/boilerplate ("This session is being continued…", "You
    are proposing a fix…") and even leaked *our own test prompts* as titles —
    the first-message-as-title grabs system/continuation text, not the topic.
  - Codex: shows "session rollout-" (filename prefix) — useless.
  - Cursor: untitled ones fall back to "session <id8>" — usable but not
    descriptive.
  Fix: title extraction must find the first *human/topic* line (skip system,
  tool, "continued from", and propose/apply-prompt boilerplate), truncate
  sensibly, and fall back to project+date, then short-id — NEVER bare "Untitled"
  or raw filename. This is the #1 thing standing between "engine works" and
  "a user can actually use the picker."

## 7.8 CLAUDE 3-ROW SPLIT + DRIVE-CONTEXT ARCHITECTURE (2026-06-01)

**Built + proven (the 3-row split).** The Claude desktop app embeds its own
Claude Code engine (bundled `claude.app` v2.1.156, newer than the standalone CLI
2.0.42) and keeps its own per-session index under
`~/Library/Application Support/Claude/{claude-code-sessions,local-agent-mode-sessions}/<acct>/<ws>/local_*.json`.
Each `local_*.json` is metadata only (rich `title`, `cwd`, `completedTurns`
count, and a `cliSessionId`); the transcript itself lives in the SHARED
`~/.claude/projects/<enc-cwd>/<cliSessionId>.jsonl` the CLI also uses.
- New `SessionFormat::ClaudeAppIndex` + reader (`sessions/claude_app.rs`): a
  **two-hop** read — parse the index for the title, follow `cliSessionId` into
  the shared JSONL (reusing `jsonl::read_transcript_file`).
- Three registry rows / kinds: `ClaudeCode` ("Claude Code (CLI)"),
  `ClaudeCodeApp` ("Claude Code (App)"), `ClaudeCodeAgent` ("Claude Code (Agent)").
  All drive via the same `claude` spec (`drive/cli.rs` maps all three →
  `KindTag::ClaudeCode`).
- **De-dup:** the CLI row excludes any session claimed by the app rows (by
  `cliSessionId`), so a conversation appears once — under the App row (richer
  title). Verified live: "Bluey repository setup" shows under `claude_code_app`,
  not `claude_code`.
- Status: detect ✅, read/titles ✅ (15 app sessions, real titles), attach ✅,
  de-dup ✅, tests green (156 bridge + 212 daemon), fmt+clippy clean.

**DRIVE-CONTEXT decision (the resume/"too long" problem).** Driving an attached
session via native `--resume` hit "Prompt is too long" on the 14MB/139-turn
"Bluey repository setup" session. Root cause (proven, sourced): native
resume/continue ALWAYS reloads the full transcript; the interactive app survives
huge sessions via **auto-compaction**, but an *already-over-limit* session can't
be compacted by anyone (Anthropic issues #26317, #25620) — it fails in the
user's own terminal too. This is NOT a Bluey bug and is RARE (only pathological
sessions; normal sessions resume fine).

**The escalation ladder (agent-agnostic — Claude/Cursor/Codex/Gemini):**
1. **Native resume first.** Run the agent's own continue/resume. Works for ~all
   sessions (the agent loads + auto-compacts itself). This IS the product: Bluey
   runs the agent command the user would. → answer, done.
2. **On a "too long"-class failure, fall back to a fresh session.** Start fresh
   in the project dir — which gives **code + CLAUDE.md + all MCP connectors for
   free** (context lives in the repo/config, NOT mostly in the chat history; a
   fresh session is not a blank slate). Pass the small recent thread as context.
3. **Deepest fallback (only if even fresh+context overflows): chunked compaction
   via the USER'S agent.** Bluey *mechanically* slices the transcript into
   window-sized chunks (no Bluey AI), feeds each chunk to the user's own agent
   asking IT to roll a running summary, then continues from that summary.

**Hard invariant:** Bluey's own AI NEVER summarizes/processes the user's data.
Summarizing is always done by the user's attached agent; Bluey only slices text
and orchestrates. Chunked compaction runs ONLY when the terminal genuinely can't
continue — never on the common path.

## 8. The honest one-liner

The **passive foundation is built and proven**. The gap to your full vision is:
**(1) make it proactive** (install CLIs — Phase A), **(2) actually run it**
(Phase B), **(3) verify across agents** (Phase C), **(4) build the meeting
question-gate** (Phase E), then **(5) cross-platform + commercial**. Phases
A→B→C→E get a real, secure, scalable product on macOS; F+H scale it out.
