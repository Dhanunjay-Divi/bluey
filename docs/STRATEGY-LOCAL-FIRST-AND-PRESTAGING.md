# Strategy — Local-First, and Why Pre-Staging Is the Whole Product

> Decision note for product + fundraising. Captures the architectural bet that
> on-device wins, why local STT is "good enough," and why pre-staging context
> is the spine of the product — not a feature.

---

## TL;DR

Bluey runs **fully on-device** — STT, diarization, speaker voiceprints, and the
agent orchestration all on the user's machine. Nothing leaves. This is not a
constraint; it is the product, the moat, and the fundable story.

The usual objection — "local STT is less accurate than cloud" — **does not
apply to Bluey**, because the answering agent is a *context-aware error
corrector*, not a transcriptionist. And the **pre-staging** design we build for
latency *also* solves the hardest accuracy cases as a side effect. One
architecture, two problems solved.

---

## 0. What Bluey is (the whole product)

**Bluey is a live meeting copilot that answers your questions during a meeting
using your own coding agent — fully on your machine, nothing leaves.**

**The problem.** In meetings, your real work context disappears. Someone asks
"can we ship the auth change?" and you're tabbing through Jira, GitHub, and your
terminal to answer. Your coding agent (Claude Code, Cursor, Codex, Copilot,
Gemini) *already* knows your repos, tickets, PRs, and tools through its MCP
connectors — it's just not in the room. Bluey brings it in.

**How it works (the core loop):**
1. **Attach your agent** — Bluey drives the seasoned coding agent you already
   use (6 supported), with its own model, MCP connectors, and sessions.
2. **A question surfaces** — you type it, or it's detected from the live meeting.
3. **Your agent answers** — grounded in your real repo + tickets via its own
   tools, streamed back with a live status feed (you see it reading files,
   querying Jira).
4. **It remembers** — follow-ups continue the same conversation thread.
5. **Local + invisible** — runs on your machine, no bot in the call, no cloud,
   no data retention; the overlay is screen-share invisible.

**Bluey is a conduit, never a holder of your data.** It does not answer with a
generic cloud model and does not keep your audio, repo, or context. The agent
supplies the *knowledge* (repo, tickets, MCP); Bluey supplies the *meeting* (the
question, transcript, attachments) and the orchestration.

**The USP, in one line:** *the live wire between what's said in a meeting and the
agent that already knows your work — grounded answers, instantly, privately.*

Three things make it defensible:
- **It uses YOUR agent's brain, not a generic one** — answers are grounded in
  your actual work; no rebuilding integrations to every tool.
- **Fully local, zero retention** — the one thing cloud competitors (Otter,
  Fireflies, Recall-based tools) structurally cannot match.
- **Invisible + instant** — surfaces the answer to you alone, in real time.

**Where it's going.** From a passive recorder to an *active participant*:
answer-while-asked, act through the agent (draft the follow-up, update the doc),
cross-meeting memory, and speaker-aware behaviors (see Future Features). Every
step compounds "your own agent, your own context, fully local."

The rest of this doc explains *why on-device is the right bet* and *why
pre-staging is the spine that makes it work*.

---

## 1. Local-first is the business decision

Bluey's reason to exist is: *answer meeting questions using your own agent +
your own context, and nothing leaves your machine.* The product **is** the
privacy promise.

- **The moat.** "We never see your audio, your voiceprints, your repo" is a
  category-of-one claim for an AI meeting tool. Cloud competitors (Otter,
  Fireflies, Recall-based tools) structurally cannot match it without abandoning
  their own model. Hosting frontier STT ourselves collapses Bluey into "trust
  our cloud" — same as everyone else, on their turf, against better funding.
- **Liability → selling point.** Diarization and voiceprints are the most
  sensitive data possible (biometrics; active BIPA suits vs Teams/Otter).
  Keeping them on-device turns our biggest legal exposure into a headline:
  *your voiceprint never leaves your laptop.*
- **Margins.** Hosting STT is a GPU cost center that scales with minutes of
  audio. Local pushes that compute onto the user's hardware → near-zero marginal
  cost per meeting → a clean SaaS story (charge for the app/orchestration, not
  for renting GPUs).
