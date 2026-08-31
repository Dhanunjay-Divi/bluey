# FIX-785: Bluey test artifact growth

> **Codex preflight:** Loaded `$bluey-ops` and `$codex-storage-archive`, reconciled their guidance
> with the dedicated Phase 625 worktree, and did not inspect or use the SSD archive.

**Status:** Independent source-delta review GREEN with no P0–P3 findings; exact-tip hosted CI and
integrated-successor evidence remain pending.

## Issue

Repeated Bluey local tests were reported to recreate approximately 7–20 GiB per hour. A failed or
interrupted verification could leave enough persistent build and temporary database state to
recreate the same root-disk pressure on the next phase.

## Root Cause

Local Rust commands normally use checkout-relative Cargo target directories. Multiple large
worktrees and repeated dev/test builds therefore retain separate compiled dependency graphs,
incremental state, and debug-heavy artifacts. Tests use operating-system temporary locations for
SQLite and related files, but there was no repository launcher that owned those paths together or
removed them after every command outcome. Shell failure and ordinary signals had no common cleanup
contract; SIGKILL and host crashes had no marker-bound later recovery.

The Jobs hosted lane already disables incremental/debug-heavy Cargo output and reclaims selected
ephemeral state, but those controls did not protect arbitrary local commands.

## Fix Summary

Add canonical `scripts/run-bluey-tests.sh` with `--self-test`, `all`, and direct-command modes. It
confines Cargo, temporary, primary SQLite, data, config, runtime, and log paths to one private
local `/tmp` root; forces SQLite; removes inherited PostgreSQL URLs; disables incremental/dev-test
debug bloat; supervises a distinct command process group; and runs every mode from the physical
repository root.

A 60-second, marker-bound next-launch reaper handles eligible SIGKILL/crash leftovers. It refuses
symlinks, unowned or malformed paths and markers, broad/repository/worktree/Downloads/Volumes
parents, young/future-dated candidates, and any run whose launcher or process group is active. It
revalidates immediately before deletion. Current cleanup checks removal status and exact absence;
signals are deferred through `mkdir` ownership; and marker establishment makes later missing or
corrupt marker state fail closed. Failure turns child success into status 74 without hiding a
nonzero/signal result.

Fresh audit found that Bash 3.2 could parse negative process-group operands differently from the
intended external `kill` contract, the self-test did not prove child receipt, stale deletion errors
were warnings, unknown owned marker state was silently skipped, and one daemon integration test
still wrote a process-ID-only database directly under `/tmp`. The correction uses `/bin/kill` with
an explicit option terminator, resets inherited ordinary-signal dispositions before `setsid`, and
proves child receipt of HUP/INT/TERM. Unknown owned retained metadata or failed eligible stale-root
deletion now returns status 75 before child start with bounded exact-root guidance. The daemon test
uses a collision-safe RAII temporary directory under the launcher-controlled temporary root.

A second independent safety audit found that marker numbers still reached Bash arithmetic without
canonical length/range bounds, the run name did not cross-bind its epoch/PID, state did not bind the
process-group shape, and the second read could accept changed-but-valid marker authority. The
follow-up rejects leading-zero/overflow values before arithmetic, binds `run.<epoch>.<pid>.<nonce>`
and starting/running group state exactly, requires both claimed and filesystem age, snapshots root
and marker device/inode/mtime plus every canonical field, and reruns fresh age/owner/group checks
immediately before deletion. A young root with an old marker or any between-check identity change
is preserved and blocks the next command.

A final delta review found a deterministic pre-gate signal race: the wrapper could wait on the
gate before signal reset/group creation, while launcher cleanup could wait indefinitely for that
wrapper. It also found incomplete structural enforcement of the nine second-read marker fields and
a marker-swap fixture that changed fields instead of isolating inode identity. The correction adds
a private post-`setsid` ready handshake, defers launcher signals through exact PID/PGID and
running-marker setup, dispatches before opening the command gate, and bounds group/PID polling plus
KILL escalation before reaping. Deterministic pre-gate HUP/INT/TERM tests prove statuses
129/130/143, no user command start, wrapper exit, and exact-root cleanup. The CI guard mutates each
of all nine comparisons, and a byte-equivalent same-field fixture preserves timestamps while
replacing only the marker inode.

