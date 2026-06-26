# Bluey — Master Doc (read this to understand everything)

> One doc. The whole product: what it is, how it works, what's built, what's
> left, the architecture, and every key decision. If you read only one file,
> read this.

---

## 1. What Bluey is (one paragraph)

Bluey is a **live meeting copilot** that answers your questions during a meeting
using **your own coding agent** (Claude Code, Cursor, Codex, Copilot, Gemini) —
the agent that already knows your repos, tickets, PRs, and tools through its MCP
connectors. Everything runs **on your machine; nothing leaves**. Bluey is a
*conduit*: it supplies the **meeting** (transcript, attachments, the question);
the agent supplies the **knowledge** (repo, tickets, MCP) and the answer.

**The problem it kills:** in meetings your real work context vanishes — you tab
through Jira/GitHub/terminal to answer. Your agent already has all of it; it's
just not in the room. Bluey brings it in.

**USP, one line:** *the live wire between what's said in a meeting and the agent
that already knows your work — grounded answers, instantly, privately.*

---

## 2. Why it wins (the moat)

1. **Uses YOUR agent's brain, not a generic model** — answers grounded in your
   real work; no rebuilding integrations to every tool.
2. **Fully local, zero retention** — audio, voiceprints, repo never leave the
   machine. The one thing cloud competitors (Otter, Fireflies, Recall) can't
   match without abandoning their model.
3. **Invisible + instant** — surfaces the answer to you alone, in real time.

**Switching cost = the compounding moat.** Months of meeting history + your
agent's codebase context is *non-exportable* — a competitor can't receive it
because they don't have your agent. Usage alone deepens the moat.

**Funding wedge:** regulated/enterprise buyers (finance, legal, healthcare, gov)
*cannot* use cloud meeting tools that exfiltrate audio. On-device zero-retention
is the only thing that clears their security review. Margins are clean — compute
runs on the user's hardware (near-zero marginal cost per meeting).

