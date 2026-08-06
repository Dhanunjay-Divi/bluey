# FIX-612: Local release recovery and target selection

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix in the isolated
> Round 603 worktree without opening a live ticket, downloading an installer, or
> enabling local Browser distribution.

## Issue

Adding strict release authority risked stranding already committed local work,
while the portal could not safely distinguish Apple-silicon from Intel Macs and
previously had no exact server-owned artifact metadata to replace a generic
download path.

## Root Cause

The original local capability carried no frozen release identity or versioned
recovery contract. Applying current release eligibility to every later request
would incorrectly invalidate submitted receipts and uncertain outcomes. In the
portal, mainstream macOS browser identity reports `MacIntel` even on Apple
silicon, so automatic architecture selection would silently offer the wrong
signed installer.

## Fix Summary

Version-two result, resume, and submit capabilities freeze the descriptor,
manifest, activation, artifact, build, protocol, target, channel, and trust
identity selected at claim. Legacy version-one capability and debug-ticket
compatibility are recovery-only and never authorize a new submit. Exact
submitted-receipt replay uses the previously frozen capability hash; bounded
late result reconciliation is limited to submitted or side-effect-unknown
states. Current revocation blocks a future pre-click authorization without
invalidating receipt/result/resume recovery that must settle an existing run.

The portal consumes only server-owned snake-case release metadata and validates
the exact origin, immutable release path, file name, package kind, target,
digest, and complete artifact set. Unknown macOS architecture exposes no URL
until the user explicitly selects Apple silicon or Intel; unsupported systems,
disabled distribution, expired activation, missing assignment, or malformed
metadata expose neither an installer nor a protocol launch link.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs_local_capability.rs` | Add release-bound v2 capabilities and recovery-only v1 verification. |
| `server/src/api/jobs.rs`, `server/src/db/jobs/local_runner.rs` | Preserve exact submitted/reconciliation authority while fencing new submit. |
| `server/tests/jobs_runner_plan_matrix.rs` | Exercise entitlement and distribution gates against genuine signed release authority. |
| `jobs/browser/src/packaged-release.ts`, `jobs/browser/tests/packaged-release.test.ts`, Browser fixtures | Load exact packaged authority and reject tampered or unsafe descriptor files. |
| `jobs/browser/src/{local-capabilities,local-protocol-handler,local-run-contracts,run-controller}.ts` | Carry claim proof and operation-scoped release-bound capabilities. |
| `jobs/browser/src/{app-lifecycle,checkpoint-recovery}.ts` | Load packaged authority and preserve bounded recovery behavior. |
| `jobs/portal/src/lib/{browser-release,runner-access}.ts` | Validate server release metadata and target authorization. |
| `jobs/portal/src/data/preview.ts` | Supply schema-exact disabled and available preview release states. |
| `jobs/portal/src/views/BrowserView.tsx`, `jobs/portal/src/types.ts`, `jobs/portal/src/styles.css` | Render exact installer evidence and an explicit unknown-Mac selector. |
| `web/jobs/` | Rebuild the checked-in portal bundle from the verified source. |
| Browser, portal, and server tests | Cover legacy recovery, revocation, malformed metadata, targets, and hidden URLs. |

## Edge Cases Handled

- Version-one capabilities verify only for result/resume recovery and cannot be
  replayed as submit authority.
- A result arriving within the exact 24-hour grace is accepted only for an
  already submitted or uncertain side effect, not for fresh execution.
- Ticket hashes are checked before replay can mutate state.
- Unknown Mac architecture can still open an already installed app, but cannot
  guess or reveal either installer URL.
- Windows arm64 and unknown operating systems are unsupported and fail closed.
- Duplicate target artifacts, mutable aliases, foreign origins, credentials,
  query strings, fragments, and package-mismatched filenames expose no URL.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-browser
npm test --workspace @bluey/jobs-portal -- --run src/views/BrowserView.test.tsx
cargo test --manifest-path ../server/Cargo.toml local_run
```

## Known Limitations

- Physical browser detection cannot reliably infer Mac CPU architecture from a
  web user agent; the explicit user choice is intentional.
- Installed-app self-update remains a separate capability and is not implied by
  the manifest-bound macOS ZIP artifact.
