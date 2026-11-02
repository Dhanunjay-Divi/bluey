import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import {
  assertApprovedCli,
  assertCleanupTargetInventory,
  assertEmptyInventories,
  assertLauncherMarker,
  assertListenerCapture,
  assertListenerSnapshotsEqual,
  assertLimaOverrideIdentity,
  assertLimaPortForwardOverride,
  assertNoAutomaticForwardAttempts,
  assertLauncherRoot,
  assertPrivateHome,
  assertPrivateRoot,
  assertRecordedPidsAbsent,
  assertResources,
  assertUnixSocketBudget,
  assertVersionOutput,
  assertVmConfiguration,
  boundedDiagnosticTail,
  cleanupArguments,
  cleanupInventoryProof,
  diagnosticRetentionPlan,
  executableIdentitySha256,
  failureReport,
  hostProcessEvidence,
  isolatedEnvironment,
  listenerDeltaFailureEvidence,
  listenerPolicy,
  listenerSnapshotPids,
  listenerDeltaPolicy,
  limaPortForwardOverrideBytes,
  parseInventory,
  parseHostProcessSnapshot,
  parseListenerRecords,
  parseUsernetPid,
  preStopEvidence,
  qemuInventoryPids,
  qemuManagementPort,
  reportMode,
  retryListenerObservation,
  assertUsernetArtifactIdentity,
  bindUsernetHelper,
  validateUsernetContinuity,
  startArguments,
} from "./run-nonprod-managed-runner-vm-tests.mjs";

const GIB = 1024 ** 3;
const profile = `p639-${"a".repeat(20)}`;
const state = "/private/tmp/bluey-jobs-linux-p639-abcdef";
const root = "/private/tmp/bluey-test-runs-phase639/run.1234567890.456.abc";
const home = `${root}/child-home`;
const listener = (pid, fd, address, port, connected = false, unbound = false,
  peerAddress = connected ? "192.0.2.20" : null, peerPort = connected ? 53 : null) => ({
  pid,
  fd,
  address,
  port,
  unbound,
  connected,
  peerAddress,
  peerPort,
  endpointSha256: fd.toString(16).padStart(64, "0"),
});
const canonicalJson = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};
const hostRecord = (pid, overrides = {}) => ({
  birthSec: "1000",
  birthUsec: "123",
  euid: "501",
  executableIdentitySha256: "a".repeat(64),
  helperClass: "lima",
  pid: String(pid),
  ppid: "1",
  ruid: "501",
  ...overrides,
});
const hostSnapshot = (...records) => parseHostProcessSnapshot(
  `${canonicalJson({ records, version: 1 })}\n`,
);
const successHostEvidence = {
  artifactIdentitySha256: "1".repeat(64),
  baselineSnapshotSha256: "2".repeat(64),
  birthIdentitySha256: "3".repeat(64),
  boundaryAfterSnapshotSha256: "4".repeat(64),
  boundaryBeforeSnapshotSha256: "5".repeat(64),
  executableIdentitySha256: "6".repeat(64),
  helperTcpDeltaCount: 1,
  helperUdpConnectedDeltaCount: 0,
  helperUdpDeltaCount: 0,
  helperUdpUnboundDeltaCount: 0,
  literalCodeSigningClaimed: false,
  literalNoHostNetworkClaimed: false,
  producerSha256: "3c53430e3e937a27b9697264b3b9e5f29ff76f395c7a52546c9e85035a88cf95",
  socketCount: 3,
  tcpDeltaCount: 1,
  udpDeltaCount: 0,
  version: 1,
};
const privateDirectory = {
  directory: true,
  file: false,
  symlink: false,
  uid: 501,
  mode: 0o700,
};
const privateFile = {
  directory: false,
  file: true,
  symlink: false,
  uid: 501,
  mode: 0o555,
};
const running = {
  name: `colima-${profile}`,
  limaVersion: "2.2.1",
  vmType: "qemu",
  arch: "x86_64",
  status: "Running",
  sshAddress: "127.0.0.1",
  sshLocalPort: 60022,
  hostAgentPID: 42,
  driverPID: 43,
};
const stableAmbientRecord = hostRecord(2000, {
  executableIdentitySha256: "b".repeat(64),
  helperClass: "python-or-other",
  ppid: "1999",
});
const usernetRecord = hostRecord(42, { executableIdentitySha256: "6".repeat(64) });
const usernetBirthSha256 = createHash("sha256").update(Buffer.from(
  `bluey-jobs-usernet-birth-v1\0${usernetRecord.pid}\0${usernetRecord.ppid}`
    + `\0${usernetRecord.birthSec}\0${usernetRecord.birthUsec}`,
)).digest("hex");
const udpAdmissionProof = (ambient = []) => {
  const baselineRecords = [hostRecord(10, { helperClass: "python-or-other" }), ...ambient]
    .sort((left, right) => Number(left.pid) - Number(right.pid));
  const postRecords = [...baselineRecords, usernetRecord]
    .sort((left, right) => Number(left.pid) - Number(right.pid));
  return {
    baselineProcesses: hostSnapshot(...baselineRecords),
    controllerPid: 5000,
    parentPid: 4999,
    postProcessesAfter: hostSnapshot(...postRecords),
    postProcessesBefore: hostSnapshot(...postRecords),
    usernetBinding: {
      artifactIdentitySha256: "1".repeat(64),
      birthIdentitySha256: usernetBirthSha256,
      executableIdentitySha256: "6".repeat(64),
      pid: 42,
    },
  };
};

test("D0 fixes the disposable QEMU resource envelope and exact cleanup target", () => {
  const resources = {
    cpus: 4,
    memoryGiB: 6,
    rootDiskGiB: 8,
    dataDiskGiB: 16,
    freeBytes: 32 * GIB,
  };
  assertResources(resources);
  for (const changed of [
    { cpus: 3 },
    { memoryGiB: 7 },
    { rootDiskGiB: 9 },
    { dataDiskGiB: 8 },
    { freeBytes: 32 * GIB - 1 },
    { freeBytes: NaN },
  ]) assert.throws(() => assertResources({ ...resources, ...changed }));

  const args = startArguments(profile);
  assert.equal(args[args.indexOf("--arch") + 1], "x86_64");
  assert.equal(args[args.indexOf("--vm-type") + 1], "qemu");
  assert.equal(args[args.indexOf("--mount") + 1], "none");
  for (const value of ["--ssh-agent=false", "--ssh-config=false", "--activate=false",
    "--template=false", "--binfmt=false", "none"]) assert(args.includes(value));
  assert.deepEqual(cleanupArguments(profile, "stop"), ["stop", "--profile", profile]);
  assert.deepEqual(cleanupArguments(profile, "force-stop"),
    ["stop", "--profile", profile, "--force"]);
  assert.deepEqual(cleanupArguments(profile, "delete"),
    ["delete", "--profile", profile, "--force", "--data"]);
  for (const invalid of ["default", "p639-", "../owned", "p639-ABC"]) {
    assert.throws(() => startArguments(invalid));
    assert.throws(() => cleanupArguments(invalid, "delete"));
  }
  assert.throws(() => cleanupArguments(profile, "prune"));
});

