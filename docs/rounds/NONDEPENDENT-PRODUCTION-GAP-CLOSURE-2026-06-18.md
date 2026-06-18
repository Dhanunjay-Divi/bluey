# Nondependent Production Gap Closure — 2026-06-18

> Branch: `codex/bluey-ai-site`  
> Scope: audit current branch against the latest review/comment backlog and separate code-complete items from operator-dependent live-test items  
> Author: Codex

## Verdict

No remaining code blocker was found in the current branch that I can close without external accounts, macOS permission prompts, real payment flow, or release-signing operator inputs.

The important correction: the older post-diff handoff still said dual-source STT billing reservation was open. That is now closed by `b93882b fix(server): reserve STT relay billing upfront`; the doc has been updated so Kiro/next agents do not chase stale work.

## Closed in Current Branch

- **STT money path** — server-side reservation/settlement prevents mic+system from opening more paid relay sessions than the account can cover; unused reserved credit/trial time is refunded on close.
- **Managed answer finality** — desktop managed streams reject `[DONE]`/EOF before final billing metadata instead of treating truncated output as success.
- **Server streaming billing/idempotency** — incomplete upstream streams fail before billing/caching; final deduction failures do not complete/collapse idempotency as a paid success.
- **Provider capacity behavior** — capacity-busy errors preserve retry/capacity semantics across fallback candidates; generic 503s and capacity 503s stay distinct.
- **Model routing safety** — current route candidates use the refreshed managed models, GPT-5-compatible output token fields, balanced lane no longer enables thinking, and deep-lane markup stays on the deep rate.
- **Live caption reliability** — relay is used only for native-helper sources, ffmpeg/AVFoundation falls back to chunked STT, chunked sources complete independently, interim hypotheses are transient, and relay stop/idle stop sends paused state.
- **Session/RAG safety** — active-session deletion stops audio and screen capture, emits paused state, and RAG indexing/reindex/delete is serialized with session-exists checks.
- **Account/session safety** — saved API URLs survive account profile creation/logout, dashboard/deep-link clients use the saved API URL, browser auth refreshes once on `401`, and raw bearer tokens from URL query strings are ignored.
- **Update safety** — signed manifests are verified before trust, release builds require an embedded updater pubkey, verified updates do not inherit checksum-skip, and manual web deploys preserve `latest.json.sig`.
- **Web/overlay review items** — reload CTA points to `/reload`, short landing viewports scroll, usage labels are DOM-rendered safely, privacy text reflects account-file token storage, and drag/drop attachments are accepted from idle.

## Still Dependent On You / Operator Inputs

1. **Provider funding and spend caps**: fund OpenAI, Anthropic, Deepgram, and optional Gemini/Google accounts enough for live tests, then set account-level caps around the test budget.
2. **Mac permission prompts**: grant Microphone, Screen Recording, Accessibility, and any native audio helper permissions on the test Mac. I can run commands, but macOS privacy prompts usually need you to approve them.
3. **Live payment smoke**: run one real low-dollar Square reload and verify `/billing/square/webhook` credits the right account, then confirm the failed-delivery warning disappears.
4. **Release signing key for publishing**: provide/use `BLUEY_RELEASE_SIGNING_KEY_FILE` when publishing a new release artifact so `latest.json` and `latest.json.sig` are emitted together.
5. **Final visual/live smoke**: after the above, run the visible overlay flow: `bluey on`, login/link, Listen, real mic/system captions, Answer, Screen, attach docs, saved session restore, and balance decrement.

## Future Nonblocking Cleanup

- Auto-renew live STT relay sessions before the server session limit; the current branch safely stops and pauses when a relay finishes.
- Expose provider-neutral non-text stream activity for deep/thinking routes; the current branch uses a safer longer deadline.
- Split the monolithic web page into route components once the launch UI is stable.
- Continue daemon decoupling beyond the first overlay-state/RAG-indexing slice.

## What To Tell Kiro

Please review the current tip against this closure note. The previous “dual-source STT reservation is still open” statement is superseded by `b93882b` and `docs/rounds/STT-RESERVATION-AND-DECOUPLING-FOR-KIRO-REVIEW.md`. I do not see a remaining non-operator-dependent code blocker; next useful review is live-smoke behavior once provider funding, macOS permissions, payment smoke, and release-signing inputs are in place.
