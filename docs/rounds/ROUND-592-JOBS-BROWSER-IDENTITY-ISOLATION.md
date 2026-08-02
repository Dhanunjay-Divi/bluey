# Round 592 - Bluey Browser Application Identity Isolation

**Date:** 2026-08-02
**Branch:** `feat/phase-jobs-full-autonomy-20260802`
**Status:** local Browser profile authority implemented; distribution remains disabled

## Result

Bluey Browser now binds every persistent Chromium profile to exactly one Bluey account
and one application identity. A user may maintain separate application emails and
profiles without Workday, Greenhouse, Lever, or another employer session leaking from
one identity into another.

The opaque profile ID uses the same deterministic format as the server execution
authority. The disk layout is intentionally unchanged from the existing Browser profile
layout, so upgrading does not log users out or strand an established employer session.

## Authority Contract

```text
Bluey account
  -> verified application identity
     -> opaque Browser profile ID
        -> persistent Chromium user-data directory
           -> one bound BrowserContext
```

The policy fails closed when:

- a context is rebound to another identity;
- two identities collide on one profile ID;
- a profile root is relative, traversing, or the filesystem root;
- an identity is empty, padded, control-bearing, or oversized; or
- a policy returns a malformed profile ID.

Errors contain typed codes only and do not echo account emails, identity names, or
filesystem values.

## Compatibility

For an account and identity, the profile ID remains:

```text
sha256(account_id)[0..24]:sha256(application_identity_id)[0..24]
```

The Chromium profile remains below:

```text
profiles/<account-hash>/identities/<identity-hash>/chromium-profile
```

Both contracts match the existing Browser and server implementation.

## Verification

- Twelve focused Browser identity tests passed.
- The full Browser suite passed: 27 files and 112 tests.
- Browser TypeScript strict checking passed.
- The production Browser TypeScript build passed.
- Tests cover deterministic IDs, account/identity isolation, ambiguous input tuples,
  path traversal, private-value redaction, legacy path compatibility, collision
  detection, context rebound, and fenced release.

## Production Boundary

No production flag changed. Local Browser and cloud Browser distribution remain off.
This round supplies a prerequisite for multiple application identities; it does not
provide signed installers, cloud worker ownership, restart recovery, or ATS submission
certification.

## Next Gate

1. Add durable cloud-runner leases, heartbeats, and process-restart recovery.
2. Reconcile side-effect-unknown submission attempts without blind retry.
3. Certify exact packaged Browser artifacts on macOS and physical Windows.
