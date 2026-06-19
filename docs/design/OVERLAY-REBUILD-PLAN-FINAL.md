# Overlay panel — build EXACTLY to the mockup (final plan)

> Build the native panel to match `bluey-overlay-redesign-v2.html` literally —
> element tree + geometry + color, taken from the CSS. NOT patch the old layout;
> rebuild `ExpandedPanelView`'s view construction fresh to this spec. Backend
> stays wired (it already works). One pass, then ONE launch.

## SURFACE — the thing that kept washing out. SOLVED:
The HTML panel is NOT system vibrancy. It's a PAINTED dark fill:
`.panel { background: rgba(17,20,26,.52) }` over a dark aurora desk.
**So: NO NSVisualEffectView defining the color.** Paint the surface directly:
- Panel root layer = SOLID dark: `NSColor(0.066, 0.078, 0.102, alpha: 0.97)`
  (deep, essentially opaque-dark so it reads dark over ANY wallpaper — the mockup
  only looks .52 because it's over a dark desk; over a bright desktop we need it
  dark, so high alpha). cornerRadius 24, masksToBounds.
- aurora tint layer on top of the dark fill: CAGradientLayer, blue→teal→violet,
  alpha 0.14/0.10/0.13 (subtle). Same as before.
- top hairline: `.white α0.09`, 1px, at the top edge.
- OPTIONAL faint blur view BEHIND the dark fill for edge depth — but it must NOT
  define color (force darkAqua + keep the dark fill on top at α0.97). If it ever
  looks washed out, the dark fill α is the single knob → raise toward 1.0.
- shadow: black α0.55, radius 40 (window-level).

## PANEL GEOMETRY (css → Swift, literal)
`.panel`: 560w × 700h, radius 24, flex column, overflow hidden. (overlay sizes to
its window; keep current min/max.)

### HEADER `.ph` (flush, transparent, hairline-bottom) — padding 13×14, gap 10:
`[dot 7px ok] [nm "Bluey" 13 semibold tx-1] [via "· managed" 11 tx-3] [spacer] [seg] [x 26px]`
- seg `.ph .seg`: bg `.white α0.05`, border hairline, radius 10, pad 2.
  buttons: 11.5 semibold, pad 4×12, radius 8; selected = accent-bg fill + accent-tx.
  → 3 segments: Ask · History · Agents. (this is the bodyTabControl — restyle to this.)
- x: 26px, radius 8, tx-3, hover `.white α0.06`.
- border-bottom: 1px hairline.

### FEED `.feed` — padding 16/16/8, the TIMELINE THREAD `.turn`s:
each `.turn`: padding-left 26, margin-bottom 15, with:
- `.rail` 16×16 radius 5 at left:8 top:3 — icon by kind:
  heard=`.white α0.07`/tx-3, screen=accent-bg/accent-tx, you=glass-hi/tx-2, bluey=accent-bg/accent-tx.
- vertical connector line: 1px hairline from rail bottom to next turn (`.turn::before`).
- `.lab` 10px 700 letterspaced: who-label (heard2=tx-3, who2=accent-tx) + `.t` time (tx-4, right).
- body by kind:
  - HEARD: `.heardtxt` 12.5 tx-2 italic.
  - SCREEN: `.scap` chip (glass-hi, hairline, radius 9) [thumb 46×30] + label, THEN `.ans2`.
  - YOU: `.ask2` 13 tx-1.
  - BLUEY: `.ans2` 13.5 tx-1 (bold spans 600, code mono).
**Map the 9 card kinds → turn types:** transcript→HEARD, question→YOU, answer→BLUEY,
screen-answer→SCREEN, others (context/action_item/decision/warning/fix)→their own turn.
This is the FeedView render change (cards become turns).

### CONTEXT BAR `.ctxbar` (above composer): accent-tinted, "In context: transcript · N screen · N turns".

### COMPOSER `.comp` — margin 8×14, min-h 44, glass-hi fill, hairline, radius 14:
`[plus 32px radius10] [field "Ask…" 13 tx-3] [listen 11.5 tx-3] [send 32px accent radius10]`

### FOOTER `.foot` — padding 0/16/13: meta chips (model · connectors) tx-3 10.5, right = keys.

## TABS — Ask / History / Agents bodies (already scaffolded):
- Ask = the timeline feed (above).
- History = sessions-at-scale (search + groups + rows + show-more) — restyle to mockup Stage 3.
- Agents = agent cards — restyle to mockup Stage 4.

## BACKEND WIRING (unchanged — already works; just keep the calls):
- feed turns ← PushCard/UpdateCard + transcript_partial/final.
- send → AskRequested. listen → Recording start/stop. +menu items → their events.
- History ← SessionsRequested→SetSessionsPage; pin/open/rename/delete events.
- Agents ← AgentListRequested→SetAgents; attach/detach/sessions/connectors.
- modals: billing (push_billing_disclosure→billing_disclosure_responded), close-confirm, connector sheet.
(Full map in OVERLAY-BUILD-SPEC.md — all verified working.)

## BUILD ORDER (one pass):
1. SURFACE: painted dark fill α0.97 + aurora + top-hi + faint blur behind (no vibrancy color). ← fixes washout for good.
2. HEADER: rebuild to `.ph` spec (already mostly done — verify dot/Bluey/seg/x + hairline).
3. COMPOSER: rebuild to `.comp` spec (+ plus, field, listen, send) + footer.
4. FEED: cards → timeline turns (`.turn` rendering) + context bar.
5. Tabs (History/Agents) restyle to Stage 3/4.
6. Compile, launch ONCE. User confirms = mockup.

## THE ONE KNOB if still washed out
The dark-fill alpha (step 1). Mockup-over-dark-desk = .52; overlay-over-any-desktop
needs ~0.95–0.99 to read dark. If it looks light, RAISE this toward 1.0. That's the
only variable for "dark vs washed-out".
