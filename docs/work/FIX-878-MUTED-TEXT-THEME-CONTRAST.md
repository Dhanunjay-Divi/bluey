# FIX-878: Muted text theme contrast

Preflight `bluey-ops`; Round644. New sidebar secondary copy reused light muted
`#78858c`, which is insufficient for normal small text on white. Muted tokens
now use `#5d6870` in light and `#8f9ca3` in dark. Worker-computed pair ratios:
5.71 on white/5.45 on light surface2;7.12 on dark background/6.87 on dark surface.

Accent and filled-button tokens were left unchanged to avoid changing label/fill
contrast inadvertently. Actual composed-theme/browser checks remain pending;
the arithmetic does not qualify every existing screen or style combination.
