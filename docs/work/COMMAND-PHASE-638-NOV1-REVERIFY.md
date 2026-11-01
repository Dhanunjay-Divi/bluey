# Phase 638 Nov1 current-source Rust re-verification

## Status

**PREPARED, NOT EXECUTED.** Parent-only local qualification command for the
Nov2 test checkpoint. It is not release, deployment or production authority.

Exact admitted source:

- HEAD `409498589b8d2e62b5ef54ebdee1372aeb8d005f`;
- 774-source manifest
  `3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1`;
- scoped-status SHA-256
  `6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5`;
- `tests.rs` SHA-256
  `cca8f7aa8fa16ae98a84c5a7e5a23d638375e7802f0ee04a4ac684905cf7bb72`.

## Reused bounded evidence

Do not rerun the 36 focused cases or 43 fresh-database PostgreSQL cases.
Run76069 recorded those cases passing on the exact manifest above, and later
quiescence reconciliation confirmed the same source, HEAD and scoped status,
an absent exact root, empty parent and no related process. Its full all-target
outcome was lost and remains unverified. This command therefore reruns format,
strict both-feature all-target Clippy and the missing no-PostgreSQL all-target
suite only. Reuse is bounded to those dated 79 passes and saves the redundant
private-PostgreSQL/test cost; it does not turn run76069 into a full pass.

The independently executed Nov1 offline Jobs gate is separate evidence: 2,380
tests passed with zero extra skips, five typechecks and all five builds. Its two
explicit browser exclusions remain unverified and are not covered here.

## Durable evidence

Before dispatch, the controller exclusively creates mode-`0600`:

`docs/work/EVIDENCE-PHASE-638-NOV1-RUST-20261101.jsonl`

It refuses an existing path and never deletes or overwrites it. Every append is
fsynced. Only whitelisted event names, source hashes, numeric summaries, exit
state and cleanup booleans are retained; no command output, candidate/auth data,
report payload, environment dump or database URL is written. The journal is
capped at 32 KiB. A partial journal or missing final event is not acceptance.

The canonical runtime parent is:

`/private/tmp/bluey-test-runs-phase638-nov1-rust-20261101`

It must be absent or empty before dispatch. The evidence file is deliberately
outside the auto-cleaned root and remains for review. Do not remove a prior
journal or any broader path to make a retry fit; use a separately reviewed name.

## Exact command

Run from the physical repository root under the shared Mac build lock.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- \
  /opt/homebrew/bin/node --input-type=module <<'OUTER_NODE'
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { constants } from "node:fs";
import { lstat, open, readFile, readdir, realpath, statfs } from "node:fs/promises";
import { createHash } from "node:crypto";
import { basename, join } from "node:path";
import { createInterface } from "node:readline";

process.umask(0o077);
const repo = "/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831";
const expectedHead = "409498589b8d2e62b5ef54ebdee1372aeb8d005f";
const expectedManifest =
  "3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1";
const expectedStatus =
  "6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5";
const expectedTests =
  "cca8f7aa8fa16ae98a84c5a7e5a23d638375e7802f0ee04a4ac684905cf7bb72";
const runParent =
  "/private/tmp/bluey-test-runs-phase638-nov1-rust-20261101";
const evidencePath = join(
  repo,
  "docs/work/EVIDENCE-PHASE-638-NOV1-RUST-20261101.jsonl",
);
assert.equal(await realpath(process.cwd()), repo);
const free = await statfs("/private/tmp", { bigint: true });
assert.ok(free.bavail * free.bsize >= 32n * 1024n ** 3n, "minimum_32GiB_free");

async function isAbsent(path) {
  try {
    await lstat(path);
    return false;
  } catch (error) {
    if (error?.code === "ENOENT") return true;
    throw error;
  }
}
assert.ok(await isAbsent(evidencePath), "evidence_file_must_be_new");
if (!(await isAbsent(runParent))) {
  const metadata = await lstat(runParent);
  assert.ok(metadata.isDirectory() && !metadata.isSymbolicLink());
  assert.equal(metadata.uid, process.getuid());
  assert.equal(metadata.mode & 0o7777, 0o700);
  assert.deepEqual(await readdir(runParent), []);
}