test("launcher, HOME, state, marker, and socket guards reject aliases and ambient roots", () => {
  assertLauncherRoot(root, root, privateDirectory, 501);
  assertPrivateRoot(state, state, privateDirectory, 501);
  assertPrivateHome(home, home, privateDirectory, 501, root);
  for (const changed of [
    { uid: 0 },
    { mode: 0o755 },
    { symlink: true },
    { directory: false },
  ]) {
    assert.throws(() => assertLauncherRoot(root, root, { ...privateDirectory, ...changed }, 501));
    assert.throws(() => assertPrivateRoot(state, state, { ...privateDirectory, ...changed }, 501));
  }
  assert.throws(() => assertLauncherRoot(root, `${root}-alias`, privateDirectory, 501));
  for (const invalid of [
    root.replace("run.1234567890", "run.01234567890"),
    root.replace(".456.", ".0456."),
    root.replace(".abc", `.${"a".repeat(65)}`),
  ]) assert.throws(() => assertLauncherRoot(invalid, invalid, privateDirectory, 501));
  assert.throws(() => assertPrivateRoot(`${state}/child`, `${state}/child`, privateDirectory, 501));
  assert.throws(() => assertPrivateHome("/Users/owner", "/Users/owner", privateDirectory, 501, root));

  const marker = Buffer.from([
    "magic=bluey-test-run-v2",
    `run_root=${root}`,
    `run_name=${root.split("/").at(-1)}`,
    "owner_uid=501",
    "owner_pid=456",
    "process_group_id=16882",
    "launcher_group_id=16881",
    "created_epoch=1234567890",
    "state=running",
    "",
  ].join("\n"));
  assertLauncherMarker(marker, root, 501);
  for (const changed of [
    marker.toString().replace("state=running", "state=done"),
    marker.toString().replace("owner_uid=501", "owner_uid=0"),
    marker.toString().replace("owner_pid=456", "owner_pid=0456"),
    marker.toString().replace("owner_pid=456", "owner_pid=457"),
    marker.toString().replace("process_group_id=16882", "process_group_id=0"),
    marker.toString().replace("launcher_group_id=16881", "launcher_group_id=1"),
    marker.toString().replace("created_epoch=1234567890", "created_epoch=1234567891"),
    marker.toString() + "extra=x\n",
  ]) assert.throws(() => assertLauncherMarker(Buffer.from(changed), root, 501));

  const env = isolatedEnvironment(state, root, home);
  assert.equal(env.LIMA_HOME, `${state}/l`);
  assert.equal(env.COLIMA_HOME, `${state}/colima`);
  for (const key of ["DOCKER_HOST", "DOCKER_CONTEXT", "SSH_AUTH_SOCK", "AWS_ACCESS_KEY_ID",
    "BLUEY_DATABASE_URL"]) assert.equal(Object.hasOwn(env, key), false);
  assert(Object.isFrozen(env));
  const dockerSocket = `${state}/colima/${profile}/docker.sock`;
  const limaSocket = `${state}/l/colima-${profile}/ssh.sock.1234567890123456`;
  assertUnixSocketBudget([dockerSocket, limaSocket]);
  assert.throws(() => assertUnixSocketBudget(["/" + "a".repeat(103)]));
  assert.throws(() => isolatedEnvironment(`${root}/vm`, root, home));
});

test("private Lima override blocks every non-SSH TCP and UDP guest port before boot", () => {
  const bytes = limaPortForwardOverrideBytes();
  assert.equal(bytes.toString("utf8"), [
    "portForwards:",
    "- guestIP: \"0.0.0.0\"",
    "  guestIPMustBeZero: false",
    "  guestPortRange: [1, 65535]",
    "  proto: any",
    "  ignore: true",
    "",
  ].join("\n"));
  assertLimaPortForwardOverride(bytes);
  assertLimaOverrideIdentity(privateDirectory, {
    ...privateFile,
    mode: 0o600,
    nlink: 1,
    size: bytes.length,
  }, 501);
  for (const mutation of [
    ["guestIPMustBeZero: false", "guestIPMustBeZero: true"],
    ["  guestIPMustBeZero: false\n", ""],
    ["guestPortRange: [1, 65535]", "guestPortRange: [2, 65535]"],
    ["proto: any", "proto: tcp"],
    ["ignore: true", "ignore: false"],
    ["guestIP: \"0.0.0.0\"", "guestIP: \"127.0.0.1\""],
  ]) {
    assert.throws(() => assertLimaPortForwardOverride(Buffer.from(
      bytes.toString("utf8").replace(...mutation),
    )));
  }
  assert.throws(() => assertLimaPortForwardOverride(Buffer.concat([bytes, Buffer.from("x")])));
  assert.throws(() => assertLimaPortForwardOverride(bytes.subarray(0, bytes.length - 1)));
  for (const changed of [
    { directory: { ...privateDirectory, mode: 0o755 } },
    { directory: { ...privateDirectory, uid: 0 } },
    { file: { ...privateFile, mode: 0o644, nlink: 1, size: bytes.length } },
    { file: { ...privateFile, mode: 0o600, nlink: 2, size: bytes.length } },
    { file: { ...privateFile, mode: 0o600, nlink: 1, size: bytes.length + 1 } },
  ]) {
    assert.throws(() => assertLimaOverrideIdentity(
      changed.directory ?? privateDirectory,
      changed.file ?? { ...privateFile, mode: 0o600, nlink: 1, size: bytes.length },
      501,
    ));
  }
  assertNoAutomaticForwardAttempts(Buffer.from("Not forwarding TCP 0.0.0.0:22\n"));
  assert.throws(() => assertNoAutomaticForwardAttempts(Buffer.from(
    "failed to set up forwarding tcp port\n",
  )));
  assert.throws(() => assertNoAutomaticForwardAttempts(Buffer.from(
    "Forwarding UDP from 0.0.0.0:53 to 127.0.0.1:53\n",
  )));
  assert.throws(() => assertNoAutomaticForwardAttempts(Buffer.alloc(256 * 1024 + 1)));
  assert.throws(() => assertNoAutomaticForwardAttempts(Buffer.from([0xff])));
  assert.throws(() => assertNoAutomaticForwardAttempts(Buffer.from("unknown\0record\n")));
});

test("CLI identity and exact version parsers reject writable binaries and near matches", () => {
  assertApprovedCli("/opt/homebrew/bin/colima", "/opt/homebrew/Cellar/colima/0.10.3/bin/colima",
    privateFile, 501);
  assertApprovedCli("/usr/bin/git", "/usr/bin/git", { ...privateFile, uid: 0 }, 501);
  assertApprovedCli("/usr/bin/python3", "/Applications/Xcode.app/python3",
    { ...privateFile, uid: 0 }, 501);
  for (const changed of [{ mode: 0o775 }, { symlink: true }, { file: false }, { uid: 777 }]) {
    assert.throws(() => assertApprovedCli("/opt/homebrew/bin/colima", "/real/colima",
      { ...privateFile, ...changed }, 501));
  }
  assert.throws(() => assertApprovedCli("/tmp/colima", "/tmp/colima", privateFile, 501));
  assertVersionOutput("colima", "colima version 0.10.3\n");
  assertVersionOutput("lima", "limactl version 2.2.1\n");
  assertVersionOutput("qemu", "QEMU emulator version 11.1.2\nCopyright\n");
  for (const [name, text] of [
    ["colima", "colima version 0.10.30\n"],
    ["lima", "limactl version 2.2.2\n"],
    ["qemu", "QEMU emulator version 11.1\n"],
    ["unknown", "colima version 0.10.3\n"],
  ]) assert.throws(() => assertVersionOutput(name, text));
});

test("inventory protocol records running PIDs and only normalizes reviewed stopped omissions", () => {
  assert.deepEqual(qemuInventoryPids(running, profile), [42, 43]);
  const stopped = { ...running, status: "Stopped", hostAgentPID: 0, driverPID: 0 };
  assert.deepEqual(qemuInventoryPids(stopped, profile, { allowStopped: true }), []);
  const omitted = {
    name: stopped.name,
    limaVersion: stopped.limaVersion,
    vmType: stopped.vmType,
    arch: stopped.arch,
    status: stopped.status,
  };
  assert.deepEqual(qemuInventoryPids(omitted, profile, { allowStopped: true }), []);
  for (const changed of [
    { limaVersion: "2.2.2" },
    { vmType: "vz" },
    { arch: "aarch64" },
    { status: "Starting" },
    { hostAgentPID: undefined },
    { driverPID: null },
    { errors: ["ambiguous"] },
  ]) assert.throws(() => qemuInventoryPids({ ...running, ...changed }, profile));
  assert.throws(() => qemuInventoryPids({ ...running, HostAgentPID: 42 }, profile));
  assert.throws(() => qemuInventoryPids(stopped, profile));

  assert.deepEqual(cleanupInventoryProof([{ name: profile, status: "Running" }], [running], profile),
    [42, 43]);
  assert.deepEqual(cleanupInventoryProof([{ name: profile, status: "Stopped" }], [stopped], profile,
    { requireStopped: true }), []);
  assert.throws(() => cleanupInventoryProof([{ name: profile, status: "Stopped" }], [running], profile,
    { requireStopped: true }));
  assert.throws(() => cleanupInventoryProof([], [], profile, { requireStopped: true }));
  assertEmptyInventories([], []);
  assert.throws(() => assertEmptyInventories([{ name: profile }], []));
});

