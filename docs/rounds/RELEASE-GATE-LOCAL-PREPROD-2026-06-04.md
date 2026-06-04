# Release Gate: Local Preprod, GitHub Prod

Date: 2026-06-04

## User Direction

Bluey should not spend GitHub Actions credits on preprod loops. Preprod
build, install, smoke, and visual QA should run on owned machines:

- this Mac (`uno`),
- the Windows bench over SSH/Tailscale when Windows is in scope.

GitHub Actions is reserved for production release packaging/publishing.

## Locked Rule

Before production promotion, precheck every release identity surface:

- `bluey --version`
- `bluey-daemon --version`
- native overlay/helper versions
- helper SHA sidecars
- release tarball SHA256 manifest
- server `/health` version/commit
- static web version/release id
- release signature or signed-manifest state

For the current unsigned/ad-hoc macOS alpha, the required signature check is
ad-hoc `codesign --verify` plus SHA manifests. Once a signed release manifest
exists, its signature becomes mandatory.

## Promotion Contract

Build once, smoke that exact stored artifact locally, then publish/promote that
same release id to production. If any version/hash/signature check disagrees,
stop and cut a new release id.

## Files Updated

- `DECISIONS.md`
- `docs/DELIVERY-LIFECYCLE.md`
- `AGENT-HANDOFF.md`

