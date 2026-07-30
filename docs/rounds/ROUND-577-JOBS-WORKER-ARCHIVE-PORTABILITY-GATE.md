# Round 577: Jobs Worker Archive Portability Gate

Date: July 30, 2026

Status: implementation verified; merged-artifact deployment pending

## Why Deployment Paused

The first Round 576 production install stopped before activation. The worker
archive contained both the intended release root and a hidden AppleDouble root
created from macOS filesystem metadata.

No worker link, service, API binary, portal file, native release, or production
flag changed. The installer failed closed as designed.

## Portability Contract

Every worker archive must:

- contain exactly one root named for the immutable release;
- contain no absolute path or parent traversal;
- contain no path component beginning with `._`;
- contain the release manifest;
- contain both direct and global discovery worker entrypoints.

The build now disables macOS copyfile metadata and validates the finished gzip
archive with Python's cross-platform `tarfile` implementation. This avoids
trusting a platform-specific listing that may hide metadata members.

## Verification

The focused archive verifier accepts a valid release and rejects:

- an AppleDouble metadata root;
- multiple ordinary roots;
- missing runtime entrypoints.

A complete worker build produced:

```text
members: 2851
roots: jobs-workers-36df9a2b5d10
AppleDouble members: 0
SHA-256: 779a6bb162452b9274253a3c635baa3141499dad46962c6911335a27e73cfb8d
```

That artifact validates the fix but is not a production candidate because it
was built from an uncommitted tree. Production will use a new artifact built
once from the exact merged commit.

## Deployment Gate

Before activation:

1. merge this scoped fix through review;
2. rebuild workers from the exact merged commit;
3. verify the archive independently on macOS and Linux;
4. confirm its manifest source commit and checksum;
5. run the immutable installer;
6. deploy the matching portal bundle;
7. verify worker and timer health, source catch-up, public routes, and all three
   disabled Jobs execution flags.

## Rollback

The production installer keeps both previous worker links until the new
services are healthy. A failed activation restores both links. The preserved
worker and portal backups remain the deployment rollback inputs.

## Live Evidence

Pending exact merged-artifact deployment.
