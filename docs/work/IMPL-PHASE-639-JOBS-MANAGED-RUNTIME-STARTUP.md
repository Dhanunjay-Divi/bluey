# IMPL639 — Isolated real managed-runner startup

Base `2b85339a403cb5272a56f1b2f0bf8065ca3b19ed`; branch
`feat/phase-639-jobs-managed-runtime-startup`. Owner approved local disposable VM.
Status: D0 disposable VM compatibility/isolation/cleanup qualified on the exact
source described below; D1 foundations implemented/tested. Actual image and
D2/D3 managed-runtime acceptance remain open.

Phase638 was locally checkpointed only after exact explicit96-path staging,
indexed whitespace/privacy and tree equality; inherited REVIEW628 was excluded
and remained untracked. No push/merge/deploy or production acceptance occurred.

Installed CLI observation: Colima0.10.3, Lima2.2.1, QEMU11.1.2, Docker client29.8.1.
Colima help exposes x86_64/QEMU, resource, mount-none, forwarding-disable and
non-activation options. The actual combination has booted; final qualification
is still open, as the dated attempt records below explain.
Default Docker daemon is absent; it was not started or replaced.

Round639 owns ordered real guest compatibility, actual image evidence, fresh
unconsumed startup grants and normal runner health/profile/browser receipt proof.
Existing Phase634 ARM-only construction and Phase638 synthetic runtime metadata
cannot substitute. VM builder writes only its assigned private harness/tests;
parent retains all runtime and integration authority.

## Executed foundation and first preflight

The parent added canonical runner-automation derivation to the release gate;
all37 release-gate tests pass, including five new bundle tests. Independent
source review found no P0-P3 algorithm finding. This is not yet CLI/runtime/
Rust parity or actual artifact execution evidence.

All13 D0 pure safety tests pass. The first frozen actual preflight, session30976,
stopped before VM startup on the macOS socket-tool protocol mismatch documented
in FIX843. Terminal1, unchanged776-source manifest
`31f17594b9bf129a4ffa3de272091ab6111a4f5670e304f53d56fc9616d5d7d5`,
HEAD/base unchanged, index empty, launcher root/parent removed. The separate
marker-only diagnostic root was exactly identity/hash checked and removed.
Journal SHA-256:
`a58f1ff3905055b3755d92299b5ea589fed23b9403c51d2555bc818ce86cccb1`.

After correction, actual TCP12/UDP129-record parser checks and the13 pure tests
pass. No VM compatibility success or runner startup has been observed yet.

Second frozen attempt16562 booted Linux/x86_64 under QEMU and generated the
approved resource/configuration fields, but was deliberately interrupted after
private hostagent logs showed automatic wildcard DNS-port forwarding attempts.
It is not a compatibility pass. Pinned upstream inspection found that Colima's
`none` rule omits Lima's `guestIPMustBeZero:false`, permitting other guest binds
to reach later/fallback rules. A private pre-boot all-port TCP/UDP ignore policy
is in progress; existing owner profiles/configuration remain untouched.

Attempt16562 terminal1, cleanup proven after bounded stop/force-stop fallback;
VM disks/socket removed, launcher run root/parent removed. Only four bounded
diagnostics and the owner marker remain in the separate0700 diagnostic root.
All observed VM/network helper PIDs were independently absent. Exact776-source
manifest stayed `ad841ab8b3c930690d9d87cfe20de06ac7c3d9b7c7c660b82a6caf8bb38ebba4`;
journal SHA-256
`b736fc3cd1336bec426901becd0989aeaacc6c111ef3fdd4f3f3846889ec9ab3`.

FIX844's measured automation/image startup comparisons are source-reviewed.
Parent session89044 terminal0: automation build, all five Jobs typechecks,
811 automation and315 runner tests passed. The existing offline browser
integration was explicitly skipped1; no actual browser/startup claim follows.
All four frozen TypeScript hashes matched afterward; launcher root/parent gone.

Third attempt74516 passed the preceding guest architecture/resource/Docker and
buildx-observation steps, but denied at `listener_observation` on a new UDP
record. Cleanup proven, terminal1, only marker retained. Exact source manifest
`e0ddb1bd21fb2bddc52ee7092333dca5bff1564d289916c77bf1f150297c7dd4`;
journal `222d6a280bdd85cf002af4dce3870f9e4a5fd4091b99b7c2fe594026e595c417`.

