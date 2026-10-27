# IMPL: Phase 629 — Jobs non-production integration validation

> Codex preflight: `bluey-ops`, `agent-toolkit`, and repository AGENTS.md.

## Scope

Does: disposable PostgreSQL/pgvector and Jobs Linux/container validation,
synthetic integration checks, bounded corrections to proven failures, and
independent review.

Does not: deploy, publish, send real applications/emails, alter production
accounts or flags, or change/start the Bluey host/overlay or Pinky product.

Base: `130ef807bc952874a2cb391e43511d7364ab1b0a`.
Plan: Round 629. The bounded nonproduction batch is complete: configured
PostgreSQL, Linux/container, Jobs workspace and full all-target Rust gates passed.
This is not production release approval.

## Files created / modified

| File | Action | Purpose |
|---|---|---|
| Round 629 | Created | Owner boundary, acceptance sequence and resource isolation |
| This IMPL | Created | Current executable evidence, corrections and cleanup ledger |
| `jobs/scripts/run-nonprod-postgres-tests.mjs` | Created | Private PostgreSQL 17/pgvector lifecycle inside the existing isolated launcher |
| Six Jobs Rust modules | Modified | Real configured-PG/API/lease/replay/cascade coverage; bounded BIGINT, typed conflict and capacity-parity corrections in FIX-795 / FIX-800 / FIX-801 |
| Jobs workspace manifests / lock | Modified | Compatible advisory corrections in FIX-796 / FIX-797; no host/overlay dependencies changed |
| CSV and current Browser packaging contract tests | Modified | Malicious and unique-extra-header behavior; exact new Jobs Browser Electron identity |

## Build and test

The existing pending Phase 628 canonical REVIEW remains preserved. A dedicated
VM was provisioned, isolated and used for a baseline Docker build. Its native
release addon and JavaScript compiled, but the stale dependency image's export
was intentionally cancelled (exit 130), not counted as a pass. The corrected
lockfile image and its smoke passed. Configured PostgreSQL failures led to the
reviewed corrections below; the final isolated batch passed all 30 cases. All PostgreSQL checks below ran
inside the canonical isolated launcher and central `mac-build` lock, not a
shared/default service. The child has a private empty home, generated SCRAM
credentials, owned source-cache/toolchain paths, no inherited provider secrets,
and only a generated disposable URL injected after the launcher's sanitization.

| Check | Observed result |
|---|---|
| PostgreSQL 17.10 / pgvector 0.8.3 cluster smoke | PASS, exit 0; real vector-distance query returned 0, exact data directory and loopback server identity verified |
| Wrong PostgreSQL major (installed 14) | Expected FAIL, exit 1 before init/start: major 17 required; task parent empty afterward |
| Owned postmaster SIGKILL while child exits 0 | Expected FAIL, exit 1: harness detects killed database and leftover postmaster identity; canonical process-group cleanup removes the exact root, parent empty |
| First configured Rust PG run | Compile FAIL, exit 101 before tests: test-only SQLite mapper treated a Result as tuple; corrected to propagate/map the decoder result, no production logic changed; PG/root cleanup verified |
| Early corrected Rust PG run | Migration/trigger regression PASS (1/1); concurrent-startup regression FAIL, exit 101: the production ready UPDATE inferred an i64 binding as int4; disposable PG shut down cleanly and exact root was removed. Remaining selected tests did not run. FIX-795 subsequently corrects the root cause. |
| Full corrected lockfile audit | PASS, exit 0 using private npm 11.21.1: 673 dependency records, zero vulnerabilities; metadata-only proof before installed-package tests |
| Corrected Docker npm ci | PASS: 88 installed packages / 91 audited records, zero vulnerabilities inside the pinned Linux build stage |
| Corrected pinned Dockerfile image | PASS, exit 0; Linux/amd64 under QEMU emulation; local image identity `sha256:bcab3b4e6cf7386acec09b1d48bfc2fd8f5293e2f495b30f7fd51f199083f385`, 1,598,872,103 bytes |
| Exact-image native addon / hardening smoke | PASS, wrapper exit 0: UID 1001, Node v24.17.0 x64, CSV 7.0.3; root write denied, no npm/alternate Node, normal N-API loader and real create/read/inventory/delete cycle |
| Exact-image missing-configuration startup | Expected exit 1 with redacted `startup_reconciliation_failed`, before real runner authority; wrapper asserts exit 1 and succeeds |
| Native Linux Rust storage | PASS: one mount-boundary unit case plus 13 explicitly enumerated release-profile integration cases, zero failures; production Docker context intentionally excludes tests, so fixtures were copied separately into the owned QA container |
| Jobs workspace quality | PASS, wrapper exit 0: all five typechecks; 2,025 tests (automation 804, Browser 219, runner 308, workflows 307, portal 387); all five builds with portal output confined to scratch |
| Actual Chromium exact-submit fixture | PASS (not skipped), 1 test inside automation count: one intercepted synthetic multipart submit accepted, delayed extra submit blocked; configured owned Chromium v1228 verified executable |
| Release / schema / CI static gates | PASS: release tests 32/32; SQLite/PostgreSQL parity 113 tables / 91 indexes; CI guard self-tests |
| Final configured PostgreSQL batch | PASS: 30 exact cases in 30 fresh private databases, zero failures; PostgreSQL 17.10 / vector 0.8.3, clean shutdown, postmaster PID file absent before the next gate |
| Final Rust formatting / strict all-target Clippy | PASS, exit 0; both integration-support features enabled, warnings denied |
| Final full all-target Rust suite | PASS, wrapper exit 0: 1,703 library + 1 binary + 134 integration = 1,838; zero failures/ignored. HTTP end-to-end 113/113. Library 2,509.19 s; HTTP 538.61 s |
| Dependency provenance / licenses | PASS: 663 registry lock entries, 631 unique versions, 14 commit-pinned references; differs intentionally from npm dependency-record count |

