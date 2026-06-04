# End-to-End Flow + Live-Test Spend Guard — 2026-06-04

## Customer Flow

1. Customer visits `https://bluey.sh`.
2. Site explains the product simply: `bluey on` opens the desktop answer layer; sign-in is needed only for managed cloud answers, vision, RAG, reloads, and cloud sync.
3. Customer installs Bluey using the terminal installer.
4. Customer runs `bluey on`.
5. If the desktop is not linked, Bluey opens `https://bluey.sh/login` and the overlay shows a signed-out state with a clear sign-in CTA.
6. Customer signs in or creates an account. The browser/device-link flow stores desktop tokens in the OS keychain.
7. Bluey starts a new recording/session by default. Old sessions remain available through history and cloud sync.
8. Customer can:
   - Listen to microphone/system transcript.
   - Attach readable files for the session knowledge base.
   - Analyse screen with user-approved capture.
   - Ask typed questions from the composer.
   - Set the answer style for the session.
9. The daemon sends managed work to `bluey-server`, not directly to provider APIs.
10. `bluey-server` routes to provider lanes:
    - Instant/easy: OpenAI mini lane.
    - Balanced/deep: Anthropic lane with OpenAI fallback.
    - Vision/screen: OpenAI vision lane.
    - STT: Deepgram with OpenAI transcription fallback on chunked STT.
11. Server records authoritative usage/cost events, updates balance, and returns cost labels/metadata to the desktop.
12. Session transcript, answers, attachments, and RAG chunks sync to the cloud account so the user can resume old sessions.

## Live-Test Spend Guard

This round adds a Bluey-side upstream spend guard:

```text
BLUEY_UPSTREAM_SPEND_LIMIT_CENTS=1000
BLUEY_UPSTREAM_SPEND_WINDOW_HOURS=24
```

When enabled, the server sums `usage_events.cost_cents_to_bluey` inside the rolling window and pauses new managed LLM/embed/STT dispatch before touching provider APIs if the projected provider exposure would exceed the cap.

This guard covers:

- `/router/complete`
- `/router/embed`
- `/router/transcribe`
- `/stt/session`

It intentionally does **not** replace provider-dashboard hard caps or alerts. Operators should still set $10 test limits/alerts in OpenAI, Anthropic, Deepgram, and any other provider dashboards where supported.

## Security Notes

- Provider keys stay server-side. Desktop users never receive raw OpenAI/Anthropic/Deepgram keys.
- Provider keys can be comma-separated pools, but only for approved provider capacity across projects/accounts. Do not use key pools to evade provider terms.
- The guard logs account hash, request id, route kind, current cents, projected cents, limit, and window. It does not log raw provider keys or emails.
- `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` remains dev-only and must never be present in production env files or release bundles.

## Verification Commands

```bash
cargo fmt --all --check
cd server && cargo test
git diff --check HEAD~1..HEAD
```

After deployment:

```bash
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/pricing/tiers
```

Then create a smoke account and run:

- One tiny `/router/complete` request.
- One tiny `/router/embed` request.
- One short `/router/transcribe` request.
- `bluey usage` or `/account/usage` to confirm costs appear.

## Operator Inputs Still Needed

- Confirm provider-dashboard test budget alerts/caps are set to $10 where each provider supports them.
- Confirm Square sandbox/prod location IDs and webhook signature keys are final.
- Confirm off-host backup destination.
- Confirm live email verification/password reset delivery after DNS propagation.
