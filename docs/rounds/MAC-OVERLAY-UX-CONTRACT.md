# Mac Overlay UX Contract

Date: 2026-05-23
Branch: `feat/phase-3-round-12`

This contract exists because isolated overlay fixes were causing regressions in adjacent areas. Future UI work should preserve the whole product shape below unless the product decision is explicitly changed first.

## Product Shape

Bluey is an always-present AI work surface. The user should be able to:

1. Start or stop listening.
2. Type a direct question.
3. Attach docs or captured context.
4. Define how Bluey should answer for the session.
5. Analyse the screen with consent.
6. Let routing/model choice stay automatic by default.
7. See answers, transcripts, costs, and canvas artifacts without losing the current session.

## Fixed Regions

### Pill

- Compact top-right launcher.
- Shows Bluey identity and status dot.
- Click expands; drag moves.
- No composer controls belong here.

### Header

- Persistent top row inside the expanded panel.
- Owns app/session chrome: recordings drawer, new recording, Bluey/status, canvas toggle, balance, hide-to-pill, close/turn-off.
- Does not own per-question controls.

### Chat Feed

- AI answers render on the left.
- User typed questions render on the right.
- System/session cards may render left, but should not dominate the feed.
- Transcript snippets must not become stacked chat cards during live listening.

### Transcript Strip

- One bounded horizontal strip above the composer.
- It tail-follows recent mic/system transcript.
- It must never resize the overlay or push the composer off-screen.

### Composer

- Multi-line input, capped growth.
- Text box is for user text only.
- Lower-left controls: attach, answer style, listen/stop, opacity.
- Lower-right controls: route/model, screen analysis, send.
- Do not use vague labels such as "Full access". Every visible label must map to a real Bluey action.

### Canvas

- Opens only when the backend returns explicit artifact metadata.
- Used for code, system design, structured plans, diagrams, or long artifacts.
- Collapsible and recoverable without losing chat.

## Backend Contract

The UI should not guess high-value behavior. The backend should return enough metadata for the overlay to make stable decisions:

- `route`: instant, balanced, deep, vision, local-fallback.
- `confidence`: classifier confidence.
- `cost_label`: per-answer cost shown on the response card.
- `artifact_type`: none, code, system_design, plan, document, screen_analysis.
- `artifact_body`: canvas payload when artifact_type is present.
- `artifact_title`: user-visible canvas title.

If the classifier is unsure, the backend should prefer a managed router/server classification step over local UI heuristics.

## Regression Gate

`scripts/macos-overlay-visual-smoke.sh` now checks both runtime behavior and source-level UX contract markers:

- Composer remains a multi-line `ComposerTextView`.
- Listen label stays concrete.
- Attach remains the `+` control.
- Style remains explicit.
- Model routing stays near Screen and Send.
- Composer height stays dynamic but bounded.
- Vague `Full access` / old `Start Bluey` labels cannot re-enter silently.
- Expanded overlay stays fixed height during transcript updates.