Successful smoke root (verified removed):
`/private/tmp/bluey-test-runs-phase629-pg-smoke-20261027/run.1793112886.258.8267166080`.
Killed-server negative root (verified removed):
`/private/tmp/bluey-test-runs-phase629-pg-kill-20261027/run.1793113005.2144.9775281620`.
No real account, resume, application, mailbox or provider was used.

Failures/corrections are retained: the initial smoke rejected PostgreSQL's
`inet_server_addr()::text` representation `127.0.0.1/32`; the identity check now
uses `host(inet_server_addr())` and still requires exactly `127.0.0.1`. That run
shut down and removed its owned root. Before smoke, separate static review caught
spawn-error completion, version-enforcement, credential-home isolation and
signal-exit false-green defects in the new harness; those were corrected before
qualification. Forced shutdown cannot count as a passing validation.

The old Phase 628 fixture preview's process handle is no longer present and
its exact owned root was verified absent. No exit code is inferred for that
expired handle; no replacement fixture server was started in this batch.

The code/dependency staged privacy checkpoint passed: 2,808 tracked paths /
2,532 text files. The documentation-inclusive checkpoint passed: 2,812 paths /
2,536 text files. Final documentation restaging/diff checks and the indexed
privacy scan also passed before commit with those same counts.
The inherited canonical Phase 628 REVIEW is excluded from the index.

## Dedicated Linux VM isolation correction

Created only `bluey-jobs-629-linux-amd64` / Lima instance
`colima-bluey-jobs-629-linux-amd64`: QEMU x86_64 emulation, 4 CPUs, 8 GiB RAM,
20 GiB data disk and 8 GiB root disk. Existing profiles stayed stopped; Docker's
default context stayed `default`. No host-folder or SSH-agent mount was requested.

Despite `--port-forwarder=none`, the generated configuration allowed a host DNS
listener from the new VM. Stopped that exact named VM and verified the listener
was gone; no existing process or profile was stopped. The owned, stopped Lima
configuration is now `plain: true`, `mounts: []`, `portForwards: []`; validation
passes without forwarding warnings. Its next start must use this validated
instance directly, not regenerate it through Colima. Docker commands ran
inside the guest through `limactl shell`, with a copied curated build context,
not a host daemon/socket or source-folder mount.

