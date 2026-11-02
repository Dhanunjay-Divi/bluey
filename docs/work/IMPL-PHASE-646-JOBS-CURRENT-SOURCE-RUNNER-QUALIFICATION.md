# IMPL646 — Current-source runner qualification integration

Preflight `bluey-ops`, agent toolkit, SPARC/team guidance and bounded Arena review.
No upstream runtime/installer/hooks were started. Basec6935225; feature branch
`feat/phase-646-jobs-current-source-runner-qualification`. Jobs only.

## Scope

Manual port of seven existing paths and eight new helper/test files from old
Phase639 onto the modern Phase645 Jobs graph. Preserve Phase642 dependency
inventory, modern asar/overrides and quarantined native SDK; do not copy A's old
package/lock graph. Gate changes include earlier independently reviewed OCI
ArgsEscaped compatibility, exact unordered Env-set checks, manifest/config
identity and bounded TAR/PAX rejection diagnostics. None admits unsupported PAX
entries or relaxes exact command/user/platform/environment values.

Failure-only retention keeps the original image failure/exit1 and all D1/D2/
release authority false; partial/uncertain bytes remain unqualified. Read FIX881
in old authority for historical rationale, but do not transfer old execution
proof to changed current-source bytes. FIX883 specifies the explicit commit
contract and exact797 selected-tree policy.

## Design / ownership

Arena compared mandatory externally approved single-source admission with a
two-revision image/harness protocol. Select the single-source design: one exact
commit equals HEAD throughout, with post-commit private controller approval.
Two-revision qualification is feasible but unnecessary here and introduces
downgrade/provenance states. No vote substitutes for actual runtime evidence.

Terra owns eight new helper/test files and the workload API; core integration
worker owns seven existing files/harness/parser/test wiring; parent owns exact
tree-count pin, docs/controllers/resources and qualification. Separate review
follows both source freezes. Source A and B fixtures remain separate/untouched
by this C integration. No merge/rebase or broad branch copy.

## Observed verification

Separate source review accepted bounded progression with no P0–P2 findings.
Ten Node22 syntax checks and four Python AST parses passed; staged whitespace
was clean. Exact-source gate24772 terminated0 on846-file manifest
`a93f795803c6b224879f5f92ee385000db16449c8afd8c7d66147e3a6b305ec1`.
It passed2685 unique cases: full Node432, Vitest2114 (812/235/315/321/431),
Python63 (39+24), release11 and managed-cloud65. Focused Node163, Python63
under each of Apple3.9.6/Homebrew3.14.6, portal68 and SDK16 repeat these cases
and are not added. All observed suites had zero skips; both actual Chromium
submit-guard and document-import fixtures passed. Five workspace typechecks/
builds, strict web/cloud audit0, provenance, modern-asar roundtrip and actual
prerender contracts passed. Repeated portal builds and the exported36 files/
4,863,397 bytes match the prior reviewed portal manifest exactly:
`7aa32ac16ae2c230d8d29766af866051371adc409a50850d8aa78f11dc645084`.
No new browser-visual, Rust/server, guest or production proof is inferred.

Private evidence `phase646-node-nyIjCf`: controller
`9594e14922cb2ad3bc1f23adca3889760de23c02caaa8d972015af62d57e3615`;
controller journal `44ffd35043a5b22248f2178b26ecd649aa3a1ed2cf942f2920faa8b6ba5a4ee1`;
inner journal `54e304e830749df2521f2d9bc63860877d8205a95238fe54db3cfe07711a8ef1`.
Source/status/index and pinned Python tool identities matched after execution;
canonical launcher self-test, owned root/parent/control-home cleanup passed.
Staged privacy3025paths/2741texts, whitespace and cargo fmt check passed.
Only documentation is updated after that frozen run; product scope remains
identical, not a claim that the final documentation index was tested unchanged.
Final separate review rehashed all72 inner raw streams plus launcher output and
all846 live product-scope files, reconciled counts/limits and accepted this
bounded feature checkpoint with no P0–P2 finding.

Private controller review first found two evidence-admission gaps (unbound Python
runtime and terminal-only focused counts). Both were corrected before execution:
exact launcher/native runtimes and npm-path shadow absence are revalidated,
positive counts and zero skips/failures are mandatory. Hash-bound parser probes
passed24/0failed/0skipped and separate review accepted the corrected controller.
No rejected controller was used for a full run. Native SDK remained uninstalled
and its strict metadata refusal was observed; native publication stays blocked.

Actual guest reader/no-follow approval and image build/
failure-retention/teardown come only after committed source/controller pins.
Count observed tests/skips, not historical full-suite totals or definitions.

No deployment/publication, flags, real applications/emails, provider credentials,
new paid resource or source-approval bypass. Whole Jobs goal remains active.
