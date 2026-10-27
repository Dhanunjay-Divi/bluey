# REVIEW: Phase 634 Jobs isolated Linux runner qualification

Date: 2026-10-27 (America/New_York)
Base: `57635b29`
Reviewer role: independent source and procedure review
Verdict: 🟡 static harness accepted; Linux runtime qualification pending

## Scope reviewed

- `jobs/scripts/run-nonprod-linux-runner-tests.mjs`, complete new harness.
- `jobs/scripts/run-nonprod-linux-runner-tests.test.mjs`, complete focused pure tests.
- `jobs/package.json`, normal-test wiring only.
- Existing unchanged `jobs/runner/Dockerfile` and
  `jobs/scripts/native-runner-addon-smoke.mjs`.
- Relevant managed-runner CI construction/smoke commands.
- `docs/rounds/ROUND-634-JOBS-ISOLATED-LINUX-RUNNER-QUALIFICATION.md` and the
  current implementation plan.

No product or test source was edited by this reviewer. No Colima/Lima/Docker command, VM, image
build, container, service, provider, account, deployment or external action was run.

## Resolved P1 cleanup finding

The initial cleanup accepted an attempted start with absent or partial Lima inventory and could
proceed from stop/delete command success directly to disk deletion. It also rejected legitimate
known-stopped zero-PID inventory without distinguishing that state from an unknown schema. A
detached VM could therefore make cleanup either unsafe or unnecessarily unrecoverable.

The frozen harness now requires exactly one owned Colima row and one corresponding Lima row after
any start attempt. Running/starting rows require bounded positive PIDs; stopped zero-PID rows are
accepted only with exact known stopped status and no reported errors. After normal or forced stop,
the harness requires both inventories to report the exact stopped state and every previously
recorded owned PID to be absent before `delete --data`. Unknown, absent, partial, foreign,
permission-ambiguous or still-live evidence retains the separately owned state root and fails the
qualification. Final empty inventories, PID absence and socket absence are rechecked before exact
state-root removal.

Locations: `jobs/scripts/run-nonprod-linux-runner-tests.mjs:70-108,410-455` and focused tests at
`jobs/scripts/run-nonprod-linux-runner-tests.test.mjs:75-128`.

## Resolved P1 Lima stopped-inventory protocol finding

Official Lima 2.2.1 source reviewed by the parent shows that the integer host-agent and driver PID
fields retain `omitempty` in `Instance.MarshalJSON`; a genuinely stopped VZ instance can therefore
omit both zero-valued fields. Treating every omission as an unknown inventory would retain a
proven-stopped owned VM indefinitely, while treating arbitrary missing fields as zero would weaken
the deletion fence.

The frozen adapter normalizes an omitted PID to zero only for the exact reviewed protocol tuple:
Lima `2.2.1`, VZ, `aarch64`, canonical `Running` or `Stopped` status and no reported errors. A
running row still requires a positive host-agent PID. Explicit null/string values, missing live
host PID, unknown versions, VM types, architectures, statuses or errors fail closed. The cleanup
path still requires the exact stopped Colima and Lima inventories and absence of every PID captured
before deletion; this adapter does not turn absent or partial inventory into deletion authority.

Locations: `jobs/scripts/run-nonprod-linux-runner-tests.mjs:119-166` and the focused positive and
negative protocol cases at `jobs/scripts/run-nonprod-linux-runner-tests.test.mjs:172-198`.

## Resolved P1 socket-budget and failure-evidence finding

The first actual attempt used a private Lima path whose reserved SSH socket was 105 bytes. Darwin's
104-byte `sun_path` includes the terminating NUL, so Lima rejected startup before creating a VM.
The initial harness discarded the useful stdout diagnostic and later cleanup failure obscured the
primary start failure.

FIX-805 shortens only the private Lima directory to `/l` and checks both the longest known Lima SSH
socket and private Docker socket in UTF-8 bytes, requiring at most 103 bytes before startup. Failed
commands retain bounded 64 KiB tails from both stdout and stderr in create-new mode-0600 files,
with at most eight commands/sixteen files. Public events expose only fixed filenames, tool/stage,
byte counts and closed failure codes; raw output and arguments are never emitted. Primary and
cleanup failures remain separate.

