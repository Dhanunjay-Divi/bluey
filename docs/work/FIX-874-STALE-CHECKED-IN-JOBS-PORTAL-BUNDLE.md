# FIX-874: Stale checked-in Jobs portal bundle

Preflight: `bluey-ops`; Round643/current code authoritative.

## Issue / observed cause

[Jobs CI run41379117231](https://github.com/Dhanunjay-Divi/bluey/actions/runs/41379117231)
on00617b2c passed Darwin storage and the Linux Jobs tests, typechecks, builds,
dependency audit and messaging containment. The next command,
`git diff --exit-code -- web/jobs`, failed because building current source
changed the checked-in portal assets. Later workflow steps are not qualified.

The earlier local controller built into its disposable private copy, proved
source unchanged and then removed that copy. It did not export or compare the
generated output against tracked assets. A successful private build therefore
did not establish the separate checked-in-bundle freshness invariant.

## Fix / acceptance

Build current reviewed Phase643 source twice with admitted Node22 and the
locked dependency graph. Compare complete generated path/size/hash inventories;
retain only matching bounded regular generated files privately before cleanup.
Parent reviews that inventory and refreshes only `web/jobs`, then verifies the
source-built bytes against the tracked/staged output. Keep the CI gate intact.

## Status / limitations

Actual gate38567 terminated0. Two portal builds and the final pre-export
inventory matched exactly.35 files/4,845,198 bytes were retained and copied back
with hash readback. The prior34 tracked files were separately backed up before
23 obsolete generated filenames were removed; Git also retains the originals.
No source/user file was deleted. Export manifest SHA256
`5f9631c543e6d8c85c1370f13213914dd8e518408bfcd6b98d8533e5234fc787`.

The private refresh tool checked clean tracked output/no untracked target files,
HEAD/branch, successful frozen-source controller terminal and exact build/export
manifest before synchronization. It did not rebuild or deploy. Hosted freshness
on the new commit is still pending. No assertion, audit threshold, CI freshness
gate or native packaging quarantine was weakened.