test("partial exact-owned inventory may be stopped but never authorizes deletion", () => {
  assert.deepEqual(assertCleanupTargetInventory([{ name: profile, status: "Starting" }], [], profile),
    { colimaPresent: true, limaPresent: false });
  assert.deepEqual(assertCleanupTargetInventory([], [{ name: `colima-${profile}`, status: "Installing" }],
    profile), { colimaPresent: false, limaPresent: true });
  assert.deepEqual(assertCleanupTargetInventory([], [], profile),
    { colimaPresent: false, limaPresent: false });
  assert.throws(() => assertCleanupTargetInventory([{ name: "default" }], [], profile));
  assert.throws(() => assertCleanupTargetInventory([], [{ name: "colima-default" }], profile));
  assert.throws(() => assertCleanupTargetInventory([{ name: profile }, { name: profile }], [], profile));
  // The weaker stop-target proof cannot satisfy the strict delete proof.
  assert.throws(() => cleanupInventoryProof([{ name: profile, status: "Starting" }], [], profile,
    { requireStopped: true }));

  assert.deepEqual(preStopEvidence([{ name: profile, status: "Running" }], [running], profile), {
    deletionBlocked: false,
    recordedPids: [42, 43],
  });
  assert.deepEqual(preStopEvidence([{ name: profile, status: "Starting" }], [], profile), {
    deletionBlocked: false,
    recordedPids: [],
  });
  assert.deepEqual(preStopEvidence([], [{ name: `colima-${profile}`, status: "Installing" }],
    profile), { deletionBlocked: false, recordedPids: [] });
  for (const inventories of [
    [[{ name: profile, status: "Running" }], []],
    [[{ name: profile, status: "Unknown" }], []],
    [[{ name: profile, status: "Running" }], [{ ...running, hostAgentPID: undefined }]],
  ]) {
    assert.equal(preStopEvidence(inventories[0], inventories[1], profile).deletionBlocked, true);
  }
});

test("VM configuration parser rejects duplicates, host mounts, and resource widening", () => {
  const config = [
    "cpu: 4", "memory: 6", "rootDisk: 8", "disk: 16", "arch: x86_64", "vmType: qemu",
    "runtime: docker", "forwardAgent: false", "sshConfig: false", "autoActivate: false",
    "rosetta: false", "binfmt: false", "portForwarder: none", "mounts: null", "",
  ].join("\n");
  assertVmConfiguration(config);
  for (const invalid of [
    config.replace("cpu: 4", "cpu: 8"),
    config.replace("disk: 16", "disk: 32"),
    config.replace("mounts: null", "mounts: []"),
    config.replace("mounts: null", "mounts:\n  - location: /Users/owner"),
    config.replace("rosetta: false", "rosetta: true"),
    config + "cpu: 4\n",
  ]) assert.throws(() => assertVmConfiguration(invalid));
  assert.equal(qemuManagementPort(running), 60022);
  for (const invalid of [
    { ...running, sshAddress: "0.0.0.0" },
    { ...running, sshLocalPort: 0 },
    { ...running, sshLocalPort: "60022" },
  ]) assert.throws(() => qemuManagementPort(invalid));
});

test("host process protocol is canonical, closed, and binds the exact Lima executable", () => {
  const limaMetadata = {
    file: true,
    symlink: false,
    dev: "16777234",
    ino: "18446744073709551614",
    uid: 501,
    mode: 0o100755,
  };
  const limaPath = "/opt/homebrew/Cellar/lima/2.2.1/bin/limactl";
  const digest = executableIdentitySha256(limaPath, limaMetadata);
  const expected = createHash("sha256").update(Buffer.concat([
    Buffer.from("bluey-jobs-host-executable-identity-v1\0"),
    Buffer.from(limaPath), Buffer.from("\0"), Buffer.from(limaMetadata.dev),
    Buffer.from("\0"), Buffer.from(limaMetadata.ino),
  ])).digest("hex");
  assert.equal(digest, expected);
  const baseline = hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }));
  const helper = hostRecord(42, { executableIdentitySha256: digest });
  const before = hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }), helper);
  const after = hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }), helper);
  const bound = bindUsernetHelper({
    baseline, before, after, pid: 42, uid: 501, limaExecutableIdentitySha256: digest,
    minimumBirthMicros: "1000000000",
  });
  assert.equal(bound.pid, 42);
  assert.equal(bound.executableIdentitySha256, digest);
  assert.match(bound.birthIdentitySha256, /^[a-f0-9]{64}$/);
  for (const changed of [
    { minimumBirthMicros: "1000000124" },
    { minimumBirthMicros: "01000000000" },
    { baseline: hostSnapshot(hostRecord(10), helper) },
    { after: hostSnapshot(hostRecord(10), { ...helper, ppid: "2" }) },
    { after: hostSnapshot(hostRecord(10), { ...helper, birthUsec: "124" }) },
    { after: hostSnapshot(hostRecord(10), { ...helper, euid: "502" }) },
    { after: hostSnapshot(hostRecord(10), { ...helper, helperClass: "python-or-other" }) },
    { after: hostSnapshot(hostRecord(10), {
      ...helper, executableIdentitySha256: "b".repeat(64),
    }) },
  ]) {
    assert.throws(() => bindUsernetHelper({
      baseline, before, after, pid: 42, uid: 501, limaExecutableIdentitySha256: digest,
      minimumBirthMicros: "1000000000",
      ...changed,
    }));
  }
  for (const [path, metadata] of [
    ["relative", limaMetadata],
    [limaPath, { ...limaMetadata, symlink: true }],
    [limaPath, { ...limaMetadata, mode: 0o100775 }],
    [limaPath, { ...limaMetadata, dev: "01" }],
    [limaPath, { ...limaMetadata, ino: "18446744073709551616" }],
  ]) assert.throws(() => executableIdentitySha256(path, metadata));
});

test("host process parser rejects noncanonical numbers, records, keys, and termination", () => {
  const first = hostRecord(10, { helperClass: "python-or-other" });
  const second = hostRecord(42);
  const valid = `${canonicalJson({ records: [first, second], version: 1 })}\n`;
  assert.equal(parseHostProcessSnapshot(valid).records.length, 2);
  for (const invalid of [
    valid.slice(0, -1),
    `${valid}\n`,
    valid.replace('"pid":"10"', '"pid":10'),
    valid.replace('"pid":"10"', '"pid":"010"'),
    `${canonicalJson({ records: [second, first], version: 1 })}\n`,
    `${canonicalJson({ records: [first, first], version: 1 })}\n`,
    `${canonicalJson({ extra: false, records: [first], version: 1 })}\n`,
    `${canonicalJson({ records: [{ ...first, rawPath: "/private/owner" }], version: 1 })}\n`,
    `${JSON.stringify({ version: 1, records: [first] })}\n`,
  ]) assert.throws(() => parseHostProcessSnapshot(invalid));
});

