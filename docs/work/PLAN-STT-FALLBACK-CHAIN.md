# PLAN — STT Fallback Chain

**Status:** design only (captured from user direction 2026-05-13).
**Sequence:** implement progressively starting Round 4+. Do NOT start until
Round 3 is merged.

## Goal

Bluey's listening pipeline should never fail silently. If the primary STT
provider is unavailable (auth, quota, network), Bluey transparently rotates
to a secondary cloud provider, and if all cloud providers are down (or the
user has explicitly chosen offline mode), Bluey falls back to a local model.
User sees a small provider indicator in the overlay; no transcripts are lost.

## Target provider chain

| Tier | Provider | Role |
|---|---|---|
| 1 | **Deepgram Nova-3** | Primary. Low latency, strong real-time, diarization, word timing. Already landed in Round 3. |
| 2 | **One of:** OpenAI Realtime / AssemblyAI streaming / Soniox / Groq Whisper | Cloud fallback. Exact choice depends on cost, latency, and geographical availability — revisit at implementation time. |
| 3 | **Local whisper.cpp (or equivalent)** | Offline/private mode. Higher latency is acceptable. `whisper.cpp` is more controllable than OS-caption scraping and doesn't depend on accessibility APIs. |

OS-caption scraping (macOS Live Captions, Windows Live Captions) is
explicitly **rejected** as a fallback path: the transcript API is not
uniformly exposed, and screen-scraping caption overlays is fragile.

## Error-classification → failover policy

The policy is encoded in `SttError`'s `is_retryable()` and
`should_failover()` helpers, already defined in Round 1. Fallback router
inspects the variant returned by the current provider:

| `SttError` | Same provider retry? | Failover to next tier? |
|---|---|---|
| `Auth` | ❌ never | ✅ immediately |
| `Quota(_)` | ❌ never | ✅ immediately |
| `Network(_)` | ✅ up to N attempts, short window (say 3 retries or 10 s wall clock) | ✅ after retry budget exhausted |
| `Protocol(_)` | ✅ once | ✅ on second occurrence |
| `Provider(_)` (provider-level error frame) | ✅ once | ✅ on second occurrence |
| `AudioFormat(_)` | ❌ | ❌ — fix config, surface to user |
| `NotActive` | ❌ | ❌ — internal state bug |

Retry budget inside a tier uses existing `reconnect_delay` exponential
backoff. Failover is immediate once budget is exhausted — no additional
backoff — because the whole point is to try a different provider.

## Component shape (provisional, to be refined in implementation round)

```rust
pub struct SttRouter {
    providers: Vec<Box<dyn SttProvider>>,  // ordered primary → local
    active: usize,
    source: AudioSource,
}

impl SttRouter {
    // Constructor takes a config describing which providers to enable
    // and in what order.
    pub async fn new(cfg: RouterConfig, source: AudioSource) -> Result<Self, SttError>;
}

#[async_trait]
impl SttProvider for SttRouter {
    // name() returns "<active-provider-name>" (changes over time)
    // connection_state() returns the active provider's state
    // send_audio() forwards to active provider; on failure classification,
    //   spawn failover and re-forward to new active
    // next_event() multiplexes events from the active provider; when
    //   failover happens, emit a synthetic event so the overlay can show
    //   "backup STT" banner
}
```

Key invariant: the rest of the daemon (audio capture, framer, VAD,
session manager) sees a single `Box<dyn SttProvider>` — the router. It
never knows which underlying provider is active at any given moment.

## Observability

The overlay gets a new `OverlayMessage::SttProviderChanged { name: String }`
variant so it can show `Listening: Deepgram` / `Listening: backup STT` /
`Listening: local (offline)`. The existing `TranscriptPartial/Final` flow
is unchanged — the overlay keeps rendering transcripts regardless of which
provider produced them.

Metrics / logs: every failover emits a single `tracing::warn!` with
`from_provider`, `to_provider`, and the triggering `SttError` variant
(but never raw error payloads that might contain secrets).

## Open questions (resolve at implementation time)

1. **Which tier-2 provider?** — cost vs latency vs availability. A quick
   survey at the point of implementation will decide.
2. **Local whisper.cpp binary distribution** — bundle with the app, or
   require user to install? Bundling adds ~60–150 MB depending on model
   size; installing separately simplifies our CI.
3. **Should local whisper be opt-in only?** — On-by-default may surprise
   users who don't expect 1 GB of RAM used by the whisper model. Likely
   off by default with a "use local transcription" toggle.
4. **Auto-failback** — if Deepgram comes back 5 minutes after we
   failed over, do we silently revert? Leaning: no, don't auto-failback
   mid-session; wait until the next session starts to reassess.
5. **Partial-transcript continuity across failover** — Deepgram's partial
   won't match Groq's partial. Options: drop the in-flight utterance on
   failover and start fresh at the next VAD boundary (simpler), or try to
   stitch (harder, probably not worth it).

## Rounds this unlocks

- **Round 4 (current candidate)** — overlay restart loop + system audio
  capture. STT fallback chain is NOT in Round 4 scope.
- **Round 5 (tentative)** — second cloud provider implementation + router.
- **Round 6 (tentative)** — local whisper.cpp provider + "offline mode"
  toggle in dashboard.

Dependencies on Round 3 to be satisfied before Round 5 can start:
- `SttProvider` trait remains stable (already designed to support
  multiple impls; Deepgram is the first).
- `SttError` classification is correct (already verified by Round 3 tests).
- `build_url` / `parse_frame` / `mask_api_key` are provider-specific and
  live in `stt/deepgram/` — new providers get their own subdirectory.
