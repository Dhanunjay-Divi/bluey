# ROUND-441 Session Migration Future Note

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Preserve the decision that local session migration should stay explicit and safe, but leave overlay/web migration UI for a future revisit.

## Decision Saved

Migration remains explicit and CLI-only for now:

```bash
bluey sessions --move-local-to-current-account --confirm-move-local-sessions
```

This should not be automatic during login, logout, account deletion, or desktop relink.

## Product Notes

- Old cloud chats stay with the old account.
- A newly linked account should not see previous-account history.
- Moving a desktop changes the desktop link and billing identity only, not chat ownership.
- Local cached sessions should be tagged with the account that created or synced them.
- Session IDs should become visible and searchable in overlay History and web Session History.
- A future overlay/web migration prompt can be considered later, but it must require explicit consent, explain the account-history impact, and avoid accidental migration.

## Files Updated

- `docs/SESSION-FLOW.md`

## Deploy Status

Not deployed. Documentation-only note.
