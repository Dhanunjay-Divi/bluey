# Round 572 - Jobs Career Ops Deep Audit And Selective Port

Date: July 24, 2026

## Scope

This round reviewed `santifer/career-ops` at
`01bf8b469ad5177a9c30230bc00509ead8e006c2` and selectively adapted the
parts that improve Bluey Jobs without replacing Bluey's existing tenant,
eligibility, discovery, evidence, or submission authority.

The disposable research clone lived outside the repository and was removed
after the audit. No raw upstream clone is shipped with Bluey.

## What Career Ops Does Well

Career Ops is a strong local-first job-search toolkit. Its useful product and
engineering patterns include:

- broad public ATS/provider coverage;
- defensive provider registration;
- explicit source-trust reason codes;
- deterministic description fingerprints for possible agency cross-listings;
- job liveness and repost reason taxonomies;
- reusable application answers;
- follow-up planning and a compact application inbox;
- CV section and template utilities.

## What Bluey Already Does Better

Bluey retains its existing implementation for:

- account and application-identity isolation;
- server-owned eligibility and final-submit policy;
- durable discovery authority and source observations;
- canonical URL deduplication and closed-job handling;
- transactional fact and eligibility rechecks;
- immutable packets, receipts, evidence, and metering;
- bounded host-pinned ATS readers;
- provider-specific Greenhouse and Lever execution state machines;
- durable Answer Memory and mailbox correlation.

Career Ops local Markdown/YAML state, browser-like request impersonation, WAF
workarounds, and generic dynamic provider loading were intentionally not
ported.

## Implemented

### Source trust

`jobs/automation/src/job-source-intelligence.ts` now returns typed advisory
signals for:

- missing or invalid application URLs;
- insecure HTTP application links;
- URL shorteners and redirectors;
- unexplained company-to-domain mismatches.

Known ATS hosts do not receive a company-domain mismatch. Trust is never
application authority: every result retains
`requiresOriginalRevalidation: true`.

Curated feed leads now carry that assessment before they enter shared staging.

### Cross-listing signals

Bluey now creates a deterministic 64-bit description fingerprint from
three-token shingles and reports near-identical descriptions appearing under
different employer names and URLs.

The signal is intentionally advisory:

- jobs are not silently merged;
- same-company reposts are excluded from this signal;
- short descriptions are not fingerprinted;
- original sources remain separate until compared and revalidated;
- application eligibility remains server-owned.

Public ATS searches add a warning when possible cross-listed pairs are found.

## Provenance

Career Ops is MIT licensed, copyright 2026 Santiago Fernandez de Valderrama.
The existing third-party notice remains in
`jobs/THIRD_PARTY_NOTICES.md`; the exact reviewed commit and adaptation map are
updated in `jobs/THIRD_PARTY_PROVENANCE.md`.

## Verification

The integrated release slice passed:

1. 188 automation tests, including source-trust, cross-listing, curated-feed,
   and public-ATS coverage;
2. 100 browser tests, 50 runner tests, 51 workflow tests, and 75 portal tests
   (464 Jobs tests total);
3. complete Jobs TypeScript checks and the portal production build;
4. `cargo fmt --all -- --check`, all Rust library tests, and
   `cargo clippy --all-targets --all-features -- -D warnings`;
5. SQLite/Postgres schema parity, privacy, provenance/license, CI guard,
   and client/server boundary checks;
6. desktop and mobile onboarding and Settings visual checks in both responsive
   layouts, with no horizontal overflow or browser-console errors;
7. direct route refresh on mobile Settings, which preserved the selected route;
8. generated-asset scanning, which found no source maps, environment files,
   backup files, or secret-like build output.

## Remaining Provider Work

Career Ops demonstrates breadth but does not prove production certification
for every tenant variation. Bluey should expand one provider family at a time:

1. add a bounded, host-pinned source reader;
2. preserve original-source observations;
3. build provider-specific fixtures and live certification;
4. keep uncertified variants Review-only;
5. enable unattended submission only after crash, duplicate-delivery,
   intervention, and receipt-evidence gates pass.

This keeps Bluey broader over time without turning a long provider list into a
false automation promise.
