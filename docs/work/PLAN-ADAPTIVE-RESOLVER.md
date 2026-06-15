# PLAN — Adaptive Resolver + deterministic hardening

> Status: DESIGN → BUILD. Branch: `agent/agent-bridge`.
> Author: Claude. Date: 2026-06-01.
> Cross-refs: `docs/work/PLAN-AGENT-BRIDGE.md`, `crates/cue-agent-bridge/`.

---

## 1. The problem

The session/connector/drive readers are **version-pinned by assumption**: they
encode *what* a format looks like (e.g. Cursor stores messages in
`fullConversationHeadersOnly` → `bubbleId` rows). When an app updates and
changes that shape, the reader silently returns 0 — exactly the drift bug we
already hit once. Hard-coding every version is unscalable whack-a-mole.

## 2. The principle

**Stop encoding answers; encode how to find the answer when you don't have it.**

Two complementary tracks, because **not every risk is a drift problem**:

- **Track A — Adaptive Resolver** (AI-assisted, for *unknown formats / drift*):
  deterministic-first, AI-fallback, cache-the-answer. Solves schema drift,
  unknown CLI output, weird configs, garbage-detection.
- **Track B — Deterministic hardening** (plain code, for *bugs / safety / perf*):
  streaming reads, security checks, arg caps, timeouts. **These must NOT use AI**
  — correctness and safety must be certain, not probabilistic.

### Hard rule

**AI is never used for safety or correctness-critical decisions.** "Did I leak
a secret?", "is this within the memory bound?", "is the apply gate satisfied?"
are always deterministic. AI only ever answers "what shape is this unknown
data?" — and its output is always passed through a deterministic validation
gate before use.

---

## 3. Track A — the Adaptive Resolver

A shared resolver every brittle reader plugs into:

```
resolve(signature, raw_sample, question) -> Recipe
  1. cache.get(signature)        → instant, free (the common case)
  2. heuristics(raw_sample)      → free; structure-shape guess
  3. ai_fallback(raw_sample, q)  → ONCE, only when 1+2 fail or yield garbage
  4. validate(result)            → deterministic gate; reject if not coherent
  5. cache.put(signature, recipe)→ solved forever for this shape
```

- **Signature**: a stable key for "this kind of data" — e.g.
  `cursor:<app_version>:<top-level-key-fingerprint>`. Keys the cache so we
  re-ask only when the shape actually changes, and never use a stale recipe
  after an update (version is part of the key).
- **Heuristics**: schema-agnostic shape detection. For a conversation: find the
  collection with many entries, each having a long-ish text field and a
  role-ish field (string user/assistant or small int), ordered by an
  index/timestamp field. Pull text+role generically — *no specific field names*.
- **AI fallback**: hand the AI a **structural sample** (a few records, keys and
  short value snippets — never bulk content, never secret values) and ask:
  *"This is a coding agent's session store. Where are the conversation messages?
  Which field is the text, which indicates user vs assistant, what orders them?
  Return a JSON recipe: {text_path, role_path, role_map, order_path}."* Route
  this through the **user's own agent** where possible (data residency), with a
  strict structural-only sample.
- **Validate** (deterministic, always): apply the recipe to the sample → does it
  yield ≥1 turn with real text and plausible alternating roles? If not, reject
  the recipe (fall through to "honest: couldn't read this format"). **Never
  apply an unvalidated recipe.**
- **Cache**: persist recipes locally (e.g. `~/Library/Application Support/bluey/
  recipes.json`), keyed by signature. First encounter is slow (AI call); every
  subsequent read is instant.

### Where Track A plugs in
| Component | Signature | What AI resolves |
|---|---|---|
| Session reading | `<agent>:<ver>:<key-fingerprint>` | text/role/order paths |
| Connector config | `<agent>:<config-key-fingerprint>` | where the MCP servers + transports are |
| Drive output parsing | `<agent>:<output-shape>` | how to extract answer + session id from CLI stdout |
| Garbage/empty detection | n/a | "is this a real conversation?" validation |

---

## 4. Track B — deterministic hardening (NO AI)

Plain bug/safety/perf fixes, built straight:

| Fix | What | Why not AI |
|---|---|---|
| **Stream JSONL reads** | read line-by-line, stop at `max_turns`; never `read_to_string` a whole 10MB+ file | memory bound must be certain |
| **Connector secret audit** | assert env/headers/url token values are NEVER serialized into `Connector`; only name+transport-kind+tier | security must be certain |
| **Context arg cap** | hard byte cap on the prompt sent to a CLI (well under the ~256KB arg limit); truncate/summarize above it | arg-length limit is a hard fact |
| **Empty-session guard** | if a session yields 0 real turns, don't drive on it — return a clear "nothing to answer from" | trivial deterministic check |
| **Cursor title perf** | bound the N+1 title lookups (cap how many composers get a title-fetch; lazy/async) | perf, not shape |
| **Not-logged-in detection** | recognize common auth-error signatures → clear guidance card | deterministic first; AI only to *phrase* if unknown |

---

## 5. What this does and does NOT solve (honest)

**Solves (Track A):** schema drift, unknown CLI output parsing, weird config
shapes, empty/garbage detection — the unknown-shape family, self-healing.

**Solves (Track B):** memory spikes, secret-leak risk, arg limits, hangs,
perf — by fixing the code.

**Does NOT solve:** a wrong *command/flag* for an untested agent (AI parses
output, but can't guess the right invocation — that still needs the real CLI to
verify); auth/login itself (only better messaging); fundamentally opaque formats
(Antigravity protobuf — AI can't decode a binary it can't sample meaningfully).

**Cursor/Copilot/Codex drive:** Track A makes their *output parsing* adaptive
(big help), but the *command + auth* still want real verification. Honest status
stays "drivable-with-adaptive-parsing, command unverified until run."

---

## 6. Build slices

| Slice | Track | Scope |
|---|---|---|
| **B1** | B | Stream JSONL reads (bounded, no whole-file load) + empty-session guard |
| **B2** | B | Connector secret audit (test no token/header/url-secret ever serialized) + http+headers tier handling |
| **B3** | B | Context arg cap in the drive layer (hard byte bound) |
| **A1** | A | `AdaptiveResolver` core: signature, cache (persisted), validate gate — no AI yet, heuristics only |
| **A2** | A | Heuristic shape-detector for sessions (schema-agnostic conversation extraction) wired as a fallback when the pinned reader yields 0 |
| **A3** | A | AI fallback (through the user's agent, structural sample only) + recipe caching, behind the validate gate |

Track B first (fast, certain, closes the scary safety/memory gaps). Track A
second (the adaptive/self-healing layer), heuristics before AI.

---

## 7. Security / data-residency (unchanged, reinforced)

- AI fallback sees **structure + short snippets only**, never bulk content or
  secret values. Route through the user's own agent where possible.
- Every AI-derived recipe passes a deterministic validation gate before use.
- Recipe cache stores *shapes*, never secrets.
- Track B's secret audit is the hard guarantee; Track A never weakens it.
