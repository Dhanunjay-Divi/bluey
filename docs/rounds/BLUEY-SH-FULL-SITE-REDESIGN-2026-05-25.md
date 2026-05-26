# Bluey.sh Full Site Redesign - 2026-05-25

## Scope

Rebuilt `web/index.html` from a mostly account-console focused page into a full public Bluey product site.

The new site is aimed at first-time users who need to understand, quickly:

- Bluey is an on-screen AI answer layer.
- It can use transcript, files, screen context, and session memory.
- It routes tasks across instant, balanced, deep, and vision lanes.
- Credits are wallet-based and visible to the user.
- The account page is still available for reloads, usage, and desktop connection.

## What Changed

- New landing hero: "Bluey - Stay present, stay unseen."
- New product mock showing transcript, routed answer, reasoning, canvas, docs, screen analysis, and composer controls.
- New sections for answer flow, use cases, model routing, credits, and control/security.
- Rebuilt `/account`, `/reload`, and `/link` page shell to match the new dark product language while preserving all existing backend-critical DOM IDs.
- Preserved the current browser account flow, Square checkout hooks, usage dashboard hooks, and sign-out path.
- Added tighter responsive constraints for the hero, product visual, account shell, and text wrapping.

## Verification

Commands run locally:

```bash
node - <<'NODE'
const fs = require('fs');
const html = fs.readFileSync('web/index.html', 'utf8');
for (const match of html.matchAll(/<script>([\s\S]*?)<\/script>/g)) new Function(match[1]);
console.log('html script ok');
NODE

curl -fsS http://127.0.0.1:8900/ | rg -n "Stay present, stay unseen|Bluey turns live context|No provider menu|curl -fsSL"
curl -fsS http://127.0.0.1:8900/account | rg -n "Bluey account|Square checkout|What this unlocks"
git diff --check
```

Visual QA screenshots generated with headless Chrome:

- `/tmp/bluey-site-new-home.png`
- `/tmp/bluey-site-new-account.png`
- `/tmp/bluey-site-new-mobile-v3.png`
- `/tmp/bluey-site-account-mobile-v3.png`

Note: headless Chrome on this Mac reports a 500px layout viewport for the mobile run while outputting a 390px screenshot crop. The CSS now caps mobile containers explicitly, but desktop/browser QA should still be used for final marketing approval.

## Deployment Notes

This is a static web redesign only. No backend endpoints or payment logic changed.

Still needed before live reloads are complete:

- `SQUARE_SANDBOX_LOCATION_ID`
- `SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY`
- `SQUARE_PRODUCTION_LOCATION_ID`
- `SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY`

Those values must be configured on the server side only; never commit them.

## What To Tell The Next Agent

Review `web/index.html` and this doc first. The design was intentionally rebuilt from scratch around the actual Bluey product story. Do not revert to the previous account-console-first layout. Preserve the account DOM IDs because the browser JS and server account/reload flow depend on them.
