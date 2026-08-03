# FIX-598: Restrict ATS Final-Submit Authority

> **Codex preflight:** Loaded `$bluey-ops`, verified the current feature branch,
> preserved unrelated Browser release scripts, and kept all production Jobs
> distribution flags disabled.

## Issue

Generic form adapters could click a final Submit button and infer successful
submission from untrusted page text. Capability detection also accepted broad
host patterns, allowing the browser and server to disagree about which ATS
families were authorized for unattended submission.

## Root Cause

Submission policy, adapter implementation and server eligibility evolved in
separate modules. The generic adapter treated visible confirmation language as
proof, while provider detection was not based on exact parsed hosts. There was
no final execution-layer check preventing an unauthorized adapter from
returning `submitted`.

## Fix Summary

Added one typed ATS capability registry, made policy and execution consume it,
removed final-submit behavior from generic adapters, and made the server derive
capability from exact HTTP/HTTPS provider hosts. The execution wrapper now
downgrades any unauthorized submitted result to a takeover state.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/src/adapter-capabilities.ts` | Defines provider capability and version authority |
| `jobs/automation/src/policy.ts` | Uses shared fail-closed capability metadata |
| `jobs/automation/src/execute.ts` | Rejects unauthorized submitted outcomes |
| `jobs/automation/src/standard-adapters.ts` | Stops generic adapters before final Submit |
| `jobs/automation/src/source-catalog.ts` | Labels uncertified ATS sources review-only |
| `server/src/db/jobs/eligibility.rs` | Parses exact provider hosts server-side |
| Automation and server tests | Exercise policy, spoofing and final-action boundaries |

## Edge Cases Handled

- Provider names embedded in attacker-controlled hostnames.
- Malformed URLs and non-HTTP schemes.
- Confirmation-like text without trusted evidence.
- A generic adapter reaching a visible final Submit button.
- An adapter returning a forged `submitted` outcome.
- Workday, Ashby and SmartRecruiters forms that can be filled but are not yet
  certified for unattended final submission.

## How to Test

```bash
npm --prefix jobs/automation test
npm --prefix jobs/automation run typecheck
npm --prefix jobs/automation run build
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path server/Cargo.toml --all -- --check
git diff --check
```

## Known Limitations

- Greenhouse and Lever remain beta-review capabilities until authorized live
  tenant and irreversible-action fault certification pass.
- Workday, Ashby, SmartRecruiters and semantic forms remain review/takeover
  paths even when form filling succeeds.
