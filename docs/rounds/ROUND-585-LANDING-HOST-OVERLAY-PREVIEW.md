# Round 585 - Landing Host Overlay Preview

## Goal

Remove the oversized landing-page product mockup that had drifted away from the
shipped Bluey host overlay and replace it with a compact, recognizable preview
of the real product surface.

## Changes

- Replaced the framed context-card mockup with the host overlay's wide,
  landscape composition.
- Matched the shipped toolbar hierarchy: session controls, Bluey identity,
  readiness, help, theme, balance, window controls, and privacy controls.
- Recreated the answer workspace, live-caption rail, Ask composer, Listen and
  Answer actions, Tone, opacity, Auto-send, mode, and Screen controls.
- Removed the fake context chips, question card, answer card, listening footer,
  marketing caption, and light-blue outer frame.
- Kept the preview dark in both site themes so it continues to resemble the
  native overlay instead of becoming a second light-theme product design.
- Added responsive behavior that preserves the core product identity and
  composer on small screens while hiding lower-priority toolbar controls.
- Bumped the landing stylesheet cache key.

## Scope

Only the web landing page and shared landing stylesheet changed. The macOS
overlay source was inspected as the visual reference and was not edited. No
native overlay, audio, daemon, server, billing, or Jobs runtime file changed.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- Desktop visual check at 1440 x 900 in dark and light themes
- Mobile visual check at 390 x 844 in dark and light themes
- Confirmed no horizontal overflow at either viewport
- Confirmed the removed mockup classes no longer appear in landing markup

No production deployment was performed in this round.
