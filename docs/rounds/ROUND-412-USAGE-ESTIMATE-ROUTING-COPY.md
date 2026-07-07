# Round 412 - Usage Estimate Routing Copy

## Scope

- Continued Bluey web UI work on `codex/bluey-web-ui-parallel-20260704`.
- Focused on dashboard Usage Summary, balance/Auto Reload copy, public FAQ/router copy, and Terms/Privacy estimate language.
- Stayed in web/static/docs files only.

## Changes

- Removed the fake-precise `$15 -> ~682 answers` fallback from the account dashboard.
- Replaced answer-count estimates with a conservative light mixed-use range (`~2-5 hrs` for a $15 balance) and recent-pace estimates when enough account usage exists.
- Added an `Auto routing` explanation in Usage Summary so users understand Bluey chooses a route by task/context and usage varies.
- Clarified Auto Reload copy as `adds $X when balance is below $Y`.
- Updated the billing/status copy to avoid internal implementation phrasing and keep the customer model as shared account balance plus optional Auto Reload.
- Updated Terms, Privacy, FAQ, auto-router, and `llms.txt` copy to say usage estimates are approximate because transcript, screen, files, saved context, and answer depth affect balance use.
- Bumped static asset cache keys to `2026070714`.

## Verification

- `node --check web/assets/bluey-site.js`
- `rg -n "682|~680|normal answers|normal text|normal questions|Bluey credit|Square handles|confirmed payment|payment is confirmed|Reloads|reloads" web/assets/bluey-site.js web/index.html web/bluey-faq/index.html web/auto-model-router/index.html web/llms.txt`

## Notes

- This round intentionally avoids backend/runtime changes. The payment and Auto Reload mechanics stay as implemented in the prior in-app balance payment round.
- The range is deliberately approximate and conservative: light text-only use can go further, while audio, screen, files, saved-session search, and deeper routing use balance faster.
