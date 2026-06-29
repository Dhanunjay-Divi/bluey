# Bluey — The Whole Architecture (the blueprint)

> One map of the entire system: every part, what it does, and **when** in the
> app's lifecycle it runs. Built from `BLUEY-MASTER.md` + `STRATEGY-LOCAL-FIRST-
> AND-PRESTAGING.md` + the real code. This is the target we build against — it
> replaces reacting turn-by-turn.

---

## 0. What Bluey actually is (so every part serves ONE goal)

Bluey is a **live, on-device meeting copilot** that answers *your* questions
during a meeting using **your own coding agent** (Claude Code / Cursor / Codex /
Copilot / Gemini) — the agent that already knows your repos, tickets, PRs, and
tools via its MCP connectors. Everything runs **on your machine; nothing leaves.**

**Bluey is a conduit, not a brain:**
- Bluey supplies the **meeting** — transcript, attachments, the question, and a
  built **context package**.
- Your **agent** supplies the **knowledge** (repo/tickets/MCP) and **writes the
  answer**.

**What Bluey is NOT** (the traps): not a transcription/notes tool (Otter clone),
not a cloud service, not a generic-LLM chatbot, and it does **not** run a local
LLM to *write answers or summaries*. A local model is used **only to build the
package** (the ledger librarian) and optionally to emit an instant first line —
never to author the real answer.

**The moat:** uses YOUR agent's brain (grounded in your real work) + fully local,
zero-retention (the thing cloud competitors can't match) + invisible & instant.

---

## 1. The two local models (do NOT confuse them)

Bluey runs small on-device models in **two distinct roles**, plus it drives a
THIRD model it does not own (your agent's).

| # | Model | Role | Owns? | Writes the answer? |
|---|-------|------|-------|--------------------|
| 1 | **Parakeet (STT)** | speech → text, streaming, on-device | Bluey | no |
| 2 | **Small local LLM** (Qwen3-4B / Phi-3 via Ollama) | the **librarian**: reads transcript → builds the ledger/package; optionally the instant first line | Bluey | **no** (builds the packet) |
| 3 | **Your coding agent** (Claude Code, etc.) | the **answer** — grounded in your repo/tickets/MCP | the user | **YES** |

The whole product is: model 1 + model 2 prepare the packet; model 3 answers.

---

## 2. The system in PHASES (what runs WHEN)

The application has four lifecycle phases. Each part of the system belongs to a
phase. This is the spine of the architecture.

### PHASE A — BEFORE the meeting (pre-staging / warming)  ← "the spine"
Goal: the agent is **warm** before a word is spoken → first reply is instant, and
mangled references resolve because the agent already knows what's in play.

```
calendar invite (title, attendees, agenda)
linked Jira/Linear tickets (AUTH-12, JIRA-4821…)
recent PRs / branches in play
        │
        ▼
 PRE-STAGE BUILDER  →  assembles a stable "pre-meeting package"
        │
        ├─► pre-seed / warm the agent SESSION with it (so it's not cold-started)
        └─► becomes the STABLE PREFIX of every in-meeting prompt (provider caches it / CAG)
```
**Two wins from one design (the doc's "two birds"):** built for **latency**
(warm session = instant answer) — but *also* fixes the hardest **accuracy** cases
free (agent knows "AUTH-12 is in play" → resolves STT's "off twelve").

### PHASE B — DURING the meeting (listen + understand, continuously)
Goal: turn the room into (a) a live transcript and (b) a maintained understanding
of *what's been decided*, cheaply, on-device, in real time.

```
mic + system audio  ──[CONTINUOUS PCM STREAM, no files]──►  Parakeet (streaming STT)
        │                                                        │
        │                                                   speaker-labeled
        │                                                   transcript (You / They)
        ▼                                                        ▼
   (every few turns)  LOCAL LLM "librarian"  reads the rolling transcript
        │                                                        │
        ▼                                                        ▼
   DECISIONS LEDGER  ◄───────────────────── extract decisions / constraints /
   (pinned, never dropped:                  owners / open-questions
    "we decided Postgres", "Alex owns runbook by Fri")
```
This phase runs the whole meeting. The ledger is the running memory that survives
the last-N transcript window.

### PHASE C — AT QUESTION TIME (trigger → assemble → drive → answer)
Goal: when a question is for *you*, build the right packet and let your agent
answer it — grounded, instant, private.

```
TRIGGER: a final transcript line that is (a) from someone else, (b) question-shaped,
         (c) mentions MY name  →  "this is for me"     (suggest by default; auto opt-in)
        │
        ▼
ASSEMBLE_CONTEXT(question)  — the projection, never a blind dump:
   [structured facts]   = PHASE-A pre-staged package   (stable → cached prefix)
   [decisions ledger]   = PHASE-B local-LLM ledger     (always sent)
   [recent transcript]  = last N tokens, speaker-labeled
   [retrieved slice]    = only on a miss (semantic search full transcript)
   + the question
        │
        ▼
DRIVE YOUR AGENT  (its model + its MCP + the warm session from Phase A)
   — Bluey NEVER fetches the repo; the agent does, via its own MCP.
        │
        ▼
ANSWER streams back  →  rendered in the invisible overlay, to you alone
   ├─ FAST layer (optional, <100ms): instant first line from LOCAL context (ledger/
   │   pre-stage), framed as MEETING-truth ("From the meeting: plan was Friday")
   └─ DEEP layer: the agent's grounded answer streams in below it
        │
        ▼
SESSION CHAINING: the agent remembers turn 1 → follow-ups are cheap
```

### PHASE D — AFTER / ACROSS meetings (the compounding layer)
Goal: history + memory that competitors can't export (the switching-cost moat).
Weak for the first ~10 meetings, strong after ~50 — so this is the RETENTION
layer, not the day-one value.
- Meeting history (transcript + Q&A + ledger), all local.
- Cross-meeting recall (the agent + the local store).
- *Day-one value is Phases A–C; Phase D compounds on top.*

---

## 3. The components (what each part IS, by crate)

| Component | Crate / file | Phase | Job |
|-----------|--------------|-------|-----|
| **Audio capture** | `cue-daemon/audio/` + `native/macos/cue-audio` (Swift) | B | Continuous PCM from mic + system audio (ScreenCaptureKit). **TARGET: stream, not chunk-files.** |
| **STT (Parakeet)** | `cue-daemon/stt/parakeet.rs` (parakeet-rs/ONNX) | B | Streaming speech→text, speaker-labeled, on-device. |
| **Speaker → trigger** | `cue-core/intelligence.rs` `detect_for_me_question` | C | "is this question for me?" (name + question-shape; mic=You, system=They). |
| **Ledger librarian** | `cue-core/intelligence.rs` (today: keyword; TARGET: local LLM) | B | Extract decisions/constraints/owners → pinned ledger. |
| **Local LLM** | `cue-llm/ollama.rs` (exists) | B/C | Runs the librarian; optionally the instant fast-line. |
| **Pre-stage builder** | `cue-core/prestage.rs` (stub) | A | Build the warm package from calendar/Jira/PR; warm the agent session. |
| **Context assembler** | `cue-daemon/app.rs` `answer_context_from_meeting` / `_for_question` | C | The projection: facts + ledger + recent + retrieved-on-miss. |
| **Agent driver (spine)** | `cue-agent-bridge` | A/C | Drive the user's agent (CLI or ACP), MCP, session chaining, resume. |
| **Overlay UI** | `cue-meeting-overlay` (Tauri + React) | C | Invisible (screen-share-excluded) surface: ask, answer feed, pill, controls. |
| **Daemon** | `cue-daemon` | all | Orchestrates everything; IPC to overlay + CLI. |

---

## 4. Build status — what's REAL vs STUB vs TARGET (honest)

| Part | Status | Note |
|------|--------|------|
| Agent driving + MCP + session chaining (spine) | ✅ real, proven | 5/5 agents; ACP + CLI fallback |
| Streaming markdown answers + tool/reasoning feed | ✅ real | |
| Speaker labels (You/They) | ✅ real | mic=You, system=They |
| Question trigger (name + shape) | ✅ real | + substance guard |
| Parakeet STT on-device | ✅ real inference | **but chunk-file capture (NOT streaming) — the latency disease** |
| Audio capture | 🔴 **chunk-file** | record 1s WAV → transcribe → delete. **Rewrite to continuous stream.** |
| Decisions ledger | 🟡 keyword stub | **upgrade to local-LLM librarian** |
| Pre-stage builder | 🟡 stub (`prestage.rs`) | shape only; **no calendar/Jira/PR fetch, no session-warming** |
| Local LLM (Ollama) | 🟡 path exists | **not wired to ledger/fast-line** |
| Overlay (invisible, pill, + menu, app-picker) | 🟡 built this session | works; uncommitted; some churn |
| Cross-meeting memory (Phase D) | 🔴 minimal | the compounding layer, post-MVP |

---

## 5. The architectural correction (why "not real-time")

The audio path is **chunk-file based**: spawn ffmpeg → record a 1-second `.wav` →
read it → transcribe → delete → repeat. This **cannot be real-time** — you wait a
full second to fill the file before anything starts; you are structurally ≥1s
behind, always. Pipelining (overlap capture with transcribe) got it from ~2.3×→
~1.4× real-time but optimizes the wrong architecture.

**The real-time design:** a **continuous PCM stream** (the native helper already
does this for system audio via `--continuous`) feeding Parakeet **incrementally
as samples arrive** — which is what the streaming model wants. No files, no
per-chunk process spawn, no fragment-accumulator, no dual drain budgets. **Most
of the chunk-path complexity DELETES.** This is *less* code, and it's the only
true real-time path.

---

## 6. The through-line (the test for every part)

Does it compound **"your own agent, your own context, fully local, instant,
invisible"**? If yes, build it. If it's generic transcription, a local summarizer
as the *output*, cloud STT, or a bot in the call — it's a trap that commoditizes
the one real advantage. **Day-one value = Phases A–C (the core loop). Phase D
compounds.**
