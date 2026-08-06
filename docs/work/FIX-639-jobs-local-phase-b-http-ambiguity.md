# FIX-639: Local Phase-B HTTP 5xx was treated as a safe denial

> **Codex preflight:** Loaded `$bluey-ops` and verified this defect against the
> current Round 604 worktree only. No SSD/archive or external environment was
> used.

## Issue

The local Browser mapped every non-success Phase-B response to
`launch_expired`. A gateway or proxy can return 500, 502, 503, or 504 after the
server has already committed the one-use binding and canary reservation, so the
client could discard its page and expose a retry-oriented path.

## Root Cause

The client treated HTTP status alone as proof that the server transaction had
not committed.

## Fix Summary

- Retained bounded 4xx responses as explicit pre-marker denials.
- Classified HTTP 5xx and all other non-4xx failures as
  `submit_outcome_unknown`.
- Added 500/502/503/504 coverage proving no local marker or employer click,
  preserved recovery state, and no retry authority.
- Corrected the earlier FIX-620 wording to match the fail-closed behavior.

## Production Impact

No local Browser distribution flag, tenant, provider, runner, or deployment was
enabled.
