import assert from "node:assert/strict";
import { test } from "node:test";
import { admittedContextPath, assertEmptyInventories, assertPrivateRoot, assertRecordedPidsAbsent, assertResources,
  assertRuntimeMeasurement, assertSmokeInspection, assertVmConfiguration, cleanupArguments,
  assertUnixSocketBudget, boundedDiagnosticTail, cleanupInventoryProof, CONSTRUCTION_BUILDER, diagnosticRetentionPlan,
  failureReport, isolatedEnvironment, ownedLimaPids, parseInventory, smokeArguments,
  startArguments } from "./run-nonprod-linux-runner-tests.mjs";

const profile = `p634-${"a".repeat(20)}`;
const state = "/private/tmp/bluey-jobs-linux-p634-abcdef";
const root = "/private/tmp/bluey-test-runs-501/run.1234567890.456.abc";
const home = `${root}/child-home`;
const image = `sha256:${"b".repeat(64)}`;
const privateInfo = { directory: true, symlink: false, uid: 501, mode: 0o700 };

test("private stdout and stderr capture keeps only bounded tails without public raw content", () => {
  let bytes = boundedDiagnosticTail(Buffer.from("first"), Buffer.from(" second"));
  assert.equal(bytes.toString(), "first second");
  bytes = boundedDiagnosticTail(bytes, Buffer.alloc(70 * 1024, 65));
  assert.equal(bytes.length, 64 * 1024);
  bytes = boundedDiagnosticTail(bytes, Buffer.from("closed diagnostic tail"));
  assert.equal(bytes.length, 64 * 1024);
  assert(bytes.subarray(-22).equals(Buffer.from("closed diagnostic tail")));
  assert.throws(() => boundedDiagnosticTail("raw text", Buffer.alloc(0)));
});

test("failure reports preserve primary cause separately from cleanup and expose only diagnostic metadata", () => {
  const report = failureReport({ stage: "vm_start", primaryFailure: "command_colima_failed",
    cleanupFailure: "cleanup_unproven_state_retained", retained: true, stateRoot: state, profile,
    diagnostics: [{ file: "command-failure-0001.stderr", tool: "colima", stage: "vm_start", bytes: 128,
      stderr: "private fixture diagnostic", authorization: "private fixture credential" }] });
  assert.equal(report.code, "command_colima_failed");
  assert.equal(report.primaryFailure, "command_colima_failed");
  assert.equal(report.cleanupFailure, "cleanup_unproven_state_retained");
  assert.equal(report.retainedState, state);
  assert.equal(JSON.stringify(report).includes("private fixture"), false);
  assert.deepEqual(report.diagnostics, [{ file: "command-failure-0001.stderr", tool: "colima", stage: "vm_start", bytes: 128 }]);
  const cleaned = failureReport({ stage: "image_build", primaryFailure: "command_docker_failed", cleanupProven: true });
  assert.equal(cleaned.cleanupFailure, null); assert.equal(cleaned.cleanupProven, true);
  assert.equal(failureReport({ stage: "vm_start", cleanupFailure: "cleanup_unproven_state_retained" }).primaryFailure, null);
  assert.throws(() => failureReport({ diagnostics: [{ file: "../outside", tool: "colima", bytes: 1 }] }));
  assert.throws(() => failureReport({ diagnostics: [{ file: "command-failure-0001.stderr", tool: "colima", bytes: 65537 }] }));
  assert.equal(failureReport({ primaryFailure: "command_colima_failed", retained: true, cleanupProven: true, retainedDiagnosticsOnly: true,
    diagnostics: [{ file: "command-failure-0001.stdout", tool: "colima", stage: "vm_start", bytes: 16 }] }).retainedDiagnosticsOnly, true);
  assert.equal(failureReport({ primaryFailure: "command_colima_failed", retained: true, cleanupProven: true,
    cleanupFailure: "cleanup_unproven_state_retained" }).retainedDiagnosticsOnly, false);
});