Large state may be reduced to the owner marker and bounded diagnostics only after the exact VM has
been deleted, inventories are empty, captured PIDs and Docker socket are absent, and the root marker
and inode are revalidated. Every removable child has an allowlisted name, owner/type/canonical-path
check and captured inode. Unknown entries or any cleanup uncertainty retain the entire state root.

Locations: `jobs/scripts/run-nonprod-linux-runner-tests.mjs:17-69,93-108,308-357,371-375,484-552`
and the first four focused tests in
`jobs/scripts/run-nonprod-linux-runner-tests.test.mjs`.

## Isolation and lifecycle review

- The canonical launcher root and a separately generated VM-state root must both be canonical,
  owner-controlled mode-0700 directories matching narrow temporary-path grammars. The VM state is
  outside the canonical root and carries an immutable private owner marker and inode identity.
- The child environment is a strict allowlist with private `HOME`, `COLIMA_HOME`,
  `COLIMA_CACHE_HOME`, `LIMA_HOME`, `DOCKER_CONFIG` and temporary paths. Owner database/provider,
  proxy, SSH-agent and Docker-context variables are not inherited.
- Colima uses one random bounded profile with activation, templates, mounts, SSH forwarding,
  binfmt, Rosetta and external port forwarding disabled. The generated config and live guest are
  re-read to require native aarch64 VZ, four CPUs, 5–6 GiB observed RAM, separate bounded root/data
  filesystems and no virtiofs/9p/SSHFS host mount. The private Docker Unix socket is addressed
  explicitly rather than selecting a global context.
- Signal and overall-timeout handlers are installed before VM start. Each child has bounded output,
  timeout, termination/escalation and cached close evidence. Cleanup is installed before startup;
  any ambiguity keeps the separate VM state instead of allowing canonical-root cleanup to unlink
  disks.
- At least 32 GiB host free space is required before start. Requested and observed VM resources are
  fixed at four CPUs, 6 GiB RAM, 8 GiB root and 16 GiB data.

The actual execution wrapper must create and pass the private mode-0700 `HOME` inside the canonical
root; the canonical launcher alone does not supply that isolation.

## Source, build and image review

- Runtime construction requires a clean committed `jobs/` tree and stable exact HEAD. The context
  is reconstructed from admitted HEAD Git blobs, checked byte-for-byte against live regular files,
  bounded to 3,000 files/64 MiB and excludes ignored, hidden, test, build-output and unrelated paths.
  Missing Docker inputs fail the real build rather than falling back to ambient files.
- Docker 29.8.1 lacks Buildx in this environment. The harness selects exactly
  `DOCKER_BUILDKIT=0` and reports `legacy-explicit`; it does not install a plugin or claim
  BuildKit/CI/OCI byte equivalence. Unsupported legacy construction must fail closed.
- Native identity is required at every layer: Darwin arm64 host, aarch64 guest, arm64 Docker engine
  and linux/arm64 image. Binfmt and Rosetta are disabled. The image is not an amd64 result.
- The unchanged Dockerfile uses digest-pinned Rust and Playwright bases. The build records exact
  committed source and CI-only zero config/migration/protocol hashes. The harness reads the embedded
  runtime measurement back from the exact image under a networkless, read-only, capability-dropped
  container and validates its full identity; build arguments alone are not accepted as proof.
- Image ID, platform and nonroot runtime user are read from the exact built image. The output is a
  local debug/nonproduction image and is not signed, published, promoted or release-qualified.

## Native smoke review

- The exact committed existing smoke script is fed through stdin unchanged. Working directory
  `/app/runner` preserves its `../runner/dist` module resolution without adding a bind mount.
- The exact image-ID container has no network, published ports, host binds, privilege, added
  capability or writable root. It drops all capabilities, forbids privilege escalation and bounds
  CPU, memory and PIDs.