- **The fundable wedge.** Regulated/enterprise buyers (finance, legal,
  healthcare, gov) *cannot* use cloud tools that exfiltrate meeting audio.
  On-device, zero-retention is the **only** thing that clears their security
  review — a wedge no cloud competitor can enter. VCs fund defensible +
  compliant; "on-device privacy AI" is thesis-aligned, "we host STT too" is a
  thinner-wrapper pitch.

**The hardware caught up.** Apple Silicon + Windows NPUs + 16GB-standard laptops
in 2026 run local STT + on-demand diarization + embedding-match comfortably.
Going local now is timing the wave, not fighting it.

---

## 2. Why local STT accuracy is a non-issue

**The agent is an error corrector, not a transcriptionist.** It reads the
transcript with the repo, tickets, PRs, and MCP tools already loaded. So when
Whisper hears *"can we ship the auth changer"* instead of *"...the auth
change,"* the agent already knows there's an open PR `fix-auth-flow` and
resolves the intent automatically — the same way a phone autocorrects
"restuarnt" → "restaurant": not by knowing the word was wrong, but because
context makes the right answer obvious.

**The error classes that actually matter are narrow:**

| Survives fine (context recovers it) | Genuinely risky (no prior context) |
|---|---|
| Common technical words | Brand-new names introduced first time in the meeting |
| Concepts already in the repo | Specific ticket numbers spoken aloud |
| Ticket topics the agent knows | Company-unique acronyms the agent hasn't seen |
| Names of systems/services it has context on | |

So the accuracy question **flips from a weakness into an architecture argument**:
we don't need perfect STT because the agent's context window does the
disambiguation. Local Whisper / Parakeet is genuinely good enough.

---

## 3. Pre-staging is the whole product

Before the meeting starts, Bluey pulls the **calendar invite, linked Jira
tickets, the agenda, recent PRs** — so the agent already knows *"today's meeting
is about JIRA-4821, AUTH-12, and the onboarding flow"* before a single word is
spoken.

This was going to be built **for latency** anyway (warm the agent's context so
answers are instant, not cold-started mid-call). But it *also* solves the
**risky accuracy cases** above as a side effect: even if Whisper mangles
"AUTH-12" into "auth twelve" or "off twelve," the agent knows what's in play and
resolves it. The known-set disambiguates the mangled audio.

**Two birds, one stone.** The latency design and the accuracy design are the
same design. Pre-staging isn't a feature — it's the spine that makes local-first
viable and the product coherent.

---

## 4. The speaker layer fits the same model (future feature)

The planned *enroll-once / recognize-forever* speaker feature is local-native:
run diarization only on demand (new/unrecognized voice), store a voice
**embedding** on-device, and match future utterances instantly with no
diarization. Combined with the calendar roster, voices get real names. Diarize
rarely, match cheaply — fits a laptop, fits the privacy promise. (Biometric/BIPA
handling is being researched separately; flagged here only as a planning note.)

---

## 5. Where a server *could* live later (without breaking the promise)

An **optional, opt-in** team layer that syncs *encrypted artifacts/voiceprints*
across a company's own devices — never the audio, never required. A future
enterprise upsell, not the core. The core stays local forever.

---

## Bottom line

Local for the core, forever. The cloud-STT option is a trap: it trades away the
one defensible advantage (privacy/zero-retention), the enterprise wedge, and the
margins, in exchange for accuracy the agent's context already recovers.
Pre-staging makes local-first not just viable but *better* — and it's the same
work we'd do for latency. That coherence — where each piece reinforces the
others — is the product thesis and the fundraising narrative.

---

## FUTURE FEATURES — "who is speaking" as an enabler (not for the transcript)

> Planning note. The diarization/voiceprint work is NOT needed to transcribe a
> meeting (the agent needs the *words* of a question, not who spoke). Its real
> value is as an **enabler for new agent behaviors.** Captured here so it's not
> re-derived; build only when the core loop is solid.

### What knowing "who" unlocks
1. **Auto-reply / auto-answer.** When *someone else* asks a question Bluey can
   answer from your repo, it drafts the answer for you to say (or posts it in
   async/text meetings). Triggers only on *others'* questions, not your own —
   so it needs the "me vs them" distinction.
