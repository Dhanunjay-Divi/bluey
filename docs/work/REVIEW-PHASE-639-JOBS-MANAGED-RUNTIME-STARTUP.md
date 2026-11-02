# Independent review — Phase 639 managed runtime startup

Date: 2026-11-01
Reviewer role: independent read-only source reviewer
Verdict: 🟡 source clear for the bounded D0 preflight; executable qualification pending

## Scope

Reviewed the frozen D0 candidate line by line:

- `jobs/scripts/run-nonprod-managed-runner-vm-tests.mjs` — SHA-256
  `d295b116273b485e631f8b4049c2db92d8fed9e125b715e1df31010e55c3e61c`
- `jobs/scripts/nonprod-managed-runner-vm.test.mjs` — SHA-256
  `63a73cd7f126af2468aea538bbb924c92f50e806c7d030060b95b9a85cdbca84`

This was a static review only. I did not run Node tests, start Colima/Lima, boot a VM, build an
image, or exercise a managed runner.

## Findings

No remaining P0–P3 source finding blocks the bounded `--preflight-only` execution.

The previously reported launch blockers are resolved in the reviewed bytes:

1. The launcher marker parser now requires the canonical nine fields and their canonical order,
   and binds the marker to the owned run root, run name, UID, PID/group identifiers, creation
   epoch, and state.
2. Setup failures are inside a closed failure boundary. A partially established private state root
   is reported and retained rather than being represented as successful cleanup; no VM has been
   started at that point.
3. Once VM startup has been attempted, an inventory-read failure no longer suppresses the stop
   attempt. It still blocks destructive deletion, so ambiguous state is retained for recovery.
4. Listener evidence compares strict same-UID TCP and UDP snapshots. New UDP sockets are denied;
   every new TCP listener must be the exact loopback management listener and belong to a recorded
   owned Lima PID. A sibling/helper listener therefore cannot be hidden by checking only the main
   host-agent PID.
5. Admission pins the complete base commit
   `2b85339a403cb5272a56f1b2f0bf8065ca3b19ed`, not an abbreviated prefix.

The cleanup path remains appropriately fail-closed: profile deletion and state-root removal occur
only after stopped-state, inventory, recorded-PID, socket, ownership, inode, and marker evidence is
available. Unknown cleanup retains state instead of claiming success. The private Colima, Lima,
Docker, HOME, cache, disk, and configuration roots remain separate from owner/global state.

## Runtime gates still open

Source clearance is not runtime acceptance. The parent-owned D0 execution must still prove all of
the following on the frozen bytes:

- the pure/syntax tests pass;
- a new x86_64 QEMU VM actually boots with 4 CPUs, 6 GiB memory, an 8 GiB root disk, and a separate
  16 GiB data disk;
- generated Lima/Colima configuration and guest inspection show no owner-home mounts, agent/config
  inheritance, published application ports, or non-loopback listener exposure;
- the exact loopback management listener and all owned VM PIDs are observed and later absent;
- stop/delete succeeds, the private socket and state root are absent, and global profiles/context
  remain unchanged;
- the canonical outer launcher reports terminal status and cleanup without a retained unknown-state
  failure.

This D0 preflight is compatibility and isolation evidence only. It does not qualify an image,
native add-on, measured runtime authority, runner startup protocol, provider action, production
deployment, or release artifact.

## D0 attempt 30976 and listener-parser correction

Parent-reported attempt 30976 failed before `vm_start`, at `source_snapshot`, with
`listener_record_malformed`. No VM booted. The canonical launcher parent was removed; the separate
VM root contained only its marker and was removed after exact marker, inode, ownership, entry, and
hash checks. This is failed prerequisite evidence, not a D0 runtime pass.

I independently reviewed the subsequent two-file correction at these frozen hashes:

- harness: `39f5c3efdd37d6f471734238e1dfa50fb7e53b3b3c77f7cd1ae245e358c355aa`
- tests: `4dfbdac6973dd5fad7427c8ce20f830073c2d490ee6b60728614d19a57918d86`

No P0–P3 finding remains in the corrected parser/delta implementation:

- The parser now consumes the actual `lsof -Fpn` process/file/name structure and requires a
  complete `p`/`f`/`n` record. It rejects missing, duplicated, truncated, out-of-range, or
  unpaired PID/file/name fields.
- Endpoint syntax is validated and reduced to bounded structural fields plus a SHA-256 identity.
  Connected endpoints preserve that fact. The macOS unconnected UDP form `*:*` is represented
  explicitly as `{ address: "*", port: null, unbound: true }`; it is not reclassified as an
  approved listener.