The fresh independent final audit then found that normal current-run cleanup still trusted a
re-readable marker without binding the exact root/marker identities established by the launcher,
that Git ancestry was checked on the requested parent's ancestor rather than the exact requested
path, and that process substitution discarded `git worktree list` failure status. The correction
binds the created root device/inode exactly once and snapshots marker device/inode/mtime plus all
nine marker fields after each verified marker write, then revalidates them before deletion.
Deterministic fixtures replace the marker
inode with identical bytes, rewrite state/group on the same inode while restoring mtime, and swap
the entire root; all retain evidence with status 74. Exact Git directory/file/broken-symlink parents
are rejected before mutation. Registered-worktree enumeration is status-checked, capped at 1 MiB,
and also runs before parent creation; guard mutations protect every new binding and failure path.

The newest independent delta review found four remaining safety defects: marker transitions could
re-authorize a substituted root, both stale and current cleanup recursively removed a pathname
after its last identity check, Git worktree output was bounded only after full buffering, and the
guard preserved the unsafe root-assignment shape without independently mutating every new fence.
The correction now treats the post-`mkdir` root identity as immutable, binds the complete marker
temp identity before an atomic rename, and rejects a byte-identical marker swap before committing
the new snapshot. Stale and current cleanup use the same atomic same-parent quarantine rename,
prove original-path absence, validate the moved root and marker twice, and delete only from a
verified quarantine/current-directory handle with same-filesystem traversal and nonrecursive
directory removal. Any ambiguity retains a `run.*` quarantine that blocks the next command.
Worktree discovery is now capped before shell buffering and time-bounded while preserving the Git
producer status. Deterministic fixtures cover the root/marker transition, the final source-to-
quarantine rename window, both current and stale final-delete substitution, forced deletion
failure, and retained-quarantine admission; the structural guard independently mutates every
identity, field, producer-status, bound, ordering, and call site.

A fresh independent line-by-line review of the corrected bytes found no P0–P3 issues. It verified
the one-time immutable root binding, pre-bound atomic marker rename, same-parent atomic quarantine,
original-path absence, repeated moved-root/marker checks, current-directory-anchored
same-filesystem deletion, nonrecursive directory removal, retained-quarantine admission block,
process-group status behavior, bounded worktree producer, and the corresponding negative guard
mutations. This GREEN verdict is limited to the source delta; hosted exact-tip and integrated-
successor evidence are still required.

## Files Modified

| File | Change |
|---|---|
| `scripts/run-bluey-tests.sh` | Add the canonical launcher, process-group supervisor, reaper, and embedded self-test. |
| `.github/workflows/jobs-ci.yml` | Run the lightweight launcher self-test before expensive Jobs gates. |
| `jobs/scripts/ci-guards-self-test.mjs` | Require the CI wiring and critical disk/signal/reaper authority. |
| `crates/cue-daemon/Cargo.toml`, `Cargo.lock` | Add the test-only temporary-directory helper. |
| `crates/cue-daemon/tests/mic_device_selection.rs` | Keep microphone-setting SQLite state inside launcher-controlled temporary storage. |
| `AGENTS.md`, `README.md` | Make the launcher the documented local test/check path and state the SIGKILL boundary. |
| `CHANGELOG.md`, Round 625, Phase 625 IMPL/REVIEW | Record the incident, corrections, evidence, and limitations. |

## Edge Cases Handled

- Success, arbitrary nonzero exit, HUP, INT, and TERM preserve their defined status unless a
  successful child cannot be cleaned, which returns explicit status 74.
- A distinct process group contains ordinary descendants; portable external process-group signals
  stop and wait for that group with bounded KILL escalation, while the stale reaper retains any root
  bound to a live group. The self-test requires a running child to record each ordinary signal and
  separately proves pre-gate delivery cannot start the user command or hang on a zombie wrapper.
- Cleanup safely removes an owned early unmarked root, but after marker establishment requires the
  same established root device/inode, marker device/inode/mtime, and all nine canonical marker
  fields. Any replaced, linked, unowned, absent, malformed, rewritten, or substituted current root
  is retained for manual audit rather than risking unrelated data.
