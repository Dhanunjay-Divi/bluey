# FIX-871: Isolate vulnerable optional native packaging tools

> Codex preflight: `bluey-ops`; current Jobs source and Round642 are authoritative.

## Issue

PR39 Jobs CI failed the unchanged moderate dependency audit with fifteen findings
from the optional native Browser release SDK. The owner selected Web/cloud Jobs
first; native packaging remains a separate blocked migration.

## Root cause

`jobs/browser/package.json` placed electron-builder in the ordinary Jobs workspace
graph. Its legacy asar/glob/dir-compare/rimraf/ejs chain depends on minimatch
versions below the patched11.0.1 boundary. Several consumers require the old
callable minimatch or callback glob APIs. A global major override would conceal
an incompatible graph, not fix the native toolchain.

The asar used by root release-contract tests also needs an explicit root
declaration: a dependency installed only under the browser workspace cannot be
reliably imported by `jobs/scripts/`.

## Fix summary

- Remove electron-builder from the Jobs workspace. Keep its exact private
  manifest/lock in `jobs/browser/packaging`, outside root workspaces/overrides.
- Declare modern asar4.3.1 at its actual consumers; scope compatible glob14.0.3
  and minimatch11.0.3 pins to it. Validate resolved graph versions independently
  of the manifest override. Retain root registry/integrity/license requirements.
- Inventory the separate SDK metadata without treating license inventory as a
  security audit. Its strict metadata-only audit copies two files into private
  scratch space, uses paired npm with a closed environment and public registry,
  disables scripts and installs/executes no SDK.
- Refuse native packaging immediately before source/key/authority/command or
  cleanup effects. Release candidate, authorization and promotion workflows
  depend on a central SDK audit followed by unconditional quarantine refusal.
  A future clean metadata audit still cannot unblock native publication.

## Files modified

Root and browser manifests/locks, provenance/graph policy and tests, native
metadata auditor and synthetic tests, package entry-point refusal, release
workflow/contract tests, SDK README, Round642 and implementation/review records.

## Edge cases handled

Separate SDK identity and exact version pin; forbidden links/parent workspaces;
non-registry URLs, credential-bearing URLs and noncanonical integrity; metadata
replacement; unavailable/failed/interrupted audits; environment secret exclusion;
clean audit without execution authority; missing or bypassed workflow dependency;
publication despite skipped/failed SDK audit; existing release-output retention.
Full runtime-backed case results are pending and must not be inferred from this
list of authored coverage.

## Observed checks

- Pure inventory-policy tests:4 passed; whitespace check passed.
- License inventory:445 web lock entries/434 unique versions and277 SDK lock
  entries/263 unique versions; fourteen source provenance rows passed.
- Root lock SHA256:
  `ac64330dca973a22beb4569c96551af683fabd5edb0ee5592c43554ac098b00b`.
- SDK lock SHA256:
  `b2f0133c0f47ebaa233e2f591e996f1ebfb456eb7f324e67fcc3ad48700a2cbb`.
- Actual public-registry metadata audits under private Node22.23.4/npm10.9.9:
  web exit0, zero vulnerabilities; SDK exit1, fifteen moderate vulnerabilities.
  No node_modules or SDK execution. Source bytes unchanged; canonical run root
  removed and parent empty after terminal0 of the audit-observation controller.
- Durable audit record `phase642-metadata-audits-1793583279234.json`, SHA256
  `d53816df9d7986183b226ef0fd7c5d6e69e857040ee02f9f793348132bb10c81`,
  in this chat's private evidence folder. Native audit failure is not a pass.

## How to test

The reviewed Node controller91518 now passes locked install, dependency-backed
contract/SDK tests, all Jobs tests/typechecks/builds and modern asar roundtrip.
Native-only10970 also passes strict Clippy and14 tests. See IMPL642 for exact
unique counts, journals and cleanup. The16 SDK synthetic cases and37 workflow
negative variants are exercised, not physical native packaging qualification.

## Known limitations

Native SDK security, installed graph, physical packaging and platform canaries
remain blocked. The root audit is a dated metadata check, not proof of installed
package bytes or product runtime correctness; the separate installed web graph
and local tests/builds are now qualified as recorded in IMPL642. Two Chromium
fixtures and hosted CI on the corrected source remain pending. The approved extra CI batch
was consumed; no retry, push-triggered synchronize, deployment, publication,
production flag or real application/email action is authorized by this fix.