const evidence = await open(
  evidencePath,
  constants.O_WRONLY |
    constants.O_CREAT |
    constants.O_EXCL |
    constants.O_APPEND |
    constants.O_NOFOLLOW,
  0o600,
);
const evidenceInitial = await evidence.stat();
const evidencePathInitial = await lstat(evidencePath);
assert.ok(evidenceInitial.isFile() && evidenceInitial.nlink === 1);
assert.ok(evidencePathInitial.isFile() && !evidencePathInitial.isSymbolicLink());
assert.equal(evidencePathInitial.nlink, 1);
assert.equal(evidenceInitial.uid, process.getuid());
assert.equal(evidencePathInitial.uid, process.getuid());
assert.equal(evidenceInitial.mode & 0o7777, 0o600);
assert.equal(evidencePathInitial.mode & 0o7777, 0o600);
assert.equal(evidenceInitial.size, 0);
assert.equal(evidencePathInitial.size, 0);
assert.equal(evidencePathInitial.dev, evidenceInitial.dev);
assert.equal(evidencePathInitial.ino, evidenceInitial.ino);
const evidenceDevice = evidenceInitial.dev;
const evidenceInode = evidenceInitial.ino;
let sequence = 0;
let writeChain = Promise.resolve();
function record(value) {
  writeChain = writeChain.then(async () => {
    const beforeDescriptor = await evidence.stat();
    const beforePath = await lstat(evidencePath);
    assert.ok(beforeDescriptor.isFile() && beforeDescriptor.nlink === 1);
    assert.ok(beforePath.isFile() && !beforePath.isSymbolicLink());
    assert.equal(beforePath.nlink, 1);
    assert.equal(beforeDescriptor.uid, process.getuid());
    assert.equal(beforePath.uid, process.getuid());
    assert.equal(beforeDescriptor.mode & 0o7777, 0o600);
    assert.equal(beforePath.mode & 0o7777, 0o600);
    assert.equal(beforeDescriptor.dev, evidenceDevice);
    assert.equal(beforeDescriptor.ino, evidenceInode);
    assert.equal(beforePath.dev, evidenceDevice);
    assert.equal(beforePath.ino, evidenceInode);
    assert.equal(beforePath.size, beforeDescriptor.size);
    const line = `${JSON.stringify({
      schemaVersion: 1,
      sequence: ++sequence,
      at: new Date().toISOString(),
      ...value,
    })}\n`;
    assert.ok(Buffer.byteLength(line) <= 2048, "bounded_evidence_event");
    const writeResult = await evidence.write(line);
    assert.equal(writeResult.bytesWritten, Buffer.byteLength(line));
    await evidence.sync();
    const metadata = await evidence.stat();
    const pathMetadata = await lstat(evidencePath);
    assert.ok(metadata.isFile() && metadata.nlink === 1);
    assert.ok(pathMetadata.isFile() && !pathMetadata.isSymbolicLink());
    assert.equal(pathMetadata.nlink, 1);
    assert.equal(metadata.uid, process.getuid());
    assert.equal(pathMetadata.uid, process.getuid());
    assert.equal(metadata.mode & 0o7777, 0o600);
    assert.equal(pathMetadata.mode & 0o7777, 0o600);
    assert.equal(metadata.dev, evidenceDevice);
    assert.equal(metadata.ino, evidenceInode);
    assert.equal(pathMetadata.dev, evidenceDevice);
    assert.equal(pathMetadata.ino, evidenceInode);
    assert.equal(pathMetadata.size, metadata.size);
    assert.ok(metadata.size <= 32 * 1024, "bounded_evidence_file");
  });
  return writeChain;
}

const gitEnv = {
  PATH: "/opt/homebrew/bin:/usr/bin:/bin",
  HOME: repo,
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_OPTIONAL_LOCKS: "0",
};
const sourceScope = [
  "jobs",
  "server",
  "crates/cue-core",
  "infra",
  ".github",
  "scripts/run-bluey-tests.sh",
  "Cargo.toml",
  "Cargo.lock",
];
const statusScope = [
  "server",
  "crates/cue-core",
  "jobs",
  "infra",
  ".github",
  "Cargo.toml",
  "Cargo.lock",
];
function git(args) {
  const result = spawnSync("/usr/bin/git", args, {
    cwd: repo,
    env: gitEnv,
    encoding: "utf8",
    timeout: 15_000,
    maxBuffer: 16 * 1024 * 1024,
  });
  assert.equal(result.status, 0, "git_read_failed");
  return result.stdout;
}
const files = [
  ...new Set(
    git([
      "ls-files",
      "--cached",
      "--others",
      "--exclude-standard",
      "-z",
      "--",
      ...sourceScope,
    ])
      .split("\0")
      .filter(Boolean),
  ),
].sort();
assert.equal(files.length, 774);
async function identity() {
  const hashes = {};
  for (const file of files) {
    hashes[file] = createHash("sha256")
      .update(await readFile(join(repo, file)))
      .digest("hex");
  }
  const manifest = createHash("sha256")
    .update(JSON.stringify(hashes))
    .digest("hex");
  const head = git(["rev-parse", "HEAD"]).trim();
  const status = git([
    "status",
    "--porcelain=v1",
    "--untracked-files=all",
    "--",
    ...statusScope,
  ]);
  return {
    hashes,
    manifest,
    head,
    status,
    statusSha256: createHash("sha256").update(status).digest("hex"),
  };
}