test("usernet PID and exact private artifacts reject identity or permission drift", () => {
  assert.equal(parseUsernetPid(Buffer.from("42\n")), 42);
  for (const invalid of ["1\n", "01\n", "42", "2147483648\n", "42 \n"]) {
    assert.throws(() => parseUsernetPid(Buffer.from(invalid)));
  }
  const directory = {
    directory: true, file: false, socket: false, symlink: false, uid: 501,
    mode: 0o40755, nlink: 2, size: 0, dev: "1", ino: "100",
  };
  const pidFile = {
    directory: false, file: true, socket: false, symlink: false, uid: 501,
    mode: 0o100644, nlink: 1, size: 3, dev: "1", ino: "102", label: "pid",
  };
  const socket = (label, ino) => ({
    directory: false, file: false, socket: true, symlink: false, uid: 501,
    mode: 0o140755, nlink: 1, size: 0, dev: "1", ino, label,
  });
  const value = {
    directories: [
      { ...directory, label: "networks-directory" },
      { ...directory, ino: "101", label: "user-v2-directory" },
    ],
    pidFile,
    sockets: [socket("qemu", "103"), socket("fd", "104"), socket("ep", "105")],
  };
  const artifactDigest = assertUsernetArtifactIdentity(value, 501);
  assert.match(artifactDigest, /^[a-f0-9]{64}$/);
  assert.notEqual(artifactDigest, assertUsernetArtifactIdentity({
    ...value,
    pidFile: { ...pidFile, ino: "106" },
  }, 501));
  assert.notEqual(artifactDigest, assertUsernetArtifactIdentity({
    ...value,
    sockets: [{ ...value.sockets[0], ino: "106" }, ...value.sockets.slice(1)],
  }, 501));
  for (const changed of [
    { directories: [{ ...value.directories[0], symlink: true }, value.directories[1]] },
    { pidFile: { ...pidFile, nlink: 2 } },
    { pidFile: { ...pidFile, mode: 0o100664 } },
    { sockets: [{ ...value.sockets[0], socket: false }, ...value.sockets.slice(1)] },
    { sockets: [{ ...value.sockets[0], uid: 502 }, ...value.sockets.slice(1)] },
    { sockets: [{ ...value.sockets[0], ino: "104" }, ...value.sockets.slice(1)] },
    { sockets: [socket("wrong", "103"), ...value.sockets.slice(1)] },
  ]) assert.throws(() => assertUsernetArtifactIdentity({ ...value, ...changed }, 501));
});