- The stale reaper checks both launcher PID and process group twice and skips permission-ambiguous
  live state conservatively, including any `ps` failure. It ignores a group only when every
  matching process is a zombie, which cannot execute or retain an open file; this avoids a
  nondeterministic cleanup failure while a parent is waiting to reap a terminated descendant.
- Marker numeric fields are canonical decimals capped below Bash signed-arithmetic limits. The run
  name repeats the exact creation epoch and owner PID, `starting` requires group `0`, and `running`
  requires a group greater than one and distinct from the launcher group.
- Root and marker device/inode/mtime and all canonical marker fields must be identical across both
  reads; a refreshed clock, filesystem age, owner PID, and command group are rechecked immediately
  before exact deletion.
- The created root identity is assigned once. Marker transitions pre-bind the temporary marker and
  atomically rename it, then verify the immutable root plus marker inode/mtime before changing
  cleanup authority.
- Eligible current and stale roots are atomically moved into a unique private same-parent
  quarantine before any recursion. The original pathname must be absent; moved root/marker
  authority is checked before and immediately after the deterministic final-delete boundary; and
  deletion is anchored inside that quarantine. Any failure retains the quarantine and blocks the
  next launch.
- A startup grace prevents a second launcher from mistaking a concurrent marker update for a
  stale valid run. Missing, linked, malformed, mismatched, or future-dated owned metadata instead
  blocks a new run because safe age/process authority cannot be established.
- Failed deletion of an otherwise eligible stale root blocks before the child command starts; the
  exact root is retained and named without dumping marker contents or widening deletion advice.
- Parent validation occurs before mutation, creates only an absent 0700 leaf, never chmods an
  existing parent, and rejects dot segments, non-private existing parents, exact or ancestral Git
  metadata (including links), registered worktree overlap, Downloads siblings, broad temp roots,
  and `/Volumes`. Worktree enumeration is producer-status checked, time-bounded, and capped before
  shell buffering; failure or a response above 1 MiB fails closed.
- Poisoned primary database, backend, PostgreSQL URL, and log values are overridden by safe
  run-local SQLite and log paths.

## How to Test

```bash
bash -n scripts/run-bluey-tests.sh
bash scripts/run-bluey-tests.sh --self-test
node jobs/scripts/ci-guards-self-test.mjs
bash scripts/run-bluey-tests.sh -- \
  cargo test -p cue-daemon --test mic_device_selection \
    load_mic_device_setting_round_trips -- --exact
bash scripts/check-bluey-ops-docs.sh
actionlint .github/workflows/jobs-ci.yml  # only when already installed
git diff --check
```

## Independent Review Evidence

Reviewed frozen 12-path canonical content manifest:

```text
37a00810346ebe0093c23267da0312c3f2fecac08fb350c17a6c1b378aee7dca
```

The independent pass recorded:

- no P0–P3 findings;
- `bash -n scripts/run-bluey-tests.sh`: PASS;
- `bash scripts/run-bluey-tests.sh --self-test`: PASS in 26.1 seconds on native macOS Bash 3.2;
- Node syntax plus `jobs/scripts/ci-guards-self-test.mjs`: PASS in 15.4 seconds;
- Bluey operations-doc guard and diff whitespace check: PASS; and
- no canonical run-root or self-test residue and no surviving Cargo, Rust compiler, or launcher
  process.

`actionlint` was unavailable and skipped. The reviewer did not run Rust, Cargo, Docker, network,
commit, push, or production actions.

## Known Limitations

- SIGKILL and host crashes cannot execute cleanup directly. Only a later launcher invocation may
  recover a sufficiently old, marker-valid leftover whose recorded processes are inactive.
- Commands that deliberately escape the supervised process group are out of scope.
- An intentionally corrupted or removed marker fails safe by retaining the directory for manual
  inspection and blocks later large commands until the exact retained root is resolved.
- A retained quarantine is intentionally not retried automatically; it blocks later large commands
  until its exact evidence is inspected and resolved.
- Isolation deliberately trades cross-run compilation reuse for bounded disk use. Related checks
  may be grouped inside one explicitly reviewed shell command when reuse is important.
- The launcher prevents new residue; it does not delete historical repository targets or databases.
