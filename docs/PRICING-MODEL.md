# Bluey Pricing Model

> **Source of truth for v0.2 pricing, account credits, markup, tiers, and where this
> info appears in the product UI.**
>
> Locked decisions live here; rationale + math live in the
> per-decision sections. Cross-referenced from `DECISIONS.md`,
> `docs/HOW-IT-WORKS.md`, `FUTURE-IMPLEMENTATIONS.md` R14.13.
>
> Last updated: 2026-06-20.

---

## 1. Locked decisions

| Field | Value | Why |
|---|---|---|
| First reload | $15 minimum | low-friction entry; enough for real testing without large chargeback exposure |
| Reload model | manual hosted checkout + opt-in saved-card Auto Reload | customer controls threshold and amount; credits are created only after processor payment success |
| Credit validity | **up to 12 months (365 days) from purchase** | per-batch, FIFO; balance carries forward across reloads as long as oldest batch is unexpired |
| Markup floor | **150%** | user direction 2026-05-19 |
| Markup tier — Easy/Medium | 200% | absolute cents are tiny; small markup absurd |
| Markup tier — Deep speculative | 150% | absolute cost is more visible to customer |
| Markup tier — Vision | 150% | GPT-5.5 vision-capable route pricing already high |
| Hard stop | balance < estimated cost → 402 | no debt, no surprise charges |
| Mid-stream cut | running cost > balance → cut + Bluey eats overrun | customer never sees overrun deduction |
| Free trial | 10 minutes of active session time | mirrors Pinky |

---

## 2. Per-question-type cost table

> **Billing precision (Codex S4.3 reconciliation, 2026-05-19):** all
> customer charges and balance accounting are in **whole cents**.
> Costs below 1¢ round up to 1¢ at charge time. The "raw" cost
> column below shows the formula output to 4 decimal places of a
> cent for transparency, but the actual amount deducted is the
> ceiling-rounded **customer cents** column. This means:
>
> - An "Easy" cue with a raw cost of $0.0008 is billed as **1¢**.
> - A "Medium code" cue with a raw cost of $0.0345 is billed as **4¢**
>   (0.0345 → 0.04, rounded up).
> - The Light tier projection (~1,500 cues per $15) reflects the
>   1¢ floor; under fractional-cent billing it would be ~10x higher.

> **Provider price snapshot date:** 2026-06-20. List prices from
> `https://platform.openai.com/docs/models`,
> `https://docs.anthropic.com/en/docs/about-claude/pricing`, and
> `https://ai.google.dev/gemini-api/docs/pricing`.
> Refresh at every minor release. Managed LLM routes currently price
> OpenAI `gpt-5.4-mini`, OpenAI `gpt-5.5`, Anthropic
> `claude-sonnet-4-6`, Anthropic `claude-opus-4-8`, Anthropic
> `claude-haiku-4-5-20251001`, Gemini `gemini-3.1-pro-preview`,
> Gemini `gemini-3-flash-preview`, and Gemini `gemini-3.1-flash-lite`.
>
> **Vision tokenization caveat:** "1 image" in the table below is a
> simplification. OpenAI GPT-5.5 vision input is billed as image/text
> input tokens; exact image tokens vary with the provider's current
> `detail` parameter and image dimensions: a 1280×720 screenshot at
> auto detail decomposes into ~3-4 256×256 tiles (~85 tokens per
> tile) plus a 85-token base. Real per-image cost depends on size,
> detail, and model input price.