test("lsof parser preserves every PID/name record including wildcard and bracketed IPv6", () => {
  const raw = [
    "p42",
    "f10",
    "n127.0.0.1:60022",
    "f11",
    "n[::1]:60022",
    "p43",
    "f12",
    "n*:60022",
    "f13",
    "n192.0.2.10:60022",
    "",
  ].join("\n");
  const parsed = parseListenerRecords(raw);
  assert(parsed.every((row) => /^[a-f0-9]{64}$/.test(row.endpointSha256)));
  assert.deepEqual(parsed.map(({ endpointSha256: _endpointSha256, ...row }) => row), [
    { pid: 42, fd: 10, address: "127.0.0.1", port: 60022, unbound: false, connected: false,
      peerAddress: null, peerPort: null },
    { pid: 42, fd: 11, address: "::1", port: 60022, unbound: false, connected: false,
      peerAddress: null, peerPort: null },
    { pid: 43, fd: 12, address: "*", port: 60022, unbound: false, connected: false,
      peerAddress: null, peerPort: null },
    { pid: 43, fd: 13, address: "192.0.2.10", port: 60022, unbound: false,
      connected: false, peerAddress: null, peerPort: null },
  ]);
  const connected = parseListenerRecords([
    "p44", "f14", "n127.0.0.1:5353->192.0.2.20:53",
    "p45", "f15", "n[::1]:5354->[2001:db8::20]:53", "",
  ].join("\n"));
  assert.deepEqual(connected.map(({ endpointSha256: _endpointSha256, ...row }) => row), [
    { pid: 44, fd: 14, address: "127.0.0.1", port: 5353, unbound: false, connected: true,
      peerAddress: "192.0.2.20", peerPort: 53 },
    { pid: 45, fd: 15, address: "::1", port: 5354, unbound: false, connected: true,
      peerAddress: "2001:db8::20", peerPort: 53 },
  ]);
  const unbound = parseListenerRecords("p46\nf16\nn*:*\n");
  assert.deepEqual(unbound.map(({ endpointSha256: _endpointSha256, ...row }) => row), [
    { pid: 46, fd: 16, address: "*", port: null, unbound: true, connected: false,
      peerAddress: null, peerPort: null },
  ]);
  assert.deepEqual(parseListenerRecords(""), []);
  assert.deepEqual(listenerPolicy([
    listener(42, 10, "127.0.0.1", 60022),
  ], 42, 60022), { literalNoHostNetworkClaimed: false, listenerCount: 1 });
  for (const entry of [
    listener(42, 10, "0.0.0.0", 60022),
    listener(42, 10, "::", 60022),
    listener(42, 10, "192.0.2.10", 60022),
    listener(42, 10, "127.0.0.1", 8080),
    listener(99, 10, "127.0.0.1", 60022),
    listener(42, 10, "127.0.0.1", 60022, true),
    listener(42, 10, "*", null, false, true),
  ]) assert.throws(() => listenerPolicy([entry], 42, 60022));
  assert.throws(() => listenerPolicy([], 42, 60022));
  assert.throws(() => listenerPolicy([
    listener(42, 10, "127.0.0.1", 60022),
    listener(42, 11, "127.0.0.1", 60022),
  ], 42, 60022));

  const ambientTcp = Array.from({ length: 129 }, (_, index) =>
    listener(1000 + index, index, "0.0.0.0", 8000 + index));
  const ambientUdp = Array.from({ length: 129 }, (_, index) =>
    listener(2000 + index, index, "*", 9000 + index));
  ambientUdp.splice(0, 12, ...Array.from({ length: 12 }, (_, index) =>
    listener(2000 + index, index, "*", null, false, true)));
  const managed = listener(42, 200, "127.0.0.1", 60022);
  const noUdpEvidence = listenerDeltaPolicy({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed],
    udpBefore: ambientUdp,
    udpAfter: ambientUdp,
  }, [42, 43], 60022, 42);
  assert.deepEqual({ ...noUdpEvidence, attributedClientDeltaSha256: "hash" }, {
    attributedClientCount: 0,
    attributedClientDeltaSha256: "hash",
    listenerCount: 1,
    literalCodeSigningClaimed: false,
    literalNoHostNetworkClaimed: false,
    literalOfficialDependencyPurposeClaimed: false,
    stableAmbientClientCount: 0,
    usernetClientCount: 0,
  });
  assert.match(noUdpEvidence.attributedClientDeltaSha256, /^[a-f0-9]{64}$/);
  const admittedUdp = [
    listener(42, 300, "127.0.0.1", 51000, true, false, "192.0.2.20", 53),
    listener(42, 301, "::1", 51001, true, false, "2001:db8::20", 443),
    listener(42, 302, "192.0.2.30", 51002, true, false, "::ffff:192.0.2.40", 123),
    listener(2000, 303, "192.0.2.50", 51003, true, false, "198.51.100.20", 443),
  ];
  const admitted = listenerDeltaPolicy({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed],
    udpBefore: ambientUdp,
    udpAfter: [...ambientUdp, ...admittedUdp],
  }, [42, 43], 60022, 42, udpAdmissionProof([stableAmbientRecord]));
  assert.equal(admitted.attributedClientCount, 4);
  assert.equal(admitted.usernetClientCount, 3);
  assert.equal(admitted.stableAmbientClientCount, 1);
  assert.equal(admitted.literalOfficialDependencyPurposeClaimed, false);
  assert.equal(admitted.literalNoHostNetworkClaimed, false);
  assert.equal(admitted.literalCodeSigningClaimed, false);
  assert.match(admitted.attributedClientDeltaSha256, /^[a-f0-9]{64}$/);
  const deniedUdp = [
    listener(42, 201, "127.0.0.1", 53, true),
    listener(42, 204, "*", null, false, true),
    listener(2000, 202, "127.0.0.1", 54),
    listener(9000, 203, "127.0.0.1", 55),
  ];
  const failureEvidence = listenerDeltaFailureEvidence({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed],
    udpBefore: ambientUdp,
    udpAfter: [...ambientUdp, ...deniedUdp],
  }, [42, 43], 42);
  assert.deepEqual({ ...failureEvidence.tcp, deltaSha256: "hash" }, {
    deltaCount: 1,
    vmOwnedCount: 1,
    baselinePidOnlyCount: 0,
    helperConnectedCount: 0,
    helperCount: 1,
    helperUnboundCount: 0,
    unknownPidCount: 0,
    deltaSha256: "hash",
  });
  assert.deepEqual({ ...failureEvidence.udp, deltaSha256: "hash" }, {
    deltaCount: 4,
    vmOwnedCount: 2,
    baselinePidOnlyCount: 1,
    helperConnectedCount: 1,
    helperCount: 2,
    helperUnboundCount: 1,
    unknownPidCount: 1,
    deltaSha256: "hash",
  });
  assert.match(failureEvidence.tcp.deltaSha256, /^[a-f0-9]{64}$/);
  assert.match(failureEvidence.udp.deltaSha256, /^[a-f0-9]{64}$/);
  const processEvidence = hostProcessEvidence({
    artifactIdentitySha256: "1".repeat(64),
    baselineSnapshotSha256: "2".repeat(64),
    birthIdentitySha256: "3".repeat(64),
    boundaryAfterSnapshotSha256: "4".repeat(64),
    boundaryBeforeSnapshotSha256: "5".repeat(64),
    executableIdentitySha256: "6".repeat(64),
    producerSha256: "3c53430e3e937a27b9697264b3b9e5f29ff76f395c7a52546c9e85035a88cf95",
  }, failureEvidence);
  assert.equal(processEvidence.helperTcpDeltaCount, 1);
  assert.equal(processEvidence.helperUdpDeltaCount, 2);
  assert.equal(processEvidence.helperUdpConnectedDeltaCount, 1);
  assert.equal(processEvidence.helperUdpUnboundDeltaCount, 1);
  const closedEvidence = JSON.stringify(failureEvidence);
  for (const raw of ["127.0.0.1", '"pid"', '"fd"', '"address"', '"port"',
    '"peerAddress"', '"peerPort"']) {
    assert.equal(closedEvidence.includes(raw), false);
  }
  assert.throws(() => listenerDeltaPolicy({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed],
    udpBefore: ambientUdp,
    udpAfter: [...ambientUdp, ...deniedUdp],
  }, [42, 43], 60022, 42), /udp_process_proof_shape/);
  assert.throws(() => listenerDeltaPolicy({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed],
    udpBefore: ambientUdp,
    udpAfter: [...ambientUdp, admittedUdp[0]],
  }, [42, 43], 60022, 42), /udp_process_proof_shape/);
  assert.throws(() => listenerDeltaFailureEvidence({
    tcpBefore: Array(1025).fill(listener(100, 1, "127.0.0.1", 1)),
    tcpAfter: [],
    udpBefore: [],
    udpAfter: [],
  }, [42, 43], 42), /listener_delta_shape/);
  for (const changed of [
    { tcpAfter: [...ambientTcp, listener(99, 200, "127.0.0.1", 60022)] },
    { tcpAfter: [...ambientTcp, listener(43, 200, "127.0.0.1", 60022)] },
    { tcpAfter: [...ambientTcp, listener(42, 200, "0.0.0.0", 60022)] },
    { udpAfter: [...ambientUdp, listener(42, 200, "127.0.0.1", 53)] },
    { udpAfter: [...ambientUdp, listener(42, 200, "*", null, false, true)] },
    { tcpBefore: [...ambientTcp, managed] },
  ]) {
    assert.throws(() => listenerDeltaPolicy({
      tcpBefore: ambientTcp,
      tcpAfter: [...ambientTcp, managed],
      udpBefore: ambientUdp,
      udpAfter: ambientUdp,
      ...changed,
    }, [42, 43], 60022, 42));
  }

  for (const denied of [
    listener(42, 400, "127.0.0.1", 51000),
    listener(42, 401, "*", null, false, true),
    listener(42, 402, "*", 51000, true),
    listener(42, 403, "0.0.0.0", 51000, true),
    listener(42, 404, "::", 51000, true),
    listener(42, 405, "127.0.0.1", 51000, true, false, "*", 53),
    listener(42, 406, "127.0.0.1", 51000, true, false, "0.0.0.0", 53),
    listener(42, 407, "127.0.0.1", 51000, true, false, "::", 53),
    listener(42, 408, "127.0.0.1", 51000, true, false, "::ffff:0.0.0.0", 53),
    listener(42, 409, "127.0.0.1", 51000, true, false, "0:0:0:0:0:ffff:0:0", 53),
    listener(42, 410, "127.0.0.1", 51000, true, false, "0:0:0:0:0:0:0:0", 53),
    listener(42, 413, "::%lo0", 51000, true, false, "2001:db8::20", 53),
    listener(42, 414, "::1", 51000, true, false, "::%lo0", 53),
    listener(42, 415, "::1%lo0%bad", 51000, true, false, "2001:db8::20", 53),
    listener(42, 416, "127.0.0.1%lo0", 51000, true, false, "192.0.2.20", 53),
    listener(43, 411, "127.0.0.1", 51000, true),
    listener(9000, 412, "127.0.0.1", 51000, true),
  ]) {
    assert.throws(() => listenerDeltaPolicy({
      tcpBefore: ambientTcp,
      tcpAfter: [...ambientTcp, managed],
      udpBefore: ambientUdp,
      udpAfter: [...ambientUdp, denied],
    }, [42, 43], 60022, 42, udpAdmissionProof([stableAmbientRecord])));
  }

  assert.throws(() => listenerDeltaPolicy({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed,
      listener(2000, 421, "127.0.0.1", 60022)],
    udpBefore: ambientUdp,
    udpAfter: ambientUdp,
  }, [42, 43], 60022, 42, udpAdmissionProof([stableAmbientRecord])));

  const proof = udpAdmissionProof();
  for (const changedProof of [
    { ...proof, baselineProcesses: hostSnapshot(
      hostRecord(10, { helperClass: "python-or-other" }), usernetRecord) },
    { ...proof, postProcessesAfter: hostSnapshot(
      hostRecord(10, { helperClass: "python-or-other" }),
      { ...usernetRecord, executableIdentitySha256: "e".repeat(64) }) },
    { ...proof, usernetBinding: { ...proof.usernetBinding,
      birthIdentitySha256: "f".repeat(64) } },
  ]) {
    assert.throws(() => listenerDeltaPolicy({
      tcpBefore: ambientTcp,
      tcpAfter: [...ambientTcp, managed],
      udpBefore: ambientUdp,
      udpAfter: [...ambientUdp, admittedUdp[0]],
    }, [42, 43], 60022, 42, changedProof));
  }

  const ambientClient = listener(2000, 420, "127.0.0.1", 51000, true);
  const identityChanges = [
    { baselineProcesses: hostSnapshot(hostRecord(10, { helperClass: "python-or-other" })) },
    { postProcessesBefore: hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }),
      usernetRecord, { ...stableAmbientRecord, ppid: "1998" }) },
    { postProcessesAfter: hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }),
      usernetRecord, { ...stableAmbientRecord, birthUsec: "124" }) },
    { postProcessesAfter: hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }),
      usernetRecord, { ...stableAmbientRecord, euid: "502" }) },
    { postProcessesAfter: hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }),
      usernetRecord, { ...stableAmbientRecord, executableIdentitySha256: "c".repeat(64) }) },
    { postProcessesAfter: hostSnapshot(hostRecord(10, { helperClass: "python-or-other" }),
      usernetRecord, { ...stableAmbientRecord, helperClass: "lima" }) },
  ];
  for (const changedProof of identityChanges) {
    assert.throws(() => listenerDeltaPolicy({
      tcpBefore: ambientTcp,
      tcpAfter: [...ambientTcp, managed],
      udpBefore: ambientUdp,
      udpAfter: [...ambientUdp, ambientClient],
    }, [42, 43], 60022, 42, { ...udpAdmissionProof([stableAmbientRecord]), ...changedProof }));
  }
  const knownHelperRecord = { ...stableAmbientRecord, helperClass: "docker" };
  assert.throws(() => listenerDeltaPolicy({
    tcpBefore: ambientTcp,
    tcpAfter: [...ambientTcp, managed],
    udpBefore: ambientUdp,
    udpAfter: [...ambientUdp, ambientClient],
  }, [42, 43], 60022, 42, udpAdmissionProof([knownHelperRecord])),
  /udp_process_identity_changed/);
  for (const changedUid of [{ euid: "0" }, { ruid: "0" }]) {
    assert.throws(() => listenerDeltaPolicy({
      tcpBefore: ambientTcp,
      tcpAfter: [...ambientTcp, managed],
      udpBefore: ambientUdp,
      udpAfter: [...ambientUdp, ambientClient],
    }, [42, 43], 60022, 42, udpAdmissionProof([{ ...stableAmbientRecord, ...changedUid }])),
    /udp_process_identity_changed/);
  }
  for (const pid of [4999, 5000]) {
    const baselineRow = listener(pid, 430, "127.0.0.1", 5353);
    const controllerRecord = hostRecord(pid, {
      executableIdentitySha256: "d".repeat(64), helperClass: "python-or-other",
    });
    assert.throws(() => listenerDeltaPolicy({
      tcpBefore: ambientTcp,
      tcpAfter: [...ambientTcp, managed],
      udpBefore: [...ambientUdp, baselineRow],
      udpAfter: [...ambientUdp, baselineRow,
        listener(pid, 431, "127.0.0.1", 51000, true)],
    }, [42, 43], 60022, 42, udpAdmissionProof([controllerRecord])),
    /unexpected_new_udp_owner/);
  }
});

