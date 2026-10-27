# IMPL: Phase 634 — Jobs isolated Linux runner qualification

Preflight: `bluey-ops`, current source/local handoff, Round634 and
`performance-analysis` reproducible evidence guidance. Base: `57635b29`.
Status: bounded local qualification complete; runtime, regressions, privacy and review passed.

## Starting evidence and scope

Phase633 actual private candidate/source/queue journey9/9 and focused/current
Jobs regressions/typechecks passed. It did not run a cloud runner or qualify its
Linux image/native addon. Host has32GiB RAM and approximately39GiB free disk.
Docker default socket is unavailable. Existing stopped profiles are unrelated
Pinky/HQ resources and must not be selected, started, altered or deleted.

The successor uses a new resource-bounded Jobs-only VM, public digest-pinned
image dependencies, no production secrets/providers and no startup of the Jobs
application service. The image uses CI-only zero authority hashes and cannot
be promoted as a production release artifact. The native smoke has no network
and verifies only its existing storage probe contract in that exact image.

The installed Docker29.8.1 client has no Buildx plugin. Rather than adding a new
binary or changing global configuration, the harness explicitly sets
`DOCKER_BUILDKIT=0` and reports `legacy-explicit`. Docker's official current
deprecation guide documents that opt-out. The unchanged Dockerfile uses no
BuildKit-only mount syntax, but actual engine support must still be observed;
failure remains failure. Do not claim CI/BuildKit byte equivalence or silently
switch modes. At preflight no image, VM, plugin installation or engine startup
had occurred; actual construction evidence is recorded below.

The admitted harness/product bytes must be committed before the runtime so its
source-commit build argument is exact. A later evidence-only docs commit may
record that earlier tested image; it does not turn it into a final-tip artifact.

VM control/config/cache/data live in one separately generated short private
temporary state root. This is intentionally outside canonical Cargo/test root:
if a detached VM cannot be proven stopped/deleted, its disks must be retained,
not unlinked by general test cleanup. No unrelated/global context is fallback.

## Ownership

Existing Sol owns only the new Linux harness and pure test file. Parent owns
the round, docs, any test registration, resource run and closeout. Separate Sol
reviews source/procedure/evidence before runtime acceptance. No heavy worker
commands or competing machine queue/board are permitted.

No benchmark framework CLI is installed. No production/host/overlay/Pinky,
provider/ATS/application/email/calendar, push/PR/deploy/signing/publication action.
No Linux, capacity, latency, runner HTTP/PG lifecycle or receipt proof is inferred
from harness construction.

## Additional preflight observations

Canonical session77720 exited0 after anonymous public manifest reads with a
private Docker configuration: both exact pinned Rust and Playwright image
indexes contain Linux/ARM64 targets. This avoids guessing native-image support
or silently relying on emulation. The manifest scratch root was removed and
its parent empty. No container daemon, VM or application service was started.

The pure harness boundary tests are registered in normal Jobs tests; invoking
those tests must never create VM/config/runtime state or start a CLI/service.
Actual Linux qualification remains an explicit separately locked command.

## Frozen source and regression gates

The Node-only harness creates an immutable context from an explicit allowlist of
committed Git blobs, not ambient/ignored files in the workspace. It validates the
canonical owned launcher root and a separate private HOME before generating the
private VM-state root. All CLI environments/arguments, actual VM resources,
native ARM64 image/user, embedded runtime metadata, exact smoke container and
pre-delete VM/PID/socket evidence remain explicit gates.

Independent source review found and resolved a cleanup blocker before any VM
startup: stop success alone could not authorize disk deletion. Cleanup now
requires complete exact owned inventory, recognized stopped status, explicit
zero Lima PIDs and absence of every captured process BEFORE `delete --data`.
Unknown/partial start state retains its separate disk root. Known stopped rows
are not confused with absent or unknown process state.

Twelve pure boundary tests, Node syntax and diff checks passed. The new tests
cover resource/root/environment/argv/context/configuration/image/metadata and
cleanup negatives without creating VM or service state.

Canonical session27127 exited0: report/Temporal8, object7, Linux harness12,
automation804, Browser219, runner308, workflows321 and portal387 all passed:
2,066 total, zero skips. Configured Chromium was installed only under that
owned scratch root, so the exact-submit/beacon fixture actually ran in the full
suite. All five workspace typechecks passed. The exact root was removed and
its parent empty. This is not a new full Rust or all-target Clippy gate.

Official v0.10.3 Colima source confirms `LIMA_HOME` is honored by `LimaDir()`
and passed to limactl; the isolated inventory and Colima therefore inspect the
same private Lima state, not an assumed global directory. No owner environment,
mount, Docker context, VM, application service or provider was used in these gates.

The harness and package must be committed after static review/privacy before the
actual resource command. Static acceptance is not Linux runtime acceptance.

## Initial actual attempt — failed, safely retained

Harness `e5532431`; canonical session10495 exited1 at `vm_start`, before image
construction or addon execution. Its cleanup fence retained the separate state
rather than unlinking possibly active disks. Scoped diagnostic78399 identified
the actual pre-instance105-byte SSH socket rejection; it was stdout, which the
initial harness discarded. Exact-profile stop returned0/not running and private
inventories were empty.

