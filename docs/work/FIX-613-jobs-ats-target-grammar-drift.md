# FIX-613: Jobs ATS target grammar drift

> **Codex preflight:** `$bluey-ops` was loaded before diagnosis and its memory
> was reconciled against the current repository state.

## Issue

Greenhouse and Lever provider classification used different URL rules across
automation policy, adapter resolution, the source catalog, and the server.
Most visibly, the exact Lever EU host was supported by the provider state
machine and server receipt proof but was omitted from the earlier automation
policy/catalog gate, so local and cloud runs handed it back to the user.

## Root Cause

- `jobs/automation/src/adapters.ts` selected provider adapters primarily by
  hostname and omitted `jobs.eu.lever.co`.
- `jobs/automation/src/source-catalog.ts` repeated the same incomplete Lever
  domain list and allowed suffix-domain matching for provider catalog entries.
- `jobs/automation/src/policy.ts` allowed HTTP provider hosts to inherit the
  provider capability because URL transport, authority, and job path were not
  parsed as one target.
- `server/src/api/jobs.rs::ats_kind` independently classified provider hosts
  without applying the exact job/application path grammar. Its Workday branch
  also used substring matching.
- Exact final-submit job-key parsing used a second, stronger-but-independent
  grammar, leaving both an earlier-policy/later-authority mismatch and latent
  drift inside receipt authority.
- Both first-pass canonical parsers searched for a slash without first stopping
  at the query or fragment delimiter. A slash in `?next=/...` or `#next=/...`
  could therefore manufacture a provider path on an otherwise pathless URL.
- The TypeScript parser accepted `URL` objects as well as raw strings. URL
  normalization erased explicit ports, dot segments, percent-encoded dot
  segments, and surrounding syntax before the authority check.
- Adapter selection was URL-only. After an irreversible Lever click redirected
  to an unrecognized success path, a second execution pass discarded the bound
  Lever state machine and fell back to the semantic adapter.

## Fix Summary

Added one canonical Greenhouse/Lever application-target parser in TypeScript
and a Rust counterpart governed by the same golden JSON vectors. Classification
now requires an exact provider host, HTTPS, no credentials, no explicit port,
bounded identifiers, and an exact provider job/application path. Exact
confirmation paths remain available only for state-machine continuation and
receipt validation; they cannot start a new automated run.

Raw-path extraction now stops at the first `/`, `?`, or `#`; URL size is
measured in UTF-8 bytes in both languages; and the exported authority parser
accepts raw strings only. Provider-first execution resolves the raw request
before URL normalization. Once a run selects Greenhouse or Lever, that exact
adapter remains bound to the run context so post-click recovery preserves its
provider state and returns an uncertain outcome without another submit.

The parser now drives automation detection, submission policy, provider URL
detection, source-catalog classification, TypeScript and Rust final-submit job
keys, server ATS classification, and receipt confirmation identity. Lever EU is
present in the source catalog and reaches the same review-only Lever state
machine in local and cloud workspaces. Malformed and spoofed targets fail closed
to semantic handoff before provider adapter selection.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/src/ats-target.ts` | Added the canonical TypeScript provider target grammar. |
| `jobs/automation/src/adapters.ts` | Resolves exact initial submit targets; `execute.ts` keeps confirmation continuation bound to the selected provider. |
| `jobs/automation/src/execute.ts` | Uses raw initial resolution and preserves the bound provider adapter through recovery. |
| `jobs/automation/src/policy.ts` | Grants provider policy only to exact submit targets. |
| `jobs/automation/src/provider-job-key.ts` | Reuses the canonical target identity instead of duplicating it. |
| `jobs/automation/src/providers/greenhouse.ts` | Uses exact Greenhouse submit/confirmation detection. |
| `jobs/automation/src/providers/lever.ts` | Uses exact Lever US/EU submit/confirmation detection. |
| `jobs/automation/src/source-catalog.ts` | Adds Lever EU and excludes loose provider-domain matches. |
| `jobs/automation/src/index.ts` | Exports the canonical target contract. |
| `jobs/automation/tests/fixtures/ats-target-vectors.json` | Adds shared positive and adversarial vectors. |
| `jobs/automation/tests/ats-target.test.ts` | Applies every vector to target, policy, detection, and catalog behavior. |
| `jobs/automation/tests/policy.test.ts` | Covers strict policy decisions from the shared grammar. |
| `jobs/automation/tests/provider-beta-adapters.test.ts` | Verifies unsupported provider paths hand off semantically without clicking. |
| `jobs/automation/tests/source-catalog.test.ts` | Covers exact source matching and Lever EU catalog reachability. |
| `jobs/browser/tests/ats-target-reachability.test.ts` | Verifies local exact EU Lever reachability and malformed-target handoff. |
| `jobs/browser/tests/local-run-contracts.test.ts` | Exercises exact Lever EU and malformed targets at local run admission. |
| `jobs/runner/tests/ats-target-reachability.test.ts` | Verifies cloud exact EU Lever reachability and spoof-target handoff. |
| `jobs/runner/tests/server-navigation-binding.test.ts` | Exercises exact Lever EU and malformed targets at cloud run admission. |
| `server/src/jobs_ats_target.rs` | Adds the Rust parser and consumes the shared vectors. |
| `server/src/db/jobs/execution_leases.rs` | Delegates final-submit and confirmation job identity to the shared Rust parser. |
| `server/src/api/jobs.rs` | Uses exact submit classification and the parsed confirmation job key directly. |
| `server/src/lib.rs` | Registers the server target parser module. |

## Edge Cases Handled

- Both official Greenhouse hosts and both Lever US/EU hosts.
- Greenhouse public job and embedded application targets.
- Lever posting and `/apply` targets.
- Confirmation URLs only in the explicit confirmation purpose.
- Provider suffix lookalikes and attacker-controlled subdomains.
- HTTP, credentials, explicit default/non-default ports, invalid URLs, and
  surrounding whitespace.
- Missing, extra, repeated, or percent-encoded path segments.
- Query/fragment slashes that previously manufactured a path on a host root.
- Normalized `URL` objects at the raw authority boundary and UTF-8 byte limits.
- Conflicting job identifiers supplied through provider query aliases.
- Workday hostname substring lookalikes on the server.
- The bare Workday parent host, which has no tenant application target.
- Unsupported post-click redirects without provider-state loss or a second
  submit attempt.

## How to Test

```bash
cd jobs/automation
npm run typecheck
npm test -- --run tests/ats-target.test.ts tests/policy.test.ts \
  tests/provider-beta-adapters.test.ts tests/greenhouse-adapter.test.ts \
  tests/lever-adapter.test.ts tests/source-catalog.test.ts

cd ../browser
npm run typecheck
npm test -- --run tests/ats-target-reachability.test.ts \
  tests/local-run-contracts.test.ts

cd ../runner
npm run typecheck
npm test -- --run tests/ats-target-reachability.test.ts \
  tests/server-navigation-binding.test.ts

cd ../../server
cargo test jobs_ats_target::tests::matches_shared_typescript_target_vectors
cargo test jobs_ats_target::tests::applies_url_limit_to_utf8_bytes
cargo test ats_kind_

cd ..
cargo fmt --all --check
git diff --check
```

## Known Limitations

- The canonical grammar covers only the existing versioned Greenhouse and
  Lever provider state machines. It does not certify either adapter, add ATS
  tenants, or enable local/cloud Browser distribution.
- Employer-domain Greenhouse embeds still require page-marker detection and do
  not gain server submission authority from this URL grammar.