test("Unix socket preflight admits103bytes, denies104bytes and counts UTF8 rather than characters", () => {
  const allowed = "/" + "a".repeat(102);
  assertUnixSocketBudget({ dockerSocket: allowed, limaSshSocket: allowed });
  for (const path of [allowed + "a", "/" + "é".repeat(52), "/socket\0", "relative"]) {
    assert.throws(() => assertUnixSocketBudget({ dockerSocket: path, limaSshSocket: allowed }));
    assert.throws(() => assertUnixSocketBudget({ dockerSocket: allowed, limaSshSocket: path }));
  }
  assertUnixSocketBudget({ dockerSocket: `${state}/colima/${profile}/docker.sock`,
    limaSshSocket: `${state}/l/colima-${profile}/ssh.sock.1234567890123456` });
  assert.throws(() => assertUnixSocketBudget({ dockerSocket: allowed,
    limaSshSocket: `${state}/lima/colima-${profile}/ssh.sock.1234567890123456` }));
});

test("proven-cleanup retention removes only explicit owned large children and rejects unknown entries", () => {
  const marker = ".phase634-owner.json";
  const names = [marker, "colima", "l", "cache", "context", "docker", "tmp", "image.id",
    "command-failure-0001.stdout", "command-failure-0001.stderr"];
  assert.deepEqual(diagnosticRetentionPlan(names), ["colima", "l", "cache", "context", "docker", "tmp", "image.id"]);
  assert.deepEqual(diagnosticRetentionPlan([marker, "command-failure-0001.stdout"]), []);
  for (const bad of [[...names, "unknown"], names.filter((name) => name !== marker),
    [...names, "../outside"], [...names, "colima"], [...names, "command-failure-9999.stdout"]]) {
    assert.throws(() => diagnosticRetentionPlan(bad));
  }
});

test("root admission rejects owner paths, aliases, permissions and canonical-root VM state", () => {
  assertPrivateRoot(state, state, privateInfo, 501, "state");
  assertPrivateRoot(root, root, privateInfo, 501, "launcher");
  for (const path of ["/", "/private/tmp", "/Users/owner", `${state}/child`, `${root}/vm`]) {
    assert.throws(() => assertPrivateRoot(path, path, privateInfo, 501, "state"));
  }
  for (const changed of [{ uid: 0 }, { mode: 0o755 }, { symlink: true }, { directory: false }]) {
    assert.throws(() => assertPrivateRoot(state, state, { ...privateInfo, ...changed }, 501, "state"));
  }
  assert.throws(() => assertPrivateRoot(state, state + "other", privateInfo, 501, "state"));
  assert.throws(() => isolatedEnvironment(`${root}/vm`, root, home));
  assert.throws(() => isolatedEnvironment(state, root, "/Users/owner"));
});

test("resource floor and fixed CPU RAM and disk bounds cannot be expanded", () => {
  const resources = { cpus: 4, memoryGiB: 6, rootDiskGiB: 8, dataDiskGiB: 16, freeBytes: 32 * 1024 ** 3 };
  assertResources(resources);
  for (const changed of [{ cpus: 8 }, { memoryGiB: 8 }, { rootDiskGiB: 20 }, { dataDiskGiB: 100 },
    { freeBytes: 32 * 1024 ** 3 - 1 }, { freeBytes: NaN }]) {
    assert.throws(() => assertResources({ ...resources, ...changed }));
  }
});

test("environment is a closed private allowlist and builder mode is explicitly legacy", () => {
  const env = isolatedEnvironment(state, root, home);
  assert.equal(env.HOME, home);
  assert.equal(env.COLIMA_HOME, `${state}/colima`);
  assert.equal(env.LIMA_HOME, `${state}/l`);
  assert.equal(env.DOCKER_CONFIG, `${state}/docker`);
  assert.equal(env.COLIMA_CACHE_HOME, `${state}/cache`);
  assert.equal(env.DOCKER_BUILDKIT, "0");
  assert.equal(CONSTRUCTION_BUILDER, "legacy-explicit");
  for (const key of ["SSH_AUTH_SOCK", "DOCKER_HOST", "DOCKER_CONTEXT", "AWS_ACCESS_KEY_ID", "HTTP_PROXY",
    "COLIMA_PROFILE", "GITHUB_TOKEN", "BLUEY_DATABASE_URL"]) assert.equal(Object.hasOwn(env, key), false);
  assert(Object.isFrozen(env));
});