test("lsof parser rejects malformed, truncated, duplicate, and partially skipped records", () => {
  for (const invalid of [
    "n127.0.0.1:60022\n",
    "p42\n",
    "p42\nf1\nn127.0.0.1:60022",
    "p42\nf1\n",
    "p42\nf1\nf2\nn127.0.0.1:60022\n",
    "p42\nf1\nxTCP\nn127.0.0.1:60022\n",
    "p42\nf1\nn::1:60022\n",
    "p42\nf1\nn127.0.0.1:0\n",
    "p42\nf1\nn127.0.0.1:70000\n",
    "p2147483648\nf1\nn127.0.0.1:60022\n",
    "p42\nf1\nn127.0.0.1:60022\nf1\nn127.0.0.1:60023\n",
    "p42\nf1\nn127.0.0.1:60022\np42\nf2\nn127.0.0.1:60023\n",
    "p42\nf1\nn127.0.0.1:1->192.0.2.1:2->192.0.2.2:3\n",
    "p42\nf1\nn127.0.0.1:1->not-an-endpoint\n",
    "p42\nf1\nn127.0.0.1:1->not-a-host:53\n",
    "p42\nf1\nn*:*->192.0.2.1:53\n",
    "p42\nf1\nn127.0.0.1:1->*:*\n",
  ]) assert.throws(() => parseListenerRecords(invalid));

  const highBaseline = `${Array.from({ length: 129 }, (_, index) =>
    `p${1000 + index}\nf${index}\nn*:1`).join("\n")}\n`;
  assert.equal(parseListenerRecords(highBaseline).length, 129);

  const actualShape = "p42\nf1\nn127.0.0.1:1\n";
  assert.equal(assertListenerCapture({ code: 0, signal: null, stdout: actualShape,
    stderr: "" }), actualShape);
  assert.equal(assertListenerCapture({ code: 1, signal: null, stdout: "", stderr: "" }), "");
  for (const invalid of [
    { code: 0, signal: null, stdout: "", stderr: "warning\n" },
    { code: 1, signal: null, stdout: "partial\n", stderr: "" },
    { code: 2, signal: null, stdout: "", stderr: "" },
    { code: null, signal: "SIGTERM", stdout: "", stderr: "" },
  ]) assert.throws(() => assertListenerCapture(invalid));
});

test("inventory parser accepts bounded array or JSON-lines only", () => {
  assert.deepEqual(parseInventory(""), []);
  assert.deepEqual(parseInventory('{"name":"one"}'), [{ name: "one" }]);
  assert.deepEqual(parseInventory('{"name":"one"}\n{"name":"two"}\n'),
    [{ name: "one" }, { name: "two" }]);
  for (const invalid of ["not json", "null", "[1]", "{}\ntrailing"]) {
    assert.throws(() => parseInventory(invalid));
  }
});

test("recorded PID absence fails closed on permissions, invalid identities, and live processes", () => {
  assertRecordedPidsAbsent([42, 43], () => false);
  for (const exists of [
    () => true,
    () => undefined,
    () => { throw Object.assign(new Error("denied"), { code: "EPERM" }); },
  ]) assert.throws(() => assertRecordedPidsAbsent([42], exists));
  for (const pids of [[0], [1], ["42"], [2_147_483_648], Array(9).fill(42)]) {
    assert.throws(() => assertRecordedPidsAbsent(pids, () => false));
  }
});

