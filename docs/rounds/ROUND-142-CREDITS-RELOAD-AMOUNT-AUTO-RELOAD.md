# Round 142 - Credits Reload Amount And Auto Reload

## Why

The credits card should feel like a simple payment profile instead of a fixed checkout link. Users should see the recommended `$30` reload, be able to choose a custom amount, and never accidentally send a checkout below Bluey's `$15` minimum.

Auto Reload should also start from the useful default rule: reload `$30` when the balance drops below `$10`.

## Changed

- Manual Add credits now reads the dashboard amount input instead of always sending a fixed value.
- The browser blocks manual reloads below `$15` before calling billing.
- The manual reload card shows `$30` as the starting amount and keeps checkout in a new tab.
- Auto Reload defaults to below `$10`, add `$30`.
- Off accounts with the old `$5` / `$15` default display the new recommended `$10` / `$30` rule.
- New accounts explicitly store the new Auto Reload defaults for both SQLite and Postgres-backed signups.

## Verified

- `node --check web/assets/bluey-site.js`
- `cargo test api::billing --lib` from `server/`
