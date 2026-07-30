# Round 578: Jobs Global Discovery Linux Import Boundary

Date: July 30, 2026

Status: implementation verified; production recovery pending

## Incident

The exact merged Round 577 worker archive passed checksum, archive-shape, and
installer verification. Production activation then produced two different
results:

- direct ATS discovery started and remained healthy;
- global curated-feed discovery failed before its poll loop started.

The portal deployment was held. Main Bluey API, standalone Jobs API, Caddy,
signed native installers, and all three disabled Jobs execution flags were
left unchanged.

## Root Cause

The global worker imported the root Jobs automation package. Loading that
barrel also loaded PDF and resume rendering modules even though global
discovery consumes only manifest metadata and streamed CSV rows.

On Linux, the transitive document stack attempted to load an optional native
canvas binding built on macOS. PDF.js then evaluated without a `DOMMatrix`
implementation and terminated the process:

```text
ReferenceError: DOMMatrix is not defined
```

This was an import-boundary defect, not a feed, database, service-manager, or
archive-integrity defect.

## Fix

The Jobs automation package now exposes:

```text
@bluey/jobs-automation/jobhive-runtime
```

That subpath contains only:

- manifest download and validation;
- artifact download and hash verification;
- bounded CSV streaming;
- the types required by the global discovery workflow.

The global worker uses this subpath. Resume/PDF helpers remain available from
the root package for workflows that intentionally use them.

## Regression Gate

After both packages are built, the new shell gate imports the global worker
through a Node ESM loader that rejects:

```text
pdfjs-dist/*
pdf-lib
@pdf-lib/fontkit
```

This verifies runtime behavior rather than relying only on source inspection.

## Verification

Verified on the scoped feature branch:

- 188 automation tests;
- 100 Browser tests;
- 50 runner tests;
- 51 workflow tests;
- 77 portal tests;
- full Jobs typecheck and production build;
- global worker document-import rejection gate;
- direct/global discovery policy tests;
- source freshness checker tests;
- portable archive verification;
- immutable installer idempotency, retention, and rollback tests;
- privacy, schema parity, provenance/license, and CI guard checks;
- `git diff --check`.

## Deployment Gate

1. Merge this scoped fix through a feature branch and reviewed pull request.
2. Build one worker archive from the exact merged commit.
3. Independently verify its checksum, single root, manifest source commit, and
   runtime entrypoints.
4. Install the exact archive and confirm both worker units are active and
   enabled with stable restart counters.
5. Wait for direct and global source timestamps to advance.
6. Run the strict discovery health checker.
7. Deploy the already-built matching portal only after worker catch-up.
8. Verify authenticated Matches in desktop/mobile light/dark views.
9. Verify protected routes, hidden internal routes, main services, and all
   three Jobs execution flags remain unchanged.

## Rollback

The worker installer retains prior immutable releases and restores both worker
links if activation fails. The portal has an independent predeployment archive.
Do not roll back API binaries or candidate data for this import-only defect.

## Live Evidence

Pending exact merged-artifact recovery and portal deployment.