2. **Routing by speaker.** Different people get different treatment (manager →
   status + risk framing; junior → how-to/docs; client → customer-facing answer,
   internal notes hidden). Pre-load context for *that person's* likely questions
   when they're in the meeting.
3. **Per-person accountability.** "Sarah said she'd own the API spec" → tracked,
   and surfaced to you before the next meeting. Commitments attached to people,
   across meetings.
4. **Speaker-aware cross-meeting memory.** "3rd time Tom raised the scope
   concern" — only possible if Bluey knows it's *Tom* each time. The
   persistent-voiceprint payoff.
5. **Talk-time / dynamics signals.** "You've spoken 80% of this meeting" / "the
   client hasn't spoken in 10 min" — coaching nudges only the operator sees.

### The engineering split that matters
- **"Me vs them" — EASY + reliable.** ONE voiceprint of *you*, enrolled once;
  everyone else is "someone else." No N-speaker clustering, no over-splitting
  (the thing that broke all session disappears). Enables #1 (auto-reply) and #5
  (dynamics). This is the cheap, demoable, novel win.
- **"Which specific person" — HARD.** Needs the full multi-speaker identity
  registry (over-splitting + drift live here). Enables #2 (routing), #3
  (commitments), #4 (cross-meeting memory). The deep moat — build toward it,
  don't let it block the first wow.

### Strategic order
Ship **"me vs them" → auto-reply** first (cheap, reliable, the most demoable
novel feature). Treat the full per-person identity registry as the moat built
later, gated by the on-device/consent privacy model. Diarization stays
**on-demand only** (run to enroll/resolve a new or unrecognized voice; recognized
voices skip it). Biometric/BIPA handling is researched separately — flagged here
only as a planning constraint, not analyzed.

### The through-line
Every one of these must compound *"your own agent, your own context, fully
local."* "Who is speaking" is only worth building where it makes the agent's
answer or action smarter — never as a generic meeting-assistant feature an
incumbent could also ship.

---

## ARCHITECTURE — context assembly ("what to send with the question")

> The single highest-leverage piece of the core loop. When a question fires,
> what context does the agent receive? Getting this right is the difference
> between a toy and a copilot that doesn't confidently mislead you.

### The projection model (how production systems do it)
You **store everything** but **send a purpose-built slice** ("projection")
assembled at question-time — never a blind dump. Three memory layers, each
retrieved differently:

- **Recency** — recent conversation, grabbed by *time* (rolling buffer, NO AI).
- **Pinned facts** — durable decisions/constraints, *promoted* so they never drop.
- **Relevance** — older/external material, grabbed by *semantic retrieval*.