test("diagnostics stay stream-separated, bounded, and metadata-only in failure reports", () => {
  let tail = boundedDiagnosticTail(Buffer.from("first"), Buffer.from(" second"));
  assert.equal(tail.toString(), "first second");
  tail = boundedDiagnosticTail(tail, Buffer.alloc(70 * 1024, 65));
  assert.equal(tail.length, 64 * 1024);
  tail = boundedDiagnosticTail(tail, Buffer.from("closed tail"));
  assert.equal(tail.subarray(-11).toString(), "closed tail");
  assert.throws(() => boundedDiagnosticTail("text", Buffer.alloc(0)));

  const names = [".phase639-owner.json", "colima", "l", "cache", "docker", "tmp",
    "command-failure-0001.stdout", "command-failure-0001.stderr"];
  assert.deepEqual(diagnosticRetentionPlan(names), ["colima", "cache", "docker", "tmp", "l"]);
  assert.throws(() => diagnosticRetentionPlan([...names, "unknown"]));
  assert.throws(() => diagnosticRetentionPlan(names.filter((name) => name !== ".phase639-owner.json")));

  const report = failureReport({
    stage: "vm_start",
    primaryFailure: "command_colima_failed",
    cleanupFailure: "cleanup_unproven_state_retained",
    interrupted: false,
    retained: true,
    stateRoot: state,
    profile,
    cleanupProven: false,
    retainedDiagnosticsOnly: false,
    hostProcessEvidence: { ...successHostEvidence, udpDeltaCount: 1 },
    listenerFailureEvidence: listenerDeltaFailureEvidence({
      tcpBefore: [],
      tcpAfter: [listener(42, 10, "127.0.0.1", 60022)],
      udpBefore: [listener(100, 11, "127.0.0.1", 5353)],
      udpAfter: [listener(100, 11, "127.0.0.1", 5353),
        listener(101, 12, "127.0.0.1", 5354)],
    }, [42, 43], 42),
    diagnostics: [
      { file: "command-failure-0001.stdout", tool: "colima", stage: "vm_start", bytes: 10,
        raw: "must not escape" },
    ].map(({ raw: _raw, ...entry }) => entry),
  });
  assert.equal(report.primaryFailure, "command_colima_failed");
  assert.equal(report.cleanupFailure, "cleanup_unproven_state_retained");
  assert.equal(report.code, "command_colima_failed");
  assert.equal(JSON.stringify(report).includes("must not escape"), false);
  assert.equal(report.listenerFailureEvidence.tcp.vmOwnedCount, 1);
  assert.equal(report.listenerFailureEvidence.udp.unknownPidCount, 1);
  assert.equal(report.hostProcessEvidence.producerSha256,
    "3c53430e3e937a27b9697264b3b9e5f29ff76f395c7a52546c9e85035a88cf95");
  assert.equal(JSON.stringify(report.listenerFailureEvidence).includes("127.0.0.1"), false);
  assert.equal(report.retainedState, state);
  const interrupted = failureReport({ stage: "cleanup", interrupted: true, retained: false,
    cleanupProven: true, retainedDiagnosticsOnly: false, diagnostics: [] });
  assert.equal(interrupted.code, "interrupted");
  assert.equal(interrupted.listenerFailureEvidence, null);
  for (const listenerFailureEvidence of [
    { ...report.listenerFailureEvidence, rawPath: "/private/owner" },
    { ...report.listenerFailureEvidence, udp: {
      ...report.listenerFailureEvidence.udp, unknownPidCount: 2,
    } },
    { ...report.listenerFailureEvidence, tcp: {
      ...report.listenerFailureEvidence.tcp, deltaSha256: "not-a-hash",
    } },
  ]) {
    assert.throws(() => failureReport({ stage: "listener_observation",
      primaryFailure: "unexpected_new_udp_listener", interrupted: false, retained: false,
      cleanupProven: true, retainedDiagnosticsOnly: false, diagnostics: [],
      listenerFailureEvidence }));
  }
  for (const processEvidence of [
    { ...successHostEvidence, rawPath: "/private/owner" },
    { ...successHostEvidence, producerSha256: "a".repeat(64) },
    { ...successHostEvidence, helperUdpConnectedDeltaCount: 2 },
  ]) {
    assert.throws(() => failureReport({ stage: "listener_observation",
      primaryFailure: "unexpected_new_udp_listener", interrupted: false, retained: false,
      cleanupProven: true, retainedDiagnosticsOnly: false, diagnostics: [],
      hostProcessEvidence: processEvidence }));
  }
});

test("closed success report validates observed evidence and denies broader claims", () => {
  const report = reportMode({
    baseCommit: "2b85339a403cb5272a56f1b2f0bf8065ca3b19ed",
    buildx: { status: "available", version: "v0.30.1" },
    claims: { api: false, image: false, productionAuthority: false, runnerStartup: false },
    cleanup: {
      ownedVmPidsAbsent: true,
      privateInventoriesEmpty: true,
      socketAbsent: true,
      stateAbsent: true,
      usernetSocketsAbsent: true,
    },
    emulated: true,
    harnessSha256: "c".repeat(64),
    hostProcessEvidence: successHostEvidence,
    identities: {
      engineArchitecture: "x86_64",
      engineOs: "linux",
      guestArchitecture: "x86_64",
      limaVersion: "2.2.1",
      vmType: "qemu",
    },
    listenerEvidence: {
      attributedClientCount: 0,
      attributedClientDeltaSha256: createHash("sha256").update("udp-client\n\n").digest("hex"),
      listenerCount: 1,
      literalCodeSigningClaimed: false,
      literalNoHostNetworkClaimed: false,
      literalOfficialDependencyPurposeClaimed: false,
      stableAmbientClientCount: 0,
      usernetClientCount: 0,
    },
    observations: {
      dataDiskBytes: 15 * GIB,
      engineCpuCount: 4,
      engineMemoryBytes: 5 * GIB,
      guestMemoryBytes: 5 * GIB,
      rootDiskBytes: 7 * GIB,
    },
    qualification: "isolated_linux_amd64_vm_compatibility_only",
    portForwardPolicySha256: createHash("sha256").update(limaPortForwardOverrideBytes())
      .digest("hex"),
    resources: {
      cpus: 4,
      memoryGiB: 6,
      rootDiskGiB: 8,
      dataDiskGiB: 16,
      freeBytes: 32 * GIB,
    },
    testSha256: "d".repeat(64),
    toolVersions: { colima: "0.10.3", lima: "2.2.1", qemu: "11.1.2" },
    version: 1,
  });
  assert.equal(report.claims.runnerStartup, false);
  assert.equal(report.claims.productionAuthority, false);
  assert(reportMode({ ...report, buildx: { status: "unsupported_known" } }));
  const outboundReport = reportMode({
    ...report,
    hostProcessEvidence: {
      ...report.hostProcessEvidence,
      helperUdpConnectedDeltaCount: 3,
      helperUdpDeltaCount: 3,
      udpDeltaCount: 4,
    },
    listenerEvidence: {
      ...report.listenerEvidence,
      attributedClientCount: 4,
      attributedClientDeltaSha256: "a".repeat(64),
      stableAmbientClientCount: 1,
      usernetClientCount: 3,
    },
  });
  assert.equal(outboundReport.listenerEvidence.literalOfficialDependencyPurposeClaimed, false);
  for (const raw of ["192.0.2.20", '"peerAddress"', '"peerPort"', '"pid"', '"fd"']) {
    assert.equal(JSON.stringify(outboundReport).includes(raw), false);
  }
  for (const changed of [
    { emulated: false },
    { baseCommit: "2b85339affffffffffffffffffffffffffffffff" },
    { claims: { ...report.claims, image: true } },
    { cleanup: { ...report.cleanup, socketAbsent: false } },
    { cleanup: { ...report.cleanup, usernetSocketsAbsent: false } },
    { hostProcessEvidence: { ...report.hostProcessEvidence, helperTcpDeltaCount: 2 } },
    { hostProcessEvidence: { ...report.hostProcessEvidence, helperUdpDeltaCount: 1 } },
    { listenerEvidence: { ...report.listenerEvidence,
      literalOfficialDependencyPurposeClaimed: true } },
    { listenerEvidence: { ...report.listenerEvidence, attributedClientCount: 1 } },
    { listenerEvidence: { ...report.listenerEvidence, rawPeer: "192.0.2.20" } },
    { identities: { ...report.identities, guestArchitecture: "aarch64" } },
    { listenerEvidence: { ...report.listenerEvidence, listenerCount: 0 } },
    { observations: { ...report.observations, engineCpuCount: 8 } },
    { buildx: { status: "unknown" } },
    { buildx: { status: "unsupported_known", detail: "unbounded" } },
  ]) assert.throws(() => reportMode({ ...report, ...changed }));
});

test("listener observation retries only complete snapshot-churn attempts", async () => {
  let attempts = 0;
  const observed = await retryListenerObservation(async (index) => {
    attempts += 1;
    if (index < 3) throw new Error("host_listener_snapshot_changed");
    return { stable: true };
  });
  assert.deepEqual(observed, { stable: true });
  assert.equal(attempts, 3);

  attempts = 0;
  await assert.rejects(() => retryListenerObservation(async () => {
    attempts += 1;
    throw new Error("host_listener_snapshot_changed");
  }), /host_listener_snapshot_changed/);
  assert.equal(attempts, 3);

  for (const code of ["host_process_snapshot_json", "usernet_artifacts_changed",
    "usernet_helper_identity_changed", "command_timed_out", "listener_record_malformed"]) {
    attempts = 0;
    await assert.rejects(() => retryListenerObservation(async () => {
      attempts += 1;
      throw new Error(code);
    }), new RegExp(code));
    assert.equal(attempts, 1);
  }
  await assert.rejects(() => retryListenerObservation(null),
    /listener_observation_attempt_shape/);
});

