# FIX-576: Jobs Discovery Stops Updating

## Issue

Bluey Jobs continued to show previously imported matches while every configured
discovery source was five or more days behind its normal cadence.

## Root Cause

The production discovery services were coupled to temporary deployment build
directories and API service maintenance:

- `bluey-jobs-discovery.service` was stopped during API maintenance and was not
  restarted;
- `bluey-jobs-global-discovery.service` referenced a deleted temporary build
  directory;
- both units used `Restart=on-failure`, so an intentional stop or a clean exit
  could leave discovery inactive indefinitely;
- the portal trusted the last stored `healthy` value without checking the age
  of `last_success_at_ms`.

The Refresh action correctly reloaded the database, but the database itself was
no longer receiving new discovery snapshots.

## Fix Summary

- Build both discovery workers into one checksummed, relocatable artifact.
- Install retained immutable releases and atomically move both worker links.
- Keep the workers enabled independently from Jobs API maintenance with
  `Restart=always`.
- Check both process availability and source freshness every 15 minutes.
- Treat a source as delayed after 12 hours, twice the slowest normal cadence.
- Show `Updates delayed` in the portal instead of presenting historical health
  as current.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/views/MatchesView.tsx` | Derive source state from status, health, and sync age |
| `jobs/portal/src/views/MatchesView.test.tsx` | Cover fresh, missing, paused, and overdue source states |
| `ops/bluey-jobs-discovery.service.example` | Decouple direct discovery from API lifecycle |
| `ops/bluey-jobs-global-discovery.service.example` | Decouple global discovery from API lifecycle |
| `ops/build-bluey-jobs-workers.sh` | Create one immutable discovery runtime |
| `ops/install-bluey-jobs-workers.sh` | Verify, activate, retain, and roll back worker releases |
| `ops/check-bluey-jobs-discovery.sh` | Check workers and configured source freshness |
| `ops/bluey-jobs-discovery-health.*.example` | Run the check every 15 minutes |
| `ops/tests/test-*.sh` | Cover service policy, health failure, and installer rollback |
| `jobs/OPERATIONS.md` | Document deployment, health, and rollback |

## Edge Cases Handled

- A source with no successful sync remains `waiting`.
- A paused source remains paused even when its prior stored health was healthy.
- A stale source cannot appear healthy merely because its last attempt did not
  overwrite the stored state.
- Reinstalling identical worker bytes is idempotent.
- A failed activation restores both previous worker links.
- The current worker release is never removed by retention.
- Candidate-feed records remain leads and still require original-employer
  revalidation before packet creation or submission.

## How to Test

```bash
ops/tests/test-bluey-jobs-discovery-units.sh
ops/tests/test-install-bluey-jobs-workers.sh
ops/tests/test-check-bluey-jobs-discovery.sh
npm --prefix jobs run typecheck
npm --prefix jobs run test
npm --prefix jobs run build
```

## Known Limitations

- A failed timer check must be connected to the production alerting path to page
  an operator; systemd records the failure immediately.
- Curated candidate feeds do not grant submission authority. The original job
  page still has to pass current availability and ATS capability checks.
