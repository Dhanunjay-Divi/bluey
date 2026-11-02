# IMPL642 — Jobs CI remediation

Status: bounded local source/dependency/native qualification passed; hosted CI,
two separate Chromium fixtures and release remain open. No production action.

## Current local qualification

Node controller session91518 terminated0 on exact832-file private-copy closure
`3c682115a8bbb1b8f2c943d5855568e3893be04c021c6b537c18cf9db381ca97`.
Official Node22.23.4/bundled npm10.9.9 locked install with scripts disabled,
root moderate audit, provenance, all five typechecks/builds, modern asar
pack/unpack/readback, SDK synthetic tests and release guards passed. Unique
automated cases2513: full Node355, Vitest2079, Python31, Browser release11 and
managed-cloud release37. The focused16 SDK rerun repeats cases in browser235
and is not added again. Two known Chromium integration fixtures were skipped;
they remain explicit separate gates, not browser-runtime proof.

The actual SDK metadata CLI returned its exact expected refusal. SDK was never
installed/executed; its separate audit still has15 moderate findings and every
native publication path stays quarantined. Root installed graph and manifest/
locks remained exact; source/index unchanged. All build/install output was in
the owned private copy; canonical parent/control home removed after terminal.

Evidence folder `phase642-node-gwTSxS` in this chat's private evidence directory:
controller journal SHA256 `c072e1441637baf246e3a68815506cba5678b95fa46193b11a181d96d9ce93ab`;
inner journal `41c4429d87f0de1bb45e6c56bc0acf2e42d611eaf7a681bc9bfd666df8c02e23`.
Controller SHA256 `9188cba1d5039daaa13dcbec4a7bb9185f66adc52021053edbb475347c592df9`.
Two prior private-copy attempts failed because the prepared closure omitted a
runner env template, then the two portal SVGs. They remain failed. Only those
tracked inputs were added to the copy; the original829-file manifest stayed
exact. No product source/test assertion was weakened to fix the harness.

Native-only10970 then terminated0: launcher self-test, native manifest formatting,
strict offline/locked all-target Clippy and14 Rust tests, zero failed/ignored.
Whole checkout2988-file snapshot, status, native module and empty index matched;
fresh Cargo/root/parent/process cleanup passed. Journal
`phase642-native-f3224b82276c.jsonl`, SHA256
`97e218d87c3731b8b3adfe9c9224e69f2a3813c556c1da6cd75c88c6d6e40ea6`.
This is native library qualification, not an addon build or physical Browser.

Independent source/controller review found no reproducible P0–P2 findings.
Final docs/privacy/checkpoint follow these results; the earlier three hosted
workflows remain failed on their original SHA. No retry or new PR-triggered
batch is authorized by those local successes.

## Implementation and earlier observations

Parent owns native test assertion corrections, root policy/inventory, generated
locks and docs. Existing workers own SDK refusal/audit helpers and release-workflow
closure. An independent reviewer checks the combined source. Work is isolated from
Phase639 image evidence and the active Phase641 Rust gate. A compact Sol worker
was disclosed for workflow source; no external model CLI or account rotation.

The minimatch advisory is GHSA-g3hv-75wg-m249 (all versions below11.0.1). Current
legacy asar/dir-compare packages call minimatch as a function; a blind11 override
would be an API break. A safe dependency choice and actual regressions are still
required. The root audit threshold and strict Clippy policy remain unchanged.

The owner's web/cloud-first choice is implemented as a separate private SDK
metadata inventory plus an unconditional native publication quarantine. A clean
metadata audit is explicitly not installed-graph or execution authority. No SDK
installation/execution has occurred. See `jobs/browser/packaging/README.md`.

Four pure inventory-policy tests passed. The release-contract test command was
attempted before dependency installation and failed module resolution for asar;
none of its cases ran. Root scripts now declare their own direct asar dependency
instead of relying on hoisting. Full dependency-backed tests remain pending.

Lock-only attempts with local npm11.3 retained the wrong glob/minimatch graph;
fresh resolution with npm11.3 and the private official Node22/npm10.9.9 crashed
in Arborist (`edgesOut`). npm12.2.1 refused a transitive remote tarball under its
default fetch policy; that policy was not relaxed. A private pinned npm11.21.1
resolver produced the required modern asar/glob/minimatch versions, but a fully
fresh graph introduced unrelated bundled optional packages without per-entry
integrity metadata and failed the existing inventory. No failed graph was
installed or copied over the source lock. A minimal update preserving unrelated
locked versions is being qualified. All tool capsules are task-owned, not global
tool updates or release artifacts.

The minimal npm11.21.1 update subsequently passed graph/workspace/provenance
inventory and was copied/read back as root lock `ac64330dca973a22beb4569c96551af683fabd5edb0ee5592c43554ac098b00b`.
The prior lock is privately backed up; canonical root cleanup passed. Actual
metadata audits under official Node22.23.4/npm10.9.9 then observed web exit0 with
zero vulnerabilities and isolated SDK exit1 with fifteen moderate findings.
No SDK installation or execution occurred. Audit record SHA256
`d53816df9d7986183b226ef0fd7c5d6e69e857040ee02f9f793348132bb10c81`;
see FIX871. This does not yet qualify an installed graph or full Jobs runtime.

The dependency-free Browser workflow contract CLI passed on the current YAML,
the new metadata auditor passed Node syntax validation, and the normal native
verifier returned its exact expected quarantine refusal without audit or SDK
execution. The dependency-backed37 workflow negatives and16 SDK cases have not
yet run. Semantic lock comparison against HEAD found only three existing-path
version changes (asar, glob, minimatch), three added modern dependencies and221
removed entries; unrelated locked versions were preserved.