Key correction to the naive view: **AI does not assemble the recent
conversation** (that's a mechanical last-N buffer). AI enters only to (a)
*summarize* on overflow and (b) power *embedding retrieval* — which is retrieval,
not generation.

### Bluey's simplification (the USP advantage)
Bluey drives the user's OWN agent, which does its own retrieval. So:

- **Transcript** (what was said in the room) — ONLY Bluey has it → Bluey stores +
  sends it.
- **Work data** (repo, PRs, tickets) — the AGENT fetches it itself via MCP →
  Bluey NEVER stores or RAGs the repo. (Doing so would duplicate the agent, break
  "Bluey holds no data," and add infra we don't need.)

So Bluey's context job is narrow: provide the **meeting**; the agent provides the
**work knowledge**.

### The assembler — `assemble_context(question)`
Fires on a question trigger, builds:

```
PROMPT CONTEXT =
  [structured facts]   participants, project, meeting title, pre-staged tickets
  [decisions ledger]   pinned constraints/decisions/owners   ← NEVER dropped
  [recent transcript]  last N turns, raw, speaker-labeled     ← recency buffer
  [retrieved slice]    ONLY IF the question references something not in
                       buffer+ledger → semantic search the full stored
                       transcript, top-k                       ← miss fallback
  + the question
→ hand to the agent; the agent fetches its own repo/ticket detail.
```

### The decisions ledger — the fix for "what if last-N missed it"
Naive "last N turns" fails when a question depends on something OLDER than the
window — e.g. a constraint set 40 min ago ("don't touch the auth schema") has
fallen out, so the agent answers confidently WRONG. This is the #1 documented
production failure ("silent context loss"). The fix is NOT a bigger window — it's
to **promote durable facts out of the raw stream into a small pinned block that
is always sent**:

- Every few turns, a tiny extraction pass pulls decisions / constraints /
  commitments / owners / dates / key numbers into a **facts ledger** (dedup,
  capped, compacted if large). This is the only always-on AI, and it's cheap.
- The ledger is in EVERY projection regardless of N, so a minute-5 constraint
  still reaches the agent at minute 40.

### Safety nets (so a miss is never a confident wrong answer)
1. **Retrieval-on-miss** — if the question names something not in buffer+ledger,
   semantically search the full stored transcript (the only place we RAG the
   transcript). Cross-MANY-meetings retrieval is a later feature.
2. **The agent self-checks** — because it has the repo, it can verify ambiguous
   transcript against real PRs/tickets (a quiet USP win).
3. **Say-what's-missing** — system prompt: if the answer depends on something not
   in the provided context, state what's missing rather than guess. Better "I
   don't see where that was decided" than a wrong ship call.

### Decide N by tokens, not turns
N = a token budget (~last 2–4k tokens), tuned to leave room for the ledger + the
answer. The ledger (small) is always included; retrieval fires only on a likely
miss; on overflow, summarize older turns (existing `maybe_compact`) while keeping
recent raw and the ledger intact.

### Maps to Bluey's code (build order)
| Layer | Status |
|---|---|
| Full transcript stored | ✅ meeting record |
| Recent buffer → agent | ⚠️ **Blocker 1** — capture exists, injection doesn't |
| Structured facts | ⚠️ partial (`answer_context_from_meeting`) + pre-staging |
| Decisions ledger | ❌ new (tiny extraction + pinned block) |
| Summary on overflow | ✅ `maybe_compact` exists |
| Transcript retrieval-on-miss | ❌ new (v2) |
| Agent fetches work data | ✅ agent's MCP already does this |

**Order:** (1) Blocker 1 = inject recent buffer + facts (day-one core loop). (2)
Decisions ledger (the trust layer — stops confident-wrong answers). (3)
Say-what's-missing in the prompt (free, big trust win). (4) v2: retrieval-on-miss
+ overflow summary (long/deep-meeting polish).

---

## LATENCY — "instant something real to say, then the deep answer"

**The problem.** When a question lands on you in a meeting, the agent's grounded
answer (Claude Code / Cursor → MCP → repo/Jira) takes *seconds*. But you need
words in ~300ms or the moment passes and you're sitting there silent. The naive
fix — show "thinking…" or filler ("good question…") — is *worse* than silence;
it's visibly stalling. The instant layer must be **real, grounded content** that
just happens to be cheap to produce.

### The two-layer answer: fork, don't fall back

One answer card, two writes — a **fork-and-merge**, not a fallback chain:

```
t=0ms     question detected (the §6 trigger already fires this)
t~50-100ms  FAST layer → push_card with a REAL first line from LOCAL context
t~2-8s      DEEP layer → same card, update_card streams the agent's grounded answer
                         (extended BELOW the fast line — never yanked away)
```

The card is never empty, and the first thing shown is *true* — not "thinking…".
This reuses what already exists: the pipeline streams (`AnswerStreamEvent`,
`replay_text`, `UpdateCard`), there's a local-answer path (`AiProviderKind::Local`
→ `local_answer`), and generation-IDs (`answer_generation`) already cancel stale
answers. The change is: write the fast card **synchronously**, then `tokio::spawn`
the agent drive into the *same* card id instead of awaiting it inline (today
`OverlayEvent::AskRequested` awaits `answer_with_provider_runtime`).

**Important:** the existing `ProviderRoute` is a *fallback* chain (try A, if A
**fails** try B). That is the wrong shape — we don't want "local only if the agent
dies." We want both, in parallel, racing to the same card. The fork is new control
flow in the daemon, not a new route.

### Where the fast layer's content comes from (it is NOT filler)

Three local, fast tiers — adopt in order:

- **Tier A — extractive (~10ms, zero model):** the single most-relevant *grounded*
  line from what Bluey already holds — decisions ledger, the matched pre-staged
  ticket, recent transcript. This is `local_answer` reorganized to lead with one
  real line. **Ship this first** — real content in <100ms, no new dependency.
- **Tier B — tiny local LLM (~300-700ms):** a 1-3B instruct model (rides the `ort`
  runtime we already ship for Parakeet) writes one grounded sentence. Better prose,
  still local. A drop-in upgrade behind the same seam; only worth it if Tier A's
  phrasing feels templated.
- **Tier C — the agent (seconds):** the real grounded answer (repo/PR/MCP). Already
  built; it *extends* the fast layer.

### The discipline that makes the fast layer safe: state provenance, not fact

The deep risk: if the fast layer says "we decided Friday" and the agent returns
"the PR slipped to Tuesday," **you've already said the wrong thing out loud.** The
fix is structural — the fast layer reports *what Bluey is the authority on*: the
**meeting** ("*From the meeting: the plan was Friday*"), which is true regardless
of repo state. The agent then adds *repo*-truth ("*and the PR merged this
morning*"). Two layers, both true, no contradiction. This is the §0/§3 split
(transcript is Bluey's, repo is the agent's) reappearing as a *latency* strategy.
When local context is thin (cold-start, first ~10 meetings), Tier A must hedge
*honestly* ("*pulling up the auth PR — last I have is it was in review*") rather
than invent. So the instant layer's quality **rides on the ledger + pre-staging
being populated** — the latency feature and the context features reinforce each
other.

### Researched-technology verdict (RAG/KAG/CAG/GraphRAG/Mem0/Zep/LangGraph)

Evaluated against what Bluey actually does. The pattern: *retrieval/caching*
techniques (about latency + prompt shape) help; *memory-store* techniques don't,
because Bluey **deliberately does not own the memory** — the agent owns
work-knowledge (via MCP), the transcript store owns meeting-knowledge.

| Technique | Verdict | What to actually do |
|---|---|---|
| **CAG / KV-cache / prompt caching** | ✅ **Real win** | We don't run the model (the agent does), so we can't touch its KV cache directly. But its **provider** (Anthropic/OpenAI) caches on a stable prompt **prefix**. So: order the package **stable-first** (system prompt + pre-staged brief + ledger), **volatile-last** (recent transcript + the question). Every follow-up in a meeting then reuses the cached prefix → faster + cheaper. Pure prompt-ordering discipline, no new infra. |
| **Agentic RAG (retrieve-decide-rewrite)** | 🟡 Mostly already have it | For *work data* the agent already does this natively (deciding what to grep IS agentic retrieval). For *transcript*, add only "**RAG on miss**" — a conditional, not a framework (already planned as v2 above). |
| **Zep/Graphiti "validity periods"** | 🟡 One idea only | Steal the idea, not the engine: make ledger decisions **supersede-able with timestamps** (~20 lines on the existing ledger) so "decided X, then changed to Y" keeps both as historical truth. Skip the graph DB. |
| **Mem0** | ❌ No | Its job (be the persistent memory store) overlaps both MCP and `cue-rag`; adds a third, drifting shadow store + a Python dep. The technique is fine; it solves a problem we architected away. |
| **GraphRAG / HippoRAG** | ❌ No | Entity-graph multi-hop over a corpus — Bluey has no corpus that needs it (transcript is recency-bounded; the repo the agent traverses live). |
| **LangGraph** | ❌ No | Multi-agent stateful orchestration for a flow that is one agent, linear. Breaks the no-vendor-SDK / one-binary constraint; wraps a flow already in `cue-agent-bridge`. |

### Build order (latency)

(1) **Fork** — write Tier-A fast card synchronously, spawn the agent into the same
card. Biggest perceived-latency win, no new deps. (2) **Prompt-ordering for
caching** — stable-first/volatile-last in the assembled package (pairs with §
pre-staging). (3) **Provenance framing** in the fast layer (the trust guard). (4)
Later: Tier-B local LLM; ledger validity-periods; pre-warm the agent session so the
*deep* layer's first token is faster too. **Skip** speculative pre-fetch (firing the
agent on *predicted* questions) — expensive guesswork; the fork already gets ~90%
of the feel.