test("source contains bounded interruption, cleanup, and delete ordering contracts", async () => {
  const source = await readFile(new URL("./run-nonprod-managed-runner-vm-tests.mjs", import.meta.url),
    "utf8");
  for (const required of [
    "check(!interrupted || cleaning",
    'child.kill("SIGTERM")',
    'child.kill("SIGKILL")',
    "45 * 60_000",
    "const cleanup = singleFlight",
    "buildx_failure_ambiguous",
    "cleanup_unproven_state_retained",
    "prestop_inventory_ambiguous",
    'ssh.sock.1234567890123456',
    '"-u", String(uid)',
    '"-iUDP"',
    'stage: "private_state_setup"',
    '"ha.stderr.log"',
    '"usernet_user-v2.pid"',
    '"user-v2_qemu.sock"',
    '"user-v2_fd.sock"',
    '"user-v2_ep.sock"',
    '"-I", "-S", "-B", HOST_PROCESS_PRODUCER,',
    '"--snapshot-pids", JSON.stringify(pids)',
    "retainFailureDiagnostics: false",
    "KNOWN_HOST_PROCESS_PRODUCER_SHA256",
  ]) assert(source.includes(required), required);
  const initialUsernetProof = source.indexOf("const initialUsernetBefore");
  const initialUsernetRestored = source.indexOf(
    "usernetIdentityAmbiguous = priorUsernetAmbiguity", initialUsernetProof,
  );
  const listenerRetry = source.indexOf(
    "retryListenerObservation(async", initialUsernetRestored,
  );
  const subsequentSticky = source.indexOf(
    "usernetIdentityAmbiguous = true", listenerRetry,
  );
  const subsequentRestored = source.indexOf(
    "usernetIdentityAmbiguous = priorListenerUsernetAmbiguity", subsequentSticky,
  );
  const thirdListenerEquality = source.indexOf(
    "assertListenerSnapshotsEqual(listenerAfter, await listenerSnapshot())", subsequentRestored,
  );
  assert(initialUsernetProof > 0 && initialUsernetRestored > initialUsernetProof
    && listenerRetry > initialUsernetRestored && subsequentSticky > listenerRetry
    && subsequentRestored > subsequentSticky && thirdListenerEquality > subsequentRestored);
  const signalRegistration = source.indexOf("process.on(signal, stop)");
  const stateCreation = source.indexOf("stateRoot = await mkdtemp");
  const setupTry = source.lastIndexOf("try {", stateCreation);
  const setupFinally = source.indexOf("} finally {", stateCreation);
  const setupReport = source.indexOf('stage: "private_state_setup"', setupFinally);
  const policyWrite = source.indexOf("await writeFile(limaPolicyFile, LIMA_PORT_FORWARD_OVERRIDE");
  const policyEstablished = source.indexOf("limaPolicyEstablished = true", policyWrite);
  const cliPreflight = source.indexOf("await assertCliPreflight()", policyEstablished);
  const vmStart = source.indexOf('await run("colima", startArguments(profile)', cliPreflight);
  const helperCapture = source.indexOf("const usernetArtifacts = await captureUsernetArtifacts()",
    vmStart);
  const initialProcessBefore = source.indexOf(
    "const initialUsernetBefore = await hostProcessSnapshot(", helperCapture,
  );
  const initialProcessAfter = source.indexOf(
    "const initialUsernetAfter = await hostProcessSnapshot(", initialProcessBefore,
  );
  const initialHelperBinding = source.indexOf(
    "const usernetHelper = bindUsernetHelper", initialProcessAfter,
  );
  const proofListenerRetry = source.indexOf(
    "retryListenerObservation(async", initialHelperBinding,
  );
  const processBefore = source.indexOf(
    "const processBeforeListeners = await hostProcessSnapshot(", proofListenerRetry,
  );
  const listenerAfter = source.indexOf("const listenerAfter = await listenerSnapshot()",
    processBefore);
  const processAfter = source.indexOf(
    "const processAfterListeners = await hostProcessSnapshot(", listenerAfter,
  );
  const observedHelperBinding = source.indexOf(
    "const observedUsernetHelper = bindUsernetHelper", processAfter,
  );
  const stop = source.indexOf('stopped = await run("colima", cleanupArguments(profile, "stop")');
  const forceStop = source.indexOf('cleanupArguments(profile, "force-stop")', stop);
  const stoppedProof = source.indexOf("cleanupInventoryProof(postStop.colima");
  const pidProof = source.indexOf("assertRecordedPidsAbsent([...vmPids]", stoppedProof);
  const deletion = source.indexOf('await run("colima", cleanupArguments(profile, "delete")');
  assert(signalRegistration >= 0 && signalRegistration < stateCreation);
  assert(setupTry >= 0 && setupTry < stateCreation && stateCreation < setupFinally
    && setupFinally < setupReport);
  assert(policyWrite >= 0 && policyWrite < policyEstablished && policyEstablished < cliPreflight
    && cliPreflight < vmStart);
  assert(vmStart < helperCapture && helperCapture < initialProcessBefore
    && initialProcessBefore < initialProcessAfter && initialProcessAfter < initialHelperBinding
    && initialHelperBinding < proofListenerRetry && proofListenerRetry < processBefore
    && processBefore < listenerAfter && listenerAfter < processAfter
    && processAfter < observedHelperBinding);
  assert(stop >= 0 && stop < forceStop && forceStop < stoppedProof
    && stoppedProof < pidProof && pidProof < deletion);
});

test("native PID selection is minimal and socket snapshots bracket exact records", () => {
  const tcp = listener(42, 1, "127.0.0.1", 60022);
  const udp = listener(7, 2, "*", 5353);
  const before = { tcp: [tcp], udp: [udp, { ...udp, fd: 3 }] };
  assert.deepEqual(listenerSnapshotPids(before, [44, 42]), [7, 42, 44]);
  assertListenerSnapshotsEqual(before, { tcp: [tcp], udp: [{ ...udp, fd: 3 }, udp] });
  for (const changed of [
    { ...before, tcp: [] },
    { ...before, tcp: [{ ...tcp, address: "0.0.0.0" }] },
    { ...before, udp: [udp] },
    { ...before, udp: [{ ...udp, fd: 9 }, { ...udp, fd: 3 }] },
    { ...before, tcp: [{ ...tcp, connected: true }] },
  ]) assert.throws(() => assertListenerSnapshotsEqual(before, changed),
    /host_listener_snapshot_changed/);
  assert.throws(() => listenerSnapshotPids({ tcp: [], udp: [] }));
  assert.throws(() => listenerSnapshotPids(before, [1]));
  assert.throws(() => listenerSnapshotPids({ tcp: [{ pid: "42" }], udp: [] }));
  assert.throws(() => listenerSnapshotPids({ tcp: Array(1025).fill(tcp), udp: [] }));
});

test("native record bound admits 4096 selected PIDs plus the producer and no more", () => {
  const records = Array.from({ length: 4097 }, (_, index) => hostRecord(index + 2));
  assert.equal(hostSnapshot(...records).records.length, 4097);
  assert.throws(() => hostSnapshot(...records, hostRecord(4099)));
  assert.throws(() => hostSnapshot(hostRecord(1)));
});

test("usernet identity drift stays ambiguous even after the original artifacts return", () => {
  const original = { pid: 42, artifactIdentitySha256: "a".repeat(64),
    identity: { socket: { dev: "1", ino: "2" } } };
  assert.equal(validateUsernetContinuity(original, { ...original }, false), false);
  assert.equal(validateUsernetContinuity(original, { ...original }, true), true);
  for (const replacement of [
    { ...original, pid: 43 },
    { ...original, artifactIdentitySha256: "b".repeat(64) },
    { ...original, identity: { socket: { dev: "1", ino: "3" } } },
  ]) {
    let ambiguous = true;
    assert.throws(() => {
      ambiguous = validateUsernetContinuity(original, replacement, false);
    }, /usernet_artifacts_changed/);
    assert.equal(ambiguous, true);
    assert.equal(validateUsernetContinuity(original, original, ambiguous), true);
  }
});