[Lima's plain-mode documentation](https://lima-vm.io/docs/config/plain/) describes
disabling dynamic forwarding and mounts while retaining shell access. The
[Colima generator](https://github.com/abiosoft/colima/blob/main/environment/vm/lima/yaml.go)
and [Lima port-forward defaults](https://github.com/lima-vm/lima/blob/master/templates/default.yaml)
explain the generated rule shape; actual observed listener absence, not the CLI
flag alone, is required before qualification.

The corrected VM was subsequently started directly through Lima and reached
Linux x86_64 / Docker 29.5.2. No host filesystem mounts or DNS listener appeared.
Plain mode also bypassed Colima's guest data-disk bind provisioning: Docker
initially pointed at the small root filesystem. Before any image build, verified
the exact owned 20 GiB ext4 disk and empty daemon, stopped only this guest's
Docker/containerd, and mounted that owned disk beneath both data roots. Both now
report the same 19 GiB free owned filesystem. No host filesystem was mounted.

The pinned Dockerfile build passed against a 2.16 MB curated Git archive of
the unchanged Jobs runner/build sources from base `130ef807`, copied into guest
`/tmp/bluey-jobs-629-build.EaSPzepY`. No checkout, home, SSH-agent, Docker socket,
provider credentials or overlay sources were mounted. The local image tag is
`bluey-jobs-runner-phase629:nonprod`. This is a non-production smoke build, not
a qualified/signed final-source release artifact. Its nonzero config/migration/
protocol contract digests describe the current working source, and do not
retroactively attest a future commit. No registry write occurs.

The corrected build overlaid only the seven updated Jobs manifest/lock files
in the owned guest context. Their SHA-256 values match the current working
files exactly; the lock digest is
`8b1f25a86b0e33953f9e32cfee818b97744518443750cd9c8a95c9d43db7ff35`.
The runtime measurement contains 410 files (unchanged closed limit 512), SHA-256
`f1b47f1c3b42f36e0379a0acf110fead1156e43f6bed9b986907f1eac3bc4615`.

Actual smoke container inspection confirmed `pwuser`, read-only rootfs,
network `none`, all capabilities dropped, `no-new-privileges`, exactly one
writable named runner volume and one read-only fixture-script bind from the
guest copy. No host path was bound into either container. The native smoke
and missing-config containers and volume were removed; cleanup was then
explicitly rechecked. Native Linux Rust storage tests were a separate executed
gate, not inferred from the addon pass.

Both native Rust gates subsequently completed with exit 0. Their private
launcher roots were verified absent. Before tearing down the owned VM, verified
its Docker daemon had no remaining containers or volumes. Gracefully stopped
and deleted only `bluey-jobs-629-linux-amd64` with its runtime data. Colima
reported a stale disk-unlock warning after the graceful stop; explicit checks
confirmed the exact instance/data disk absent and its host-agent/QEMU gone.
All four pre-existing product VMs remain stopped, Docker context is still
`default`, and no host DNS listener exists. About 16 GiB was recovered; no
source, user document, shared cache, production artifact or backup was deleted.

The Jobs quality run installed 668 packages from the corrected lock, audited
zero vulnerabilities, and downloaded Chromium only into its owned scratch
root. Electron binary download was disabled: Browser source/type/package
contracts were tested, not a signed native Electron artifact. Chromium fixtures
intercept requests and do not reach an employer. Its root was removed after
exit 0. Expected invalid-PDF/font fixture warnings and the existing >500 kB
portal-chunk warning remain visible; no warning threshold was increased.

The baseline export used default compression under x86_64 emulation; it was
cancelled only after verifying the exact task-owned build client PID/cwd/tag,
once the lockfile correction made that candidate stale. Its exit was 130,
even though some local image-store metadata was created. The corrected build
reuses the successful native compile cache and requests the documented local
image exporter with `compression=uncompressed,push=false`; Dockerfile/runtime
contents and pinned base identities are unchanged. This is an emulated local
smoke optimization, not a production OCI digest/attestation claim.

## Dependency audit corrections

The initial selected automation/runner audit did not cover the whole Jobs
workspace. The full audit found 13 package findings: one critical, nine high,
three moderate. FIX-796 records the CSV option/reachability distinction and six
new header cases (four reject, two compatibility-positive). FIX-797 records the
compatible workspace/leaf corrections, including the isolated Jobs Browser,
not the Bluey desktop overlay. The corrected lock changes 38 existing entries,
adds/removes none, and changes no major line except the documented CSV 6-to-7
update. Independent static review cleared the closure and positive-test gap.

Installed npm 11.3.0 crashed on workspace update with `edgesOut` and failed to
refresh overridden entries through install-only fallback. Those attempts are
not successful validation. Generated the lock using npm 11.21.1 in a private
disposable tooling directory, scripts disabled and no owner config/credentials;
targeted audited package names only, audited the result, then adopted its
generated lock. No global npm update or broad forced audit fix occurred.

## Deviations and follow-ups

- The prior handoff described pgvector as unavailable. Fresh path inspection
  found PostgreSQL 17.10 and pgvector 0.8.3 control/dylib files already installed.
  Actual disposable server/extension startup is now proven above.
- The shared scheduler policy is at
  `/Users/uno/Projects/shared-runners/SCHEDULER_POLICY.md`, not the stale
  toolkit-root path. Use its existing central default state and `mac-build`
  lock; do not create a parallel quota ledger.
- Rosetta is not present in the inspected path/receipt. QEMU amd64 Linux execution
  is observed; native amd64 hardware/performance is not proven.

## Final configured PostgreSQL coverage

The final batch runs the real Axum-handler/PG response path, migration catalog
and trigger checks, concurrent boot-ready replay, legacy v2 heartbeat
compatibility, source-verifier lease/attempt/terminal replay and parent cascades,
and the selected operational/local authority, policy, packet, deletion and
account-lock regressions. The API test injects an already-authenticated worker
extension; it does not prove fleet HMAC middleware or a listening socket.

Each of the 30 exact cases receives its own fresh database. The aggregate
reported `tests:30, passed:30, failed:[]`; PostgreSQL reported status 0,
shutdown attempted and server exited. The following Rust quality child asserts
the postmaster PID file is absent, strips both PostgreSQL URLs and the require
flag, and uses SQLite as primary backend. Consequently configured-only tests
in the full offline suite are not counted again as live PostgreSQL evidence.

Earlier shared-database and isolated attempts remain failures, not overwritten
passes: 15/27 with immutable identity collisions; 22/28 with the reverted
generation-three fixture and stale beta fixtures; then 25/28 exposing the exact
HTTP conflict classification, stale replay assertion and pre-authorization
capacity parity defect. FIX-798 through FIX-801 record the corrections without
loosening production identity, sequence, admission or irreversible-action fences.

Final successful canonical root (verified absent after wrapper exit 0):
`/private/tmp/bluey-test-runs-phase629-final30-20261027/run.1793120675.5618.2087088370`.
It remained owned and isolated through the full Rust child, then the canonical
launcher removed it automatically. All 20 task-specific Phase 629 run parents
were explicitly empty afterward. Host free space recovered from about 32 GiB
to 39 GiB; no live root or shared cache was deleted manually. Source/manifests/
harness still matched the staged tested bytes at completion.

Final read-only VM inventory again confirmed all four pre-existing product
profiles stopped, the exact Jobs profile/instance/data absent under Colima's
actual `_lima` / `_disks` paths, and Docker context `default`. No production
account or credentials were needed. No deployment, registry write, application,
email, calendar action, production flag or host/overlay change occurred.

## Reproduction commands and exact configured selection

All commands below require the authoritative Jobs checkout. Wrap heavy work in
the existing `runner_schedule.py with-machine-lock --machine mac-build --`
queue and `scripts/run-bluey-tests.sh --`; never run Cargo or the database
harness against shared/default paths. The harness supplies private credentials
and the disposable database URL after launcher sanitization.

The final configured selection, executed serially in fresh databases:

```sh
node jobs/scripts/run-nonprod-postgres-tests.mjs \
  --pg-bin /opt/homebrew/opt/postgresql@17/bin \
  --case-filter 'api::jobs_managed_cloud_releases::tests::postgres_boot_routes_with_authenticated_extension_persist_and_replay_when_configured' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_local_run_claim_and_submit_recheck_current_authority' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_local_unknown_before_authorize_reserves_and_replays_capacity' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_local_unknown_click_started_missing_capacity_is_rejected_without_mutation' \
  --case-filter 'postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing' \
  --case-filter 'db::jobs::managed_cloud_release_authority_tests::postgres_source_verifier_boot_migrations_replay_once_and_install_fences_when_configured' \
  --case-filter 'db::jobs::managed_cloud_release_authority_tests::postgres_original_source_verifier_startup_is_atomic_and_replays_under_concurrency_when_configured' \
  --case-filter 'db::jobs::managed_cloud_release_authority_tests::postgres_original_source_verifier_v2_direct_ready_and_refresh_remain_nonboot_when_configured' \
  --case-filter 'db::jobs::original_source_verification_tests::postgres_boot_leaseable_probe_attempt_fences_replay_and_cascades_when_configured' \
  --case-filter 'db::jobs::managed_cloud_release_authority_tests::postgres_managed_unmanaged_submit_pairing_guards_are_fail_closed_when_configured' \
  --case-filter 'db::jobs::original_source_verification_tests::postgres_subject_scan_skips_invalid_first_when_configured' \
  --case-filter 'db::jobs::job_integrity_composition_tests::postgres_lock_first_read_committed_observes_waited_writer_when_configured' \
  --case-filter 'db::jobs::workspace_representation_tests::postgres_workspace_list_and_detail_allow_lockable_snapshot_reads' \
  --case-filter 'db::jobs::workspace_representation_tests::postgres_account_export_chunks_102_source_positive_jobs_at_authority_cap' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_auto_submit_revocation_and_execution_share_one_fence' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_canonical_bytes_conflicting_with_projection_block_release_and_listing' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_canonical_taxonomy_migration_rejects_out_of_range_authority_values' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_canonical_track_policy_ledger_encrypts_and_rejects_projection_drift' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_corrupted_operational_hold_sha_blocks_release_and_listing' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_execution_authority_locks_integrity_before_account_policy' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_execution_lease_claim_rejects_deleting_account_without_creating_lease' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_operational_context_holds_account_fence_through_admission_commit' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_operational_hold_advisory_lock_serializes_two_connections' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_operational_hold_migration_replay_is_exact_and_seedless' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_operational_holds_preserve_history_refs_and_encrypted_context' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_packet_commit_exact_replay_precedes_current_policy_revalidation' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_packet_commit_locks_entitlement_before_rechecking_attempt_reservations' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_packet_commit_recomposes_current_effect_inputs_before_any_meter_or_debit' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_packet_commit_waits_on_parent_account_before_deletion_children' \
  --case-filter 'db::jobs::postgres_local_authority_tests::postgres_resume_publication_locks_account_before_policy_children' \
  -- cargo \
  --config profile.test.package.curve25519-dalek.opt-level=3 \
  --config profile.test.package.ed25519-dalek.opt-level=3 \
  --config profile.test.package.sha2.opt-level=3 \
  test --offline --locked --manifest-path server/Cargo.toml --lib \
  --features integration-test-support,business-messaging-simulator-test-support
```

After clean PG shutdown, the same private Cargo root was reused by a child with
an allowlisted environment, empty credential home, SQLite primary and neither
PostgreSQL URL nor `BLUEY_JOBS_REQUIRE_POSTGRES_TESTS`. It executed:

```sh
cargo fmt --manifest-path server/Cargo.toml -- --check
cargo clippy --offline --locked --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support -- -D warnings
cargo --config profile.test.package.curve25519-dalek.opt-level=3 \
  --config profile.test.package.ed25519-dalek.opt-level=3 \
  --config profile.test.package.sha2.opt-level=3 \
  test --offline --locked --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support
```

Dependency/metadata checks used `node jobs/scripts/check-provenance-licenses.mjs`,
`node jobs/scripts/privacy-gate.mjs`, `node --check` for the new harness and
`git diff --check`. The privacy gate scans indexed blobs; documentation is
restaged before its final scan. Commands do not enable production effects.

## Local closeout

The separate reviewer accepted the bounded source/evidence batch with no open
finding. It is an independent AI review, not human or production release approval.
After the full Rust run, the final unchanged-source release checks passed again:
32/32, schema parity 113 tables / 91 indexes and CI guard self-tests. Their
additional private launcher root was removed after exit 0; all 21 Phase 629 run
parents were then verified empty. Formatting/diff and dependency provenance
checks passed. Local commits are split into PostgreSQL authority/validation,
Jobs dependency corrections, and this evidence/handoff; no push or PR is created.

Exact-tip hosted CI, hosted service/network tests, signing and registry read-back,
provider/ATS/OAuth qualification, live capacity, and rollout/in-flight rollback
rehearsal remain separate release gates. No production flag is enabled by this
batch; the inherited canonical Phase 628 REVIEW remains unchanged and untracked.