**Cold-start truth (important):** the relationship/memory layer is weak for the
first ~10 meetings, strong after ~50. So **day-one value MUST be the core loop**
("your agent answers your standup question, grounded in your real codebase,
privately, instantly"), good enough to pay for with zero history. Everything
else is the compounding layer that retains.

---

## 3. The core loop (how it works)

```
listen (local audio) → transcribe (Parakeet, speaker-labeled)
  → a question surfaces (you tap it, OR it's detected)
    → assemble context package (recent transcript + facts/ledger)   ← "what to send"
      → drive YOUR agent with that package (its model + MCP + session)
        → grounded answer streams back (with live tool/reasoning feed)
          → it remembers (session chaining) for follow-ups
```

Local + invisible: no bot in the call, no cloud, no data retention.

---

## 4. Local-first + why local STT is "good enough"

**Local-first is the product, not a constraint.** Hosting STT in the cloud
collapses the USP into "trust our cloud" (same as everyone, worse-funded).

**The accuracy objection dissolves:** the agent is a **context-aware error
corrector, not a transcriptionist**. If Whisper/Parakeet hears "auth changer"
for "auth change", the agent already knows there's an open `fix-auth-flow` PR and
resolves intent — like a phone autocorrecting "restuarnt" → "restaurant". So
local STT is good enough.

**Pre-staging is the spine (one design, two wins):** before the meeting, Bluey
pre-loads the calendar invite, linked Jira tickets, agenda, recent PRs. This
gives (a) **speed** — answers instant, not cold-started; and (b) **accuracy** —
even mangled "AUTH-12" resolves because the agent already knows AUTH-12 is in
play. Built for latency, it *also* fixes the hardest accuracy cases.

**Latency — instant *real* content, then the deep answer (fork, not filler):**
the agent's grounded answer takes seconds; you need words in ~300ms. So **one
card, two writes**: a FAST line from local context (ledger / pre-staged ticket /
recent transcript) shows in <100ms, then the agent's grounded answer streams in
*below* it. The fast line is never filler — it states **meeting-truth** ("*From
the meeting: plan was Friday*"), which Bluey is the authority on; the agent adds
**repo-truth** — two layers, both true, no contradiction. Also: order the package
**stable-first / volatile-last** so the agent's provider caches the prefix
(prompt-caching / CAG) across follow-ups. *Researched memory/RAG stacks (Mem0,
Zep, GraphRAG, LangGraph) are NOT adopted — Bluey deliberately doesn't own the
memory; the agent owns work-data via MCP, the transcript store owns meeting-data.
Only the caching/retrieval-shape ideas help.* Full design + the per-technology
verdict: `docs/STRATEGY-LOCAL-FIRST-AND-PRESTAGING.md` → "LATENCY".

---

## 5. Context assembly — "what to send with the question"

The single highest-leverage piece. **Store everything; send a built slice (a
"projection") at question-time** — never a blind dump.

- **Transcript** (what was said) — ONLY Bluey has it → Bluey stores + sends it.
- **Work data** (repo, PRs, tickets) — the AGENT fetches it itself via MCP →
  Bluey NEVER stores or RAGs the repo.

**The assembler (`assemble_context(question)`):**
```
[structured facts]   participants, project, title, pre-staged tickets
[decisions ledger]   pinned constraints/decisions/owners   ← NEVER dropped
[recent transcript]  last N tokens, speaker-labeled         ← recency buffer
[retrieved slice]    only if the question references something not in
                     buffer+ledger → semantic search the full transcript
+ the question  →  hand to the agent; it fetches its own repo/ticket detail.
```

**The decisions ledger (fixes "what if last-N missed it"):** a small local-AI
pass every few turns extracts decisions/constraints/owners into a **pinned
block always sent regardless of N**, so a minute-5 constraint still reaches the
agent at minute 40. The local AI's job is to *build the package*, not to answer.

**Safety nets:** retrieval-on-miss; the agent self-checks ambiguous transcript
against the real repo; "say what's missing rather than guess" in the prompt.
This is `answer_context_from_meeting` / `answer_context_for_question` — already
wired into the overlay ask path (transcript + Q&A + artifacts + summary).

---

## 6. "Who is speaking" → the question trigger (connected feature)

Knowing *who* speaks isn't for the transcript — it's the **trigger**:

1. Transcribe (speaker-labeled via Parakeet/Sortformer).
2. A line mentions **MY name** + is **question-shaped** → high-precision "this is
   for me" signal (no biometrics, no heavy ML — just name + shape).
3. → local AI assembles the context package (§5) → hand to MY agent → answer
   (suggest by default; auto in an opt-in mode).

This unifies three roadmap items into ONE: **speaker name + question detection +
local-AI package build.**

**Engineering split:**
- **"Me vs them"** (one voiceprint of you, enrolled once) — easy + reliable;
  enables auto-reply. The cheap demoable win.
- **"Which specific person"** (full identity registry) — hard; the deeper moat
  (routing, commitments, cross-meeting memory). Build later.
- Diarization runs **on-demand only** (enroll/resolve a new voice; recognized
  voices skip it).

---

## 7. The STT stack (decided + built)

**Adopt `parakeet-rs`** (crates.io, MIT/Apache, ONNX Runtime) — NOT build our own,
NOT a full voice-agent platform (VAPI/EchoKit bundle TTS/telephony/dialogue we
don't use; Bluey listens and hands text to the agent).

- English Nemotron model (beats Whisper on English, ~27× faster on CPU),
  CPU-capable, on-demand Sortformer diarization built in.
- **Per-target backend:** native ONNX prebuilts on Apple Silicon / Windows /
  Linux; `load-dynamic` fallback on Intel Mac (no prebuilt). Verified building
  native arm64.
- **Packaging:** the `ort` native lib bundles INTO the binary; the ~600MB model
  weights are **fetched on first run + cached** (atomic, resumable), with an
  **offline-bundle build** for air-gapped enterprise. `BLUEY_PARAKEET_MODEL_URL`
  points at an internal mirror. Nothing hardcoded — model dir defaults under the
  app data dir (`AppPaths::discover`), env-overridable.

A voice-agent pipeline's extra bits (VAD, endpointing, TTS, interruption) are
mostly irrelevant: Bluey doesn't speak. We need STT + diarization + a small VAD/
endpointing helper. The "what to send" brain is OURS (§5), not in any pipeline.

---

## 8. What's BUILT (verified this work) — branch `agent/parakeet-stt`

- Drive your own agent (6 agents) + MCP + sessions.
- Streaming markdown answers + live tool/reasoning **status feed**.
- **Conversation memory** (session chaining — turn 2 remembers turn 1; verified
  live). Conversation **feed** UI.
- Meeting **context injection** into the agent (transcript/artifacts/Q&A) —
  already wired on the overlay ask path.
- **Local STT (Parakeet):** provider (STT + diarization), factory wiring,
  **first-run model auto-download**. Builds + clippy-clean. (commits 7411aff →
  0590570).
- Honest failure recovery; crash fixes; mic-state UI; `+` context menu.
- Model **selection plumbing exists** (`DriveOptions.model_override`; overlay
  AskRequested already carries `provider`/`model`/`mode`) — needs a UI.

---

## 9. What's LEFT for a working MVP (build order; Windows deferred)

1. **Mic capture → Parakeet** — unify the mic path onto the `SttProvider` trait
   (today the mic posts WAV to REST and bypasses the trait; only continuous
   system-audio uses the factory). **Last link to live transcription.**
2. **Speaker name on transcribe** — "me vs them" + label (uses Parakeet's
   diarization).
3. **Question detection → trigger** — name-mention + question-shape (§6).
4. **Decisions ledger** — local-model pass that maintains the pinned facts (§5).
5. **Pre-meeting context builder** — warm the package with calendar/Jira/PR (§4).
6. **Model / speed selection UI** — expose the existing override plumbing.
7. **Mac package builder** — installable bundle (ships `ort` + model
   auto-download), verified on a fresh machine.
8. **Windows** (DEFERRED) — overlay invisibility via
   `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` + build/test. Native code,
   can't verify from a Mac. ~1–2 days, after the Mac MVP works.

**Not needed for MVP (defer):** permission UI (agent writing files mid-call),
ACP held-session (CLI chaining already works), full per-person speaker identity.

**Critical path to YOUR first end-to-end test:** #1 + a live run. **To
first real-user test:** add #7 (installer) + fresh-machine model-download +
permission-flow check + native Apple Silicon (not Rosetta) + one Windows pass
once #8 lands. The hard architectural work is done; this is wiring + packaging.

---

## 10. Build discipline (how to do #1–#7 safely)

These mostly touch the SAME core files (`crates/cue-daemon/src/app.rs`, the audio
+ answer pipeline), so **do them SEQUENTIALLY, build-checking + committing each**
— NOT parallel agents on the same files (they collide into broken diffs). Only
genuinely independent pieces (model-selection UI, package-builder config, the
ledger as a self-contained module, Windows design) are safe to fan out.

Verify on **native arm64** (`arch -arm64 cargo ... --target aarch64-apple-darwin`)
— a Rosetta/x86_64 shell builds emulated and hides the real prebuilt path.

---

## 11. Repo state (where things live)

- **`meeting-main`** (on origin) — source-of-truth for the meeting product.
- **`agent/meeting-frontend`** (on origin) — the meeting work history.
- **`agent/parakeet-stt`** (on origin) — branched from meeting work; the STT
  integration (this build). Pushed/backed up.
- **`main`** (origin) — frozen at the initial commit; the real work was never
  merged into it. Decide later whether `meeting-main` becomes `main`.

Key code: agent driving = `crates/cue-agent-bridge`; daemon/answer/audio =
`crates/cue-daemon/src/app.rs`; STT = `crates/cue-daemon/src/stt/`; overlay UI =
`crates/cue-meeting-overlay/ui`. Strategy detail = `docs/STRATEGY-LOCAL-FIRST-
AND-PRESTAGING.md`; STT decision = `docs/DECISION-VOICE-STT-STACK.md`.

---

## 12. The through-line (the test for every feature)

Does it compound **"your own agent, your own context, fully local"**? If yes,
build it. If it's generic transcription, cloud STT, or a bot in the call, it's a
trap that commoditizes the one real advantage. Day-one value = the core loop;
everything else compounds on top.
