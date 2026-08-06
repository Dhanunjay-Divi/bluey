# FIX-611: Release policy origin and revocation recovery

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix against the
> shared Node/Rust authority fixture in the isolated Round 603 worktree.

## Issue

Two trust-transition gaps could either admit stale hosting authority or prevent
root recovery: a fresh successor activation could reference a manifest from a
retired artifact origin, and delegated incident revocation semantics could be
applied to root keys needed to authorize the next trust generation.

## Root Cause

Artifact origin was initially checked only against the policy stored with a
manifest, not the latest policy used for a fresh channel decision. Revocation
subject parsing also treated every policy key uniformly, even though delegated
incident authority must not be able to remove the root authority that fences and
rotates delegated roles.

## Fix Summary

The signed trust policy now carries one exact HTTPS artifact origin. Fresh
manifest import, activation import/apply, channel selection, claim, and portal
availability require the current policy's origin, while byte-identical immutable
historical replay remains available for recovery. Signing-key revocations may
target only an exact known delegated key ID and public-key digest; root IDs and
root material are rejected. Root-policy rotation is verified against the
independently configured root anchor, and delegated revocation cannot remove
that root material, so an independently anchored successor threshold can
recover the system. Node and Rust apply the same key, origin, timestamp, URL,
package-kind, target, and Chromium-revision limits.

## Files Modified

| File | Change |
|------|--------|
| `jobs/browser/src/release-authority.ts` | Bind artifact origin and exact delegated revocation semantics in Node. |
| `jobs/browser/fixtures/release-authority-v1.json` | Refresh shared canonical cross-language vectors. |
| `server/src/db/jobs/browser_release_trust.rs` | Mirror origin, root recovery, delegated revocation, URL, and target validation in Rust. |
| `server/src/db/jobs/browser_release_registry.rs` | Enforce latest-origin authority on fresh transitions while preserving exact replay. |
| `jobs/browser/tests/release-authority.test.ts`, server release tests | Cover origin rotation, root/delegated subjects, future dates, replay, and parity. |

## Edge Cases Handled

- A case-variant mutable release identity remains mutable and is rejected.
- Artifact URLs must be unique, immutable HTTPS paths whose basename extension
  matches the declared package kind.
- macOS DMG/ZIP records for one target bind the same application-content digest.
- A signing-key revocation cannot use a known delegated ID with another key's
  digest, an unknown key, a root ID, or raw root public-key material.
- A stored immutable policy/manifest replay remains byte-identical after
  rotation, but it cannot become a fresh current channel head under a new
  artifact origin.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-browser -- --run tests/release-authority.test.ts
cargo test --manifest-path ../server/Cargo.toml browser_release_trust
cargo test --manifest-path ../server/Cargo.toml current_artifact_origin
```

## Known Limitations

- Root ceremonies, offline signatures, public read-back, and emergency rotation
  remain operational processes requiring independently approved keys and people.
