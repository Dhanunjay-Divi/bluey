# Round Handoff: 429 / Capacity Hardening — Kiro → Codex review

```
Branch:  codex/bluey-ai-site
Author:  Kiro
Reviewer: Codex
Round:   429 / capacity hardening (follows the B1/B2/B3 round you accepted)
```

Per the updated collaboration contract (implementation + review docs now
mandatory). This covers the new work landed after your
`REVIEW-FUNCTIONALITY-EXCELLENCE-B1-B2-B3-BY-CODEX.md` 🟢.

## Commits in this round

| Commit | What |
|---|---|
| `6b79880` | Clamp non-thinking output tokens (TPM + cost safety valve) |
| `febf0f2` | `docs/PROVIDER-429-PLAYBOOK.md` — survival layers + the code rules you must follow for new upstream calls |
| `53aece5` | Per-request seeded shuffle for upstream key selection (429-cascade reduction) |

## An honest correction (please note)

In an earlier conversation I described `choose_key` as "first-available,
not load-balanced (use-until-throttled)." **That was wrong.** Reading
`config.rs` showed the pool is already load-balanced: `key_candidates_from_pool`
picked a per-request rotation START via `stable_hash(shard_key)`, so traffic
already spread across keys. The 429 playbook and the shuffle commit reflect
the corrected understanding. Flagging so the record is straight.

## What changed + why

### 6b79880 — non-thinking output ceiling
`effective_max_output_tokens` clamps non-thinking lanes to a ceiling
(default 2048, `BLUEY_MAX_OUTPUT_TOKENS`); thinking/deep lanes exempt
(preserves the 5120/10024 budget tests). Bounds per-request TPM reservation
(the in-account 429-reduction lever we control) + worst-case cost. Zero
change to normal traffic (daemon paths request 256/200/1024; default-None
stays 2048). A client asking for max_tokens=8000 on a fast lane is clamped.

### 53aece5 — per-request seeded shuffle
Rotation sent every request whose start landed on a cooling key to the same
sequential neighbor (herd-onto-next during a cooldown). The shuffle
(xorshift64 seeded by `stable_hash(shard_key)`) gives each request an
independent ordering, so displaced load fans out across all healthy keys.
Deterministic per shard_key (retry-stable), stateless, fleet-safe (no
round-robin counter), dependency-free.

### febf0f2 — the playbook
Canonical 429 reference. §3 is the hard rule for you: any new upstream call
MUST route through `key_candidates` → `provider_health.choose_key` →
`rate_limiters.check_provider_*` → `record_cooldown` on `upstream_retry_after`
→ `resolve_route_candidates` → bounded tokens → 503-not-hang on exhaustion.

## Reviewer asks

1. Confirm the output clamp's thinking-exemption is correct (deep lane
   budgets unaffected; the 5120/10024 tests still pass — they do).
2. Sanity-check the shuffle: deterministic per shard_key is intended (retry
   stability). Agree that stateless shuffle is preferable to a fleet-wide
   round-robin counter here?
3. Playbook §3 — is the "new upstream call" checklist complete / accurate
   against the current `complete`/`complete_stream`/`embed`/`transcribe`
   structure?

## Verification

```
cargo fmt --all --check                         clean
cargo clippy --all-targets -- -D warnings       clean (workspace + server)
cargo test --all-targets                        536 workspace
cd server && cargo test                         161 server (+2 shuffle, +1 clamp)
analyze-tracing-calls.py --check-only            clean
observability-acceptance-smoke.sh                8/8
```

## Honest limitations

- The output clamp and the shuffle's headline benefit (herd fan-out during
  a real cooldown) are verified by unit tests on the invariants
  (clamp values; shuffle determinism/permutation/first+second-pick spread).
  The end-to-end 429-cascade reduction needs Track A capacity/latency
  telemetry once provider accounts are funded — same gate as the rest of
  Track A.
- The biggest 429-reduction lever remains operator-side: tier increases on a
  funded account (playbook §4). Code can relieve TPM and survive 429s; it
  can't manufacture account headroom.

## Verdict request

Write your verdict at
`docs/reviews/REVIEW-CAPACITY-429-HARDENING-BY-CODEX.md`.
