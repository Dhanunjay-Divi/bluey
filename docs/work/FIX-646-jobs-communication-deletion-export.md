# FIX-646: Communication Actions Were Outside Deletion And Export Fences

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Account deletion did not wait for dispatching or unknown communication actions,
mailbox disconnect could cascade-delete unresolved history, and account export
omitted provider messages and reviewed actions.

## Root Cause

The communication tables landed after the existing account lifecycle and were
not included in its active-work, disconnect, or export inventories.

## Fix Summary

Serialize new side effects against deletion, prevent disconnect from destroying
ambiguous authority, drain or reconcile unresolved work conservatively, and add
sanitized customer-visible communication history to export without credentials,
lease secrets, worker identities, or raw provider errors.

## Files Modified

| File | Change |
|------|--------|
| Account deletion/export DB modules | Active-work fence and sanitized export |
| Mailbox disconnect DB/API | Conservative tombstone/refusal behavior |
| Lifecycle tests | Race, purge, and privacy coverage |

## Edge Cases Handled

- Deletion wins before claim; claim wins before deletion; disconnected unknown;
  terminal cleanup; cross-account export; encrypted private evidence.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml deletion --quiet
cargo test --manifest-path server/Cargo.toml communication_ --quiet
```

## Known Limitations

- Production retention duration still requires owner/data-rights approval.