test("start and cleanup argv bind one private profile without activation mounts or emulation", () => {
  const args = startArguments(profile);
  for (const value of ["--activate=false", "--template=false", "--ssh-agent=false", "--ssh-config=false",
    "--binfmt=false", "--vz-rosetta=false"]) assert(args.includes(value));
  assert.equal(args[args.indexOf("--mount") + 1], "none");
  assert.equal(args[args.indexOf("--vm-type") + 1], "vz");
  assert.deepEqual(cleanupArguments(profile, "delete"), ["delete", "--profile", profile, "--force", "--data"]);
  for (const target of ["default", "pinky", "--all", "../other", "p634-"]) {
    assert.throws(() => startArguments(target));
    assert.throws(() => cleanupArguments(target, "delete"));
  }
  assert.throws(() => cleanupArguments(profile, "prune"));
});

test("context includes only tracked admitted Jobs inputs, not owner or ignored ambient files", () => {
  for (const path of ["jobs/runner/Dockerfile", "jobs/.dockerignore", "jobs/package-lock.json",
    "jobs/runner/src/server.ts", "jobs/runner/native-storage/src/lib.rs"]) assert(admittedContextPath(path));
  for (const path of [".env", "jobs/runner/src/.env", "jobs/node_modules/private", "jobs/runner/dist/server.js",
    "jobs/runner/src/../../.env", "jobs/runner/tests/private.ts", "native/macos/overlay.swift", "../jobs/package.json"]) {
    assert.equal(admittedContextPath(path), false);
  }
});

test("private inventory cleanup and PID observation reject foreign unknown or partial state", () => {
  assertEmptyInventories([], []);
  for (const pair of [[[{ name: profile }], []], [[], [{ name: "pinky" }]], [null, []]]) {
    assert.throws(() => assertEmptyInventories(...pair));
  }
  assert.deepEqual(parseInventory(""), []);
  assert.deepEqual(parseInventory('{"name":"a"}\n{"name":"b"}\n'), [{ name: "a" }, { name: "b" }]);
  assert.throws(() => parseInventory("ambiguous output"));
  assert.deepEqual(ownedLimaPids({ name: `colima-${profile}`, hostAgentPID: 12345, driverPID: 12346 }, profile), [12345, 12346]);
  const stopped = { name: `colima-${profile}`, status: "Stopped", hostAgentPID: 0, driverPID: 0 };
  assert.deepEqual(ownedLimaPids(stopped, profile, { allowStopped: true }), []);
  assert.throws(() => ownedLimaPids(stopped, profile));
  assert.throws(() => ownedLimaPids({ ...stopped, status: "Unknown" }, profile, { allowStopped: true }));
  assert.throws(() => ownedLimaPids({ ...stopped, errors: ["ambiguous"] }, profile, { allowStopped: true }));
  assert.throws(() => ownedLimaPids({ ...stopped, driverPID: undefined }, profile, { allowStopped: true }));
  for (const row of [{ name: "pinky", hostAgentPID: 12345, driverPID: 12346 },
    { name: `colima-${profile}` }, { name: `colima-${profile}`, hostAgentPID: 0, driverPID: 0 },
    { name: `colima-${profile}`, hostAgentPID: "12345", driverPID: 12346 }]) {
    assert.throws(() => ownedLimaPids(row, profile));
  }
});

test("generated VM configuration requires exact resources and nil rather than default HOME mounts", () => {
  const config = "cpu: 4\nmemory: 6\nrootDisk: 8\ndisk: 16\narch: aarch64\nvmType: vz\nruntime: docker\nforwardAgent: false\nsshConfig: false\nautoActivate: false\nrosetta: false\nbinfmt: false\nportForwarder: none\nmounts: null\n";
  assertVmConfiguration(config);
  for (const invalid of [config.replace("cpu: 4", "cpu: 8"), config.replace("mounts: null", "mounts: []"),
    config.replace("mounts: null", "mounts:\n  - location: /Users/owner"), config + "cpu: 4\n",
    config.replace("rosetta: false", "rosetta: true")]) assert.throws(() => assertVmConfiguration(invalid));
});

test("destructive deletion requires complete exact stopped inventory, never absent or ambiguous start state", () => {
  const colima = [{ name: profile, status: "Stopped" }];
  const lima = [{ name: `colima-${profile}`, status: "Stopped", hostAgentPID: 0, driverPID: 0 }];
  assert.deepEqual(cleanupInventoryProof(colima, lima, profile, { requireStopped: true }), []);
  const running = [{ ...lima[0], status: "Running", hostAgentPID: 12345, driverPID: 12346 }];
  assert.deepEqual(cleanupInventoryProof([{ name: profile, status: "Running" }], running, profile), [12345, 12346]);
  assert.throws(() => cleanupInventoryProof([{ name: profile, status: "Running" }], lima, profile));
  for (const [left, right] of [[[], []], [colima, []], [[], lima], [colima, [...lima, ...lima]],
    [[{ name: "default", status: "Stopped" }], lima], [colima, [{ ...lima[0], status: "Unknown" }]],
    [colima, [{ ...lima[0], status: "Starting" }]], [colima, [{ ...lima[0], driverPID: undefined }]],
    [colima, [{ ...lima[0], status: "Stopped", hostAgentPID: 12345 }]], [colima, running]]) {
    assert.throws(() => cleanupInventoryProof(left, right, profile, { requireStopped: true }));
  }
});

