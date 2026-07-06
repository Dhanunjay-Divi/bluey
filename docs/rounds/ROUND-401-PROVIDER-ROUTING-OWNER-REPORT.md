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

Not deployed at the moment this doc was written. This is a backend-only admin endpoint and should be included in the next API deploy.

## Remaining

- Add route-attempt persistence for first-candidate versus final-provider reporting.
- Add a small owner dashboard/table that calls `/admin/provider-routing`.
- Keep provider keys server-side only; this report should never expose secrets or user content.
