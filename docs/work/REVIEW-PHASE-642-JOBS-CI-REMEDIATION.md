# REVIEW642 — Web/cloud dependency isolation and native assertion lint

Preflight: `bluey-ops`; Round642/current code authoritative. Base32ca96db, owned
working-tree delta. Date2026-11-01. Reviewers: parent line-by-line/semantic lock
review plus independent native agent source/controller review.

## Findings and scope

No reproducible P0–P2 source/controller finding remains. Root importers declare
asar4.3.1; its scoped glob14/minimatch11 graph is checked independently of npm
overrides. Root registry/integrity/license and moderate audit remain strict.
The separate private native SDK is not a workspace, inherits no root override,
and is inventoried/audited independently. Its fifteen moderate findings remain
open; there is no claim of a patched native SDK.

The package entrypoint refuses before target/source/env/key/authority/cleanup or
commands. Metadata auditing copies only two files, derives paired npm without
PATH/owner configuration, disables scripts and returns no execution capability.
Native workflow candidate/authorization/promotion chains depend on an exact
audit job which fails unconditionally even after clean metadata. PR contract
checks stay independent, and tested dependency/bypass variants fail closed.

The two native-storage assertions still require the exact empty collection;
native behavior, deletion/retention fences and lint policy did not change.
Generated lock changes were semantically reviewed: only existing-path asar,
glob and minimatch versions changed,221 entries removed and3 modern entries
added. No unrelated locked-version churn or blind legacy API override.

## Verification

Actual2513 unique Node/Vitest/Python/release cases, all five typechecks/builds,
locked private install, root audit0, asar roundtrip, provenance and cleanup pass.
Two Chromium fixtures remain skipped/separate. Native strict Clippy and14 Rust
tests pass. Focused16 SDK repeats are not double counted. Exact controller and
native journals/hashes are in IMPL642. All source/index checks stayed exact;
private build/Cargo roots were removed, with no owner database or real effects.

Two earlier copy-recipe failures are retained honestly; adding the checked-in
env template and SVG inputs corrected the harness, not product assertions.

## Verdict

Local source/test slice accepted for feature checkpoint/push after final privacy
readback. Merge/release remains held for exact corrected-tip hosted evidence,
separate browser/runtime/provider/source/operational gates and owner release
authority. Native publication stays quarantined. No merge, deployment, real
application/email, paid resource or new hosted batch was performed by this phase.
