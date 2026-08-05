# FIX-603: Preserve the Complete Confirmation Screenshot Set

> **Codex preflight:** Loaded `$bluey-ops` and verified the immutable receipt,
> account scope, exact-resume, and evidence-download invariants before diagnosis
> and implementation.

## Issue

A receipt could contain multiple bounded confirmation screenshots while the
durable evidence record and portal verification represented only one image.
That made a valid receipt appear incomplete or allowed later images in the
captured confirmation set to remain outside the immutable download manifest.

## Root Cause

The receipt contract already allowed one to four confirmation screenshots, but
server materialization, object naming, final-submission validation, and portal
verification retained a legacy single-screenshot assumption.

## Fix Summary

Every captured screenshot now receives a unique indexed filename, immutable
account-scoped object binding, evidence record, and manifest entry. Final
submission and authenticated download require exact one-to-one coverage of the
full ordered screenshot set. The portal verifies and renders each record while
accepting a coherent legacy receipt with one unindexed screenshot.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs.rs` | Uploads and verifies every bounded screenshot object. |
| `server/src/db/jobs/customer_data.rs` | Materializes indexed immutable evidence records. |
| `server/src/db/jobs/tests.rs` | Covers exact multi-image records and finalization. |
| `jobs/portal/src/views/ApplicationsView.tsx` | Verifies and displays the complete screenshot set. |
| `jobs/portal/src/views/ApplicationsView.test.ts` | Covers multi-image and legacy evidence behavior. |

## Edge Cases Handled

- One through four confirmation screenshots.
- Missing, duplicated, reordered, extra, tampered, or wrong-media images.
- Object/evidence entries bound to another account, application, or resume.
- Unique download names for every screenshot.
- Legacy single-screenshot evidence without a modern index.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml final_submission_requires_exact_multi_screenshot
cargo test --manifest-path server/Cargo.toml evidence_download_requires_the_exact_complete
npm test --prefix jobs --workspace @bluey/jobs-portal
```

## Known Limitations

- Evidence remains intentionally bounded to four confirmation screenshots per
  submission; a provider needing more must first receive a reviewed schema and
  quota change.