| Type | Input tokens | Output tokens | Image tokens | Provider/model | Bluey raw cost (input/output split) | Customer pays |
|---|---|---|---|---|---|---|
| Easy code | ~150 | ~100 | 0 | `gpt-5.4-mini` Instant | in: $0.000113 + out: $0.000450 = **$0.000563** | raw markup $0.0017; 1¢ minimum |
| Medium code | ~800 | ~600 | 0 | `claude-sonnet-4-6` Balanced | in: $0.0024 + out: $0.0090 = **$0.0114** | raw markup $0.034; charged 4¢ |
| Hard code (speculative) | ~1500 + ~1000 | ~500 + ~500 | 0 | `gpt-5.4-mini` + `claude-opus-4-8` | Instant: $0.0034 + Deep: $0.0175 = **$0.0209** | raw markup ~$0.054; charged by actual route(s) |
| System design (speculative) | ~2000 + ~1200 | ~500 + ~2500 | 0 | `gpt-5.4-mini` + `claude-opus-4-8` | Instant: $0.0038 + Deep: $0.0685 = **$0.0723** | raw markup ~$0.18; charged by actual route(s) |
| Vision | ~200 | ~300 | ~3 tiles | `gpt-5.5` vision | image+text in: ~$0.0027 + out: $0.0090 = **$0.0117** | raw markup ~$0.029; charged 3¢ |
| Easy general | ~150 | ~150 | 0 | `gpt-5.4-mini` Instant | in: $0.000113 + out: $0.000675 = **$0.000788** | raw markup $0.0024; 1¢ minimum |
| Medium general | ~400 | ~500 | 0 | `claude-sonnet-4-6` | in: $0.0012 + out: $0.0075 = **$0.0087** | raw markup $0.026; charged 3¢ |

**STT pricing used by server meters:**

| Provider/model | Upstream list price | Customer markup | Notes |
|---|---:|---:|---|
| Deepgram `nova-3` | $0.0043/min | 150% | Primary chunked `/router/transcribe` route |
| OpenAI `gpt-4o-mini-transcribe` | $0.0030/min | 150% | Cloud fallback when Deepgram is busy/unavailable |

**Raw-cost formula:**

```
LLM cost  = (input_tokens  / 1_000_000) * provider_input_price_per_1M
          + (output_tokens / 1_000_000) * provider_output_price_per_1M

Vision    = (n_tiles * 85 + 85) tokens at the model's input price
          + output_tokens at the model's output price

Customer = LLM cost * (1 + markup_percent / 100)

Markup tiers: 200% Easy/Medium, 150% Deep speculative, 150% Vision.
```

The table above is a rounded view; the running implementation in
`server/src/pricing/mod.rs::compute_cost` does the full integer-microcent
arithmetic without floats.

## 2.1 Customer billing formula for discussion

This is the product/business formula we should use when deciding what a
customer pays for each paid Bluey action. The implementation currently uses
the simplified `usage_charge_cents` path in `server/src/pricing/mod.rs`;
future pricing changes should be measured against this formula before code
changes land.

### Variables

| Symbol | Meaning |
|---|---|
| `U` | Upstream provider cost for the request: LLM tokens, STT seconds, vision tokens, embeddings, or RAG calls |
| `M` | Bluey usage markup for the lane: 200% easy/balanced, 150% deep/vision/STT by current policy |
| `P` | Payment processor fee allocation. Usually handled at reload time, not per request |
| `R` | Risk reserve for refunds/disputes/provider variance. Recommended starting value: 5-10% of `U * (1 + M)` |
| `F` | Customer-facing minimum billable request floor. Current implementation: 1 cent |
| `C` | Customer charge deducted from wallet credits |
| `B` | Bluey internal cost recorded for margin/reconciliation |

### Formula

```
B = U

gross_usage = U * (1 + M)
risk_reserve = gross_usage * R

C_raw = gross_usage + risk_reserve
C = ceil_to_cent(max(C_raw, F))
```

For v0.2 alpha, keep `P` out of per-request metering. Processor fees are
absorbed when the customer reloads credits. Example: if a user buys $30, the
wallet receives $30, while finance/reconciliation tracks Square fees separately
against gross margin.

### Recommended starting policy

| Lane | Markup `M` | Risk reserve `R` | Customer floor `F` |
|---|---:|---:|---:|
| Instant/easy text | 200% | 5% | 1 cent |
| Balanced text/code | 200% | 5% | 1 cent |
| Deep/system design | 150% | 7.5% | 1 cent |
| Vision/screen analysis | 150% | 7.5% | 1 cent |
| Live STT/captions | 150% | 10% | aggregate per session, not per tiny chunk |
| Embeddings/RAG | 200% | 5% | bundled into answer/session action unless surfaced separately |

### Guardrails

- Never credit spendable balance until Square/Stripe reports a successful payment.
- Never let a request start unless estimated `C` is available or reserved.
- Never allow final billing to complete if the final provider usage/billing event is missing.
- Store both `B` and `C` on usage events so margin can be audited later.
- Reconcile daily: `sum(customer_charges) - sum(upstream_costs) - processor_fees - refunds/disputes`.
- Flag accounts for review when expected margin turns negative, payment is disputed, refund is issued, or provider usage exceeds customer wallet deductions.

