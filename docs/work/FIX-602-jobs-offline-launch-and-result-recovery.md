# FIX-602: Fence Browser Launch and Submitted-Result Recovery

> **Codex preflight:** Loaded `$bluey-ops` and checked the cloud/local runner,
> checkpoint, browser-profile, and side-effect-unknown boundaries before
> diagnosis and implementation.

## Issue

A fresh or recovered browser could regain network before Bluey had selected and
guarded the exact page. A crash after an employer submission or server finish
could also leave a durable result unpromoted, while corrupt recovery state risked
blocking unrelated browser profiles.

## Root Cause

Browser construction, navigation, network policy, checkpoint restore, server
finish, and local result publication were separate transitions. Recovery lacked
one offline-first ordering rule and one exact staged-to-committed result
protocol, and profile failures were not isolated from other scopes.

## Fix Summary

Fresh and recovered cloud/local pages now start offline, admit exactly one bound
HTTP page, reject surviving service workers, install the provider guard, and
only then enable network and navigate. Submitted results are encrypted and
staged with exact request, account, application, identity, session, run, token,
and fence bindings. A lost response replays the same server finish before the
matching result is promoted. Profile recovery failures stay scoped to one
content-addressed profile and browser-session mutations are serialized.

## Files Modified

| File | Change |
|------|--------|
| `jobs/browser/src/{run-controller,checkpoint-recovery,browser-network}.ts` | Enforces offline-first local launch and exact recovery. |
| `jobs/browser/src/{irreversible-submit,execution-result,local-failure}.ts` | Persists irreversible state and conservative local outcomes. |
| `jobs/runner/src/{server,run-checkpoint-store,result-store}.ts` | Adds guarded cloud launch and staged result promotion. |
| `jobs/runner/src/{recovery-isolation,submitted-result-recovery}.ts` | Isolates corrupt scopes and replays exact submitted authority. |
| `jobs/runner/src/profile-snapshot-client.ts` | Retries and publishes fenced encrypted profile snapshots. |
| `jobs/workflows/src/{activities,workflows}.ts` | Recovers durable runner results before canonical receipt persistence. |
| Browser, runner, and workflow tests | Cover crash windows, response loss, isolation, and navigation order. |

## Edge Cases Handled

- Extra HTTP pages, non-HTTP active pages, or surviving service workers.
- Recovery to another provider job or a navigation initiated before guard install.
- Crash after local durable marker, employer request, server finish, or staged
  result write.
- Timeout after a committed result-store or canonical-receipt response.
- Tampered result bindings, token/fence mismatch, stale request IDs, and bounded
  retry exhaustion.
- One corrupt checkpoint without suppressing another profile's valid recovery.
- Parallel writes for the same browser session or profile.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-browser
npm test --workspace @bluey/jobs-runner
npm test --workspace @bluey/jobs-workflows
npm run typecheck
```

## Known Limitations

- The local evidence proves process-crash ordering. Parent-directory sync is
  attempted where supported; physical power-cut and per-platform filesystem
  guarantees still require real-device certification.
- Cloud browser distribution and real profile-object storage remain disabled
  pending independent infrastructure fault tests.
