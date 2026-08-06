# FIX-628: Keep Certified Receipt Coverage Within Strict Rust Expansion Limits

> **Codex preflight:** Loaded `$bluey-ops` and used the current Round 604 test
> fixture and strict all-target Clippy output only.

## Issue

The new complete schema-4 certified-receipt regression used deeply nested
`serde_json::json!` fixtures that exceeded Rust's macro recursion limit during
strict all-target analysis.

## Root Cause

One test macro constructed the full approved packet, final-submit proof,
certification binding, runtime, target evidence, and receipt authority in a
single nested token expansion. The focused test could compile in one path, but
the required all-target Clippy gate failed.

## Fix Summary

- Replaced the oversized nested fixture expansion with typed Rust authority
  structures and smaller bounded JSON values.
- Kept the test on real server receipt validators and exact stored terminal
  authority rather than weakening it to shape-only assertions.
- Did not raise the crate recursion limit.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs.rs` | Build the certified receipt fixture from typed authority structures and retain exact mismatch tests. |
| `docs/work/FIX-628-jobs-certified-receipt-test-expansion.md` | Record the strict-test regression and repair. |

## Edge Cases Handled

- Exact schema-4 authority still validates successfully.
- Document, screenshot, confirmation, attempt, and metering mutations still
  fail through the production validator stack.
- Strict compilation no longer depends on a larger global recursion limit.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  api::jobs::tests::schema_four_certified_receipt_requires_exact_terminal_and_evidence_authority \
  -- --exact
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- The fixture is local source evidence and does not claim a live provider or
  PostgreSQL receipt canary.