- Baseline and post-start inventories are bounded at 1,024 records. Exact PID, descriptor, and
  endpoint identity determines the delta. An unchanged ambient TCP/UDP baseline is permitted;
  every new UDP record is denied.
- New TCP records remain bounded to 64 and pass the pre-existing strict policy only when they are
  unconnected, bound to the exact loopback management port, and owned by a recorded VM PID.
  Unbound, connected, non-loopback, wrong-port, and sibling-PID records fail closed.
- Reuse of an owned VM PID already present in the baseline is rejected as ambiguous.

The accompanying tests cover the observed multi-file `lsof` format, more than 64 ambient records,
wildcard/IPv6/connected/unbound forms, duplicate and malformed records, new sibling TCP listeners,
and new bound or unbound UDP records. Test execution is parent-owned; this paragraph records static
coverage, not an independently observed test pass.

The D0 verdict therefore remains source-clear but runtime-pending. A fresh attempt must still boot,
inspect, stop, and delete the VM and satisfy all cleanup and host-state gates listed above.

## D1 managed-runner bundle and claimed-image bindings

I independently reviewed the following four-file D1 source snapshot:

- `jobs/automation/src/managed-cloud-runtime.ts` —
  `1632268e66f13f5194cecc30f4a3597e71eb55d41f6c2e08da55e656d4e2f430`
- `jobs/automation/tests/managed-cloud-runtime.test.ts` —
  `79433491fd23ba9da599453b87c3c2db9ab2827869bfe60129c4550b6b7f65a4`
- `jobs/runner/src/server.ts` —
  `2db2efc7ae8a9af60712316db416e6d039042c0ef855038054ea811acde637fb`
- `jobs/runner/tests/server-storage-startup.test.ts` —
  `3548046d1f5f5b3a03dfa8bc088fbed3c88020088ec0d9abf423ef6a518ae75e`

No P0–P3 source finding blocks focused execution.

The automation-bundle digest is derived only for the exact `jobs-runner` / `managed_runner` role,
after the existing canonical measurement document, complete current file-set, and every measured
file byte have been validated. Its entries are the UTF-8-sorted measured `app/automation/` paths
and digests, with both `package.json` and `dist/index.js` required, under the same v1 domain used by
the release-gate derivation. `managedCloudRuntimeConfig` carries that locally derived value and the
existing managed API client remeasures it before claim and subsequent protected requests.

Runner startup derives the process-runtime grant once. For a managed runner it compares the
locally measured bundle digest with the grant's bundle digest before the managed runtime claim. It
then compares the claimed runtime instance's exact, canonical artifact digest with the grant's
runner-image digest before constructing `RunnerVolumeClient`. Claim failure cannot produce an
undefined managed instance, and the existing nonmanaged path continues to skip only these two
managed-specific comparisons. Existing process-runtime validation remains responsible for the
rest of the runtime-grant fields.

The five automation tests cover canonical derivation, all measured automation files, coherent byte
changes, runner-only changes, missing required files, tampering, unmeasured files, wrong roles, and
post-configuration remeasurement. The five startup tests cover exact matches, bundle drift or
absence, wrong component/role, image mismatch, nonmanaged compatibility, and the source ordering
relative to claim and volume construction. This is static coverage only; the parent had not yet
executed these tests at this review checkpoint.

D1 remains a foundation, not artifact-execution proof. Still unverified are the focused test run,
CLI/Rust parity and wiring, an actual built image containing the measured bytes, actual managed
claim/startup behavior, native/browser runtime authority, and any production or release action.

## D0 retry 16562 and Lima forwarding override

Parent-reported retry 16562 was terminated through the owned harness after Lima attempted an
automatic wildcard DNS forward despite Colima's `none` setting. It ended with terminal failure and
`cleanupProven: true`; the VM and helper PIDs were absent and only private bounded diagnostics plus
the ownership marker were retained. It is not a successful D0 boot qualification.

The parent traced the behavior to the pinned Colima 0.10.3 and Lima 2.2.1 forwarding defaults. I
then independently reviewed the two-file corrective snapshot:

- harness: `f331bb6ec81ffa314077f0faa06e5890f8ddd7fe5bea4eb8192cc1f628923428`
- tests: `b4474a5de3b5b40f478fee260088de6958a370addeb3018c39d2e2c1a24f554f`

No P0–P3 finding blocks the next parent-owned D0 attempt.

