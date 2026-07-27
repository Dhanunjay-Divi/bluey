# FIX-012: Release Staging Invalidates the Outer macOS App Signature

## Issue

The release job copied daemon, CLI, helper executables, and nested apps into an
already-built `Bluey.app`, potentially invalidating its resource seal.

## Root Cause

The workflow verified the nested audio helper but did not sign or verify the
fully assembled outer application after modifying `Contents/Resources`.

## Fix Summary

After every release resource is staged, the workflow deep-signs the complete
application with the configured identity, preserves identifiers and
entitlements, timestamps certificate-backed signatures, and runs strict deep
verification before archiving.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/release.yml` | Final-sign and verify the fully staged `Bluey.app`. |

## Edge Cases Handled

- Manual non-tag builds retain the existing ad-hoc fallback.
- Certificate-backed releases request a trusted timestamp.
- Nested helper identifiers and TCC entitlements are preserved.

## How to Test

```bash
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'
codesign --verify --deep --strict --verbose=2 staging/Bluey.app
```

## Known Limitations

- Tagged releases still require the configured PKCS#12 certificate secrets.