### Discussion points before locking v0.2 pricing

1. Keep the 1-cent floor or move to microcent wallet accounting?
2. Apply the risk reserve now or only track it internally until we have real data?
3. Bundle embeddings/RAG into the answer charge or show separate line items?
4. Charge live STT by exact seconds, rounded session total, or a per-minute floor?
5. Keep 150%/200% markup or move to a simpler "2.5x provider cost, 1-cent minimum" rule?


## 3. Three usage tiers (the product MUST show these to customers)

These are realistic mixes used to project credit duration. The product
UI shows the customer their current rolling-7-day mix and tells them
which tier they're in, so the $15 → time projection makes sense.

### Light — quick lookups, occasional medium

```
Mix:    55% Easy code · 15% Medium code · 3% Hard ·
        1% System design · 3% Vision ·
        18% Easy general · 5% Medium general

Avg per cue:        $0.011
Cues per $15:       ~1,500
Hours focused work: ~60–95
$15 lasts:          ~6 weeks at 30 min/day
```

### Typical tech user — real coding + occasional design + few screenshots

```
Mix:    35% Easy code · 22% Medium code · 8% Hard ·
        6% System design · 5% Vision ·
        18% Easy general · 6% Medium general

Avg per cue:        $0.022
Cues per $15:       ~690
Hours focused work: ~25–45
$15 lasts:          ~2-3 weeks at 1 hr/day
```

### Heavy user — lots of Hard, vision attachments, design work

```
Mix:    18% Easy code · 28% Medium code · 18% Hard ·
        12% System design · 8% Vision ·
        10% Easy general · 6% Medium general

Avg per cue:        $0.036
Cues per $15:       ~410
Hours focused work: ~15–25
$15 lasts:          ~5 days at multi-hour daily use
```

---

## 4. Where this appears in the product UI

**Decision (2026-05-19, user):** keep tier breakdown ON SCREEN —
customers should see what they're getting before they pay. Three
surfaces:

### 4.1 Onboarding screen (after first $15 reload)

Bluey dashboard at `https://bluey.sh/onboarding/welcome` shows:

```
┌─────────────────────────────────────────────────────────────────┐
│  Welcome to Bluey — your $15 is loaded.                         │
│                                                                 │
│  Here's roughly what $15 buys, depending on how you use Bluey:  │
│                                                                 │
│    💼  Light user        ~1,500 cues   ~6 weeks                 │
│    ⚙️   Typical tech     ~690 cues     ~2-3 weeks               │
│    🔥  Heavy user        ~410 cues     ~5 days                  │
│                                                                 │
│  Reload when ready. Bluey is not a monthly subscription, and     │
│  credits stop at zero so there is no surprise usage debt.        │
│                                                                 │
│  Credits stay active for up to 12 months (365 days). Unused     │
│  balance expires after that — we'll email you 30 days before.   │
│                                                                 │
│                          [   Got it   ]                         │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Live dashboard `/account/usage`

```
┌──────────────────────────────────────────────────────────────────┐
│  Balance:  $27.43           Reload: $15 minimum                  │
│                                                                  │
│  Your last 7 days:                                               │
│    132 cues · $4.12 spent                                        │
│    ┌────────────────────────────────────────┐                    │
│    │ Easy code       ████████████  40%      │                    │
│    │ Medium code     ████          14%      │                    │
│    │ Hard code       █             5%       │                    │
│    │ System design   ▎             2%       │                    │
│    │ Vision          ██            7%       │                    │
│    │ Easy general    ████████      28%      │                    │
│    │ Medium general  █             4%       │                    │
│    └────────────────────────────────────────┘                    │
│                                                                  │
│  You're a Typical tech user.                                     │
│  At your current rate, $27.43 lasts ~32 more days.               │
│                                                                  │
│  ┌────── Tier comparison ──────┐                                 │
│  │ Light      ~1,500 cues / $15  ~6 weeks                       │
│  │ Typical    ~690 cues / $15    ~2-3 weeks  ← you              │
│  │ Heavy      ~410 cues / $15    ~5 days                        │
│  └─────────────────────────────┘                                 │
│                                                                  │
│  [ Add $15 now ]  [ View credit batches ]                        │
└──────────────────────────────────────────────────────────────────┘
```

### 4.3 CLI command `bluey usage`

```
$ bluey usage