The harness creates the exact private `$LIMA_HOME/_config/override.yaml` before its first CLI call.
The policy is an exact canonical byte sequence whose first rule matches guest IP `0.0.0.0`, sets
`guestIPMustBeZero: false`, spans ports 1–65535 and both protocols, and sets `ignore: true`. The
directory is required to be owned mode 0700; the single-link file is owned mode 0600 with exact
size, bytes, and digest. Their device/inode identities and contents are rechecked before every
allowed external command, including cleanup. That guard is disarmed only immediately before the
already identity-checked private Lima directory or complete private state root is removed; no later
VM command is authorized without it.

The policy is not the sole evidence. The same-UID before/after TCP and UDP delta remains mandatory
and fail-closed. Before a success report, the bounded, stable, owner-controlled `ha.stderr.log` must
be valid UTF-8/ASCII-compatible text and must contain neither a successful TCP/UDP forwarding line
nor a forwarding-setup failure. Oversized, malformed, replaced, writable-by-others, or otherwise
ambiguous logs fail. The report also requires the exact policy SHA-256, preventing a different
policy from being represented as the reviewed one.

The focused tests statically cover exact policy bytes and negative field mutations, ownership,
mode, link count and size, forwarding success/failure log forms, malformed/oversized logs, creation
before CLI/start, per-command validation, and report identity. Their execution remains
parent-owned; at this checkpoint the corrected D0 runtime had not yet passed.

## D0 attempt 74516 and closed listener-failure evidence

Parent-reported attempt 74516 reached `listener_observation` and failed with
`unexpected_new_udp_listener`. Cleanup was proven, but the run did not retain enough information to
attribute the UDP delta; it therefore does not establish either ambient churn or VM ownership. The
terminal failure journal SHA-256 was
`222d6a280bdd85cf002af4dce3870f9e4a5fd4091b99b7c2fe594026e595c417`. D0 remained unqualified.

I reviewed the diagnostic-only two-file successor:

- harness: `4b3bf0ddb73d3b04b326d4676d614c45a7be55589763538161dc167c00ca4e0c`
- tests: `73ab2ab1455d4bda919aa567f168f209ba9138e838f1c46bbec38dff8929e38e`

No P0–P3 finding blocks one diagnostic retry. The change does not add a process tool, process-name
heuristic, listener exemption, or acceptance path. `listenerDeltaAnalysis` remains the single
validated source for both evidence and policy. The strict policy still rejects every new UDP row
and still requires the exact owned loopback management TCP listener.

Immediately before policy evaluation, the harness records only closed per-transport metadata:
delta count, counts categorized as recorded-VM PID / baseline-observed PID only / unknown PID, and
one SHA-256 over the protocol plus sorted PID/file-descriptor/endpoint identities. It emits no IP,
port, PID, descriptor, executable path, argv, environment, raw `lsof` output, or extra event. Exact
keys, integer bounds, count sums, version, no-network-claim flag, and digest shape are revalidated
by `failureReport`. Passing the unchanged policy clears the temporary evidence, so it cannot appear
in a success report or qualify a run.

The tests exercise owned, baseline-only and unknown categorization, count-sum and digest
validation, rejection of extra/raw fields, and absence of endpoint text in serialized failure
evidence. Test execution and the fourth VM attempt remain parent-owned and unverified here.

## Native process and usernet candidate review

Reviewed the four-file candidate at these hashes:

- native producer: `3c53430e3e937a27b9697264b3b9e5f29ff76f395c7a52546c9e85035a88cf95`
- producer tests: `959e03d70d195225a7664bf0c219ae2b5e5b4af432eec8493ae2b65543cdcae1`
- D0 harness: `00bf9c9de3ad3452289cb1488e727ae2aa95e85d9b16e5d6b3bf0a6002f488b9`
- Node tests: `60ea994ad0efee557b6d113e812464d41b92ed585202638b0cd3752489087704`

The native producer is otherwise well bounded: exact Darwin/Python/ABI and launcher/native-image
pair; canonical selected-PID input; double BSD identity, path and device/inode observation; strict
UID and birth checks; canonical path-free output; fixed error codes; and no argv, environment, or
process-memory inspection. Listener snapshots bracket native capture and preserve strict UDP denial
and one exact usernet-owned loopback management TCP listener. Parent-reported focused evidence was
31 Python tests, 19 Node tests, an own/parent native capture, and a host socket bracket pass; those
are parent-observed, not independently rerun here.

### P1 — usernet identity drift does not block destructive cleanup

