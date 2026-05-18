# Bluey UX/UI Product Direction

This is the working design brief for Bluey's overlay and customer-facing UI.
Use it before changing layout, icons, copy, or interaction behavior.

## Reference Lessons

Reviewed local references:

- Pinky: compact native pill, visible heartbeat/status dot, capture-excluded
  native overlay, split between interactive chrome and readable/click-through
  content, scroll history, explicit close/stop semantics.
- Natively: polished chat overlay, strong markdown/code rendering, streaming
  cursor, copy affordances, meeting context folded into follow-up chat.
- Pluely: sticky bottom composer, conversation history rhythm, clean attach and
  audio controls, screenshot selection overlay with clear cancel path.
- Aura/OpenCluely: markdown streaming, code-block headers/copy controls, visible
  recording/interaction state, clear error/status messaging.

Bluey should borrow the useful interaction patterns while keeping its own
identity: tiny native command layer, strong cyan/black visual language, source
labeled audio, answer modes, and managed auto routing.

## Design Position

Bluey should feel like:

- a command layer, not a website inside a floating box;
- dense enough for live work, but not cluttered;
- calm, premium, and readable at low opacity;
- fast to understand without onboarding text;
- reliable when resized, hidden, moved, or left running for hours.

The visual signature is **black glass + Bluey cyan + compact native chrome**.
Avoid one-note blue slabs, oversized marketing panels, and text-heavy controls.

## UI Architecture

### Collapsed Pill

Purpose: always-available presence without blocking work.

Requirements:

- Compact, movable, click-to-open.
- Bluey mark, status dot, concise label.
- Green dot when connected/listening-ready; warning color for degraded state.
- Strong enough border/glow to see on light and dark backgrounds.
- No text truncation at default size.

### Expanded Overlay

Purpose: answer feed + command composer.

Requirements:

- Top status strip: product identity, current route/mode/session status, close.
- Middle feed: chronological source-labeled cards.
- Bottom composer: primary place to ask, answer, attach, set rules, and recap.
- Header/composer/resize are interactive; readable feed can remain click-through
  where native implementation supports it.
- Resize should preserve usable proportions; controls must not disappear.

### Card System

Cards should be visually distinct by role:

- `YOU`: typed/user question.
- `BLUEY`: streamed answer.
- `TRANSCRIPT`: source-labeled system/mic transcript.
- `CONTEXT`: file, page, screenshot, memory.
- `ACTION`: action item.
- `DECISION`: decision.
- `WARNING`: permission/provider/capture issue.

Every card needs:

- compact role badge;
- readable title;
- body text with stable wrapping;
- streaming state when not final;
- enough contrast even when background opacity is low.

### Composer

Requirements:

- Placeholder explains capability without long instruction text.
- Primary action is `Answer`, not a generic send-only mental model.
- Secondary actions are short and icon-led: Attach, Rules, Recap.
- Enter submits when focused; empty Answer should answer the latest clear
  question/context.
- If provider/context is missing, show a warning card in the feed instead of
  only printing terminal output.

## Required States To Design And Test

Before calling the UI production-ready, test these states:

1. First launch: pill only, boot card buffered, no session confusion.
2. Click to open: panel appears below pill without covering the pill.
3. Drag pill: expanded panel follows the pill on next open.
4. Resize expanded panel: controls stay visible and cards reflow.
5. Hide/collapse: only pill remains; passive updates do not reopen the panel.
6. Close/quit: user understands Bluey stops only through confirmed quit or
   `bluey off`.
7. Mic off/on: recording state and source labels are obvious.
8. System transcript + mic transcript streaming together.
9. Typed question -> `YOU` card -> streamed `BLUEY` card.
10. Coding answer with fenced code, long lines, and explanation.
11. System design answer with sections and compact diagrams.
12. Analyse Screen succeeds from browser page text.
13. Analyse Screen falls back to screenshot/vision or emits a clear warning.
14. Attach files succeeds, fails, or rejects unsupported files.
15. Rules/instructions are saved, cleared, and visibly acknowledged.
16. Provider missing/key missing/offline.
17. Long session with 100+ cards.
18. Low opacity on light and dark backgrounds.
19. Small panel size and large panel size.
20. Keyboard-only basics: focus composer, submit, escape/collapse.

## Current Implementation Status

Implemented in the macOS native overlay:

- compact pill-first launch;
- dark Bluey pill with logo, status dot, border, and glow;
- smaller pill footprint so it feels like a command layer, not a floating
  toolbar;
- launch flow with `New` and `Continue` session actions before the user starts
  working;
- expanded dark panel with top status strip, feed, composer, and action row;
- role-labeled cards with accent rails and streaming status;
- icon-led actions for Answer, Analyse, Attach, Rules, Recap, and close;
- capture-excluded native windows and daemon-tokenized IPC.

Still needed:

- full recent-session picker, not only latest-session continue;
- copy button on final answer/code cards;
- richer markdown/code rendering in the native Swift feed;
- visible provider/model/mode picker in the current native overlay source;
- attachment drawer and attached-context list;
- audio/provider/cloud health chips;
- warning-card patterns for every permission/provider failure;
- Windows parity after Windows becomes a supported target;
- clean visual QA screenshots/video on real displays.

## Next UI Slice

Recommended next implementation round:

1. Native markdown renderer for card bodies:
   - headings, bullets, inline code, fenced code blocks;
   - code block header with language and copy button.
2. Status chips:
   - route/mode, mic/system state, provider health, context count.
3. Attachment drawer:
   - attached files, page captures, screenshot captures, status/error.
4. Empty/feed states:
   - first-run hint card, no transcript yet, provider missing, offline fallback.
5. Visual regression harness:
   - scripted boot/push/update commands into the overlay;
   - capture-exclusion-aware smoke plus non-excluded debug render path for QA.

## Non-Negotiables

- Do not make the overlay unreadable to gain transparency.
- Do not let text or icons clip in default or resized states.
- Do not add controls without a clear state and error path.
- Do not make daily work depend on terminal commands.
- Keep capture and screen context user-controlled and visible.