FIX805 records root/marker/inode/uid/mode/open-file/inventory checks and exact
private CLI deletion/recovery. Only the task's331MiB recreatable cache/snapshot/
sparse disk state was removed; the path is absent, free space about39GiB, and
unrelated profiles/global context remain unchanged. No automatic safety gate was
weakened. Corrected socket preflight, shorter `/l`, separate primary/cleanup
failures and bounded private capture of BOTH output streams required fresh
source review/runtime; no Linux pass was inferred from diagnosis alone.

FIX805 correction is now frozen and independently statically clear. Parent and
reviewer each observed16/16 focused pure tests plus syntax/diff checks. New
coverage includes103/104-byte UTF8 socket boundaries, separate primary/cleanup
causes, both bounded private output streams, and unknown-entry retention. Raw
output is never emitted in the public report. Proven cleanup removes only known
large state children before retaining at most1MiB diagnostics/owner marker;
uncertainty preserves the entire state. This static checkpoint preceded actual
Linux qualification, recorded below.

Before another VM attempt, a primary-source check found an additional protocol
detail: Lima2.2.1's `Instance.MarshalJSON` preserves `omitempty` for zero-valued
host/driver PIDs. Requiring literal serialized0 fields would reject normal
stopped inventory. A narrow adapter now interprets those documented omissions
only for canonical2.2.1/VZ/aarch64 Running-or-Stopped/no-error rows. Running still
requires a positive host-agent PID; unknown/malformed omission is not zero and
retains state. Full prior-PID absence remains mandatory before destructive
deletion. Parent/reviewer each passed17/17 current pure units, syntax and diff
checks; this is a reviewed protocol adapter, not relaxed shutdown authority.

The source is [the exact Lima2.2.1 instance contract](https://github.com/lima-vm/lima/blob/v2.2.1/pkg/limatype/lima_instance.go).

## Frozen corrected Linux run — passed

Canonical session74829 exited0 on source
`d9a5b0db398386c2701b98ecb006cc3e0f46c96b`. Product sources stayed clean/unchanged.
The immutable context held111 files/2,508,373 bytes from exact HEAD, not ambient
ignored files, owner homes or credentials. The new private VZ/Colima VM passed
generated/live native aarch64, four CPUs,6GiB RAM, separate8GiB root/16GiB data
filesystems and no host filesystem mounts. No existing profile/context was used.

The unchanged pinned Dockerfile built in explicit legacy mode. Exact image ID:
`sha256:ef2ae6e9c1976a490d08081579c2570373f1d39e8190772581778f98506b96f3`.
Embedded measurement was read from that exact Linux/ARM64 image and matched
source/build/component/role and all three CI-only zero authority hashes. Actual
measurement SHA256:
`b69ad2395160ade4337a5e2708be2bdb2dad9522f7d90c93f8df5d64727c1dd6`.

The unchanged native-addon script ran from stdin, passed1/1 and exited0. Actual
finished-container inspection confirmed nonroot UID, no network/ports/host binds,
read-only root, dropped capabilities, no privilege escalation, bounded CPU/RAM/
PIDs and only the two bounded tmpfs mounts. Native open/identity, exclusive write,
bounded exact read, inventory and delete completed. No runner server was started.

Automatic cleanup observed stopped private inventories and PID2988 absent before
deleting the private VM/data; final inventories empty, Docker socket absent,
state `/private/tmp/bluey-jobs-linux-p634-IfYImk` removed. Parent checks also found
observed helper/mux/client PIDs2986/3019/3183 absent and canonical parent empty.
The four unrelated HQ/Pinky profiles stayed stopped, global context `default`
unchanged. No image, disk or private key was retained for promotion/reuse.

Host free bytes41,385,975,808 before and41,337,602,048 after, delta-48,373,760
(about46MiB). This whole-host sample includes unrelated background activity;
it is not attributed to a leak because all owned paths/VMs were absent. A build
sample showed about31GiB free. No latency, capacity, cost or SLO benchmark claim.

## Fresh corrected-source Jobs gates

Canonical session80351 exited0 on unchanged source: report/Temporal8, object7,
Linux harness17, automation804, Browser219, runner308, workflows321, portal387:
2,071 passed, zero skipped. Private configured Chromium made the synthetic
exact-submit/beacon guard execute. All five workspace typechecks passed. Root/
parent cleanup, final launcher self-test/privacy/review are checked at closeout,
not inferred from counts.

The original startup remains failed. This local ARM64 legacy construction/addon
probe is not amd64/BuildKit CI, a signed/final-tip published artifact, hosted
runtime/storage attestation, full Rust/all-target lint, runner HTTP/PG lease,
real application/receipt, model/mailbox/calendar, production capacity or release
approval. No external production flag changed.

## Closeout

Launcher self-test session17598 exited0, including intentional negative cleanup
cases. All five canonical Phase634 parents were empty; both separate failed and
successful VM-state roots absent. Independent review is green for this bounded
local batch with no remaining P0–P3. Privacy passed2,850 tracked paths/2,574 text
files, including the new indexed handoff; cached diff checks passed. These gates
do not change any of the external/release limitations above.
