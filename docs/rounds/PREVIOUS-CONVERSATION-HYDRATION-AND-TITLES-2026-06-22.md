# Previous conversation hydration and contextual titles

## Why

Clicking a saved recording in the History drawer updated daemon state but did not rebuild the visible chat surface. The overlay only received context/session metadata plus a small "Session loaded" card, so the conversation UI looked blank or stale.

The drawer and main feed also had fragile scroll constraints, so larger histories could feel stuck or fight manual scrolling.

## Change

- Rehydrate selected recordings into the overlay by clearing the current feed and replaying saved question/answer turns.
- If a recording has no saved Q&A, show a bounded transcript card or a clear empty-session message.
- Rehydrate saved history when the overlay reconnects while an active session already has saved conversation/transcript content.
- Generate short contextual titles for generic recordings from the first useful transcript, question, or attached document name.
  - Example: "Hi. So I wanna know about DDoS attacks." becomes `DDoS Attacks`.
- Existing saved recordings with generic names display and persist a contextual name when listed or reopened.
- Keep user-renamed recordings unchanged.
- Make the main conversation feed and History drawer use flipped document stacks with explicit scroll-content constraints.
- Track manual feed scrolling so new incoming content does not constantly pull the user away from older messages.

## Verification

- Swift release overlay build.
- Focused daemon tests for contextual titles and history card replay.
- `cargo fmt --all -- --check`.
- `git diff --check`.
