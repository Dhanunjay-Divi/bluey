# Overlay CSS → Swift — exact value mapping (no guessing)

> Source of truth: `docs/design/bluey-overlay-redesign-v2.html`.
> Every Swift color/alpha/gradient below is the mechanical translation of a CSS
> value. NSColor RGB are 0–1 (css/255). The build is translation, not eyeballing.

## CRITICAL: why the overlay differs from the mockup math
In the HTML, the panel's `--glass: rgba(17,20,26,.52)` blurs a **dark aurora desk**
behind it (`.desk` gradient). The overlay sits over the **real desktop** (maybe a
BRIGHT wallpaper) — so a `.52` translucent fill shows the bright wallpaper through
and washes out (the bug just seen). **Fix:** the panel must paint its OWN dark
aurora so it reads like the dashboard regardless of what's behind. So:
1. Keep the `NSVisualEffectView` blur (depth) BUT set it to a **dark** material
   (`.underWindowBackground` or tint it) so it darkens rather than clears.
2. Paint a **denser dark base** on the panel root: deep `#0B0D10`-ish at a HIGH
   alpha (~0.86, not 0.52) so it's a dark panel, lightly translucent — like the
   dashboard, not see-through glass.
3. Layer a **subtle aurora gradient** (CAGradientLayer, low-alpha blue/violet/teal)
   over the base, matching `--aurora`.

## TOKEN MAP (css → NSColor)
| token | css | NSColor(r,g,b,a) |
|---|---|---|
| text primary `--tx-1` | rgba(255,255,255,.96) | `.white α0.96` |
| text secondary `--tx-2` | rgba(255,255,255,.62) | `.white α0.62` |
| text tertiary `--tx-3` | rgba(255,255,255,.40) | `.white α0.40` |
| text quaternary `--tx-4` | rgba(255,255,255,.24) | `.white α0.24` |
| hairline | rgba(255,255,255,.10) | `.white α0.10` |
| hairline-strong | rgba(255,255,255,.17) | `.white α0.17` |
| accent | #3B82F6 | `(0.231, 0.510, 0.965, 1)` |
| accent-tx | #AFC9FB | `(0.686, 0.788, 0.984, 1)` |
| accent-bg | rgba(59,130,246,.18) | `(0.231,0.510,0.965, 0.18)` |
| accent-bg-s | rgba(59,130,246,.30) | `(0.231,0.510,0.965, 0.30)` |
| ok | #3BD17B | `(0.231, 0.820, 0.482, 1)` |
| warn | #E3A93F | `(0.890, 0.663, 0.247, 1)` |
| danger | #E5564E | `(0.898, 0.337, 0.306, 1)` |
| glass-hi (card/inset fill) | rgba(255,255,255,.05) | `.white α0.05` |
| radii | r-md 11, r-lg 14, r-xl 18, r-2xl 24 | cornerRadius 11/14/18/24 |

## PANEL SURFACE (the part I kept getting wrong)
**Panel root `ExpandedPanelView` layer** — DARK aurora base, NOT translucent-.52:
- base fill: `NSColor(0.043, 0.051, 0.063, alpha: 0.88)` (deep `#0B0D10`-ish, alpha
  HIGH so it's a dark panel; the blur view adds depth, the wallpaper does NOT
  bleed through to wash it out). `cornerRadius = 24`.
- **glassBackdrop NSVisualEffectView:** material `.underWindowBackground` (DARK,
  not `.hudWindow` which is light/clear), `blendingMode = .behindWindow`,
  `state = .active`. This blurs+darkens what's behind for depth without clearing.
- **aurora layer (NEW CAGradientLayer over the base, under content):** 3–4 radial-ish
  tints, very low alpha, from `--aurora`:
  - blue `(0.227,0.424,0.824, α0.16)` top-left
  - violet `(0.353,0.275,0.784, α0.12)` bottom-left
  - teal `(0.157,0.588,0.549, α0.10)` mid
  - over deep base `(0.039,0.047,0.063)`.
  (CAGradientLayer is linear; approximate the radial blobs with an axial blue→violet
  diagonal at α~0.12 — subtle. Good enough; the dashboard aurora is subtle too.)
- top highlight `--top-hi inset 0 1px 0 rgba(255,255,255,.09)`: a 1px top hairline
  sublayer at `.white α0.09`.
- shadow `--shadow 0 28px 80px rgba(0,0,0,.55)`: window/panel shadow black α0.55, r40.

**Header** — flush, transparent over the panel (DONE): clear bg + 1px bottom hairline
`.white α0.10`. Brand: dot + "Bluey" `.white α0.96` 13.5 semibold + "· managed"
`.white α0.62` 11.5. Tabs = segmented (accent-bg selection). ✓ mostly done.

**Feed** — TRANSPARENT (DONE): no fill, cards float on the panel's dark aurora.
- YOU bubble: `glass-hi` `.white α0.05` fill, hairline border, radius 14, text `tx-1`.
- BLUEY answer: plain text `tx-1` (`.white α0.96`) — readable on the dark panel. Label
  "BLUEY" accent-tx 10.5 bold.
- **Readability now works** because the PANEL is dark (α0.88), not clear.

**Composer** — inset glass field (DONE-ish): `composerBar`/`composerSurface` = `glass-hi`
`.white α0.05`, hairline border, radius 14. "+" left, field, Listen, send (accent).
Footer meta `tx-3` 10.5.

**Modals** — `--glass-2 rgba(26,30,38,.58)` but same reasoning → make denser:
`NSColor(0.10,0.115,0.14, α0.92)` (dark, readable, slight translucency), radius 18,
hairline-strong border, over a `bg-black α0.42` dim backdrop.

## THE FIX IN ONE LINE
The panel is a **dark aurora panel that is lightly translucent**, NOT a clear window.
Base alpha ~0.86–0.88 (dark), aurora tint layer, dark blur material. Everything
readable because the surface is dark. This matches the dashboard + the HTML.

## REMAINING LAYOUT (after surface is right) — the feed → timeline thread
Per mockup Stage 2: each turn = a rail icon (heard/screen/you/bluey) + label + body,
connected by a 1px timeline line. HEARD (italic tx-2), SCREEN (capture chip + answer),
YOU (tx-1), BLUEY (tx-1 + accent label). Context bar above composer. This is the
FeedView card→turn rendering change — separate step after the surface reads correct.

## BUILD ORDER (one pass, compile, ONE launch)
1. Panel surface: dark base α0.88 + dark blur material + aurora CAGradientLayer +
   top hairline. (the fix)
2. Confirm feed/composer/modals use the exact token fills above.
3. Compile. Launch ONCE. User confirms surface = dashboard dark-aurora + readable.
4. THEN feed→timeline thread (separate pass).
