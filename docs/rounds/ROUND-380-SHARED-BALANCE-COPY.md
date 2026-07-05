# Round 380 - Shared Balance Copy

## Trigger

Owner pointed out that the dashboard copy said "Web balance is for this browser account," which was misleading. Bluey's balance is account-scoped: the same login across web browsers and linked desktops spends from the same wallet.

## Change

- Reworded the desktop-connect panel to explain that balance is shared by the signed-in Bluey account across web logins and linked desktops.
- Reworded the top balance hint to focus on the real mismatch case: a desktop overlay using a different account.
- Reworded the landing connect helper to say a desktop code connects the desktop to this same account balance.

## Verification

- `node --check web/assets/bluey-site.js` passed.
- `rg` confirmed the old "Web balance" / "browser account" wording is gone from the web UI.

