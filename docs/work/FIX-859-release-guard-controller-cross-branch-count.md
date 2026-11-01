# FIX-859: Release-guard controller inherited another branch's test count

> **Codex preflight:** `$bluey-ops`; current source and observed output govern.

## Issue

Lightweight Phase640 release-authority execution47632 returned outer exit1,
although its actual Node child completed exit0 with37/37 passing tests and no
failures, cancellations, skips or todos. The private one-off controller expected40.

## Root Cause

The controller copied the count from held Phase639's uncommitted image-inspector
successor. Phase640 intentionally starts at32ca and does not contain those image
changes. Its release test file is byte-identical to committed32ca, SHA-256
`e9e326068d7ca61040a94ec608e3d2bca31d614c0c29787dd9005ad71f43da32`,
with37 registrations. Phase639's separate three-case dynamic PAX loop produces40.
No test was deleted or skipped to lower a release threshold.

## Fix Summary

Preserve the original controller and failed journal unchanged. A separately
named private v2 controller expects37, additionally binds the exact test-file
SHA and static registrations, and uses a new journal prefix. Its syntax check
passes; it has NOT run. Do not rerun the unchanged suite solely to manufacture
an outer green result or retroactively relabel the predecessor successful.

## Files / Evidence

- Private predecessor: `phase640-release-guards.mjs`, SHA-256
  `b25a86432b794e311d661578fa14c2e6e4b29e23a83b8653057aa0c8d9e9578b`.
- Private successor: `phase640-release-guards-v2.mjs`, SHA-256
  `ec1c5019fae9a67620f6f7be178af07b637f8721662f45fbb21ad52d76c3b4ab`.
- Both are under `/private/tmp/bluey-phase639-controller-OR0wQV/`.
- Preserved journal under this task's external visualization directory:
  `phase640-release-guards-ab08d14a8b11.jsonl`, SHA-256
  `fc461a952da06100200ee44bd7640f547c492f1a5e816647715ba12a1ef954c2`.
- Journal source before/after:778 files, manifest
  `5d0a37d8afac19b0cec963f18aa93627d62e51446b0da9512b78d169e159bde1`.
  Exact owned parent removed, real index empty. Product Rust bytes unchanged.

## How to Test / Acceptance

Independent review reconciled the exact32ca test file, held Phase639 diff and
immutable journal. Accept only narrow child-pass/harness-count-failure evidence.
The journal has parsed TAP and output hashes, not retained raw TAP or controller
self-hash; it does not replace full CI or artifact qualification. A future actual
run must bind the then-current source and expected test vector.

## Known Limitations

This is a local result-controller bug, not a production incident. It does not
qualify the new Rust report, PostgreSQL, held PAX/OCI changes, an image, provider
delivery, or release. The32-GiB heavy-gate admission remains unchanged.
