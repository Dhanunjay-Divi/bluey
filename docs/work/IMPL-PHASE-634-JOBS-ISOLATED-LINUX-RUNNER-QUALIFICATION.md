# IMPL: Phase 634 — Jobs isolated Linux runner qualification

Preflight: `bluey-ops`, current source/local handoff, Round634 and
`performance-analysis` reproducible evidence guidance. Base: `57635b29`.
Status: initial startup failed before VM creation; FIX805 correction pending.

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
switch modes. No image, VM, plugin installation or engine startup has occurred.

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
failures and bounded private capture of BOTH output streams remain pending
fresh source review/runtime. No Linux pass is inferred from the diagnosis.

FIX805 correction is now frozen and independently statically clear. Parent and
reviewer each observed16/16 focused pure tests plus syntax/diff checks. New
coverage includes103/104-byte UTF8 socket boundaries, separate primary/cleanup
causes, both bounded private output streams, and unknown-entry retention. Raw
output is never emitted in the public report. Proven cleanup removes only known
large state children before retaining at most1MiB diagnostics/owner marker;
uncertainty preserves the entire state. Actual Linux qualification is pending.

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