`captureUsernetArtifacts()` initially sets `usernetPidRecorded = true` and adds that PID to
`vmPids`. Later, `revalidateUsernetArtifacts(usernetArtifacts)` can fail because the PID file,
sockets, or their identities changed. That failure reaches `finally`, but cleanup initializes
`usernetDeleteBlocked = false` and only recaptures usernet identity when
`!usernetPidRecorded`. Because the stale flag remains true, cleanup may stop the profile, prove only
the originally recorded PID absent, and authorize `colima delete --data`. A replacement helper PID
or changed private usernet identity can therefore be discarded with the state instead of triggering
the required unknown-state retention.

Reproduction at the source-contract level: let the first capture return PID `p`; replace the private
PID file/artifacts with PID `q` before the explicit revalidation; the main path fails
`usernet_artifacts_changed`, but cleanup skips recapture, checks only `p`, and can proceed to delete.

Required correction: persist an ambiguity/deletion-block flag before every usernet revalidation,
clear it only after an exact successful comparison, and have cleanup always revalidate the recorded
identity before stop/delete (or conservatively block deletion on any prior drift). Stop/force-stop
must remain attempted for the exact private profile, while delete and state removal remain denied
unless the current helper identity is proven and every recorded/current helper PID is absent.

Until this is corrected and re-reviewed, the native/usernet D0 source is not cleared for another VM
boot. No image, runtime-startup, or release authority is established.

### P1 resolution review

Re-reviewed the Node-only correction at:

- harness: `525726f870bbd18ac3d608e3838f676c3f85b6c10a452b015e7112a4909561fe`
- tests: `9b29e52a71c755a505bc5afc533ca5ff929363b430feec0d77bbd7a19c3c1638`

The P1 is resolved. `usernetIdentityAmbiguous` is set before each live artifact/native binding and
is restored only to its prior value after the complete exact validation succeeds. A prior failure
therefore remains sticky even if the original bytes or sockets later reappear. Cleanup always
recaptures usernet artifacts before stop, adds the currently observed PID to `vmPids` before
continuity comparison, and compares current artifacts with the immutable original authority when
one exists. Capture failure, replacement PID, inode/hash drift, native-binding failure, or any
earlier ambiguity blocks delete for the rest of the run.

The exact private profile still receives bounded stop/force-stop. Destructive delete is authorized
only after strict stopped inventories, absence of every original/current/VM PID, and both the local
and sticky ambiguity flags being false. No post-stop artifact-presence requirement was added; paths
may legitimately disappear when the daemon stops, while their three socket paths are still required
absent after owned state removal.

Focused tests cover identical continuity, prior sticky ambiguity, replacement PID, artifact digest
or inode drift, and source ordering. Parent reported 20 Node tests and the unchanged 31 Python tests
passing, plus the targeted native and socket-bracket checks. These are parent-observed focused
results; a fresh VM boot/stop/delete remains required before D0 acceptance.

## Connected-UDP attribution review

Reviewed the arena-B implementation at these exact hashes:

- harness: `70d0d35bb7cf25fd6d9977bcc63ce9d0f913b8fbe8d84ee4f368412ed50f5eed`
- Node tests: `405465f4fb6189534433f56a49104427fac1328ad02b309b818c7a825a3ec18a`
- unchanged native producer: `3c53430e3e937a27b9697264b3b9e5f29ff76f395c7a52546c9e85035a88cf95`
- unchanged producer tests: `959e03d70d195225a7664bf0c219ae2b5e5b4af432eec8493ae2b65543cdcae1`

No P0–P3 finding blocks a fresh parent-owned D0 VM attempt. The one TCP delta still has to be the
private, native-bound usernet process listening on the exact loopback SSH management port. A UDP
delta is admitted only when it is connected and non-unbound, with concrete numeric local and peer
endpoints. The private usernet process must match its artifact authority, executable identity and
kernel birth across the native brackets. An ambient row must instead belong to a PID already seen
before VM start, remain byte-identical across all three native records, share the verified usernet
UID pair, have the generic non-helper class, and not be a controller, parent, known VM or helper
PID. Missing proof, PID reuse, executable/birth/UID/parent drift, unknown ownership, wildcard or
unspecified endpoints, unconnected/unbound UDP, and every additional TCP shape remain denials.

The accepted report publishes only bounded counts and hashes. It cross-checks total and helper UDP
counts against the listener delta and preserves all three negative claim flags:
`literalNoHostNetworkClaimed:false`, `literalCodeSigningClaimed:false`, and
`literalOfficialDependencyPurposeClaimed:false`. It therefore proves only the reviewed
host-service-listener boundary under the declared trusted local-host model. It does not establish
the purpose or trust of any peer, guest egress isolation, a firewall, image/runtime authority, or
production readiness. Attempt 92676 remains a failure and is not retroactively qualified.

