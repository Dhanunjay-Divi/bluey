# Round 401 - Provider Routing Owner Report

Date: 2026-07-06

## Goal

Give Bluey an owner/operator view for real provider routing behavior so we can answer questions like:

- Which provider/model is actually being used?
- Are fallbacks happening?
- Which lanes are slow?
- What did customers get charged versus what Bluey estimates as upstream cost?

This was added after the Round 400 provider-routing posture review, where we confirmed `provider_mix` is active but the owner did not yet have a clean report for real usage distribution.

## Implemented

- Added backend aggregation in `server/src/db/usage.rs`.
- Added admin endpoint:

```text
GET /admin/provider-routing?hours=24
```

- The report returns:
  - total LLM usage events in the window
  - fallback event count and fallback rate
  - average latency
  - input/output token totals
  - customer charge cents
  - Bluey upstream-cost estimate cents
  - grouped distribution by provider/model/lane
  - grouped distribution by lane
  - grouped distribution by task type

## Privacy Boundary

The report intentionally does not return:

- prompts
- transcript text
- document text
- screenshots
- generated answer text

It is an aggregate operational report only.

## Current Limitation

This round reports the final provider/model recorded in `usage_events`.

It does not yet show the full attempted route chain, such as:

```text
planned lane -> first candidate -> provider failure/cooldown/429 -> fallback provider
```

For that, Bluey needs a small follow-up persistence layer, likely `route_attempt_events` or structured route metadata on `usage_events`.

## Verification

Passed:

```text
cargo test --manifest-path server/Cargo.toml provider_routing_summary_groups_final_provider_usage_without_content --quiet
cargo check --manifest-path server/Cargo.toml --quiet
git diff --check
```

## Deployment Status

Deployed to the production API droplet on 2026-07-06.

Deploy proof:

- Commit: `ba093252ba6426cae98fe743433cb5888586fb39`
- Source was uploaded as a git archive and built on the droplet as Linux x86_64.
- Fresh Postgres backup before swap:

```text
/var/backups/bluey-api/hourly/bluey-postgres-20260706T063718Z.pgdump
```

- Installed binary SHA-256:

```text
dfc6dd42f3e38b355af92fcf2cddc339f6699494c293091e4b9b085cca87d7e7
```

- Previous binary backup:

```text
/var/backups/bluey-api/bin/bluey-server.previous-20260706T064203Z
```

- `bluey-api.service`: active
- `NRestarts`: 0
- `/health` reports commit `ba093252ba6426cae98fe743433cb5888586fb39`
- Unauthenticated `/admin/provider-routing?hours=24` returns `401`.
- Admin-authenticated smoke returned `200` with aggregate rows.
- `journalctl -u bluey-api.service --since "10 min ago" -p warning` showed no entries.

Live 24-hour admin smoke summary immediately after deploy:

```json
{
  "total_events": 19,
  "fallback_events": 3,
  "provider_rows": 6,
  "lane_rows": 3,
  "task_rows": 5
}
```

## Remaining

- Add route-attempt persistence for first-candidate versus final-provider reporting.
- Add a small owner dashboard/table that calls `/admin/provider-routing`.
- Keep provider keys server-side only; this report should never expose secrets or user content.
