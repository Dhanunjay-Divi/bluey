# Round 108 - Canvas Trigger And Timing Polish - 2026-06-22

## Why This Round Exists

The live overlay misclassified a normal AWS VPC explanation as code work. That
created a side canvas, left the chat saying `Code is in the canvas`, and showed
the raw total request time (`7600 ms`) as if it were answer-start latency.

## Changes

- Tightened automatic code-canvas detection in the daemon and macOS overlay.
  Public/private networking words no longer count as code signals.
- Removed the generic long-answer workspace fallback. Bluey now keeps ordinary
  explanations in chat unless the answer is clearly code, system design, screen
  analysis, or document-focused.
- Kept the canvas toggle button visible whenever canvases exist, even after the
  user collapses the pane.
- Added first-answer timing capture to `OverlayAnswerStream`.
- Changed the badge text to show `started in ...` from first visible answer
  text, falling back to `finished in ...` only when start timing is unavailable.
- Removed visible input-token counts from the overlay badge. Customers now see
  answer/output tokens only, while internal billing still keeps full usage.

## UX Contract

- Normal explanations should not open canvas.
- Real coding answers should still open canvas when code blocks or strong coding
  intent are present.
- System-design answers should still open the architecture canvas.
- Closing canvas should leave a visible reopen button in the header.
- The time shown in the answer metadata should describe when the answer started
  streaming, not how long the whole answer took.
- The token count shown in the overlay should describe the visible answer only,
  not hidden prompt/context input.

## Verification

- Focused daemon tests cover:
  - first-answer timing label over total latency,
  - total-duration fallback wording,
  - fenced-code canvas detection,
  - system-design canvas detection,
  - AWS VPC explanation not becoming a code artifact.
- macOS overlay build should pass after the header/canvas trigger changes.

## Follow-Ups

- Add a UI smoke that asks a known non-code cloud/networking question and
  verifies the canvas button stays hidden.
- Add a UI smoke that asks a fenced-code question and verifies the canvas button
  remains available after collapsing the pane.