Parent reported the new default Jobs harness command passing 20 Node tests and the unchanged 31
Python tests. Those focused results were not independently rerun here. The fresh exact-source VM
boot, listener observation, stop/PID/socket/state cleanup, broader Jobs regression, and D1–D3
artifact/startup/browser/receipt gates remain unverified.

## FIX848 listener-churn correction

Attempt 28269 remains a failed D0 run. It stopped at `host_listener_snapshot_changed`; cleanup did
not prove the original in-memory PID set absent, so it is not a cleanup-proven or runtime-qualified
result. A separately reviewed one-time storage recovery cannot change that classification.

Reviewed the two-file FIX848 successor at:

- harness: `c723c0a0a2d38221d924e2e56a22038deb60955b1bcb451259d702bc2e8c1856`
- Node tests: `fdec8364d1dd352e8f6df56acec9eb2bfd367113de97256cbb6dd7d9ff190732`

No P0–P3 finding blocks a fresh parent-owned VM attempt. Before ordinary socket observation, the
harness now performs a dedicated artifact/native proof for the private usernet helper and binds its
immutable PID, executable identity, kernel birth and artifact authority. Any failure in that proof
leaves `usernetIdentityAmbiguous` sticky and therefore blocks destructive cleanup.

The later listener observation allows at most three complete attempts and retries only the exact
`host_listener_snapshot_changed` error. Each attempt independently captures the first socket
snapshot, captures the selected native records, requires the second socket snapshot identical,
revalidates the immutable usernet artifacts, captures the later native records, requires the helper
binding unchanged, restores the prior ambiguity value only after those identity checks succeed,
and finally requires a third socket snapshot identical. A native parse/selection error, artifact
drift, helper identity change, timeout, malformed listener record, or any other error is not
retryable. A third-snapshot churn can retry because it records no helper-identity defect; a failed
artifact or native binding cannot be laundered by a later stable attempt.

The existing exact TCP rule, connected-UDP attribution policy, sticky cleanup gates and
original/current PID absence requirements are unchanged. Parent reported 21 Node and 31 Python
tests passing after correcting test-only ordering/name issues. Those results were not independently
rerun. Fresh exact-source VM listener and cleanup evidence remains mandatory.

## D0 compatibility acceptance — run 8728

I read the parent-generated durable journal
`phase639-vm-compatibility-8b8a3074412a.jsonl`; its SHA-256 is
`868d0f8f1a2bb4ee03f59ea48bc45a8dbc5001d3bdd081aa8b1fbd342d6e9ddb`. The eight-event sequence is
complete: source-before, dispatch, owned-state observation, terminal command, qualified harness
report, residue, source-after, and successful controller completion. This was an independent
document review of parent-run evidence, not an independently repeated VM execution.

The source identity matches before and after: branch
`feat/phase-639-jobs-managed-runtime-startup`, HEAD
`2b85339a403cb5272a56f1b2f0bf8065ca3b19ed`,778 files, manifest
`bd363dd8e10901188f6169ca1c90d43f47301311430a85e73f6718c9c7baafa1`, and raw-status digest
`0c435183af6f4fc078c824d4d5416e2909dcb4b016add4c8a4b8d5421db5f845`. The admitted harness and
test hashes are the reviewed FIX848 values. The command ended code0, no signal, no observer failure,
no overflow and empty stderr.

The report qualifies only `isolated_linux_amd64_vm_compatibility_only`. It observed QEMU Linux
x86_64 guest and engine identities, Colima0.10.3, Lima2.2.1, QEMU11.1.2 and buildx v0.34.1. The
resource evidence is within the declared4-CPU/6-GiB-memory/8-GiB-root/16-GiB-data bounds, with more
than32 GiB free before dispatch. Host exposure was one exact private-helper loopback SSH listener.
Eight connected UDP client deltas were attributed: two to the exact usernet helper and six to
stable pre-existing ambient processes; helper unbound UDP was zero. Count and identity hashes are
cross-bound in the report, while no-network, code-signing and official-dependency-purpose claims
all remain explicitly false.

All five cleanup flags are true: private inventories empty, recorded VM PIDs absent, management
socket absent, private state absent and usernet sockets absent. The controller independently records
the exact owned root absent, its parent absent and empty, and zero pre-existing owned state roots.
Source identity remained unchanged through those final checks.

Verdict for D0 is **bounded local accept**. The six preceding attempts remain failures and are not
retroactively accepted. This result does not build or inspect the actual runner image, start the
managed runner, exercise native volume/runtime authority, browser/provider transport, receipts, or
any production/release path. D1–D3, broader regression gates, and the full release objective remain
open.