- Only bounded `noexec,nosuid,nodev` tmpfs mounts are writable: 32 MiB `/tmp` and a mode-0700,
  image-UID/GID 16 MiB runner-data root. The finished container is inspected for the actual runtime
  configuration and clean exit before exact removal.
- The unchanged probe checks native open/identity, exclusive write, bounded exact read, inventory,
  delete and final unchanged-root state. It does not start the runner server or exercise an API,
  lease, browser, employer, receipt or provider.

## Reviewer-observed checks

- `node --check` passed for the harness and focused test file.
- `node --test jobs/scripts/run-nonprod-linux-runner-tests.test.mjs` passed 17/17 with zero failures,
  skips or cancellations.
- `git diff --check 57635b29 -- jobs/package.json
  jobs/scripts/run-nonprod-linux-runner-tests.mjs
  jobs/scripts/run-nonprod-linux-runner-tests.test.mjs` passed.

These checks do not prove any Linux, Docker, VM, image or native-addon runtime behavior.

## Prior parent-reported committed-source regression gates

The following evidence was not independently reproduced by this reviewer:

- Before FIX-805, canonical session 27127 exited zero. Report/Temporal tests passed 8/8, object tests
  7/7 and the then-current Linux harness tests 12/12. Workspace tests passed automation 804, Browser 219, runner 308,
  workflows 321 and portal 387: 2,066 total passes and zero skips.
- Configured Chromium was installed only inside the owned scratch root before the suite, so the
  existing exact-submit/beacon synthetic-route test executed rather than conditionally skipping.
  All five workspace typechecks passed. The exact scratch root was removed and its parent was empty.
- Parent read-only inspection of official Colima v0.10.3 source confirmed that `LimaDir()` honors
  `LIMA_HOME` and passes it to limactl. This supports the harness's single private Colima/Lima
  inventory boundary; the actual installed CLI/config/runtime behavior remains a pending run gate.

This is not a fresh full Rust suite, all-target Clippy, VM, image or native-addon result.

## Failed actual attempt and recovery

Parent-reported evidence below was not independently reproduced and is not a Linux qualification:

- Harness source `e5532431`, session 10495 exited one at `vm_start`, before image construction or
  addon smoke. The existing cleanup fence retained the separate state root rather than treating the
  failed start as safe deletion authority.
- Scoped diagnostic session 78399 reproduced the exact 105-byte-versus-104-byte Lima SSH socket
  rejection. No VM instance, configuration or VM process was created; exact-profile stop reported
  not running and both private inventories were empty.
- Before manual recovery the parent revalidated the exact root's inode, UID, mode, marker, empty
  inventories and absence of open files. Exact private-profile delete was followed by another
  identity/open-file check; only the task's 331 MiB recreatable cache/context/sparse-disk state was
  removed. The exact path is absent, free space returned to about 39 GiB, four unrelated profiles
  remain stopped and the global Docker context remains `default`.

The manual recovery is not evidence that automatic cleanup passed and does not weaken the frozen
cleanup rules.

## Pending executable gates

- Commit the reviewed harness/test/package/plan bytes so the image's source commit names the exact
  clean admitted HEAD, then rerun source review if those bytes change.
- Run the normal Jobs regression suite, configured Chromium guard and five workspace typechecks on
  the committed source.
- Under the shared machine lock and canonical launcher, supply an explicit private child `HOME`,
  execute the actual isolated VM/image/smoke harness and require exact result parsing, clean
  VM/container termination, absent owned PIDs/socket/state and canonical-root cleanup.
- Treat unsupported CLI/config/legacy-builder behavior or ambiguous cleanup as a recorded failure;
  do not switch profiles, install tools, broaden resources or weaken isolation.

## Current verdict and limits

🟡 **STATIC HARNESS ACCEPTED; LINUX RUNTIME QUALIFICATION PENDING** — no remaining P0–P3
finding in the reviewed frozen source. This is not Linux runtime evidence and does not approve a
production runner, amd64 image, BuildKit/CI equivalence, release artifact, registry publication,
signing, deployment, capacity/latency, live API/PostgreSQL lease, browser/employer action, receipt,
provider behavior or production readiness.