FIX846 adds bounded diagnostic classification without changing admission.
Fourth attempt30106 likewise terminated1/cleanupProven at the strict listener
gate: TCP delta1 unknown; UDP delta5 (baseline-PID-only3, unknown2, recorded-VM0).
Source/HEAD/index remained exact; launcher parent removed. Journal SHA-256
`585afe0e336fe74e0dfc97a183a5a07d03cb2a50d523f23d967fd590deef98cf`.
Parent retained a stable complete current hostagent log snapshot76443 bytes,
SHA `14f768734c744b65cc177bda4a05f5dba020dc84aa853155ed9f4649d1a20926`,
with no forwarding attempt. This is supporting policy evidence, not D0 success.

No additional VM boot is scheduled until the observed helper/host-process
attribution gap is addressed. Native libproc producer implementation and pinned
Lima management-helper mapping are bounded parallel source work; no PS-name
heuristic, arbitrary loopback allowance or UDP exemption has been enabled.

## Native attribution / usernet source successor

Pinned Lima source identifies a separate private `limactl usernet` daemon as
the SSH loopback listener owner, outside instance host-agent/QEMU PID fields.
Its private PID file and three Unix sockets now have a source binding, native
kernel UID/parent/microsecond-birth/executable identity check, exact SSH listener
rule and post-stop PID/socket cleanup requirements. Live proof is still pending.

`managed-runner-host-processes.py` uses only fixed public libproc metadata APIs
under root-owned isolated system Python. Exact observed Xcode launcher/native
interpreter pairing is required; `ps` names are never authority. The whole-UID
mode honestly denied three uninspectable processes on this Mac. The reviewed
least-data mode instead captures a canonical, bounded selected PID list and
denies the whole selection on any unknown/changed selected identity. It adds
its own PID and emits only closed fields and executable-identity hashes.

Parent31 Python mocked tests and20 Node safety tests pass. Actual targeted
native capture passed2 records; actual same-UID socket A/native/socket B bracket
passed with TCP12/UDP125 records,10 selected PIDs/11 native records, identical
socket rows. No raw PID, endpoint or executable path was published.

FIX847 closes a pre-launch draft cleanup drift/alias defect. Independent review
continues; no VM has run on the new native/usernet successor. New UDP remains
strictly denied, including usernet traffic; its purpose is not inferred from
counts. Image/Rust parity/fresh grants/normal startup/browser receipt gates
remain open.

## Fifth attempt, reviewed successor and full regression

Fifth attempt92676 terminated1 with cleanup proven. It bound the private usernet
helper's native identity, one exact loopback SSH TCP listener and three private
Unix sockets. The old all-new-UDP rule denied eight connected-client changes:
three helper-owned, five baseline-PID-only, zero unknown. This remains a failed
attempt; neither counts nor later source changes retroactively qualify it.
Exact778-source manifest
`464d5ceacbd970fa6b98af88046464c2f1ada1c6d8a9732b4dc98bf5cf9d2ae6`;
journal SHA-256
`2e7ccdfc53f23b3fc85d3f58da8423450ddec4be76c39755150e395512c773dd`.

The bounded arena decision in DECISION-PHASE-639-D0-CONNECTED-UDP-ATTRIBUTION.md
distinguishes native-attributed connected UDP clients from service listeners.
The independently reviewed successor still denies unknown/new/reused/changed
ownership, wildcard/unspecified endpoints, unconnected or unbound UDP, extra TCP
and other VM/helper/controller processes. It makes no no-network, code-signing,
peer-purpose, browser-egress or production claim. Default Jobs tests now include
the20 Node and31 Python pure checks without booting any VM.

Parent full regression94378 terminated0: 2,481 tests passed, two existing browser
integrations skipped; all five Jobs workspace builds and37 release-gate tests
passed. Counts include Node306+7+17+20+37, Python31 and Vitest
811+219+315+321+397. All778 admitted source files remained exact at manifest
`151acfcf23d6a9664a3fe10be0c3db07c3f6893ec0c9f8e2a68f7ec91902f134`;
launcher root/parent were removed. Journal SHA-256
`47d3ba74fafc1f85fdf5334344941aec7dce1f8cdf3b10f561b226afb2a294f2`.
The build's generated portal output was privately preserved and only those
build-owned generated changes restored; no source or owner edit was discarded.

