# FIX880 — Public Jobs content depends on application JavaScript

Preflight `bluey-ops`; Round645. The owner reports bluey.sh/jobs loading slowly.
Production HTTP sampling did not reproduce a long server wait. An empty public
root and eager current-code App/API/taxonomy imports are confirmed weaknesses,
not proof of the complete production root cause or deployed source identity.

Render the existing AuthGate into generated HTML during the existing build and
separate public startup from authenticated/preview App loading. Preserve all
login, storage precedence, routes, server admission and execution authority.
Public content remains usable when JavaScript or storage is unavailable. Lazy
application failures need visible recovery rather than a blank page.

The same-environment retained Phase644 baseline is715 HTML bytes (398 gzip),
487,828 entry-JS bytes (145,772 gzip), and134,041 CSS bytes (21,576 gzip), using
Node22 gzip level9/mtime0. The HTML contains no hero. This is not a production
LCP measurement or whole-import-graph size. Final successor gate91375 passed2548
unique cases, types/builds/deterministic output/audit and cleanup. Actual local
Chrome retained readable landing content and anchors while the entry script
returned503; normal anonymous startup made zero account/API or App requests.
Entry JS is255,206 bytes/81,332 gzip, about44% smaller for that script; HTML now
contains the public content (15,620 bytes/5,473 gzip). These are output and local
behavior results, not a measured production speedup. IMPL645 has exact evidence
hashes, superseded failure, browser limitations and remaining release gates.

No production deployment, security-protection reduction or competitor private
session inspection is needed or authorized by this correction.
