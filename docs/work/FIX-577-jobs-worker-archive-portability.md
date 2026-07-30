# FIX-577: Jobs Worker Archive Is Not Portable

## Issue

The first Round 576 production worker archive passed local inspection on macOS
but failed the Linux installer before activation because it contained a second
hidden AppleDouble release root.

## Root Cause

macOS `tar` serialized `com.apple.provenance` metadata into `._*` members. The
local BSD `tar` listing suppressed those members, while Python and GNU `tar`
correctly observed both the intended release root and the hidden metadata root.

The installer rejected the archive before changing production, but the build
had no independent portability check to catch the defect earlier.

## Fix Summary

- Disable macOS copyfile metadata while creating the archive.
- Verify the completed archive with Python's standard `tarfile` reader.
- Reject absolute paths, parent traversal, AppleDouble members, multiple roots,
  an unexpected release root, or missing worker entrypoints.
- Cover valid, AppleDouble, and multiple-root archives in a focused shell test.

## Files Modified

| File | Change |
|------|--------|
| `ops/build-bluey-jobs-workers.sh` | Create metadata-free archives and run the verifier |
| `ops/verify-bluey-jobs-workers-archive.py` | Enforce portable archive structure |
| `ops/tests/test-verify-bluey-jobs-workers-archive.sh` | Cover accepted and rejected structures |
| `CHANGELOG.md` | Record the deployment-blocking portability fix |

## Edge Cases Handled

- A hidden `._<release>` root is rejected even when the platform `tar` listing
  does not display it.
- AppleDouble files nested below the expected root are also rejected.
- A second ordinary root, absolute path, or `..` traversal is rejected.
- An otherwise well-formed archive without either worker entrypoint is
  rejected.

## How to Test

```bash
python3 -m py_compile ops/verify-bluey-jobs-workers-archive.py
ops/tests/test-verify-bluey-jobs-workers-archive.sh
ops/tests/test-install-bluey-jobs-workers.sh
ops/tests/test-bluey-jobs-discovery-units.sh
BLUEY_ALLOW_DIRTY_WORKER_BUILD=1 \
  ops/build-bluey-jobs-workers.sh "$(mktemp -d)"
```

Inspect the final archive with an independent `tarfile` reader and confirm one
release root and zero `._*` path components.

## Known Limitations

- The verifier proves archive structure and required entrypoints. The
  production installer remains responsible for checksum verification,
  immutable activation, service health, and rollback.