test("recorded process absence is required before disk deletion and permission uncertainty cannot pass", () => {
  assertRecordedPidsAbsent([12345, 12346], () => false);
  for (const exists of [() => true, () => undefined, () => { throw Object.assign(new Error("denied"), { code: "EPERM" }); }]) {
    assert.throws(() => assertRecordedPidsAbsent([12345], exists));
  }
  for (const pids of [[0], [1], ["12345"], [2_147_483_648]]) {
    assert.throws(() => assertRecordedPidsAbsent(pids, () => false));
  }
});

test("exact image smoke argv is nonnetworked readonly bounded and nonroot", () => {
  const args = smokeArguments(image, 1000, 1000, `p634-smoke-${"a".repeat(20)}`);
  for (const value of ["none", "--read-only", "ALL", "no-new-privileges", "/app/runner", "--input-type=module"]) {
    assert(args.includes(value));
  }
  assert.equal(args.includes("--privileged"), false);
  assert.equal(args.includes("--volume"), false);
  assert.throws(() => smokeArguments("latest", 1000, 1000, `p634-smoke-${"a".repeat(20)}`));
  assert.throws(() => smokeArguments(image, 0, 0, `p634-smoke-${"a".repeat(20)}`));
  assert.throws(() => smokeArguments(image, 1000, 1000, "owner-container"));
});

test("actual container inspection rejects published ports host mounts and weakened runtime isolation", () => {
  const value = { Image: image, Config: { User: "1000:1000", WorkingDir: "/app/runner" },
    State: { Running: false, ExitCode: 0 }, Mounts: [], NetworkSettings: { Ports: { "8091/tcp": null } },
    HostConfig: { NetworkMode: "none", ReadonlyRootfs: true, Privileged: false, CapDrop: ["ALL"],
      CapAdd: [], SecurityOpt: ["no-new-privileges"], PidsLimit: 64, Memory: 512 * 1024 * 1024,
      NanoCpus: 1_000_000_000, Binds: [], PortBindings: {}, Tmpfs: {
        "/tmp": "rw,noexec,nosuid,nodev,size=32m,mode=1777",
        "/var/lib/bluey-jobs-runner": "rw,noexec,nosuid,nodev,size=16m,mode=0700,uid=1000,gid=1000" } } };
  assertSmokeInspection(value, image, 1000, 1000);
  for (const change of [{ Privileged: true }, { ReadonlyRootfs: false }, { NetworkMode: "host" },
    { Binds: ["/Users/owner:/owner"] }, { PortBindings: { "8091/tcp": [{ HostPort: "8091" }] } },
    { Tmpfs: {} }, { CapAdd: ["SYS_ADMIN"] }]) {
    assert.throws(() => assertSmokeInspection({ ...value, HostConfig: { ...value.HostConfig, ...change } }, image, 1000, 1000));
  }
});

test("runtime metadata is readback authority and never inferred solely from build arguments", () => {
  const source = "c".repeat(40); const buildId = `nonprod-${profile}`;
  const value = { version: 1, audience: "bluey-jobs-managed-cloud-runtime-measurement-v1", sourceCommit: source,
    buildId, componentId: "jobs-runner", roles: ["managed_runner"], configSchemaSha256: "0".repeat(64),
    migrationSetSha256: "0".repeat(64), protocolSetSha256: "0".repeat(64) };
  assertRuntimeMeasurement(value, source, buildId);
  for (const changed of [{ sourceCommit: "d".repeat(40) }, { componentId: "jobs-workflows" },
    { roles: ["source_verifier"] }, { protocolSetSha256: "a".repeat(64) }, { version: 2 }]) {
    assert.throws(() => assertRuntimeMeasurement({ ...value, ...changed }, source, buildId));
  }
});