Balance         $27.43      ($15 minimum reload)
Last 7 days     132 cues, $4.12 spent
Tier            Typical tech user
Projection      $27.43 lasts ~32 days at your current rate

Tier comparison:
  Light       ~3,000 cues per $30 (~3 months)
  Typical     ~1,380 cues per $30 (~5 weeks)   ← you
  Heavy         ~825 cues per $30 (~10 days)

Per-cue cost (last 50):
  $0.0003   Easy code        (×18)
  $0.034    Medium code      (×9)
  $0.050    Hard code        (×3)
  $0.046    Vision           (×2)
  ...

Credits are valid for up to 12 months (365 days) from purchase. Run `bluey credits` for
batch-by-batch expiration dates.
```

### 4.4 Overlay top strip (always visible, minimal)

```
💰 $27.43   ●  Bluey   ▾
```

The overlay top strip is intentionally minimal — just the live
balance + status dot. Tier info and projections live in the
expanded panel + dashboard + CLI to avoid cluttering the pill.

### 4.5 Per-cue cost label (inline in the feed)

```
$0.04 · 412 in / 89 out · 1.8s
```

This appears below each cue card after the stream completes.
Always visible, always honest.

---

## 5. Bluey gross margin

Per-request margin given the markup tiers:

| Lane | Bluey raw | Customer pays | Bluey margin per request |
|---|---|---|---|
| Easy/Medium (200% markup) | $0.0001–$0.011 | 3× raw | ~67% |
| Deep speculative (150% markup) | $0.020–$0.041 | 2.5× raw | ~60% |
| Vision (150% markup) | $0.018 | 2.5× raw | ~60% |

After payment processing fees (~3% per reload) and cloud infra costs,
the per-request margin floor stays above 55%. Healthy SaaS economics.

---

## 6. Competitive positioning

| Product | Cost | What's included |
|---|---|---|
| ChatGPT Plus | $20/mo | unlimited GPT-4 (rate-limited) |
| Cursor Pro | $20/mo | 500 fast requests + unlimited slow |
| GitHub Copilot | $10–19/mo | autocomplete + chat |
| **Bluey, light** | **~$10/mo equivalent** | managed routing + auto draft+refine + RAG memory + transcript-aware cues |
| **Bluey, typical** | **~$25–30/mo equivalent** | same |
| **Bluey, heavy** | **~$60–90/mo equivalent** | same, more usage |

Bluey lands in the standard SaaS pricing band for light/typical
users and scales naturally for heavy users through reloadable account credits,
not a monthly subscription.

---

## 7. Levers if we want to tune later

These are NOT changes for v0.2; they're knobs for future iterations
once we have real usage data.

| Want | Lever | Effect |
|---|---|---|
| Cheaper for light users | Drop Easy markup to 100% | $30 lasts ~30% longer for light tier; margin drops to 50% |
| More premium positioning | Raise Easy markup to 300% | $30 lasts 25% less; absolute cents still tiny ($0.0004) |
| More predictable revenue | Optional monthly plan later | separate future product decision; not part of v0.2 credit model |
| Encourage Hard usage | Drop Deep markup to 100% | speculative cost more attractive; margin still 50% |
| Discourage Hard usage | Raise Deep markup to 200% | $30 lasts shorter for heavy users; pushes Easy/Medium |

---

## 8. Implementation references

- `FUTURE-IMPLEMENTATIONS.md::R14.13` — server-side account-credit implementation.
- `FUTURE-IMPLEMENTATIONS.md::R14.14` (NEW, see below) — daemon-side
  cost label + balance display + tier visibility UX.
- `docs/HOW-IT-WORKS.md::Section 3` — overlay UI mockup including
  per-card cost label.
- `docs/HOW-IT-WORKS.md::Section 4–7` — request flow, balance check,
  hard stop, mid-stream cut.
- `DECISIONS.md` — the account-credit, no-BYOK, markup, and "always
  visible" decisions.