The four earlier retained diagnostic roots were removed only after exact
identity/hash checks and verified private preservation (eight files/12,261 bytes;
preservation manifest SHA-256
`fc29ff782022c104b5aa7c89ac006ae1137560b3b2e09987b2baf709c2772d55`).
No previous failed run was rewritten as successful. A fresh sixth VM attempt is
dispatched under the shared machine lock, canonical launcher and the reviewed
source. Its outcome and corrective successor follow below.

## Sixth attempt and socket-churn correction

Attempt28269 terminated1 at `listener_observation` with
`host_listener_snapshot_changed`. Source778 stayed exact at the preceding
`151acfcf...` manifest; launcher root/parent were removed. VM stop succeeded,
but the original harness did not prove cleanup and retained its private state.
Journal SHA-256
`3d6e33c8ee820621a1377a280f724aa750a3948829f9e4fe15bba34ed66e0433`.
This attempt remains failed and NOT `cleanupProven`.

FIX848 identifies an overly broad sticky-ambiguity scope, not permission to
ignore unknown networking. The successor establishes the immutable helper's
dedicated native proof before collecting unrelated socket frames, then retries
only exact frame-churn errors three times. Artifact/native uncertainty remains
sticky and all listener admission rules stay unchanged. Parent21 Node and31
Python tests pass. Independent exact-source review found no P0-P3 blocker to a
fresh attempt. Parent launcher self-test also passed, including deliberate
cleanup-failure and signal cases; its expected negative-case diagnostics are
not a failed self-test.

Separate recovery independently required exact stopped private inventories,
no PID/socket/symlink nodes and an unfiltered no-open-file result before its
exact-profile delete. Delete returned0, but post-delete verification halted on
Lima's exact normal empty-inventory warning. Independently reviewed finalizer
75107 then terminated0: both inventories empty, unfiltered no-open-file checks,
no PID/socket/symlink nodes, exact-root removal and final absence proved.
Thirteen bounded log/config/marker files remain privately recoverable. Recovery
record SHA-256
`5c97dfed00114884dac4548e55df7948908a73c69a2d204308e8567c2c3475bd`.
It explicitly preserves `originalAttemptPassed:false`,
`originalCleanupProven:false` and `originalPidAbsenceUnavailable:true`.
It cannot reconstruct original PID absence or retroactively qualify the run.

## Seventh attempt — qualified D0 compatibility only

Parent8728 terminated0 on unchanged778-source manifest
`bd363dd8e10901188f6169ca1c90d43f47301311430a85e73f6718c9c7baafa1`.
Actual Linux/x86_64 guest and engine, QEMU emulation,4 CPUs,6,207,385,600 bytes
guest/engine memory,7,203,201,024-byte root filesystem and16,794,906,624-byte
separate data filesystem were observed. These filesystem capacities are not the
nominal8/16 GiB disk configuration. Buildx v0.34.1 was actually available.

The listener evidence admitted one exact native-bound loopback SSH listener
and eight attributed connected UDP clients (two private usernet, six stable
ambient). No wildcard/unbound/unknown/client-purpose exemption was inferred.
All three negative network/signing/purpose claims remain false. The private
all-interface forwarding policy hash stayed
`3149915d25ca09becc50c6834df7a17e646e3d52f8df4c81121d5c13c5b10991`.

Normal cleanup proved both private inventories empty, every recorded VM/helper
PID absent, Docker socket and all three usernet sockets absent, exact state
root absent and outer launcher parent removed. Source/HEAD/index stayed exact.
Journal SHA-256
`868d0f8f1a2bb4ee03f59ea48bc45a8dbc5001d3bdd081aa8b1fbd342d6e9ddb`.
Earlier six attempts remain failed. This is D0 compatibility and declared
host-listener-boundary proof, not native performance, peer-purpose, browser
egress, actual image, API/runner startup, release or production acceptance.

The final full Jobs regression17088 on that same778-source manifest terminated0:
2,482 tests passed, two existing browser integrations skipped, all five Jobs
workspace builds and37 release-gate tests passed. Counts: Node306+7+17+21+37,
Python31, Vitest811+219+315+321+397. Source and empty index stayed exact; outer
launcher root/parent were removed. Journal SHA-256
`aef6ee2adf73917309a7d17f193e7f83651847d98cd17aa39706cf8d09f9e0b7`.
Generated portal output matched all25 privately preserved file hashes exactly;
only those generated changes were restored. Server/workspace formatting check
26302 terminated0. No Rust source changed in this batch; the earlier qualified
Rust evidence retains its explicitly dated scope rather than being called a
new execution. D1/D2/D3 and the overall production goal remain open.