let launcher = null;
let launcherPid = null;
let rootBasename = null;
let interrupted = false;
const seen = [];

try {
  const before = await identity();
  git(["diff", "--cached", "--quiet", "--exit-code", "--"]);
  assert.equal(before.manifest, expectedManifest);
  assert.equal(before.head, expectedHead);
  assert.equal(before.statusSha256, expectedStatus);
  assert.equal(before.hashes["server/src/db/jobs/tests.rs"], expectedTests);
  await record({
    event: "phase638_nov1_rust_started",
    sourceFiles: files.length,
    sourceManifestSha256: before.manifest,
    head: before.head,
    statusSha256: before.statusSha256,
    reusedFocused: 36,
    reusedActualPostgres: 43,
    realIndexEmpty: true,
  });

  async function innerMain() {
    const assert = (await import("node:assert/strict")).default;
    const { spawn, spawnSync } = await import("node:child_process");
    const { createHash } = await import("node:crypto");
    const { lstat, mkdir, readFile, realpath, symlink } = await import(
      "node:fs/promises"
    );
    const { basename, join } = await import("node:path");
    const expectedManifest =
      "3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1";
    const expectedHead = "409498589b8d2e62b5ef54ebdee1372aeb8d005f";
    const expectedStatus =
      "6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5";
    const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT);
    const emit = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);
    const gitEnvironment = {
      PATH: "/opt/homebrew/bin:/usr/bin:/bin",
      HOME: root,
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_OPTIONAL_LOCKS: "0",
    };
    const sourceScope = [
      "jobs",
      "server",
      "crates/cue-core",
      "infra",
      ".github",
      "scripts/run-bluey-tests.sh",
      "Cargo.toml",
      "Cargo.lock",
    ];
    const statusScope = [
      "server",
      "crates/cue-core",
      "jobs",
      "infra",
      ".github",
      "Cargo.toml",
      "Cargo.lock",
    ];
    const git = (args) => {
      const result = spawnSync("/usr/bin/git", args, {
        env: gitEnvironment,
        encoding: "utf8",
        timeout: 15_000,
        maxBuffer: 16 * 1024 * 1024,
      });
      assert.equal(result.status, 0);
      return result.stdout;
    };
    const files = [
      ...new Set(
        git([
          "ls-files",
          "--cached",
          "--others",
          "--exclude-standard",
          "-z",
          "--",
          ...sourceScope,
        ])
          .split("\0")
          .filter(Boolean),
      ),
    ].sort();
    assert.equal(files.length, 774);
    async function identity() {
      const hashes = {};
      for (const file of files) {
        hashes[file] = createHash("sha256").update(await readFile(file)).digest("hex");
      }
      const manifest = createHash("sha256")
        .update(JSON.stringify(hashes))
        .digest("hex");
      const head = git(["rev-parse", "HEAD"]).trim();
      const status = git([
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--",
        ...statusScope,
      ]);
      return {
        hashes,
        manifest,
        head,
        status,
        statusSha256: createHash("sha256").update(status).digest("hex"),
      };
    }
    const before = await identity();
    assert.equal(before.manifest, expectedManifest);
    assert.equal(before.head, expectedHead);
    assert.equal(before.statusSha256, expectedStatus);
    emit({
      event: "phase638_nov1_inner_admitted",
      rootBasename: basename(root),
      sourceManifestSha256: before.manifest,
      head: before.head,
      statusSha256: before.statusSha256,
    });

    const home = join(root, "rust-home");
    const cargoHome = join(home, ".cargo");
    await mkdir(home, { mode: 0o700 });
    await mkdir(cargoHome, { mode: 0o700 });
    const sourceCargo = await realpath(
      process.env.CARGO_HOME || join(process.env.HOME, ".cargo"),
    );
    for (const directory of ["registry", "git"]) {
      try {
        const cached = await realpath(join(sourceCargo, directory));
        const metadata = await lstat(cached);
        assert.ok(metadata.isDirectory());
        assert.equal(metadata.uid, process.getuid());
        await symlink(cached, join(cargoHome, directory), "dir");
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
    }
    const allowed = new Set([
      "PATH", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "SDKROOT",
      "DEVELOPER_DIR", "CARGO_TARGET_DIR", "CARGO_INCREMENTAL", "TMPDIR",
      "TMP", "TEMP", "SQLITE_TMPDIR", "BLUEY_TEST_RUN_ROOT", "BLUEY_DB_PATH",
      "BLUEY_SERVER_DB_BACKEND", "BLUEY_DATA_DIR", "BLUEY_CONFIG_DIR",
      "BLUEY_RUNTIME_DIR", "BLUEY_LOG_DIR", "CUE_DATA_DIR", "CUE_CONFIG_DIR",
      "CUE_RUNTIME_DIR", "CUE_LOG_DIR",
    ]);
    const clean = Object.fromEntries(
      Object.entries(process.env).filter(
        ([key]) => allowed.has(key) || /^CARGO_PROFILE_(DEV|TEST)_/.test(key),
      ),
    );
    const environment = {
      ...clean,
      HOME: home,
      CARGO_HOME: cargoHome,
      RUSTUP_HOME: await realpath(
        process.env.RUSTUP_HOME || join(process.env.HOME, ".rustup"),
      ),
      BLUEY_SERVER_DB_BACKEND: "sqlite",
    };
    for (const forbidden of [
      "BLUEY_TEST_POSTGRES_URL",
      "BLUEY_DATABASE_URL",
      "BLUEY_JOBS_REQUIRE_POSTGRES_TESTS",
    ]) assert.ok(!Object.hasOwn(environment, forbidden));

    const features =
      "integration-test-support,business-messaging-simulator-test-support";
    const crypto = [
      "--config", "profile.test.package.curve25519-dalek.opt-level=3",
      "--config", "profile.test.package.ed25519-dalek.opt-level=3",
      "--config", "profile.test.package.sha2.opt-level=3",
    ];
    let active = null;
    const handlers = ["SIGHUP", "SIGINT", "SIGTERM"].map((signal) => {
      const handler = () => active?.kill(signal);
      process.on(signal, handler);
      return [signal, handler];
    });
    async function run(args) {
      const child = spawn("cargo", args, {
        env: environment,
        stdio: ["ignore", "pipe", "inherit"],
      });
      active = child;
      let output = "";
      child.stdout.on("data", (chunk) => {
        process.stdout.write(chunk);
        output += chunk;
        if (output.length > 4 * 1024 * 1024) child.kill("SIGTERM");
      });
      const ended = await new Promise((resolve) => {
        child.once("error", () => resolve({ code: null, signal: "spawn_error" }));
        child.once("close", (code, signal) => resolve({ code, signal }));
      });
      active = null;
      assert.deepEqual(ended, { code: 0, signal: null });
      return output;
    }

    try {
      await run(["fmt", "--manifest-path", "server/Cargo.toml", "--all", "--", "--check"]);
      emit({ event: "phase638_nov1_fmt_passed" });
      await run([
        "clippy", "--offline", "--locked", "--manifest-path", "server/Cargo.toml",
        "--all-targets", "--features", features, "--", "-D", "warnings",
      ]);
      emit({ event: "phase638_nov1_clippy_passed" });
      const output = await run([
        ...crypto, "test", "--offline", "--locked", "--manifest-path",
        "server/Cargo.toml", "--all-targets", "--features", features,
      ]);
      const summaries = [
        ...output.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g),
      ];
      assert.ok(summaries.length >= 11, "all_target_summary_vector_missing");
      assert.ok(summaries.every((row) => row[2] === "0" && row[3] === "0"));
      const counts = summaries.map((row) => Number(row[1]));
      const total = counts.reduce((sum, count) => sum + count, 0);
      assert.ok(total >= 1875, "all_target_floor_shrank");
      const required = [
        "db::jobs::managed_cloud_prepared_ats_retry_requires_same_owner_live_exact_lease",
        "db::jobs::ats_certification_authority_tests::managed_cloud_ats_retry_rejects_incompatible_record_and_runtime",
        "db::jobs::ats_certification_authority_tests::phase_a_nonce_scope_packet_and_exact_replay_fail_closed",
        "db::jobs::ats_certification_authority_tests::certified_cloud_context_runs_phase_a_then_atomic_phase_b",
      ];
      for (const name of required) {
        const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        const matches = output.match(new RegExp(`^test ${escaped} \\.\\.\\. ok$`, "gm")) || [];
        assert.equal(matches.length, 1, `missing_or_ambiguous_FIX831:${name}`);
      }
      emit({
        event: "phase638_nov1_full_summary",
        counts,
        total,
        failed: 0,
        ignored: 0,
        exactFix831Passed: required.length,
      });
      const after = await identity();
      assert.equal(after.manifest, before.manifest);
      assert.equal(after.head, before.head);
      assert.equal(after.status, before.status);
      assert.deepEqual(after.hashes, before.hashes);
      emit({
        event: "phase638_nov1_inner_finished",
        sourceManifestSha256: after.manifest,
        head: after.head,
        statusSha256: after.statusSha256,
        sourceUnchanged: true,
      });
    } finally {
      for (const [signal, handler] of handlers) process.off(signal, handler);
    }
  }

  const innerSource = `(${innerMain.toString()})().catch(() => {\n` +
    `  console.error("phase638_nov1_inner_failed");\n` +
    `  process.exitCode = 1;\n});\n`;
  const handlers = ["SIGHUP", "SIGINT", "SIGTERM"].map((signal) => {
    const handler = () => {
      interrupted = true;
      void record({ event: "phase638_nov1_outer_signal", signal }).finally(() =>
        launcher?.kill(signal),
      );
    };
    process.on(signal, handler);
    return [signal, handler];
  });
  launcher = spawn(
    "scripts/run-bluey-tests.sh",
    ["--", "/usr/bin/env", "-u", "NODE_OPTIONS", "-u", "NODE_PATH",
      "/opt/homebrew/bin/node", "--input-type=module"],
    {
      cwd: repo,
      env: { ...process.env, BLUEY_TEST_RUN_PARENT: runParent },
      stdio: ["pipe", "pipe", "inherit"],
    },
  );
  launcherPid = launcher.pid;
  const endedPromise = new Promise((resolve) => {
    launcher.once("error", () => resolve({ code: null, signal: "spawn_error" }));
    launcher.once("close", (code, signal) => resolve({ code, signal }));
  });
  launcher.stdin.on("error", () => {});
  launcher.stdin.end(innerSource);
  const allowedEvents = new Set([
    "phase638_nov1_inner_admitted",
    "phase638_nov1_fmt_passed",
    "phase638_nov1_clippy_passed",
    "phase638_nov1_full_summary",
    "phase638_nov1_inner_finished",
  ]);
  let pumpFailure = null;
  const pump = (async () => {
    for await (const line of createInterface({ input: launcher.stdout, crlfDelay: Infinity })) {
      process.stdout.write(`${line}\n`);
      let value;
      try { value = JSON.parse(line); } catch { continue; }
      if (!allowedEvents.has(value?.event)) continue;
      if (value.event === "phase638_nov1_inner_admitted") {
        assert.match(value.rootBasename, /^run\.[0-9]{1,20}\.[0-9]{1,20}\.[0-9]{1,20}$/);
        assert.equal(value.sourceManifestSha256, expectedManifest);
        assert.equal(value.head, expectedHead);
        assert.equal(value.statusSha256, expectedStatus);
        rootBasename = value.rootBasename;
        await record({
          event: value.event,
          rootBasename,
          sourceManifestSha256: value.sourceManifestSha256,
          head: value.head,
          statusSha256: value.statusSha256,
        });
      } else if (value.event === "phase638_nov1_full_summary") {
        assert.ok(Array.isArray(value.counts) && value.counts.length >= 11);
        assert.ok(value.counts.every(Number.isSafeInteger));
        assert.equal(value.total, value.counts.reduce((sum, count) => sum + count, 0));
        assert.ok(value.total >= 1875);
        assert.equal(value.failed, 0);
        assert.equal(value.ignored, 0);
        assert.equal(value.exactFix831Passed, 4);
        await record({
          event: value.event,
          counts: value.counts,
          total: value.total,
          failed: 0,
          ignored: 0,
          exactFix831Passed: 4,
        });
      } else if (value.event === "phase638_nov1_inner_finished") {
        assert.equal(value.sourceManifestSha256, expectedManifest);
        assert.equal(value.head, expectedHead);
        assert.equal(value.statusSha256, expectedStatus);
        assert.equal(value.sourceUnchanged, true);
        await record({
          event: value.event,
          sourceManifestSha256: value.sourceManifestSha256,
          head: value.head,
          statusSha256: value.statusSha256,
          sourceUnchanged: true,
        });
      } else {
        await record({ event: value.event });
      }
      seen.push(value.event);
    }
  })().catch((error) => {
    pumpFailure = error;
    if (launcher.exitCode === null && launcher.signalCode === null) {
      launcher.kill("SIGTERM");
    }
  });

  const ended = await endedPromise;
  await pump;
  if (pumpFailure) throw pumpFailure;
  for (const [signal, handler] of handlers) process.off(signal, handler);
  await record({
    event: "phase638_nov1_launcher_exit",
    code: ended.code,
    signal: ended.signal,
    interrupted,
  });
  assert.deepEqual(seen, [
    "phase638_nov1_inner_admitted",
    "phase638_nov1_fmt_passed",
    "phase638_nov1_clippy_passed",
    "phase638_nov1_full_summary",
    "phase638_nov1_inner_finished",
  ]);
  assert.deepEqual(ended, { code: 0, signal: null });
  assert.equal(interrupted, false);
  assert.ok(rootBasename);
  assert.ok(await isAbsent(join(runParent, rootBasename)));
  const parentMetadata = await lstat(runParent);
  assert.ok(parentMetadata.isDirectory() && !parentMetadata.isSymbolicLink());
  assert.equal(parentMetadata.uid, process.getuid());
  assert.equal(parentMetadata.mode & 0o7777, 0o700);
  assert.deepEqual(await readdir(runParent), []);
  let launcherAbsent = false;
  try { process.kill(launcherPid, 0); }
  catch (error) {
    if (error?.code === "ESRCH") launcherAbsent = true;
    else throw error;
  }
  assert.equal(launcherAbsent, true);
  const after = await identity();
  assert.equal(after.manifest, before.manifest);
  assert.equal(after.head, before.head);
  assert.equal(after.status, before.status);
  assert.deepEqual(after.hashes, before.hashes);
  git(["diff", "--cached", "--quiet", "--exit-code", "--"]);
  await record({
    event: "phase638_nov1_outer_final_verified",
    sourceManifestSha256: after.manifest,
    head: after.head,
    statusSha256: after.statusSha256,
    exactRootAbsent: true,
    runParentEmpty: true,
    launcherPidAbsent: true,
    realIndexEmpty: true,
  });
  await record({
    event: "phase638_nov1_reverification_complete",
    acceptedLocalRustEvidence: true,
    reusedFocused: 36,
    reusedActualPostgres: 43,
  });
} catch {
  try {
    await record({
      event: "phase638_nov1_reverification_failed",
      code: "gate_failed",
    });
  } catch {}
  process.stderr.write("phase638_nov1_reverification_failed:gate_failed\n");
  process.exitCode = 1;
} finally {
  await writeChain.catch(() => {});
  await evidence.close().catch(() => {});
}
OUTER_NODE
```

## Acceptance

A pass requires command exit `0` and the journal sequence:

1. `phase638_nov1_rust_started`;
2. `phase638_nov1_inner_admitted`;
3. `phase638_nov1_fmt_passed`;
4. `phase638_nov1_clippy_passed`;
5. `phase638_nov1_full_summary`;
6. `phase638_nov1_inner_finished`;
7. `phase638_nov1_launcher_exit` with code `0`, null signal and no interruption;
8. `phase638_nov1_outer_final_verified`; and
9. `phase638_nov1_reverification_complete` with
   `acceptedLocalRustEvidence:true`.

The full summary must contain at least 11 target summaries, total at least 1875,
zero failed, zero ignored and exactly four observed FIX831 pass lines. Source,
HEAD and status must match before, inside and after the launcher. The exact root
must be absent, its parent empty and the launcher PID absent. Any partial journal,
missing terminal event, nonzero exit, signal, summary mismatch, identity mismatch
or cleanup mismatch is a failure. Keep the journal for review; an absent runtime
root alone is never a pass.

## Limits

This command plus the separately dated 36 focused/43 PostgreSQL passes qualifies
only local current Rust source. It does not qualify the API retry journey,
browser exclusions, Temporal, Linux/VM, provider receipts, hosted CI, release
artifacts, production flags, employer effects, cost, latency or capacity.
