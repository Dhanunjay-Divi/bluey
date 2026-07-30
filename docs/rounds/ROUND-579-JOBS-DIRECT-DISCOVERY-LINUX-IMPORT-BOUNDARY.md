# Round 579 - Jobs Direct Discovery Linux Import Boundary

Date: 2026-07-30

## Objective

Make both production discovery workers load on Linux without importing Bluey
Jobs resume/PDF rendering dependencies.

## Production Finding

The first immutable worker activation exposed the same `DOMMatrix is not
defined` startup failure in both systemd services:

- `bluey-jobs-discovery.service`
- `bluey-jobs-global-discovery.service`

The global worker was corrected in Round 578. Direct discovery still reached
the root automation barrel from two imports and therefore still loaded
`pdfjs-dist`.

## Implementation

Bluey now provides two purpose-built server-runtime entry points:

- `@bluey/jobs-automation/discovery-runtime` for curated feeds and public ATS
  snapshots.
- `@bluey/jobs-automation/jobhive-runtime` for the global Jobhive artifact
  reader.

Neither entry point exports document generation, PDF inspection, Playwright
execution, or application submission helpers.

The Linux-style import guard executes both built worker entry points through a
custom ESM loader. The guard rejects `pdfjs-dist`, `pdf-lib`, and
`@pdf-lib/fontkit` before activation.

## Safety Invariants

- The main Bluey API, standalone Jobs API, Caddy, and native releases are
  unchanged.
- Model generation and local/cloud Browser distribution remain disabled.
- Discovery output remains a lead until original-source revalidation.
- Employer-facing submission authority is unchanged.

## Verification

The complete Jobs package test/typecheck/build matrix, focused discovery and
archive tests, server Jobs tests, privacy/schema/license gates, and the
executable two-worker import guard must pass before producing a replacement
immutable worker artifact.

## Production Status

Pending activation of the exact merged worker artifact and source-freshness
catch-up. The Jobs portal must remain on the prior release until both workers
are stable and source timestamps advance.
