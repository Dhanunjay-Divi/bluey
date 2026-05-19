# How Bluey Works (v0.2 customer flow)

> **End-to-end walkthrough of the v0.2 paid product.** This is the doc
> a new engineer reads to understand what happens when a real
> customer uses Bluey, from signup through their first cue request
> through their first auto top-up.
>
> v0.1 BYOK is dev-only (see `DECISIONS.md` 2026-05-19); v0.2 is
> managed-only. This doc describes v0.2 unless explicitly marked
> otherwise.

---

## 0. Pricing model (decided 2026-05-19)

**Prepaid wallet with auto top-up.** Customer pays Bluey directly,
metered per request.

| Item | Value |
|---|---|
| Free trial | 10 minutes of active session time (mirrors Pinky's free model) |
| Initial top-up | $30 USD |
| Auto top-up trigger | balance drops below $5 (configurable per account) |
| Auto top-up amount | $30 USD |
| Pricing basis | per request: input tokens + output tokens + model used |
| Bluey markup over upstream cost | 100–200% (target gross margin) |
| Hard stop on $0 balance | stream cut mid-response; UI shows "Add $30 to continue" |

**Hard guarantees:**
1. Customer cannot rack up debt. Server checks balance before each
   request AND mid-stream; cuts the stream the moment estimated
   completion cost exceeds remaining balance.
2. Customer always sees current balance at the top of the overlay
   (and a per-card cost label after each cue).
3. Customer always sees what would have happened if Bluey were
   broken — i.e. if the wallet hits zero, the UI tells them
   exactly why and what to do about it.

---

## 1. Customer signup + first install

```
┌──────────────────────────────────────────────────────────────────┐
│ 1. Customer visits  https://bluey.dev/signup                     │
│    enters email + password                                       │
│    bluey-server creates account, balance = $0.00, trial = 600s   │
│                                                                  │
│ 2. Verification email confirms account.                          │
│                                                                  │
│ 3. Customer redirected to /account dashboard                     │
│    Sees: "10 minutes free trial. After that, $30 minimum         │
│           reload. We deduct as you use."                         │
│    Copies install command:                                       │
│       curl https://bluey.dev/install.sh | sh                     │
│                                                                  │
│ 4. Install script (templated by bluey-server):                   │
│    - downloads bluey-0.2.0-darwin-universal.tar.gz               │
│    - verifies sha256 against /latest.json                        │
│    - extracts to ~/.local/bluey/0.2.0/                           │
│    - symlinks bluey + bluey-daemon into ~/.local/bin/            │
│                                                                  │
│ 5. Customer runs `bluey on`. Pill appears top-center.            │
│    Daemon has no token yet → overlay shows                       │
│    "Sign in to Bluey"                                            │
└──────────────────────────────────────────────────────────────────┘
```

## 2. `bluey login` (one-time)

OAuth-style device flow, mirrors Pinky:

```
Daemon                                      bluey-server
─────                                       ────────────
POST /auth/device/start ──────────────────►
                          ◄────────────── { device_code, user_code,
                                            verification_uri }

Daemon prints to terminal:
  Open https://bluey.dev/device, enter code: ABCD-1234

(customer opens browser, signs in,
 enters code)

POST /auth/device/poll ───────────────────►
  (every 5s)             ◄────────────── { access_token,
                                            refresh_token,
                                            expires_in,
                                            balance_cents,
                                            trial_seconds_remaining }

  Daemon stores tokens in macOS keyring under "bluey_account".
  Daemon flips ProviderRegistry to ManagedProvider mode.
  Daemon shows balance + trial state in overlay top strip.
```

## 3. Overlay UI (paid customer, balance > $0)

```
┌────────────────────────────────────────────┐
│  💰 $27.43   ●  Bluey   ▾                  │  ← top strip:
│                                            │     balance (live),
└────────────────────────────────────────────┘     status dot, pill

   (click expands to feed)

┌────────────────────────────────────────────┐
│  💰 $27.43   ●  Bluey         [✕]          │
├────────────────────────────────────────────┤
│  💬  Q: how do I sort a HashMap by value?  │
│      ┌─────┐                               │
│      │INSTANT│ code · openai/gpt-4o-mini · │
│      └─────┘ 88%                           │
│      Use a Vec<(K, V)> and sort_by_key…    │
│      $0.04 · 412 in / 89 out · 1.8s        │  ← per-card cost
├────────────────────────────────────────────┤
│  💬  Q: design a system for 10k qps writes │
│      ┌────┐                                │
│      │DEEP│ system_design ·                │  ← LaneBadge
│      └────┘ anthropic/3-7-sonnet · 92%     │
│      ┌─────────┐                           │
│      │ REFINED │                           │  ← appeared after
│      └─────────┘                           │     deep replaced draft
│      Architectural choices: …              │
│      $0.32 · draft+deep · 4.7s             │  ← shows BOTH costs
├────────────────────────────────────────────┤
│  [ Ask Bluey…           ] [Ask] [Attach]   │
│  [Instructions] [Recap]                    │
└────────────────────────────────────────────┘
```

## 4. Normal request flow (online, paid, Easy/Medium difficulty)

```
User asks something:                    cue-router (laptop, in-process):
   "what's the weather in NYC"             classify(prompt, …)
     │                                     → TaskClassification {
     ▼                                         task_type: General,
   bluey-daemon                                difficulty: Easy,
     │                                         latency_lane: Instant,
     │                                         confidence: 0.85
     │                                       }
     │                                     ManagedPolicy::route(…)
     ▼                                     → ProviderRoute { lane: Instant,
   BlueyManagedProvider                        provider: bluey-server }
     │
     │ HTTPS POST /router/complete
     │ Authorization: Bearer <bluey_account_token>
     │ Body: LlmRequest {
     │   system, user, max_tokens, temp, lane: "instant"
     │ }
     ▼
   bluey-server (cloud):
     1. validate token
     2. check balance >= estimated_cost (input_tokens × markup_in
                                          + max_output_tokens × markup_out)
        if NOT enough → return 402 Payment Required { balance, needed }
        daemon shows "Add $30 to continue" banner
     3. pick actual provider (e.g. OpenAI gpt-4o-mini)
     4. dispatch to OpenAI with Bluey's API key
     5. stream chunks back to daemon
     6. mid-stream cost check: if running_cost > balance, cut stream
        + emit { type: "balance_exhausted" } event
     7. on completion: deduct actual_cost from balance
                       atomic UPDATE accounts SET balance = balance - cost
     8. return final chunk + new balance in trailer
     │
     ▼ chunks
   bluey-daemon emits cue_response_chunk events
     │
     │ on completion:
     ▼
   POST /usage/event {
     request_id, account_id, lane: "instant",
     provider: "openai", model: "gpt-4o-mini",
     input_tokens: 412, output_tokens: 89,
     ms: 1840,
     cost_cents_to_customer: 4,    // 0.04 USD
     cost_cents_to_bluey: 1.3       // upstream cost
   }
     │
     ▼ Daemon updates the live balance shown in overlay top strip
       Customer sees: $27.43 → $27.39
```

## 5. Hard question (speculative draft + final, ~$0.30)

```
User asks: "design a system to handle 10k qps writes"

Classifier → Hard / Deep / 0.90 confidence
SpeculativeRouter fires BOTH lanes:

  ┌─ Instant lane (fires first, fast) ─────────────────┐
  │  POST /router/complete  lane=instant               │
  │  → OpenAI gpt-4o-mini                              │
  │  → balance check: $27.39 - est $0.05 = OK          │
  │  → streams Draft chunks                            │
  │  → final cost: $0.04                               │
  └────────────────────────────────────────────────────┘

  ┌─ Deep lane (fires in parallel, slower) ────────────┐
  │  POST /router/complete  lane=deep                  │
  │  → Anthropic claude-3-7-sonnet                     │
  │  → balance check: $27.35 - est $0.30 = OK          │
  │  → returns full response (single chunk)            │
  │  → final cost: $0.28                               │
  └────────────────────────────────────────────────────┘

Daemon emits cue_response_chunk with replace_body: true
  → UI swaps draft for refined answer
  → LaneBadge gains REFINED tag
  → cost label shows "$0.32 · draft+deep · 4.7s"

Total deducted: $0.04 + $0.28 = $0.32
Customer balance: $27.39 → $27.07
```

## 6. Hard stop on $0 balance

```
Customer's balance has dropped to $0.18 over the day.
Customer asks a Hard question (estimated cost $0.30).

bluey-server:
  1. balance check: $0.18 < est $0.30 → NOT enough
  2. responds with 402 Payment Required {
       balance_cents: 18,
       estimated_cost_cents: 30,
       reason: "insufficient_balance",
       reload_url: "https://bluey.dev/reload"
     }

bluey-daemon:
  receives 402, does NOT start streaming.
  emits a synthetic cue_response_chunk with:
    {
      response_id, kind: "answer", finished: true,
      partial_text: "",
      replace_body: true,
      router_meta: { ..., status: "balance_exhausted" }
    }
  AND emits a banner event the dashboard renders as:
    ┌────────────────────────────────────────────────────┐
    │  💰 Balance: $0.18 — Bluey can't answer that one.  │
    │                                                    │
    │  Add $30 to continue (auto top-up enabled by       │
    │  default; turn off in Settings).                   │
    │                                                    │
    │              [   Add $30 now   ]                   │
    └────────────────────────────────────────────────────┘

Click "Add $30 now":
  daemon → POST /billing/topup { amount_cents: 3000 }
  bluey-server → Stripe charge using saved card
  on Stripe webhook success → balance += 3000
  daemon polls /account/me, sees new balance, refreshes UI

If auto-top-up is enabled (default ON):
  the moment balance < $5, server triggers Stripe charge automatically.
  customer sees a non-blocking notification:
    "Auto top-up: charged $30, new balance $35.18".
```

## 7. Mid-stream cut (rare edge case)

If a streaming response runs longer than the customer's balance can
afford:

```
Customer has $0.05 balance.
Asks an Easy question (gpt-4o-mini, est $0.06).

Server passed the entry check ($0.05 ≥ $0.06 is false → 402, blocked).

But suppose it WAS $0.07:
  - Entry check passes ($0.07 ≥ $0.06).
  - Server starts streaming.
  - Mid-stream the model produces unexpectedly long output (say 500
    tokens vs the 200 estimated). Running cost climbs to $0.10.
  - Server's mid-stream cost check trips: $0.10 > $0.07 balance.
  - Server cuts the upstream call, emits final chunk with
    { type: "balance_exhausted", actual_cost_cents: 7 }, and
    deducts the $0.07 (NOT the full $0.10 — Bluey eats the
    overrun rather than putting the customer in the red).

Customer sees a partial answer + "Bluey ran out of mid-answer.
Add $30 to keep going." banner.

Server-side audit: track customer-overage events; if too many,
recalibrate the estimate model OR cap individual request size
more aggressively.
```

## 8. Free trial (first 10 minutes)

```
On account creation:
  account.balance_cents = 0
  account.trial_seconds_remaining = 600

For the first 10 minutes of active session time (when daemon is
attached + emitting requests), bluey-server:
  - serves /router/complete normally
  - decrements trial_seconds_remaining by request duration
  - does NOT charge balance
  - returns trial_seconds_remaining in every response

When trial_seconds_remaining hits 0:
  - server returns 402 Payment Required with reason = "trial_ended"
  - daemon shows: "Trial complete. Load $30 to keep using Bluey."
  - same UI as hard-stop on balance.

If customer never reloads: account stays in "trial_ended"; no
charges; daemon is essentially read-only for the customer.
```

## 9. Offline / privacy fallback

```
bluey-daemon checks bluey-server health every 30s.

If unreachable for >2 minutes (or user runs `bluey privacy-mode on`):

  Auto Router policy switches: ManagedPolicy → LocalFallbackPolicy
    LLM lane         → local Ollama (llama3.1)
    Embedding lane   → local sentence-transformer (R14.x)
    STT              → local whisper.cpp (already on disk)

  LaneBadge in UI gains "OFFLINE" tag.
  No usage events emitted. No balance deductions.
  Customer is still on a paid Bluey subscription;
  inference cost just shifts to their hardware.

When connectivity returns: daemon polls /admin/health,
flips policy back to ManagedPolicy automatically. Balance
is what it was before the disconnect.
```

## 10. The single billing relationship

```
                 Anthropic                      Bluey owns the keys.
                    ▲                           Customer never sees them.
                    │ (pays from Bluey key)
                    │
Customer ──$──► Bluey ──$──► OpenAI            Customer has ONE bill: Bluey.
                    │                           Bluey statement shows
                    │                           per-cue cost breakdown.
                    ▼ (pays from Bluey key)
                Deepgram (cloud STT, when used)
```

## 11. Architecture cross-references

- `ARCHITECTURE.md` — three-layer model, monetization plug-points.
- `DECISIONS.md` — the no-BYOK + prepaid-wallet decisions and rationale.
- `FUTURE-IMPLEMENTATIONS.md` R14.13 — per-use metering implementation
  detail.
- `crates/cue-router/` — classifier + policy + speculative dispatch
  (already in the workspace; `BlueyManagedProvider` is the v0.2
  swap-in).
- `bluey-server` repo (when stood up) — the server side of every
  HTTPS call described above.

## 12. What the customer sees vs what bluey-server does

| Customer sees | Server does |
|---|---|
| Pill with live balance | `/account/me` polled every 30s OR balance pushed via WebSocket |
| Per-card cost label | response header trailer with `actual_cost_cents` |
| LaneBadge with provider/model | response trailer with `lane`, `provider`, `model` |
| "Add $30" banner | 402 Payment Required from server with reason field |
| Auto top-up notification | Stripe webhook → balance increment → daemon refresh |
| 10-min trial countdown | `trial_seconds_remaining` field in every response |
| Hard stop with partial answer | mid-stream balance check → cut + truncated chunk + banner |
| OFFLINE tag | daemon detects bluey-server unreachable, flips local-fallback |
