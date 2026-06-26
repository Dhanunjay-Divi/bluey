# Round 034 - Bluey/Pinky parity web cleanup - 2026-06-07

## Goal

Make the Bluey web shell closer to Pinky's useful patterns without copying
Pinky controls that do not yet have a real Bluey action. Keep the public site,
download page, account flow, and docs visually consistent and easier to maintain.

## Completed in this pass

- Removed the human/profile icon from Bluey navs because it only duplicated the
  Dashboard link. In Pinky the icon is valid because it opens a real account
  menu; in Bluey it was decorative until that menu exists.
- Added a real signed-in Sign out action across product, download, docs, and
  account navs. Signed-out users see Login; signed-in users see Dashboard and
  Sign out.
- Simplified `syncAccountNav()` so it only owns guest/auth visibility. Sign-out
  is handled through a single global `[data-sign-out]` click path.
- Reduced `web/assets/bluey-site.css` from roughly 6.9k lines to roughly 2.4k
  lines by removing old stacked design generations and keeping the current live
  product/account/download/docs shell.
- Kept Product text links on non-home pages. The logo also goes home, but the
  explicit Product link is clearer from account, download, and legal pages.
- Preserved the existing account form IDs, billing IDs, session IDs, and
  download copy buttons so server/desktop wiring keeps working.
- Kept the small `file://` route-preview shim for agents opening
  `web/index.html` directly, but removed duplicate fallback asset tags so
  deployed HTTP pages load each CSS/JS asset once under SRI.
- Updated the example Caddy CSP to keep `unsafe-inline` out while allowing the
  external static assets needed by production.

## Verification

- `node --check web/assets/bluey-site.js`
- Local route-aware server smoke for `/`, `/download`, `/login`, `/account`,
  `/docs/privacy`, `/docs/terms`, and `/docs/disguise`.
- Confirmed the macOS platform card expands and reveals terminal instructions.
- Confirmed mobile width does not introduce horizontal overflow.
- Confirmed the signed-in nav contract statically: four Sign out buttons, three
  public-route buttons gated by `data-auth-only`, and one account-page button.
- Confirmed no `.account-icon` or `data-account-icon` selectors remain.
- Static-verified direct file preview route-link rewrites for
  `?route=/download`, `?route=/login`, and docs routes. Production preview and
  deploy verification should use HTTP, not `file://`, because SRI is attached to
  root-relative assets.
- Confirmed CSS/JS SRI hashes in `web/index.html` match the actual assets and
  `ops/Caddyfile.example` does not reintroduce `unsafe-inline`.

## Notes for the next agent

- Do not re-add the human icon unless you also implement a real account menu
  with useful actions such as Dashboard, billing/credits, password, delete
  account, and Sign out. Otherwise it is worse than the current explicit nav.
- If you add an account menu, mirror Pinky's behavior: the icon should be a
  button, show the signed-in email, and expose real wired actions. Do not make
  it another Dashboard link.
- Keep Bluey's language AI-specific. Pinky's "remote access" and "viewer"
  language should not leak into Bluey except as a visual/layout reference.
- The next high-value Bluey improvement is dashboard depth: add Pinky-like tabs
  for Credits, Saved Sessions, Account/Security, and Settings only when the
  backend endpoints are wired.
- The next structural improvement is splitting `web/index.html` into templates
  or route partials once the server supports it. For now the CSS is the cleaner
  boundary and the DOM IDs must remain stable.
- Prefer previewing with a small route-aware HTTP server from `web/`. If
  previewing from disk, `?route=/download` still rewrites route links, but CSS/JS
  assets are intentionally root-relative for production SRI.
- If `web/assets/bluey-site.js` changes, recompute the SHA-384 hash in
  `web/index.html`.

## Current local caveat

`bluey-dev.db` is still an untracked local database file in the repo root. It
was present before this pass and should not be committed unless another agent
has a very specific reason.
