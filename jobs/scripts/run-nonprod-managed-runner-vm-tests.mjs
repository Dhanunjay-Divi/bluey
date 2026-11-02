#!/usr/bin/env node
// Disposable QEMU qualification. The image-smoke mode additionally exercises
// an actual isolated image; neither mode starts the normal Jobs runner/API.
import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  statfs,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { isIP } from "node:net";
import { fileURLToPath } from "node:url";
import { TextDecoder } from "node:util";
import { ARTIFACT_CAPS, canonicalArtifactJson, copyArtifactStream, guestReaderArguments,
  guestReaderDiscoveryArguments, validateReaderApproval, validateReaderObservation }
  from "./managed-runner-artifact-stream.mjs";

const GIB = 1024 ** 3;
const SELF = fileURLToPath(import.meta.url);
const TEST = fileURLToPath(new URL("./nonprod-managed-runner-vm.test.mjs", import.meta.url));
const HOST_PROCESS_PRODUCER = fileURLToPath(
  new URL("./managed-runner-host-processes.py", import.meta.url),
);
const ARTIFACT_HELPER = fileURLToPath(new URL("./managed-runner-artifacts.py", import.meta.url));
const IMAGE_TOOL_FILES = Object.freeze([
  "managed-cloud-release-gate.mjs", "managed-cloud-release-gate.test.mjs",
  "managed-runner-image-workload.mjs", "managed-runner-image-workload.test.mjs",
  "nonprod-managed-runner-image-inspect.mjs", "nonprod-managed-runner-image-inspect.test.mjs",
  "managed-runner-artifacts.py", "managed-runner-artifacts.test.py",
  "managed-runner-artifact-stream.mjs", "managed-runner-artifact-stream.test.mjs",
]);
const IMAGE_PROGRESS_PHASES = Object.freeze([
  "vm_starting", "vm_isolation_checked", "inputs_prepared", "builder_started",
  "image_build_started", "image_build_finished", "image_inspection_started",
  "image_inspection_finished",
]);
const REPO = resolve(dirname(SELF), "../..");
const SOURCE_COMMIT = /^(?!0{40}$)[a-f0-9]{40}$/;
const CLI = Object.freeze({
  colima: "/opt/homebrew/bin/colima",
  lima: "/opt/homebrew/bin/limactl",
  qemu: "/opt/homebrew/bin/qemu-system-x86_64",
  git: "/usr/bin/git",
  lsof: "/usr/sbin/lsof",
  python: "/usr/bin/python3",
  tar: "/usr/bin/tar",
});
const OWNER_MARKER = ".phase639-owner.json";
const LAUNCHER_MARKER = ".bluey-test-run-v2";
const MAX_DIAGNOSTIC = 64 * 1024;
const MAX_COMMAND_OUTPUT = 2 * 1024 * 1024;
const MAX_FAILURE_COMMANDS = 8;
const MAX_HOST_AGENT_LOG = 256 * 1024;
const MAX_HOST_PROCESS_OUTPUT = 1024 * 1024;
// Mandatory outer tail only. Workload cleanup is reserved separately until done;
// post-export observations share a deadline rather than summing all retry caps.
export const ARTIFACT_REMAINING_BUDGET_MS = Object.freeze({
  vmCleanup: 3 * 2 * 30_000 + 90_000 + 60_000 + 120_000, // three inventories, stop/force/delete
  cleanupGrace: 9 * 5_000,
  sourceAfter: 35_000,
  qualification: 35_000,
  localIoMargin: 120_000,
  activeTeardown: 5_000,
});
export const ARTIFACT_CLEANUP_RESERVE_MS = Object.values(ARTIFACT_REMAINING_BUDGET_MS)
  .reduce((total, value) => total + value, 0);
export const ARTIFACT_WORKLOAD_CLEANUP_MS = 60_000 + 120_000 + 2 * 30_000 + 4 * 5_000;
export const ARTIFACT_WORKLOAD_PENDING_MS = ARTIFACT_WORKLOAD_CLEANUP_MS + 35_000;
export const ARTIFACT_POST_OBSERVATION_MS = 120_000; // includes current-child termination grace
export const ARTIFACT_SOURCE_FINAL_MS = 35_000;
const POST_EXPORT_RESERVE_MS = ARTIFACT_POST_OBSERVATION_MS + ARTIFACT_SOURCE_FINAL_MS;
const KNOWN_VERSIONS = Object.freeze({ colima: "0.10.3", lima: "2.2.1", qemu: "11.1.2" });
const KNOWN_LIMA_BINARY = "/opt/homebrew/Cellar/lima/2.2.1/bin/limactl";
const KNOWN_HOST_PROCESS_PRODUCER_SHA256 =
  "bac1d707b8d077bde9ed6dd787d801550b198fefc908c82e25edb4cae3f6481a";
const LIMA_PORT_FORWARD_OVERRIDE = Buffer.from([
  "portForwards:",
  "- guestIP: \"0.0.0.0\"",
  "  guestIPMustBeZero: false",
  "  guestPortRange: [1, 65535]",
  "  proto: any",
  "  ignore: true",
  "",
].join("\n"));
const check = (value, code) => { if (!value) throw new Error(code); };
const own = (value, key) => Object.hasOwn(value, key);
const inside = (parent, child) => child.startsWith(`${parent}/`);
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const identity = (metadata) => `${metadata.dev}:${metadata.ino}`;
const canonicalDecimal = (value) => value === "0" || /^[1-9][0-9]{0,14}$/.test(value);
const canonicalDecimalGtOne = (value) => canonicalDecimal(value) && Number(value) > 1;
const canonicalUint64 = (value) => typeof value === "string"
  && /^(?:0|[1-9][0-9]{0,19})$/.test(value) && BigInt(value) <= 18_446_744_073_709_551_615n;
const canonicalPositiveUint64 = (value) => canonicalUint64(value) && BigInt(value) > 0n;
const codeOf = (error, fallback = "closed_preflight_failure") =>
  /^[a-zA-Z0-9_]{1,80}$/.test(error?.message) ? error.message : fallback;

function exactKeys(value, keys, code) {
  check(value && typeof value === "object" && !Array.isArray(value), code);
  check(JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...keys].sort()), code);
  return value;
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

async function imageToolsetSha256() {
  const entries = [];
  for (const file of IMAGE_TOOL_FILES) {
    const bytes = await readFile(join(dirname(SELF), file));
    check(bytes.length > 0 && bytes.length <= 2 * 1024 * 1024,
      "image_tool_source_bound");
    entries.push({ path: file, sha256: sha256(bytes) });
  }
  return sha256(Buffer.from(canonicalJson(entries)));
}

export function parseHostProcessSnapshot(raw) {
  check(typeof raw === "string" && Buffer.byteLength(raw) <= MAX_HOST_PROCESS_OUTPUT
    && raw.endsWith("\n") && !raw.slice(0, -1).includes("\n")
    && /^[\x20-\x7e]*\n$/.test(raw), "host_process_snapshot_encoding");
  let value;
  try { value = JSON.parse(raw); } catch { throw new Error("host_process_snapshot_json"); }
  exactKeys(value, ["records", "version"], "host_process_snapshot_shape");
  check(value.version === 1 && Array.isArray(value.records)
    && value.records.length >= 1 && value.records.length <= 4097,
  "host_process_snapshot_shape");
  let previous = 0;
  for (const record of value.records) {
    exactKeys(record, ["birthSec", "birthUsec", "euid", "executableIdentitySha256",
      "helperClass", "pid", "ppid", "ruid"], "host_process_record_shape");
    for (const key of ["pid", "ppid", "euid", "ruid", "birthSec", "birthUsec"]) {
      check(typeof record[key] === "string" && canonicalUint64(record[key]),
        "host_process_record_decimal");
    }
    const pid = Number(record.pid);
    check(pid > 1 && pid > previous && pid <= 2_147_483_647
      && Number(record.ppid) <= 2_147_483_647 && record.ppid !== record.pid
      && Number(record.euid) <= 4_294_967_295 && Number(record.ruid) <= 4_294_967_295
      && Number(record.birthSec) > 0 && Number(record.birthUsec) < 1_000_000
      && /^[a-f0-9]{64}$/.test(record.executableIdentitySha256)
      && ["colima", "lima", "qemu", "ssh", "docker", "python-or-other"]
        .includes(record.helperClass), "host_process_record_shape");
    previous = pid;
  }
  check(raw === `${canonicalJson(value)}\n`, "host_process_snapshot_not_canonical");
  return Object.freeze({
    rawSha256: sha256(Buffer.from(raw, "ascii")),
    records: Object.freeze(value.records.map((record) => Object.freeze({ ...record }))),
  });
}

export function executableIdentitySha256(canonicalPath, metadata) {
  check(typeof canonicalPath === "string" && canonicalPath.startsWith("/")
    && !canonicalPath.includes("\0") && Buffer.byteLength(canonicalPath) < 4096
    && metadata?.file && !metadata.symlink && typeof metadata.dev === "string"
    && typeof metadata.ino === "string" && canonicalUint64(metadata.dev)
    && canonicalPositiveUint64(metadata.ino) && Number.isInteger(metadata.uid)
    && metadata.uid >= 0 && (metadata.mode & 0o022) === 0
    && (metadata.mode & 0o111) !== 0, "host_executable_identity_shape");
  return sha256(Buffer.concat([
    Buffer.from("bluey-jobs-host-executable-identity-v1\0"),
    Buffer.from(canonicalPath, "utf8"), Buffer.from("\0"),
    Buffer.from(metadata.dev, "ascii"), Buffer.from("\0"),
    Buffer.from(metadata.ino, "ascii"),
  ]));
}

export function parseUsernetPid(bytes) {
  check(Buffer.isBuffer(bytes) && bytes.length >= 2 && bytes.length <= 12,
    "usernet_pid_bytes");
  const value = bytes.toString("ascii");
  check(/^[1-9][0-9]{0,9}\n$/.test(value), "usernet_pid_bytes");
  const pid = Number(value.slice(0, -1));
  check(pid > 1 && pid <= 2_147_483_647, "usernet_pid_invalid");
  return pid;
}

export function assertUsernetArtifactIdentity(value, uid) {
  exactKeys(value, ["directories", "pidFile", "sockets"], "usernet_artifact_shape");
  check(Array.isArray(value.directories) && value.directories.length === 2
    && Array.isArray(value.sockets) && value.sockets.length === 3,
  "usernet_artifact_shape");
  check(JSON.stringify(value.directories.map((directory) => directory.label))
    === JSON.stringify(["networks-directory", "user-v2-directory"]),
  "usernet_artifact_labels");
  for (const directory of value.directories) {
    check(directory?.directory && !directory.symlink && directory.uid === uid
      && canonicalUint64(directory.dev) && canonicalPositiveUint64(directory.ino)
      && (directory.mode & 0o022) === 0, "usernet_directory_identity");
  }
  check(value.pidFile?.file && !value.pidFile.symlink && value.pidFile.uid === uid
    && value.pidFile.nlink === 1 && canonicalUint64(value.pidFile.dev)
    && canonicalPositiveUint64(value.pidFile.ino) && (value.pidFile.mode & 0o022) === 0,
  "usernet_pid_file_identity");
  check(value.pidFile.label === "pid"
    && JSON.stringify(value.sockets.map((socket) => socket.label))
      === JSON.stringify(["qemu", "fd", "ep"]), "usernet_artifact_labels");
  for (const socket of value.sockets) {
    check(socket?.socket && !socket.symlink && socket.uid === uid && socket.nlink === 1
      && canonicalUint64(socket.dev) && canonicalPositiveUint64(socket.ino)
      && (socket.mode & 0o022) === 0, "usernet_socket_identity");
  }
  const identities = [...value.directories, value.pidFile, ...value.sockets]
    .map((entry) => `${entry.label}:${entry.dev}:${entry.ino}`);
  const inodeIdentities = [...value.directories, value.pidFile, ...value.sockets]
    .map((entry) => `${entry.dev}:${entry.ino}`);
  check(new Set(inodeIdentities).size === identities.length, "usernet_artifact_identity_reused");
  return sha256(Buffer.from(`bluey-jobs-usernet-artifacts-v1\0${identities.join("\0")}`));
}

export function validateUsernetContinuity(expected, current, priorAmbiguity) {
  check(typeof priorAmbiguity === "boolean" && expected && current
    && current.pid === expected.pid
    && current.artifactIdentitySha256 === expected.artifactIdentitySha256
    && canonicalJson(current.identity) === canonicalJson(expected.identity),
  "usernet_artifacts_changed");
  return priorAmbiguity;
}

export function bindUsernetHelper({ baseline, before, after, pid, uid,
  limaExecutableIdentitySha256, minimumBirthMicros }) {
  check(baseline && before && after && Number.isInteger(pid) && pid > 1
    && Number.isInteger(uid) && uid >= 0 && uid <= 4_294_967_295
    && /^[a-f0-9]{64}$/.test(limaExecutableIdentitySha256)
    && canonicalPositiveUint64(minimumBirthMicros), "usernet_helper_binding_shape");
  const pidText = String(pid);
  check(!baseline.records.some((record) => record.pid === pidText),
    "usernet_helper_pid_preexisting");
  const first = before.records.filter((record) => record.pid === pidText);
  const second = after.records.filter((record) => record.pid === pidText);
  check(first.length === 1 && second.length === 1
    && JSON.stringify(first[0]) === JSON.stringify(second[0]),
  "usernet_helper_identity_changed");
  const record = first[0];
  check(record.euid === String(uid) && record.ruid === String(uid)
    && record.helperClass === "lima"
    && record.executableIdentitySha256 === limaExecutableIdentitySha256,
  "usernet_helper_executable_identity");
  check(BigInt(record.birthSec) * 1_000_000n + BigInt(record.birthUsec)
    >= BigInt(minimumBirthMicros), "usernet_helper_birth_preexisting");
  return Object.freeze({
    pid,
    birthIdentitySha256: processBirthIdentitySha256(record),
    executableIdentitySha256: record.executableIdentitySha256,
  });
}

export function listenerSnapshotPids(snapshot, knownPids = []) {
  check(snapshot && Array.isArray(snapshot.tcp) && Array.isArray(snapshot.udp)
    && snapshot.tcp.length <= 1024 && snapshot.udp.length <= 1024
    && Array.isArray(knownPids) && knownPids.length <= 8, "host_process_selection_shape");
  const pids = [...snapshot.tcp, ...snapshot.udp].map((row) => row?.pid).concat(knownPids);
  check(pids.every((pid) => Number.isInteger(pid) && pid > 1 && pid <= 2_147_483_647),
    "host_process_selection_pid");
  const selected = [...new Set(pids)].sort((left, right) => left - right);
  check(selected.length >= 1 && selected.length <= 4096, "host_process_selection_bound");
  return selected;
}

export function assertListenerSnapshotsEqual(before, after) {
  listenerSnapshotPids(before);
  listenerSnapshotPids(after);
  const canonical = (snapshot) => canonicalJson({
    tcp: [...snapshot.tcp].sort((left, right) => listenerKey(left).localeCompare(listenerKey(right))),
    udp: [...snapshot.udp].sort((left, right) => listenerKey(left).localeCompare(listenerKey(right))),
  });
  check(canonical(before) === canonical(after), "host_listener_snapshot_changed");
}

export async function retryListenerObservation(attempt) {
  check(typeof attempt === "function", "listener_observation_attempt_shape");
  for (let index = 1; index <= 3; index += 1) {
    try {
      return await attempt(index);
    } catch (error) {
      if (error?.message !== "host_listener_snapshot_changed" || index === 3) throw error;
    }
  }
  throw new Error("host_listener_snapshot_changed");
}

export function boundedDiagnosticTail(previous, chunk) {
  check(Buffer.isBuffer(previous) && Buffer.isBuffer(chunk), "diagnostic_bytes_required");
  if (chunk.length >= MAX_DIAGNOSTIC) return Buffer.from(chunk.subarray(-MAX_DIAGNOSTIC));
  const retained = Math.min(previous.length, MAX_DIAGNOSTIC - chunk.length);
  return Buffer.concat([previous.subarray(previous.length - retained), chunk]);
}

export function limaPortForwardOverrideBytes() {
  return Buffer.from(LIMA_PORT_FORWARD_OVERRIDE);
}

export function assertLimaPortForwardOverride(bytes) {
  check(Buffer.isBuffer(bytes) && bytes.equals(LIMA_PORT_FORWARD_OVERRIDE),
    "lima_port_forward_override_bytes");
}

export function assertLimaOverrideIdentity(directory, file, uid) {
  check(directory?.directory && !directory.symlink && directory.uid === uid
    && (directory.mode & 0o7777) === 0o700, "lima_override_directory_identity");
  check(file?.file && !file.symlink && file.uid === uid && file.nlink === 1
    && (file.mode & 0o7777) === 0o600 && file.size === LIMA_PORT_FORWARD_OVERRIDE.length,
  "lima_override_file_identity");
}

export function assertNoAutomaticForwardAttempts(bytes) {
  check(Buffer.isBuffer(bytes) && bytes.length <= MAX_HOST_AGENT_LOG,
    "host_agent_log_bound");
  let text;
  try { text = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
  catch { throw new Error("host_agent_log_encoding"); }
  check(!/[^\x09\x0a\x0d\x20-\x7e]/.test(text), "host_agent_log_characters");
  check(!/(?:failed to set up forwarding|\bForwarding (?:TCP|UDP) from\b)/i.test(text),
    "automatic_port_forward_attempted");
}

export function assertResources({ cpus, memoryGiB, rootDiskGiB, dataDiskGiB, freeBytes }) {
  check(cpus === 4 && memoryGiB === 6 && rootDiskGiB === 8 && dataDiskGiB === 16,
    "resource_bounds");
  check(Number.isSafeInteger(freeBytes) && freeBytes >= 32 * GIB, "host_capacity");
}

export function assertLauncherRoot(path, canonical, metadata, uid) {
  const match = /^\/(?:private\/)?tmp\/bluey-test-runs-[^/]+\/(run\.([0-9]+)\.([0-9]+)\.([A-Za-z0-9]+))$/
    .exec(path);
  check(path === canonical && path === resolve(path) && match
    && canonicalDecimal(match[2]) && canonicalDecimalGtOne(match[3])
    && match[4].length <= 64, "launcher_root_scope");
  check(metadata?.directory && !metadata.symlink && metadata.uid === uid
    && (metadata.mode & 0o7777) === 0o700, "launcher_root_identity");
}

export function assertPrivateRoot(path, canonical, metadata, uid) {
  check(path === canonical && path === resolve(path)
    && /^\/(?:private\/)?tmp\/bluey-jobs-linux-p639-[A-Za-z0-9]{6}$/.test(path),
  "private_state_root_scope");
  check(metadata?.directory && !metadata.symlink && metadata.uid === uid
    && (metadata.mode & 0o7777) === 0o700, "private_state_root_identity");
}

export function assertPrivateHome(path, canonical, metadata, uid, root) {
  check(path === canonical && inside(root, path), "private_home_scope");
  check(metadata?.directory && !metadata.symlink && metadata.uid === uid
    && (metadata.mode & 0o7777) === 0o700, "private_home_identity");
}

export function assertLauncherMarker(bytes, root, uid) {
  check(Buffer.isBuffer(bytes), "launcher_marker_bytes");
  const lines = bytes.toString("utf8").trimEnd().split("\n");
  const run = /^run\.([0-9]+)\.([0-9]+)\.([A-Za-z0-9]+)$/.exec(basename(root));
  check(lines.length === 9 && lines[0] === "magic=bluey-test-run-v2"
    && run && canonicalDecimal(run[1]) && canonicalDecimalGtOne(run[2]) && run[3].length <= 64
    && lines[1] === `run_root=${root}`
    && lines[2] === `run_name=${basename(root)}`
    && lines[3] === `owner_uid=${uid}`
    && lines[4] === `owner_pid=${run[2]}`
    && lines[5].startsWith("process_group_id=")
    && canonicalDecimalGtOne(lines[5].slice("process_group_id=".length))
    && lines[6].startsWith("launcher_group_id=")
    && canonicalDecimalGtOne(lines[6].slice("launcher_group_id=".length))
    && lines[7] === `created_epoch=${run[1]}`
    && lines[8] === "state=running", "launcher_marker");
}

export function assertUnixSocketBudget(paths) {
  for (const path of paths) {
    check(typeof path === "string" && path.startsWith("/") && !path.includes("\0")
      && Buffer.byteLength(path, "utf8") <= 103, "unix_socket_path_budget");
  }
}

export function isolatedEnvironment(stateRoot, canonicalRoot, home) {
  check(!inside(canonicalRoot, stateRoot) && stateRoot !== canonicalRoot && inside(canonicalRoot, home),
    "private_environment_roots");
  return Object.freeze({
    PATH: "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
    HOME: home,
    LANG: "C",
    LC_ALL: "C",
    TZ: "UTC",
    TMPDIR: join(stateRoot, "tmp"),
    TMP: join(stateRoot, "tmp"),
    TEMP: join(stateRoot, "tmp"),
    COLIMA_HOME: join(stateRoot, "colima"),
    COLIMA_CACHE_HOME: join(stateRoot, "cache"),
    LIMA_HOME: join(stateRoot, "l"),
    DOCKER_CONFIG: join(stateRoot, "docker"),
    DOCKER_CONFIG_FILE: join(stateRoot, "docker", "config.json"),
    DOCKER_BUILDKIT: "0",
  });
}

function assertProfile(profile) {
  check(/^p639-[a-f0-9]{20}$/.test(profile), "owned_profile_required");
}

export function startArguments(profile) {
  assertProfile(profile);
  return [
    "start", "--profile", profile, "--runtime", "docker", "--arch", "x86_64",
    "--vm-type", "qemu", "--cpus", "4", "--memory", "6", "--root-disk", "8",
    "--disk", "16", "--mount", "none", "--ssh-agent=false", "--ssh-config=false",
    "--activate=false", "--template=false", "--binfmt=false", "--port-forwarder", "none",
  ];
}

export function cleanupArguments(profile, action) {
  assertProfile(profile);
  if (action === "stop") return ["stop", "--profile", profile];
  if (action === "force-stop") return ["stop", "--profile", profile, "--force"];
  check(action === "delete", "cleanup_action");
  return ["delete", "--profile", profile, "--force", "--data"];
}

export function parseInventory(raw) {
  check(typeof raw === "string" && Buffer.byteLength(raw) <= 256 * 1024,
    "inventory_output_bound");
  const text = raw.trim();
  if (!text) return [];
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    try { parsed = text.split("\n").map((line) => JSON.parse(line)); }
    catch { throw new Error("inventory_json_required"); }
  }
  const rows = Array.isArray(parsed) ? parsed : [parsed];
  check(rows.length <= 8 && rows.every((row) => row && typeof row === "object"
    && !Array.isArray(row)), "inventory_shape");
  return rows;
}

function aliasedField(row, lower, upper) {
  const present = [lower, upper].filter((key) => own(row, key));
  check(present.length <= 1, "qemu_duplicate_alias");
  return present.length === 1 ? { present: true, value: row[present[0]] } : { present: false };
}

// Missing PID fields are interpreted only for the exact reviewed stopped
// Lima 2.2.1 QEMU/x86_64 protocol. Running or unknown records stay closed.
export function qemuInventoryPids(row, profile, { allowStopped = false } = {}) {
  assertProfile(profile);
  check((row?.name ?? row?.Name) === `colima-${profile}`, "foreign_lima_instance");
  check((row.limaVersion ?? row.LimaVersion) === KNOWN_VERSIONS.lima
    && (row.vmType ?? row.VMType) === "qemu"
    && (row.arch ?? row.Arch) === "x86_64", "qemu_protocol_identity");
  const status = row.status ?? row.Status;
  check(status === "Running" || status === "Stopped", "qemu_status_unknown");
  const errors = row.errors ?? row.Errors;
  check(errors === undefined || (Array.isArray(errors) && errors.length === 0),
    "qemu_errors_present");
  const fields = [
    aliasedField(row, "hostAgentPID", "HostAgentPID"),
    aliasedField(row, "driverPID", "DriverPID"),
  ];
  if (status === "Running") check(fields.every((field) => field.present), "qemu_pid_fields_missing");
  const pids = fields.map((field) => field.present ? field.value : 0);
  check(pids.every((pid) => Number.isInteger(pid) && pid >= 0 && pid <= 2_147_483_647),
    "qemu_pid_invalid");
  if (status === "Running") check(pids.every((pid) => pid > 1), "qemu_live_pids_required");
  if (status === "Stopped") {
    check(allowStopped && pids.every((pid) => pid === 0), "qemu_stop_not_proven");
  }
  return [...new Set(pids.filter((pid) => pid > 1))];
}

export function cleanupInventoryProof(colima, lima, profile, {
  requireStopped = false,
  allowStopped = false,
} = {}) {
  assertProfile(profile);
  check(Array.isArray(colima) && colima.length === 1
    && Array.isArray(lima) && lima.length === 1, "complete_owned_vm_inventory_required");
  check((colima[0].name ?? colima[0].Name) === profile, "foreign_colima_profile");
  const colimaStatus = colima[0].status ?? colima[0].Status;
  check(colimaStatus === "Running" || colimaStatus === "Stopped", "colima_status_unknown");
  const stoppedAllowed = requireStopped || allowStopped;
  const pids = qemuInventoryPids(lima[0], profile, { allowStopped: stoppedAllowed });
  const limaStatus = lima[0].status ?? lima[0].Status;
  check(colimaStatus === limaStatus, "vm_inventory_status_mismatch");
  if (requireStopped) {
    check(colimaStatus === "Stopped" && pids.length === 0, "owned_vm_not_stopped");
  }
  return pids;
}

// This admits only the exact private target for a bounded stop attempt. It is
// deliberately weaker than cleanupInventoryProof and never authorizes delete.
export function assertCleanupTargetInventory(colima, lima, profile) {
  assertProfile(profile);
  check(Array.isArray(colima) && colima.length <= 1
    && Array.isArray(lima) && lima.length <= 1, "cleanup_target_inventory_shape");
  if (colima.length === 1) {
    check((colima[0]?.name ?? colima[0]?.Name) === profile, "foreign_colima_profile");
  }
  if (lima.length === 1) {
    check((lima[0]?.name ?? lima[0]?.Name) === `colima-${profile}`, "foreign_lima_instance");
  }
  return { colimaPresent: colima.length === 1, limaPresent: lima.length === 1 };
}

export function preStopEvidence(colima, lima, profile) {
  assertCleanupTargetInventory(colima, lima, profile);
  const statuses = [...colima, ...lima].map((row) => row.status ?? row.Status);
  let deletionBlocked = statuses.some((status) =>
    !["Running", "Stopped", "Starting", "Installing"].includes(status))
    || statuses.includes("Running");
  let recordedPids = [];
  if (colima.length === 1 && lima.length === 1) {
    try {
      recordedPids = cleanupInventoryProof(colima, lima, profile, { allowStopped: true });
      deletionBlocked = false;
    } catch {
      // Starting/Installing can still be stopped and freshly proven stopped.
      // A live PID ambiguity must survive that transition and block deletion.
      deletionBlocked ||= statuses.includes("Running");
    }
  }
  return Object.freeze({ deletionBlocked, recordedPids: Object.freeze(recordedPids) });
}

export function assertEmptyInventories(colima, lima) {
  check(Array.isArray(colima) && Array.isArray(lima)
    && colima.length === 0 && lima.length === 0, "private_vm_inventory_not_empty");
}

export function assertRecordedPidsAbsent(pids, exists) {
  check(Array.isArray(pids) && pids.length <= 8, "recorded_pid_bound");
  for (const pid of pids) {
    check(Number.isInteger(pid) && pid > 1 && pid <= 2_147_483_647, "recorded_pid_invalid");
    let absent = false;
    try { absent = exists(pid) === false; } catch { /* ambiguity retains state */ }
    check(absent, "owned_vm_pid_still_present_or_ambiguous");
  }
}

export function parseListenerRecords(raw) {
  check(typeof raw === "string" && Buffer.byteLength(raw) <= MAX_DIAGNOSTIC,
    "listener_output_bound");
  if (raw === "") return [];
  check(raw.endsWith("\n"), "listener_output_termination");
  const records = [];
  const seenPids = new Set();
  const seenDescriptors = new Set();
  let currentPid;
  let currentDescriptor;
  let filesForPid = 0;
  const parseEndpoint = (value, { allowUnbound = false } = {}) => {
    if (value === "*:*") {
      check(allowUnbound, "listener_unbound_endpoint_unknown");
      return { address: "*", port: null, unbound: true };
    }
    const bracketed = /^\[([0-9A-Fa-f:.]+(?:%[A-Za-z0-9._-]+)?)\]:([0-9]{1,5})$/
      .exec(value);
    const ordinary = /^([^:\[\]]+):([0-9]{1,5})$/.exec(value);
    check(bracketed || ordinary, "listener_name_malformed");
    const address = (bracketed?.[1] ?? ordinary?.[1]).toLowerCase();
    const port = Number(bracketed?.[2] ?? ordinary?.[2]);
    check(address === "*" || (bracketed ? isIP(address) === 6 : isIP(address) === 4),
      "listener_address_unknown");
    check(address.length >= 1 && address.length <= 128
      && port > 0 && port <= 65535, "listener_name_malformed");
    return { address, port, unbound: false };
  };
  for (const line of raw.slice(0, -1).split("\n")) {
    if (/^p[1-9][0-9]{0,9}$/.test(line)) {
      check(currentDescriptor === undefined
        && (currentPid === undefined || filesForPid > 0), "listener_unpaired_field");
      currentPid = Number(line.slice(1));
      check(currentPid <= 2_147_483_647, "listener_pid_invalid");
      check(!seenPids.has(currentPid), "listener_duplicate_pid_record");
      seenPids.add(currentPid);
      filesForPid = 0;
      continue;
    }
    if (/^f[0-9]{1,7}$/.test(line)) {
      check(currentPid !== undefined && currentDescriptor === undefined,
        "listener_unpaired_field");
      const descriptor = Number(line.slice(1));
      check(descriptor <= 1_048_575, "listener_descriptor_invalid");
      const key = `${currentPid}\0${descriptor}`;
      check(!seenDescriptors.has(key), "listener_duplicate_descriptor");
      seenDescriptors.add(key);
      currentDescriptor = descriptor;
      continue;
    }
    check(line.startsWith("n") && currentPid !== undefined
      && currentDescriptor !== undefined, "listener_record_malformed");
    const value = line.slice(1);
    const endpoints = value.split("->");
    check(endpoints.length >= 1 && endpoints.length <= 2
      && endpoints.every((endpoint) => endpoint.length > 0), "listener_name_malformed");
    const local = parseEndpoint(endpoints[0], { allowUnbound: endpoints.length === 1 });
    const peer = endpoints.length === 2 ? parseEndpoint(endpoints[1]) : null;
    records.push({
      pid: currentPid,
      fd: currentDescriptor,
      address: local.address,
      port: local.port,
      unbound: local.unbound,
      connected: endpoints.length === 2,
      peerAddress: peer?.address ?? null,
      peerPort: peer?.port ?? null,
      endpointSha256: sha256(Buffer.from(value)),
    });
    currentDescriptor = undefined;
    filesForPid += 1;
    check(records.length <= 1024, "listener_record_bound");
  }
  check(currentPid !== undefined && filesForPid > 0
    && currentDescriptor === undefined, "listener_unpaired_field");
  return records;
}

export function assertListenerCapture(result) {
  check(result && typeof result === "object" && typeof result.stdout === "string"
    && typeof result.stderr === "string", "listener_capture_shape");
  const observed = result.code === 0 && result.signal === null && result.stderr === "";
  const observedEmpty = result.code === 1 && result.signal === null
    && result.stdout === "" && result.stderr === "";
  check(observed || observedEmpty, "listener_capture_failed");
  return result.stdout;
}

export function listenerPolicy(listeners, usernetPid, managementPort) {
  check(Array.isArray(listeners) && listeners.length === 1
    && Number.isInteger(usernetPid) && usernetPid > 1
    && Number.isInteger(managementPort) && managementPort > 0 && managementPort <= 65535,
  "listener_inventory_shape");
  for (const row of listeners) {
    check(row && typeof row === "object" && typeof row.address === "string"
      && Number.isInteger(row.port) && Number.isInteger(row.pid)
      && Number.isInteger(row.fd) && row.fd >= 0 && row.fd <= 1_048_575
      && row.connected === false && row.unbound === false
      && row.peerAddress === null && row.peerPort === null
      && /^[a-f0-9]{64}$/.test(row.endpointSha256), "listener_record_shape");
    check(row.address === "127.0.0.1" && row.port === managementPort
      && row.pid === usernetPid, "unexpected_host_listener");
  }
  return { literalNoHostNetworkClaimed: false, listenerCount: listeners.length };
}

function listenerKey(row) {
  return `${row.pid}\0${row.fd}\0${row.endpointSha256}`;
}

function listenerDeltaAnalysis({ tcpBefore, tcpAfter, udpBefore, udpAfter }, ownedPids) {
  check(Array.isArray(ownedPids) && ownedPids.length >= 1, "listener_delta_owned_pids");
  const owned = new Set(ownedPids);
  for (const rows of [tcpBefore, tcpAfter, udpBefore, udpAfter]) {
    check(Array.isArray(rows) && rows.length <= 1024, "listener_delta_shape");
    for (const row of rows) {
      check(row && typeof row === "object" && Number.isInteger(row.pid) && row.pid > 1
        && row.pid <= 2_147_483_647
        && Number.isInteger(row.fd) && row.fd >= 0 && row.fd <= 1_048_575
        && typeof row.address === "string" && typeof row.unbound === "boolean"
        && typeof row.connected === "boolean"
        && (row.peerAddress === null || typeof row.peerAddress === "string")
        && (row.peerPort === null || Number.isInteger(row.peerPort))
        && /^[a-f0-9]{64}$/.test(row.endpointSha256), "listener_delta_record");
      if (row.unbound) {
        check(row.connected === false && row.address === "*" && row.port === null,
          "listener_unbound_record");
      } else {
        check(Number.isInteger(row.port) && row.port > 0 && row.port <= 65535,
          "listener_delta_record");
      }
      check(row.connected
        ? row.peerAddress !== null && Number.isInteger(row.peerPort)
          && row.peerPort > 0 && row.peerPort <= 65535
        : row.peerAddress === null && row.peerPort === null,
      "listener_peer_record");
    }
  }
  check([...tcpBefore, ...udpBefore].every((row) => !owned.has(row.pid)),
    "listener_pid_reuse_ambiguous");
  const delta = (before, after) => {
    const baseline = new Set(before.map(listenerKey));
    return after.filter((row) => !baseline.has(listenerKey(row)));
  };
  const tcpDelta = delta(tcpBefore, tcpAfter);
  const udpDelta = delta(udpBefore, udpAfter);
  return { baselinePids: new Set([...tcpBefore, ...udpBefore].map((row) => row.pid)), owned,
    tcpDelta, udpDelta };
}

function listenerDeltaSummary(protocol, rows, baselinePids, owned, helperPid) {
  const vmOwnedCount = rows.filter((row) => owned.has(row.pid)).length;
  const baselinePidOnlyCount = rows.filter((row) => !owned.has(row.pid)
    && baselinePids.has(row.pid)).length;
  const unknownPidCount = rows.length - vmOwnedCount - baselinePidOnlyCount;
  return Object.freeze({
    deltaCount: rows.length,
    vmOwnedCount,
    baselinePidOnlyCount,
    helperConnectedCount: rows.filter((row) => row.pid === helperPid && row.connected).length,
    helperCount: rows.filter((row) => row.pid === helperPid).length,
    helperUnboundCount: rows.filter((row) => row.pid === helperPid && row.unbound).length,
    unknownPidCount,
    deltaSha256: sha256(Buffer.from(`${protocol}\n${rows.map(listenerKey).sort().join("\n")}\n`)),
  });
}

export function listenerDeltaFailureEvidence(input, ownedPids, helperPid = null, proof = null) {
  const { baselinePids, owned, tcpDelta, udpDelta } = listenerDeltaAnalysis(input, ownedPids);
  check(helperPid === null || (Number.isInteger(helperPid) && helperPid > 1
    && owned.has(helperPid)), "listener_helper_pid_shape");
  return Object.freeze({
    version: proof === null ? 1 : 2,
    literalCodeSigningClaimed: false,
    literalNoHostNetworkClaimed: false,
    literalOfficialDependencyPurposeClaimed: false,
    tcp: listenerDeltaSummary("tcp", tcpDelta, baselinePids, owned, helperPid),
    udp: listenerDeltaSummary("udp", udpDelta, baselinePids, owned, helperPid),
    ...(proof === null ? {} : { udpRefusals: udpRefusalCounts(udpDelta, baselinePids, owned, helperPid, proof) }),
  });
}

function udpRefusalCounts(rows, baselinePids, owned, helperPid, proof) {
  const counts = { endpointShapeRefused: 0, ownerRefused: 0, nativeBindingRefused: 0,
    nativeBindingNotEvaluated: 0, predicatePassed: 0 };
  for (const row of rows) {
    // Diagnostic order only. Admission still executes the unchanged udpClientEvidence.
    let endpoint = false;
    try { endpoint = row.connected && !row.unbound && concreteNumericEndpoint(row.address, row.port)
      && concreteNumericEndpoint(row.peerAddress, row.peerPort); } catch {}
    if (!endpoint) { counts.endpointShapeRefused += 1; continue; }
    if (row.pid !== helperPid && (owned.has(row.pid) || row.pid === proof.controllerPid
      || row.pid === proof.parentPid || !baselinePids.has(row.pid))) {
      counts.ownerRefused += 1;
      continue;
    }
    try {
      udpClientEvidence([row], baselinePids, owned, helperPid, proof);
      counts.predicatePassed += 1;
    } catch (error) {
      if (["udp_process_identity_changed", "udp_usernet_identity_changed",
        "udp_usernet_identity_preexisting"].includes(error.message)) counts.nativeBindingRefused += 1;
      else counts.nativeBindingNotEvaluated += 1;
    }
  }
  return Object.freeze(counts);
}

function ipv6Words(address) {
  let value = address.toLowerCase();
  const lastColon = value.lastIndexOf(":");
  const tail = value.slice(lastColon + 1);
  if (tail.includes(".")) {
    const octets = tail.split(".").map(Number);
    value = `${value.slice(0, lastColon + 1)}${((octets[0] << 8) | octets[1]).toString(16)}`
      + `:${((octets[2] << 8) | octets[3]).toString(16)}`;
  }
  const halves = value.split("::");
  check(halves.length <= 2, "listener_address_unknown");
  const left = halves[0] ? halves[0].split(":") : [];
  const right = halves.length === 2 && halves[1] ? halves[1].split(":") : [];
  const omitted = 8 - left.length - right.length;
  check((halves.length === 2 && omitted >= 1) || (halves.length === 1 && omitted === 0),
    "listener_address_unknown");
  return [...left, ...Array(omitted).fill("0"), ...right].map((word) => Number.parseInt(word, 16));
}

function concreteNumericEndpoint(address, port) {
  if (typeof address !== "string") return false;
  const scopeIndex = address.indexOf("%");
  if (scopeIndex !== -1 && (scopeIndex === 0 || address.indexOf("%", scopeIndex + 1) !== -1
    || !/^[A-Za-z0-9._-]+$/.test(address.slice(scopeIndex + 1)))) return false;
  const numericAddress = scopeIndex === -1 ? address : address.slice(0, scopeIndex);
  const family = isIP(numericAddress);
  if (!Number.isInteger(port) || port <= 0 || port > 65535 || family === 0
    || (scopeIndex !== -1 && family !== 6)) return false;
  if (family === 4) return numericAddress !== "0.0.0.0";
  const words = ipv6Words(numericAddress);
  const allZero = words.every((word) => word === 0);
  const mappedZero = words.slice(0, 5).every((word) => word === 0)
    && words[5] === 0xffff && words[6] === 0 && words[7] === 0;
  return !allZero && !mappedZero;
}

function processRecord(snapshot, pid) {
  check(snapshot && Array.isArray(snapshot.records), "udp_process_proof_shape");
  const records = snapshot.records.filter((record) => record.pid === String(pid));
  check(records.length === 1, "udp_process_identity_missing");
  return records[0];
}

function processBirthIdentitySha256(record) {
  return sha256(Buffer.from(
    `bluey-jobs-usernet-birth-v1\0${record.pid}\0${record.ppid}\0${record.birthSec}\0${record.birthUsec}`,
  ));
}

function udpClientEvidence(rows, baselinePids, owned, usernetPid, proof) {
  if (rows.length === 0) {
    return Object.freeze({
      attributedClientCount: 0,
      attributedClientDeltaSha256: sha256(Buffer.from("udp-client\n\n")),
      stableAmbientClientCount: 0,
      usernetClientCount: 0,
    });
  }
  exactKeys(proof, ["baselineProcesses", "controllerPid", "parentPid", "postProcessesAfter",
    "postProcessesBefore", "usernetBinding"], "udp_process_proof_shape");
  check(Number.isInteger(proof.controllerPid) && proof.controllerPid > 1
    && Number.isInteger(proof.parentPid) && proof.parentPid > 1,
  "udp_process_proof_shape");
  exactKeys(proof.usernetBinding, ["artifactIdentitySha256", "birthIdentitySha256",
    "executableIdentitySha256", "pid"], "udp_usernet_binding_shape");
  check(proof.usernetBinding.pid === usernetPid
    && [proof.usernetBinding.artifactIdentitySha256, proof.usernetBinding.birthIdentitySha256,
      proof.usernetBinding.executableIdentitySha256]
      .every((value) => /^[a-f0-9]{64}$/.test(value)), "udp_usernet_binding_shape");
  check(!proof.baselineProcesses.records.some((record) => record.pid === String(usernetPid)),
    "udp_usernet_identity_preexisting");
  const usernetBefore = processRecord(proof.postProcessesBefore, usernetPid);
  const usernetAfter = processRecord(proof.postProcessesAfter, usernetPid);
  check(JSON.stringify(usernetBefore) === JSON.stringify(usernetAfter)
    && usernetBefore.helperClass === "lima"
    && usernetBefore.executableIdentitySha256
      === proof.usernetBinding.executableIdentitySha256
    && processBirthIdentitySha256(usernetBefore) === proof.usernetBinding.birthIdentitySha256,
  "udp_usernet_identity_changed");
  let usernetClientCount = 0;
  let stableAmbientClientCount = 0;
  for (const row of rows) {
    check(row.connected && !row.unbound
      && concreteNumericEndpoint(row.address, row.port)
      && concreteNumericEndpoint(row.peerAddress, row.peerPort),
    "unexpected_new_udp_listener");
    if (row.pid === usernetPid) {
      usernetClientCount += 1;
      continue;
    }
    check(!owned.has(row.pid) && row.pid !== proof.controllerPid && row.pid !== proof.parentPid
      && baselinePids.has(row.pid), "unexpected_new_udp_owner");
    const baseline = processRecord(proof.baselineProcesses, row.pid);
    const before = processRecord(proof.postProcessesBefore, row.pid);
    const after = processRecord(proof.postProcessesAfter, row.pid);
    check(JSON.stringify(baseline) === JSON.stringify(before)
      && JSON.stringify(baseline) === JSON.stringify(after)
      && baseline.euid === usernetBefore.euid && baseline.ruid === usernetBefore.ruid
      && baseline.helperClass === "python-or-other", "udp_process_identity_changed");
    stableAmbientClientCount += 1;
  }
  return Object.freeze({
    attributedClientCount: rows.length,
    attributedClientDeltaSha256: sha256(Buffer.from(
      `udp-client\n${rows.map(listenerKey).sort().join("\n")}\n`,
    )),
    stableAmbientClientCount,
    usernetClientCount,
  });
}

export function listenerDeltaPolicy(input, ownedPids, managementPort, usernetPid,
  udpProof = null) {
  const { baselinePids, owned, tcpDelta, udpDelta } = listenerDeltaAnalysis(input, ownedPids);
  check(tcpDelta.length <= 64, "owned_listener_delta_bound");
  const tcpEvidence = listenerPolicy(tcpDelta, usernetPid, managementPort);
  const udpEvidence = udpClientEvidence(udpDelta, baselinePids, owned, usernetPid, udpProof);
  return Object.freeze({
    ...tcpEvidence,
    ...udpEvidence,
    literalCodeSigningClaimed: false,
    literalOfficialDependencyPurposeClaimed: false,
  });
}

export function qemuManagementPort(row) {
  const port = row?.sshLocalPort ?? row?.SSHLocalPort;
  const address = row?.sshAddress ?? row?.SSHAddress;
  check(Number.isInteger(port) && port > 0 && port <= 65535
    && (address === "127.0.0.1" || address === "::1"), "qemu_management_port_unknown");
  return port;
}

function yamlValue(raw, key) {
  const lines = raw.split("\n");
  const matches = lines.flatMap((line, index) => line.startsWith(`${key}:`) ? [index] : []);
  check(matches.length <= 1, "vm_config_duplicate");
  if (matches.length === 0) return undefined;
  const index = matches[0];
  let value = lines[index].slice(key.length + 1).split("#")[0].trim();
  for (let next = index + 1; next < lines.length && !/^[A-Za-z]/.test(lines[next]); next += 1) {
    const extra = lines[next].split("#")[0].trim();
    if (extra) value += ` ${extra}`;
  }
  return value.trim().replace(/^"([^"\n]*)"$/, "$1");
}

export function assertVmConfiguration(raw) {
  check(typeof raw === "string" && Buffer.byteLength(raw) <= 128 * 1024, "vm_config_size");
  const expected = {
    cpu: "4", memory: "6", rootDisk: "8", disk: "16", arch: "x86_64", vmType: "qemu",
    runtime: "docker", forwardAgent: "false", sshConfig: "false", autoActivate: "false",
    rosetta: "false", binfmt: "false", portForwarder: "none",
  };
  for (const [key, value] of Object.entries(expected)) {
    check(yamlValue(raw, key) === value, `vm_config_${key}`);
  }
  check([undefined, "", "null", "~"].includes(yamlValue(raw, "mounts")), "vm_host_mounts");
}

export function assertApprovedCli(path, canonical, metadata, uid) {
  check((typeof path === "string" && path.startsWith("/opt/homebrew/bin/"))
    || path === "/usr/bin/git" || path === "/usr/sbin/lsof" || path === "/usr/bin/python3"
    || path === "/usr/bin/tar",
  "approved_cli_path");
  check(typeof canonical === "string" && canonical.startsWith("/"), "approved_cli_realpath");
  check(path !== "/usr/bin/tar"
    || (canonical === "/usr/bin/bsdtar" && metadata?.uid === 0),
  "approved_system_tar_identity");
  check(metadata?.file && !metadata.symlink && (metadata.uid === 0 || metadata.uid === uid)
    && (metadata.mode & 0o022) === 0 && (metadata.mode & 0o111) !== 0, "approved_cli_identity");
}

export function assertVersionOutput(name, text) {
  check(typeof text === "string" && Buffer.byteLength(text) <= MAX_DIAGNOSTIC,
    `version_${name}_unsupported`);
  const patterns = {
    colima: /(?:^|\n)colima version 0\.10\.3(?:\n|$)/,
    lima: /(?:^|\n)limactl version 2\.2\.1(?:\n|$)/,
    qemu: /(?:^|\n)QEMU emulator version 11\.1\.2(?:\s|\n|$)/,
  };
  check(own(patterns, name) && patterns[name].test(text), `version_${name}_unsupported`);
}

export function diagnosticRetentionPlan(names) {
  check(Array.isArray(names) && names.includes(OWNER_MARKER)
    && names.length <= 1 + 5 + MAX_FAILURE_COMMANDS * 2
    && new Set(names).size === names.length, "diagnostic_retention_shape");
  const removable = ["colima", "cache", "docker", "tmp", "l"];
  const diagnostic = /^command-failure-000[1-8]\.(?:stdout|stderr)$/;
  check(names.every((name) => name === OWNER_MARKER || removable.includes(name)
    || diagnostic.test(name)), "diagnostic_retention_unknown_entry");
  return removable.filter((name) => names.includes(name));
}

function closedListenerFailureEvidence(value) {
  if (value === null || value === undefined) return null;
  exactKeys(value, ["literalCodeSigningClaimed", "literalNoHostNetworkClaimed",
    "literalOfficialDependencyPurposeClaimed", "tcp", "udp", "version",
    ...(value.version === 2 ? ["udpRefusals"] : [])],
    "listener_failure_evidence_shape");
  check([1, 2].includes(value.version) && value.literalCodeSigningClaimed === false
    && value.literalNoHostNetworkClaimed === false
    && value.literalOfficialDependencyPurposeClaimed === false,
    "listener_failure_evidence_shape");
  for (const protocol of ["tcp", "udp"]) {
    const summary = value[protocol];
    exactKeys(summary, ["baselinePidOnlyCount", "deltaCount", "deltaSha256",
      "helperConnectedCount", "helperCount", "helperUnboundCount", "unknownPidCount",
      "vmOwnedCount"], "listener_failure_evidence_shape");
    const counts = [summary.deltaCount, summary.baselinePidOnlyCount, summary.unknownPidCount,
      summary.vmOwnedCount, summary.helperConnectedCount, summary.helperCount,
      summary.helperUnboundCount];
    check(counts.every((count) => Number.isInteger(count) && count >= 0 && count <= 1024)
      && summary.deltaCount === summary.baselinePidOnlyCount + summary.unknownPidCount
        + summary.vmOwnedCount
      && summary.helperConnectedCount <= summary.helperCount
      && summary.helperUnboundCount <= summary.helperCount
      && /^[a-f0-9]{64}$/.test(summary.deltaSha256), "listener_failure_evidence_shape");
  }
  if (value.version === 2) {
    exactKeys(value.udpRefusals, ["endpointShapeRefused", "ownerRefused", "nativeBindingRefused",
      "nativeBindingNotEvaluated", "predicatePassed"], "listener_failure_evidence_shape");
    const counts = Object.values(value.udpRefusals);
    check(counts.every((count) => Number.isInteger(count) && count >= 0 && count <= 1024)
      && counts.reduce((sum, count) => sum + count, 0) === value.udp.deltaCount,
    "listener_failure_evidence_shape");
  }
  return value;
}

function closedHostProcessEvidence(value, success = false, listenerEvidence = null) {
  if (value === null || value === undefined) return null;
  exactKeys(value, ["artifactIdentitySha256", "baselineSnapshotSha256",
    "birthIdentitySha256", "boundaryAfterSnapshotSha256", "boundaryBeforeSnapshotSha256",
    "executableIdentitySha256", "helperTcpDeltaCount", "helperUdpConnectedDeltaCount",
    "helperUdpDeltaCount", "helperUdpUnboundDeltaCount", "literalCodeSigningClaimed",
    "literalNoHostNetworkClaimed", "producerSha256", "socketCount", "tcpDeltaCount",
    "udpDeltaCount", "version"],
  "host_process_evidence_shape");
  check(value.version === 1 && value.literalCodeSigningClaimed === false
    && value.literalNoHostNetworkClaimed === false
    && [value.artifactIdentitySha256, value.baselineSnapshotSha256,
      value.birthIdentitySha256, value.boundaryAfterSnapshotSha256,
      value.boundaryBeforeSnapshotSha256, value.executableIdentitySha256,
      value.producerSha256].every((item) => /^[a-f0-9]{64}$/.test(item))
    && value.producerSha256 === KNOWN_HOST_PROCESS_PRODUCER_SHA256
    && value.socketCount === 3
    && [value.helperTcpDeltaCount, value.helperUdpConnectedDeltaCount,
      value.helperUdpDeltaCount, value.helperUdpUnboundDeltaCount, value.tcpDeltaCount,
      value.udpDeltaCount]
      .every((item) => Number.isInteger(item) && item >= 0 && item <= 1024)
    && value.helperTcpDeltaCount <= value.tcpDeltaCount
    && value.helperUdpConnectedDeltaCount <= value.helperUdpDeltaCount
    && value.helperUdpUnboundDeltaCount <= value.helperUdpDeltaCount
    && value.helperUdpDeltaCount <= value.udpDeltaCount,
  "host_process_evidence_shape");
  if (success) {
    check(listenerEvidence && value.tcpDeltaCount === 1 && value.helperTcpDeltaCount === 1
      && value.helperUdpConnectedDeltaCount === value.helperUdpDeltaCount
      && value.helperUdpUnboundDeltaCount === 0
      && value.udpDeltaCount === listenerEvidence.attributedClientCount
      && value.helperUdpDeltaCount === listenerEvidence.usernetClientCount
      && value.udpDeltaCount - value.helperUdpDeltaCount
        === listenerEvidence.stableAmbientClientCount,
    "host_process_evidence_success");
  }
  return value;
}

export function hostProcessEvidence(base, listenerEvidence) {
  exactKeys(base, ["artifactIdentitySha256", "baselineSnapshotSha256",
    "birthIdentitySha256", "boundaryAfterSnapshotSha256", "boundaryBeforeSnapshotSha256",
    "executableIdentitySha256", "producerSha256"], "host_process_evidence_base");
  const closedListener = closedListenerFailureEvidence(listenerEvidence);
  check(closedListener !== null, "host_process_listener_evidence");
  return closedHostProcessEvidence(Object.freeze({
    version: 1,
    ...base,
    socketCount: 3,
    helperTcpDeltaCount: closedListener.tcp.helperCount,
    helperUdpDeltaCount: closedListener.udp.helperCount,
    helperUdpConnectedDeltaCount: closedListener.udp.helperConnectedCount,
    helperUdpUnboundDeltaCount: closedListener.udp.helperUnboundCount,
    tcpDeltaCount: closedListener.tcp.deltaCount,
    udpDeltaCount: closedListener.udp.deltaCount,
    literalCodeSigningClaimed: false,
    literalNoHostNetworkClaimed: false,
  }));
}

export function validateFailureArchiveRetention(value) {
  exactKeys(value, ["version", "state", "qualified", "d2Authority", "releaseAuthority", "childName",
    "manifestSha256", "retentionFailure"], "failed_archive_retention_shape");
  check(value.version === 1 && ["incomplete", "retained"].includes(value.state)
    && value.qualified === false && value.d2Authority === false && value.releaseAuthority === false
    && (value.childName === null || (typeof value.childName === "string"
      && /^phase639-failed-image-artifacts-[a-f0-9]{24}$/.test(value.childName)))
    && (value.retentionFailure === null || ["transfer_failed", "post_checks_failed", "source_failed",
      "cleanup_failed", "finalization_failed"].includes(value.retentionFailure)), "failed_archive_retention_shape");
  if (value.state === "retained") {
    check(value.childName !== null && typeof value.manifestSha256 === "string"
      && /^(?!0{64}$)[a-f0-9]{64}$/.test(value.manifestSha256)
      && value.retentionFailure === null, "failed_archive_retention_shape");
  } else check(value.manifestSha256 === null, "failed_archive_retention_shape");
  return { ...value };
}

export function failedArchiveFinalizationAllowed(value) {
  return value.pending === true && value.postChecks === true && value.cleanupProven === true
    && value.cleanupFailure === undefined && value.sourceUnchanged === true && value.interrupted === false;
}

export function failureReport({ stage, primaryFailure, cleanupFailure, interrupted, retained,
  stateRoot, profile, cleanupProven, retainedDiagnosticsOnly, diagnostics,
  listenerFailureEvidence = null, hostProcessEvidence: processEvidence = null, archiveRetention = null }) {
  check(Array.isArray(diagnostics) && diagnostics.length <= MAX_FAILURE_COMMANDS * 2,
    "diagnostic_count_bound");
  const closed = diagnostics.map((entry) => {
    exactKeys(entry, ["file", "tool", "stage", "bytes"], "diagnostic_report_shape");
    check(/^command-failure-000[1-8]\.(?:stdout|stderr)$/.test(entry.file)
      && own(CLI, entry.tool) && /^[a-zA-Z0-9_]{1,80}$/.test(entry.stage)
      && Number.isInteger(entry.bytes) && entry.bytes > 0 && entry.bytes <= MAX_DIAGNOSTIC,
    "diagnostic_report_shape");
    return { ...entry };
  });
  const primary = primaryFailure ? codeOf({ message: primaryFailure }, "closed_stage_failure") : null;
  const cleanup = cleanupFailure ? codeOf({ message: cleanupFailure }, "cleanup_unproven_state_retained") : null;
  const closedListener = closedListenerFailureEvidence(listenerFailureEvidence);
  const closedProcess = closedHostProcessEvidence(processEvidence);
  if (closedListener && closedProcess) {
    check(closedProcess.tcpDeltaCount === closedListener.tcp.deltaCount
      && closedProcess.udpDeltaCount === closedListener.udp.deltaCount
      && closedProcess.helperTcpDeltaCount === closedListener.tcp.helperCount
      && closedProcess.helperUdpDeltaCount === closedListener.udp.helperCount
      && closedProcess.helperUdpConnectedDeltaCount === closedListener.udp.helperConnectedCount
      && closedProcess.helperUdpUnboundDeltaCount === closedListener.udp.helperUnboundCount,
    "failure_evidence_mismatch");
  }
  return {
    event: "managed_runner_vm_preflight_failed",
    version: 1,
    stage: codeOf({ message: stage }, "unknown_stage"),
    code: primary || cleanup || (interrupted ? "interrupted" : "qualification_incomplete"),
    primaryFailure: primary,
    cleanupFailure: cleanup,
    interrupted,
    cleanupProven,
    retainedDiagnosticsOnly: Boolean(retained && cleanupProven && retainedDiagnosticsOnly),
    diagnostics: closed,
    hostProcessEvidence: closedProcess,
    listenerFailureEvidence: closedListener,
    ...(archiveRetention ? { archiveRetention: validateFailureArchiveRetention(archiveRetention) } : {}),
    ...(retained ? { retainedState: stateRoot, profile } : {}),
  };
}

export function reportMode(value, expectedSourceCommit) {
  check(typeof expectedSourceCommit === "string" && expectedSourceCommit.length === 40
    && SOURCE_COMMIT.test(expectedSourceCommit),
    "report_source_scope");
  exactKeys(value, ["baseCommit", "buildx", "claims", "cleanup", "emulated", "harnessSha256",
    "hostProcessEvidence", "identities", "listenerEvidence", "observations", "qualification",
    "resources", "testSha256", "toolVersions", "version", "portForwardPolicySha256"],
  "report_shape");
  check(value.version === 1 && value.qualification === "isolated_linux_amd64_vm_compatibility_only"
    && value.emulated === true && value.baseCommit === expectedSourceCommit
    && /^[a-f0-9]{64}$/.test(value.harnessSha256)
    && /^[a-f0-9]{64}$/.test(value.testSha256)
    && value.portForwardPolicySha256 === sha256(LIMA_PORT_FORWARD_OVERRIDE), "report_identity");
  assertResources(value.resources);
  closedHostProcessEvidence(value.hostProcessEvidence, true, value.listenerEvidence);
  exactKeys(value.cleanup, ["ownedVmPidsAbsent", "privateInventoriesEmpty", "socketAbsent",
    "stateAbsent", "usernetSocketsAbsent"], "report_cleanup_shape");
  check(Object.values(value.cleanup).every((item) => item === true), "report_proof_required");
  exactKeys(value.claims, ["api", "image", "productionAuthority", "runnerStartup"],
    "report_claims_shape");
  check(Object.values(value.claims).every((item) => item === false), "report_claims_false");
  exactKeys(value.toolVersions, ["colima", "lima", "qemu"], "report_tool_versions");
  check(value.toolVersions.colima === KNOWN_VERSIONS.colima
    && value.toolVersions.lima === KNOWN_VERSIONS.lima
    && value.toolVersions.qemu === KNOWN_VERSIONS.qemu, "report_tool_versions");
  exactKeys(value.listenerEvidence, ["attributedClientCount", "attributedClientDeltaSha256",
    "listenerCount", "literalCodeSigningClaimed", "literalNoHostNetworkClaimed",
    "literalOfficialDependencyPurposeClaimed", "stableAmbientClientCount",
    "usernetClientCount"],
    "report_listener_shape");
  check(value.listenerEvidence.literalCodeSigningClaimed === false
    && value.listenerEvidence.literalNoHostNetworkClaimed === false
    && value.listenerEvidence.literalOfficialDependencyPurposeClaimed === false
    && Number.isInteger(value.listenerEvidence.listenerCount)
    && [value.listenerEvidence.attributedClientCount,
      value.listenerEvidence.stableAmbientClientCount, value.listenerEvidence.usernetClientCount]
      .every((count) => Number.isInteger(count) && count >= 0 && count <= 1024)
    && value.listenerEvidence.attributedClientCount
      === value.listenerEvidence.stableAmbientClientCount + value.listenerEvidence.usernetClientCount
    && /^[a-f0-9]{64}$/.test(value.listenerEvidence.attributedClientDeltaSha256)
    && value.listenerEvidence.listenerCount === value.hostProcessEvidence.helperTcpDeltaCount,
  "report_listener_evidence");
  exactKeys(value.identities, ["engineArchitecture", "engineOs", "guestArchitecture", "limaVersion",
    "vmType"], "report_identities_shape");
  check(value.identities.engineArchitecture === "x86_64" && value.identities.engineOs === "linux"
    && value.identities.guestArchitecture === "x86_64" && value.identities.limaVersion === "2.2.1"
    && value.identities.vmType === "qemu", "report_identities");
  exactKeys(value.observations, ["dataDiskBytes", "engineCpuCount", "engineMemoryBytes",
    "guestMemoryBytes", "rootDiskBytes"], "report_observations_shape");
  check(value.observations.engineCpuCount === 4
    && [value.observations.engineMemoryBytes, value.observations.guestMemoryBytes]
      .every((item) => Number.isSafeInteger(item) && item >= 5 * GIB && item <= 6 * GIB)
    && Number.isSafeInteger(value.observations.rootDiskBytes)
    && value.observations.rootDiskBytes > 6 * GIB && value.observations.rootDiskBytes <= 8 * GIB
    && Number.isSafeInteger(value.observations.dataDiskBytes)
    && value.observations.dataDiskBytes > 14 * GIB && value.observations.dataDiskBytes <= 16 * GIB,
  "report_observations");
  check(value.buildx && typeof value.buildx === "object"
    && (value.buildx.status === "available" || value.buildx.status === "unsupported_known"),
  "report_buildx_evidence");
  if (value.buildx.status === "available") {
    exactKeys(value.buildx, ["status", "version"], "report_buildx_evidence");
    check(/^v[0-9]+\.[0-9]+\.[0-9]+$/.test(value.buildx.version), "report_buildx_evidence");
  } else {
    exactKeys(value.buildx, ["status"], "report_buildx_evidence");
  }
  return Object.freeze(value);
}

function metadata(stat) {
  return {
    directory: stat.isDirectory(), file: stat.isFile(), symlink: stat.isSymbolicLink(), uid: stat.uid,
    mode: stat.mode, nlink: stat.nlink, size: stat.size,
  };
}

function kernelMetadata(stat, label) {
  return {
    directory: stat.isDirectory(),
    file: stat.isFile(),
    socket: stat.isSocket(),
    symlink: stat.isSymbolicLink(),
    uid: Number(stat.uid),
    mode: Number(stat.mode),
    nlink: Number(stat.nlink),
    size: Number(stat.size),
    dev: String(stat.dev),
    ino: String(stat.ino),
    ...(label === undefined ? {} : { label }),
  };
}

async function privateFile(path, uid, maximum) {
  const before = await lstat(path);
  check(before.isFile() && !before.isSymbolicLink() && before.uid === uid && before.nlink === 1
    && (before.mode & 0o7777) === 0o600 && before.size > 0 && before.size <= maximum,
  "private_file_identity");
  const bytes = await readFile(path);
  const after = await lstat(path);
  check(identity(before) === identity(after) && before.size === after.size && bytes.length === before.size,
    "private_file_changed");
  return bytes;
}

async function boundedGeneratedFile(path, uid, maximum) {
  const before = await lstat(path);
  check(before.isFile() && !before.isSymbolicLink() && before.uid === uid && before.nlink === 1
    && (before.mode & 0o022) === 0 && before.size >= 0 && before.size <= maximum,
  "generated_file_identity");
  const bytes = await readFile(path);
  const after = await lstat(path);
  check(identity(before) === identity(after) && before.size === after.size
    && bytes.length === before.size, "generated_file_changed");
  return bytes;
}

function singleFlight(operation) {
  let pending;
  return () => pending ??= operation();
}

export function parseVmMode(argv) {
  check(Array.isArray(argv) && argv.length === 3 && argv[1] === "--source-commit"
    && typeof argv[2] === "string" && argv[2].length === 40 && SOURCE_COMMIT.test(argv[2])
    && ["--preflight-only", "--runner-image-smoke", "--reader-runtime-observation"].includes(argv[0]),
  "exact_preflight_cli_required");
  return argv[0] === "--runner-image-smoke" ? "image"
    : argv[0] === "--reader-runtime-observation" ? "reader" : "compatibility";
}

export function parseArtifactOptions(argv) {
  check(Array.isArray(argv) && [3, 9].includes(argv.length), "exact_preflight_cli_required");
  const mode = parseVmMode(argv.slice(0, 3));
  const sourceCommit = argv[2];
  if (argv.length === 3) return { mode, sourceCommit, artifact: null };
  check(mode === "image" && argv[3] === "--artifact-parent" && argv[5] === "--artifact-filesystem-dev"
    && argv[7] === "--guest-reader-approval", "exact_preflight_cli_required");
  check([argv[4], argv[8]].every((path) => typeof path === "string" && path.startsWith("/")
    && resolve(path) === path && path !== "/") && canonicalUint64(argv[6]), "artifact_option_scope");
  return { mode, sourceCommit,
    artifact: { parent: argv[4], filesystemDev: argv[6], approvalPath: argv[8] } };
}

export function requireExpectedSourceCommit(observed, expectedSourceCommit, code = "base_commit_changed") {
  check(typeof expectedSourceCommit === "string" && expectedSourceCommit.length === 40
    && SOURCE_COMMIT.test(expectedSourceCommit),
    "expected_source_commit_required");
  check(typeof observed === "string" && observed === expectedSourceCommit, code);
  return observed;
}

// Read-only source admission precedes HOME/launcher/VM-state mutation. It is not
// a second general command launcher and cannot select another executable/argv.
async function readApprovedGitHead() {
  const canonical = await realpath(CLI.git);
  const before = await lstat(canonical);
  assertApprovedCli(CLI.git, canonical, metadata(before), process.getuid());
  const child = spawn(CLI.git, ["-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false",
    "rev-parse", "--verify", "HEAD^{commit}"], {
    cwd: REPO,
    env: { PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_SYSTEM: "/dev/null", GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_COUNT: "0",
      GIT_OPTIONAL_LOCKS: "0", GIT_NO_REPLACE_OBJECTS: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let bytes = Buffer.alloc(0), stderrBytes = 0, refused = false, spawnFailed = false;
  let escalation, terminalLimit;
  let failTerminal;
  const terminate = () => {
    refused = true;
    child.kill("SIGTERM");
    escalation ??= setTimeout(() => {
      child.kill("SIGKILL");
      terminalLimit ??= setTimeout(() => failTerminal(new Error("source_preflight_cleanup_unproven")), 5000);
    }, 5000);
  };
  const signals = ["SIGHUP", "SIGINT", "SIGTERM"];
  for (const signal of signals) process.on(signal, terminate);
  child.stdout.on("data", (chunk) => {
    if (bytes.length + chunk.length > 1024) terminate();
    else bytes = Buffer.concat([bytes, chunk]);
  });
  child.stderr.on("data", (chunk) => { stderrBytes += chunk.length; if (stderrBytes > 1024) terminate(); });
  const timer = setTimeout(terminate, 30_000);
  try {
    const terminal = await new Promise((resolve, reject) => {
      failTerminal = reject;
      child.once("error", () => { spawnFailed = true; });
      child.once("close", (code, signal) => resolve({ code, signal }));
    });
    check(!refused && !spawnFailed && terminal.code === 0 && terminal.signal === null
      && stderrBytes === 0 && bytes.length === 41 && bytes.every((byte) => byte < 128)
      && /^[a-f0-9]{40}\n$/.test(bytes.toString("ascii")), "source_preflight_failed");
    const after = await lstat(canonical);
    check(await realpath(CLI.git) === canonical && identity(before) === identity(after)
      && before.uid === after.uid && before.mode === after.mode && before.size === after.size
      && before.mtimeMs === after.mtimeMs && before.ctimeMs === after.ctimeMs, "source_preflight_tool_changed");
    check(!refused, "source_preflight_failed");
    return bytes.toString("ascii").slice(0, -1);
  } finally {
    clearTimeout(timer);
    clearTimeout(escalation);
    clearTimeout(terminalLimit);
    for (const signal of signals) process.off(signal, terminate);
  }
}

export async function sourceCommitPreflight(expectedSourceCommit, readHead = readApprovedGitHead) {
  check(typeof expectedSourceCommit === "string" && expectedSourceCommit.length === 40
    && SOURCE_COMMIT.test(expectedSourceCommit),
    "expected_source_commit_required");
  check(typeof readHead === "function", "source_preflight_reader");
  return requireExpectedSourceCommit(await readHead(), expectedSourceCommit);
}

export function qualificationDuration(options) {
  check(options && ["compatibility", "image", "reader"].includes(options.mode)
    && (options.artifact === null || (options.mode === "image"
      && typeof options.artifact === "object")), "qualification_mode_duration");
  return options.artifact ? 60 * 60_000 : 45 * 60_000;
}

export function artifactExportDeadline(now, overallDeadline) {
  check(Number.isSafeInteger(now) && now >= 0 && Number.isSafeInteger(overallDeadline)
    && overallDeadline - now > ARTIFACT_CLEANUP_RESERVE_MS + POST_EXPORT_RESERVE_MS,
  "artifact_cleanup_reserve");
  return Math.min(now + 15 * 60_000,
    overallDeadline - ARTIFACT_CLEANUP_RESERVE_MS - POST_EXPORT_RESERVE_MS);
}

export function phaseCommandTimeout(maximum, now, overallDeadline, reserveMs, phaseDeadline = null) {
  check([maximum, now, overallDeadline, reserveMs].every(Number.isSafeInteger)
    && maximum > 0 && now >= 0 && reserveMs >= 0
    && (phaseDeadline === null || Number.isSafeInteger(phaseDeadline)), "phase_budget_shape");
  const available = Math.min(overallDeadline - now - reserveMs,
    phaseDeadline === null ? Infinity : phaseDeadline - now - 5_000);
  check(available > 0, "phase_budget_exhausted");
  return Math.min(maximum, available);
}

export const READER_OBSERVATION_OPERATIONS = Object.freeze([
  "reader_archive_create", "reader_stage_create", "reader_archive_extract", "reader_source_hash",
  "reader_install_dir", "reader_install", "reader_discover",
]);

export function readerArchiveArguments(directory) {
  check(typeof directory === "string" && directory.startsWith("/") && !directory.includes("\0"),
    "reader_archive_directory");
  return ["-cf", "-", "--format=ustar", "--no-mac-metadata", "--no-acls", "--no-xattrs",
    "-C", directory, "managed-runner-artifacts.py"];
}

// This is deliberately a single pinned regular member, not a general tar reader.
export function validateReaderArchive(archive, source) {
  check(Buffer.isBuffer(archive) && archive.length >= 1536 && archive.length <= 1024 * 1024
    && archive.length % 512 === 0 && Buffer.isBuffer(source)
    && source.length > 0 && source.length <= 128 * 1024, "reader_archive_shape");
  const header = archive.subarray(0, 512);
  const field = (start, length) => header.subarray(start, start + length);
  const octal = (start, length) => {
    const bytes = field(start, length);
    const text = bytes.toString("ascii");
    check(bytes.every((byte) => byte < 128) && /^[0-7]+[\0 ]*$/.test(text), "reader_archive_octal");
    const value = Number.parseInt(text, 8);
    check(Number.isSafeInteger(value), "reader_archive_octal");
    return value;
  };
  const name = Buffer.alloc(100);
  name.write("managed-runner-artifacts.py", "ascii");
  check(field(0, 100).equals(name) && field(257, 6).equals(Buffer.from("ustar\0"))
    && field(263, 2).equals(Buffer.from("00"))
    && [0, 48].includes(header[156]) && field(157, 100).every((byte) => byte === 0)
    && field(345, 155).every((byte) => byte === 0), "reader_archive_member");
  const checksum = header.reduce((sum, byte, index) => sum + (index >= 148 && index < 156 ? 32 : byte), 0);
  check(octal(148, 8) === checksum, "reader_archive_checksum");
  const size = octal(124, 12);
  const paddedEnd = 512 + Math.ceil(size / 512) * 512;
  check(size === source.length && archive.length >= paddedEnd + 1024
    && archive.subarray(512, 512 + size).equals(source)
    && archive.subarray(512 + size).every((byte) => byte === 0), "reader_archive_payload");
}

export function readerOperationDiagnostic(operation, terminal, stderrTail) {
  check(READER_OBSERVATION_OPERATIONS.includes(operation) && Buffer.isBuffer(stderrTail)
    && stderrTail.length <= MAX_DIAGNOSTIC, "reader_observation_diagnostic_shape");
  exactKeys(terminal, ["code", "signal", "stderrBytes", "stderrSha256"], "reader_observation_diagnostic_shape");
  check((terminal.code === null || (Number.isInteger(terminal.code) && terminal.code >= 0 && terminal.code <= 255))
    && (terminal.signal === null || typeof terminal.signal === "string")
    && Number.isSafeInteger(terminal.stderrBytes) && terminal.stderrBytes >= 0
    && stderrTail.length === Math.min(terminal.stderrBytes, MAX_DIAGNOSTIC)
    && /^[a-f0-9]{64}$/.test(terminal.stderrSha256)
    && (terminal.stderrBytes > MAX_DIAGNOSTIC || sha256(stderrTail) === terminal.stderrSha256),
  "reader_observation_diagnostic_shape");
  const signal = terminal.signal === null ? null : ["SIGTERM", "SIGKILL", "SIGABRT", "SIGSEGV", "SIGPIPE",
    "SIGBUS", "SIGINT", "SIGHUP", "SIGQUIT", "SIGILL", "SIGFPE", "SIGTRAP", "spawn_error"]
    .includes(terminal.signal) ? terminal.signal : "other";
  return Buffer.from(`${JSON.stringify({ version: 1, operation, ...terminal, signal,
    retainedStderrBytes: stderrTail.length, stderrTruncated: terminal.stderrBytes > stderrTail.length })}\n`);
}

export async function refuseReaderStderr(operation, terminal, stderrTail, retain) {
  const metadata = readerOperationDiagnostic(operation, terminal, stderrTail);
  if (terminal.stderrBytes === 0) return;
  check(typeof retain === "function", "reader_observation_diagnostic_failed");
  try { await retain(metadata, stderrTail); } catch (error) {
    throw new Error(error?.message === "reader_observation_diagnostic_limit"
      ? "reader_observation_diagnostic_limit" : "reader_observation_diagnostic_failed");
  }
  throw new Error(operation === "reader_archive_create"
    ? "reader_observation_archive_stderr" : "reader_observation_stderr");
}

export async function observeGuestReaderRuntime(guest, archive, readerSourceSha256) {
  check(typeof guest === "function" && Buffer.isBuffer(archive) && archive.length > 0
    && archive.length <= 1024 * 1024 && /^[a-f0-9]{64}$/.test(readerSourceSha256),
  "reader_observation_input");
  const staging = "/var/tmp/bluey-jobs-reader-observation-p639";
  const readerDirectory = "/var/tmp/bluey-jobs-artifact-reader-p639";
  await guest(["mkdir", "-m", "700", staging], { maximum: 1024 }, "reader_stage_create");
  await guest(["tar", "-xf", "-", "-C", staging], { inputBuffer: archive, maximum: 1024 },
    "reader_archive_extract");
  const copied = await guest(["sha256sum", `${staging}/managed-runner-artifacts.py`], { maximum: 1024 },
    "reader_source_hash");
  check(copied === `${readerSourceSha256}  ${staging}/managed-runner-artifacts.py\n`,
    "reader_observation_source_changed");
  await guest(["/usr/bin/sudo", "-n", "mkdir", "-m", "700", readerDirectory], { maximum: 1024 },
    "reader_install_dir");
  await guest(["/usr/bin/sudo", "-n", "install", "-o", "0", "-g", "0", "-m", "500",
    `${staging}/managed-runner-artifacts.py`, `${readerDirectory}/reader.py`], { maximum: 1024 },
    "reader_install");
  const text = await guest(guestReaderDiscoveryArguments(readerSourceSha256),
    { maximum: 8192, timeout: 30_000 }, "reader_discover");
  const observed = JSON.parse(text);
  check(`${canonicalArtifactJson(observed)}\n` === text, "reader_observation_canonical");
  validateReaderObservation(observed, readerSourceSha256); // observation is never assigned as an approval
  return observed;
}

export function readerRuntimeObservationReport(value, sourceSha256, expectedSourceCommit) {
  exactKeys(value, ["sourceCommit", "harnessSha256", "toolSourcesSha256", "observation", "cleanup"],
    "reader_observation_report");
  requireExpectedSourceCommit(value.sourceCommit, expectedSourceCommit, "reader_observation_source_mismatch");
  check(SOURCE_COMMIT.test(value.sourceCommit)
    && [value.harnessSha256, value.toolSourcesSha256].every((digest) => /^[a-f0-9]{64}$/.test(digest)),
  "reader_observation_report");
  exactKeys(value.cleanup, ["privateInventoriesEmpty", "ownedVmPidsAbsent", "socketAbsent",
    "stateAbsent", "usernetSocketsAbsent"], "reader_observation_report");
  check(Object.values(value.cleanup).every((flag) => flag === true), "reader_observation_report");
  validateReaderObservation(value.observation, sourceSha256);
  return { event: "managed_runner_vm_reader_runtime_observation_finished", version: 1,
    qualified: false, ...value,
    claims: { compatibility: false, image: false, runnerStartup: false, release: false } };
}

export async function emitImageReport(fullReport, qualify = null,
  write = (bytes) => process.stdout.write(bytes)) {
  check((qualify === null || typeof qualify === "function") && typeof write === "function",
    "image_report_sink");
  // This exact insertion-ordered UTF-8 record, including its newline, is both
  // hashed for the private receipt and emitted. No semantic reserialization.
  const bytes = Buffer.from(`${JSON.stringify(fullReport)}\n`, "utf8");
  check(bytes.length <= 64 * 1024, "image_report_bound");
  if (qualify) await qualify(sha256(bytes));
  write(bytes);
}

export function imageProgressRecord(phase) {
  check(IMAGE_PROGRESS_PHASES.includes(phase), "image_progress_phase");
  return { event: "managed_runner_vm_image_progress", version: 1, phase };
}

export async function main(argv = process.argv.slice(2)) {
  const options = parseArtifactOptions(argv);
  const mode = options.mode;
  const budgeted = options.artifact !== null || mode === "reader";
  const expectedSourceCommit = options.sourceCommit;
  let lastImageProgress = -1;
  const emitImageProgress = (phase) => {
    const record = imageProgressRecord(phase);
    const ordinal = IMAGE_PROGRESS_PHASES.indexOf(phase);
    check(mode === "image" && ordinal === lastImageProgress + 1, "image_progress_order");
    lastImageProgress = ordinal;
    console.log(JSON.stringify(record));
  };
  check(process.platform === "darwin" && process.arch === "arm64", "native_darwin_arm64_required");
  check(process.env.BLUEY_DATABASE_URL === undefined
    && process.env.BLUEY_TEST_POSTGRES_URL === undefined, "database_environment_forbidden");

  const uid = process.getuid();
  await sourceCommitPreflight(expectedSourceCommit);
  const configuredRoot = process.env.BLUEY_TEST_RUN_ROOT;
  check(typeof configuredRoot === "string", "launcher_root_required");
  const rootReal = await realpath(configuredRoot);
  const rootStat = await lstat(rootReal);
  assertLauncherRoot(configuredRoot, rootReal, metadata(rootStat), uid);
  const launcherIdentity = identity(rootStat);
  const launcherMarker = await privateFile(join(rootReal, LAUNCHER_MARKER), uid, 4096);
  assertLauncherMarker(launcherMarker, rootReal, uid);
  const home = process.env.HOME;
  check(typeof home === "string", "private_home_required");
  const homeReal = await realpath(home);
  const homeStat = await lstat(homeReal);
  assertPrivateHome(home, homeReal, metadata(homeStat), uid, rootReal);
  const homeIdentity = identity(homeStat);

  const fs = await statfs(rootReal);
  const freeBytes = fs.bavail * fs.bsize;
  assertResources({ cpus: 4, memoryGiB: 6, rootDiskGiB: 8, dataDiskGiB: 16, freeBytes });
  const tmpReal = await realpath("/tmp");
  const tmpIdentity = identity(await lstat(tmpReal));
  let stage = "preflight";
  let primaryFailure;
  let cleanupFailure;
  let interrupted = false;
  let cleaning = false;
  let startAttempted = false;
  let retained = true;
  let cleanupProven = false;
  let retainedDiagnosticsOnly = false;
  let active;
  let result;
  const signals = ["SIGHUP", "SIGINT", "SIGTERM"];
  const stop = () => {
    interrupted = true;
    if (active && !cleaning) active.terminate();
  };
  for (const signal of signals) process.on(signal, stop);
  let overall;
  const profile = `p639-${randomBytes(10).toString("hex")}`;
  let stateRoot;
  let stateIdentity;
  let env;
  let ownerMarker;
  let harnessSha256;
  let testSha256;
  let producerSha256;
  let producerSource;
  let limaPolicyEstablished = false;
  let limaPolicyDirectoryIdentity;
  let limaPolicyFileIdentity;
  const vmPids = new Set();
  const diagnostics = [];
  let listenerFailureEvidence = null;
  let processEvidence = null;
  let imageEvidence;
  let imageToolSourcesSha256;
  let artifactApproval;
  let artifactParent;
  let artifactBinding;
  let artifactManifest;
  let failedArchiveBinding;
  let failedArchivePending;
  let archiveRetention;
  let deferredImageFailure;
  let failedArchivePostChecks = false;
  let failedArchiveSourceAfter = false;
  let readerObservation;
  let readerSourceSha256;
  let budgetPhase = "prepare";
  let phaseDeadline = null;
  let workloadCleanupPending = false;
  let workloadFailed = false;
  let usernetSocketsAbsent = false;
  let usernetExpected = false;
  let usernetArtifactsAuthority;
  let usernetIdentityAmbiguous = false;
  let failureSequence = 0;
  let setupFailure;

  try {
    stateRoot = await mkdtemp(join(tmpReal, "bluey-jobs-linux-p639-"));
    const stateStat = await lstat(stateRoot);
    assertPrivateRoot(stateRoot, await realpath(stateRoot), metadata(stateStat), uid);
    stateIdentity = identity(stateStat);
    env = isolatedEnvironment(stateRoot, rootReal, homeReal);
    ownerMarker = Buffer.from(`${JSON.stringify({
      version: 1, root: stateRoot, profile, uid, nonce: randomBytes(24).toString("hex"),
    })}\n`);
    await writeFile(join(stateRoot, OWNER_MARKER), ownerMarker, { flag: "wx", mode: 0o600 });
    await privateFile(join(stateRoot, OWNER_MARKER), uid, 4096);
    const sourceBefore = {
      harness: await readFile(SELF),
      test: await readFile(TEST),
      producer: await readFile(HOST_PROCESS_PRODUCER),
    };
    harnessSha256 = sha256(sourceBefore.harness);
    testSha256 = sha256(sourceBefore.test);
    producerSha256 = sha256(sourceBefore.producer);
    check(producerSha256 === KNOWN_HOST_PROCESS_PRODUCER_SHA256,
      "host_process_producer_source_unknown");
    producerSource = sourceBefore.producer;
    if (mode === "image" || mode === "reader") imageToolSourcesSha256 = await imageToolsetSha256();
  } catch (error) {
    setupFailure = codeOf(error, "private_state_setup_failed");
  } finally {
    if (setupFailure) {
      for (const signal of signals) process.off(signal, stop);
    }
  }
  if (setupFailure) {
    console.log(JSON.stringify(failureReport({
      stage: "private_state_setup",
      primaryFailure: setupFailure,
      interrupted,
      retained: stateRoot !== undefined,
      stateRoot,
      profile,
      cleanupProven: false,
      retainedDiagnosticsOnly: false,
      diagnostics,
    })));
    throw new Error("nonprod_managed_runner_vm_not_qualified");
  }

  const limaPolicyDirectory = join(env.LIMA_HOME, "_config");
  const limaPolicyFile = join(limaPolicyDirectory, "override.yaml");
  const usernetNetworksDirectory = join(env.LIMA_HOME, "_networks");
  const usernetDirectory = join(usernetNetworksDirectory, "user-v2");
  const usernetPidFile = join(usernetDirectory, "usernet_user-v2.pid");
  const usernetSocketPaths = [
    ["qemu", join(usernetDirectory, "user-v2_qemu.sock")],
    ["fd", join(usernetDirectory, "user-v2_fd.sock")],
    ["ep", join(usernetDirectory, "user-v2_ep.sock")],
  ];

  async function revalidateLimaPolicy() {
    const directory = await lstat(limaPolicyDirectory);
    const file = await lstat(limaPolicyFile);
    assertLimaOverrideIdentity(metadata(directory), metadata(file), uid);
    check(identity(directory) === limaPolicyDirectoryIdentity
      && identity(file) === limaPolicyFileIdentity, "lima_override_identity_changed");
    assertLimaPortForwardOverride(await privateFile(limaPolicyFile, uid,
      LIMA_PORT_FORWARD_OVERRIDE.length));
  }

  async function revalidate() {
    const currentRoot = await lstat(rootReal);
    assertLauncherRoot(rootReal, await realpath(rootReal), metadata(currentRoot), uid);
    check(identity(currentRoot) === launcherIdentity, "launcher_root_changed");
    check((await privateFile(join(rootReal, LAUNCHER_MARKER), uid, 4096)).equals(launcherMarker),
      "launcher_marker_changed");
    const currentHome = await lstat(homeReal);
    assertPrivateHome(homeReal, await realpath(homeReal), metadata(currentHome), uid, rootReal);
    check(identity(currentHome) === homeIdentity, "private_home_changed");
    check(identity(await lstat(tmpReal)) === tmpIdentity, "state_parent_changed");
    const currentState = await lstat(stateRoot);
    assertPrivateRoot(stateRoot, await realpath(stateRoot), metadata(currentState), uid);
    check(identity(currentState) === stateIdentity, "state_root_changed");
    check((await privateFile(join(stateRoot, OWNER_MARKER), uid, 4096)).equals(ownerMarker),
      "state_marker_changed");
    if (limaPolicyEstablished) await revalidateLimaPolicy();
  }

  async function writeDiagnostics(tool, stdoutTail, stderrTail, required = false) {
    if (failureSequence >= MAX_FAILURE_COMMANDS) {
      check(!required, "reader_observation_diagnostic_limit");
      return;
    }
    failureSequence += 1;
    await revalidate();
    for (const [stream, tail] of [["stdout", stdoutTail], ["stderr", stderrTail]]) {
      const file = `command-failure-${String(failureSequence).padStart(4, "0")}.${stream}`;
      const bytes = tail.length ? tail : Buffer.from(`No ${stream} captured.\n`);
      await writeFile(join(stateRoot, file), bytes, { flag: "wx", mode: 0o600 });
      const observed = await privateFile(join(stateRoot, file), uid, MAX_DIAGNOSTIC);
      check(observed.equals(bytes), "failure_diagnostic_changed");
      diagnostics.push({ file, tool, stage: cleaning ? "cleanup" : stage, bytes: bytes.length });
    }
  }

  function commandBudget(maximum) {
    if (!budgeted) return maximum;
    check(!interrupted || cleaning, "interrupted");
    let reserve = ARTIFACT_CLEANUP_RESERVE_MS + POST_EXPORT_RESERVE_MS
      + (workloadCleanupPending ? ARTIFACT_WORKLOAD_PENDING_MS : 0);
    if (budgetPhase === "workloadDiagnostic") reserve = ARTIFACT_CLEANUP_RESERVE_MS + ARTIFACT_WORKLOAD_CLEANUP_MS;
    if (budgetPhase === "workloadCleanup") reserve = ARTIFACT_CLEANUP_RESERVE_MS
      + (workloadFailed ? 0 : POST_EXPORT_RESERVE_MS);
    if (budgetPhase === "postObservation") reserve = ARTIFACT_CLEANUP_RESERVE_MS + ARTIFACT_SOURCE_FINAL_MS;
    if (budgetPhase === "sourceFinal") reserve = ARTIFACT_CLEANUP_RESERVE_MS;
    if (budgetPhase === "outerCleanup") reserve = 35_000 + 35_000;
    if (budgetPhase === "sourceAfter") reserve = 35_000 + 120_000;
    if (budgetPhase === "qualify") reserve = 5_000;
    return phaseCommandTimeout(maximum, Date.now(), overallDeadline, reserve, phaseDeadline);
  }

  function workloadPhase(next, { failed = false } = {}) {
    check(["failure_diagnostic", "cleanup", "cleaned"].includes(next), "workload_budget_phase");
    if (next === "failure_diagnostic") {
      workloadFailed = true;
      budgetPhase = "workloadDiagnostic";
      phaseDeadline = null;
    } else if (next === "cleanup") {
      workloadFailed = failed;
      budgetPhase = "workloadCleanup";
      phaseDeadline = Math.min(Date.now() + ARTIFACT_WORKLOAD_CLEANUP_MS,
        overallDeadline - ARTIFACT_CLEANUP_RESERVE_MS - (failed ? 0 : POST_EXPORT_RESERVE_MS));
    } else {
      workloadCleanupPending = false;
      budgetPhase = "prepare";
      phaseDeadline = null;
    }
  }

  async function run(tool, args, {
    timeout = 30_000, maximum = MAX_COMMAND_OUTPUT, allowFailure = false,
    retainFailureDiagnostics = true, inputBuffer, binaryStdout = false, readerOperation = null,
  } = {}) {
    check(own(CLI, tool), "command_tool_not_allowed");
    check(readerOperation === null || (mode === "reader"
      && READER_OBSERVATION_OPERATIONS.includes(readerOperation)
      && tool === (readerOperation === "reader_archive_create" ? "tar" : "colima")
      && retainFailureDiagnostics), "reader_observation_operation");
    if (readerOperation !== null) stage = readerOperation;
    check(inputBuffer === undefined || (["image", "reader"].includes(mode) && Buffer.isBuffer(inputBuffer)
      && inputBuffer.length > 0 && inputBuffer.length <= 64 * 1024 * 1024),
    "command_input_buffer_bound");
    check(!binaryStdout || ((mode === "image" && ["git", "tar"].includes(tool))
      || (mode === "reader" && tool === "tar")),
      "command_binary_output_scope");
    check(Number.isSafeInteger(maximum) && maximum > 0 && maximum <= 64 * 1024 * 1024,
      "command_output_bound");
    check(!interrupted || cleaning, "interrupted");
    await revalidate();
    const commandPath = await realpath(CLI[tool]);
    assertApprovedCli(CLI[tool], commandPath, metadata(await lstat(commandPath)), uid);
    const admittedTimeout = commandBudget(timeout); // after awaited identity checks, before spawning
    const child = spawn(CLI[tool], args, {
      cwd: REPO, env, stdio: [inputBuffer === undefined ? "ignore" : "pipe", "pipe", "pipe"],
    });
    let inputFailed = false;
    if (inputBuffer !== undefined) {
      child.stdin.on("error", () => { inputFailed = true; });
      child.stdin.end(inputBuffer);
    }
    let stdout = [];
    let stdoutTail = Buffer.alloc(0);
    let stderr = [];
    let stderrTail = Buffer.alloc(0);
    let stderrBytes = 0;
    const stderrHash = readerOperation === null ? null : createHash("sha256");
    let total = 0;
    let timedOut = false;
    let overflow = false;
    let terminated = false;
    let escalation;
    const terminate = () => {
      if (terminated) return;
      terminated = true;
      child.kill("SIGTERM");
      escalation = setTimeout(() => child.kill("SIGKILL"), 5_000);
    };
    active = { terminate };
    child.stdout.on("data", (chunk) => {
      total += chunk.length;
      stdoutTail = boundedDiagnosticTail(stdoutTail, chunk);
      if (total <= maximum) stdout.push(chunk);
      if (total > maximum) { overflow = true; terminate(); }
    });
    child.stderr.on("data", (chunk) => {
      total += chunk.length;
      stderrBytes += chunk.length;
      stderrHash?.update(chunk);
      stderrTail = boundedDiagnosticTail(stderrTail, chunk);
      if (total <= maximum) stderr.push(chunk);
      if (total > maximum) { overflow = true; terminate(); }
    });
    const timer = setTimeout(() => { timedOut = true; terminate(); }, admittedTimeout);
    const closed = await new Promise((done) => {
      let settled = false;
      const finish = (value) => { if (!settled) { settled = true; done(value); } };
      child.once("error", () => finish({ code: null, signal: "spawn_error" }));
      child.once("close", (code, signal) => finish({ code, signal }));
    });
    clearTimeout(timer);
    clearTimeout(escalation);
    active = undefined;
    const failed = timedOut || overflow || closed.code !== 0 || closed.signal !== null;
    const readerTerminal = readerOperation === null ? null : {
      ...closed, stderrBytes, stderrSha256: stderrHash.digest("hex"),
    };
    const retainReaderDiagnostic = async (metadata, bytes) => {
      try { await writeDiagnostics(tool, metadata, bytes, true); } catch (error) {
        throw new Error(error?.message === "reader_observation_diagnostic_limit"
          ? "reader_observation_diagnostic_limit" : "reader_observation_diagnostic_failed");
      }
    };
    if ((failed || (readerOperation !== null && inputFailed)) && retainFailureDiagnostics) {
      if (readerOperation === null) await writeDiagnostics(tool, stdoutTail, stderrTail);
      else await retainReaderDiagnostic(
        readerOperationDiagnostic(readerOperation, readerTerminal, stderrTail), stderrTail);
    }
    if (timedOut) throw new Error("command_timed_out");
    if (overflow) throw new Error("command_output_too_large");
    check(!inputFailed, "command_input_failed");
    if (!allowFailure) check(closed.code === 0 && closed.signal === null, `command_${tool}_failed`);
    if (readerOperation !== null && !failed) {
      // Preserve raw bounded stderr and closed metadata before semantic rejection.
      // Never retain discovery stdout (candidate data) or the binary host archive.
      await refuseReaderStderr(readerOperation, readerTerminal, stderrTail, retainReaderDiagnostic);
    }
    const output = {
      ...closed,
      stdout: binaryStdout ? Buffer.concat(stdout) : Buffer.concat(stdout).toString("utf8"),
      stderr: Buffer.concat(stderr).toString("utf8"),
    };
    stdout = [];
    stderr = [];
    return output;
  }

  function artifactRequest(extra = {}) {
    return { parent: options.artifact.parent, filesystemDev: options.artifact.filesystemDev,
      excludedRoots: [REPO, rootReal, stateRoot], producerPath: HOST_PROCESS_PRODUCER,
      producerSha256, ...extra };
  }

  async function artifactControl(operation, request, timeoutMs = 30_000) {
    check(options.artifact && canonicalArtifactJson(request).length <= 65536, "artifact_control_scope");
    check(Number.isSafeInteger(timeoutMs) && timeoutMs > 0 && timeoutMs <= 15 * 60_000,
      "artifact_control_timeout");
    check(await imageToolsetSha256() === imageToolSourcesSha256, "image_tool_source_changed");
    const pythonPath = await realpath(CLI.python);
    assertApprovedCli(CLI.python, pythonPath, metadata(await lstat(pythonPath)), uid);
    check(!interrupted, "interrupted");
    const admittedTimeout = commandBudget(timeoutMs);
    const child = spawn(CLI.python, ["-I", "-S", "-B", ARTIFACT_HELPER,
      operation, canonicalArtifactJson(request)], { cwd: REPO, env, stdio: ["ignore", "pipe", "pipe"] });
    let output = Buffer.alloc(0);
    let errorOutput = Buffer.alloc(0);
    let exceeded = false;
    let escalation;
    const terminate = () => {
      child.kill("SIGTERM");
      escalation ??= setTimeout(() => child.kill("SIGKILL"), 5000);
    };
    active = { terminate };
    const collect = (prior, chunk) => {
      if (prior.length + chunk.length > 65536) { exceeded = true; terminate(); return prior; }
      return Buffer.concat([prior, chunk]);
    };
    child.stdout.on("data", (chunk) => { output = collect(output, chunk); });
    child.stderr.on("data", (chunk) => { errorOutput = collect(errorOutput, chunk); });
    const timer = setTimeout(() => { exceeded = true; terminate(); }, admittedTimeout);
    try {
      const terminal = await new Promise((done) => {
        child.once("error", () => done({ code: null, signal: "spawn_error" }));
        child.once("close", (code, signal) => done({ code, signal }));
      });
      check(!exceeded && !interrupted && terminal.code === 0 && terminal.signal === null
        && errorOutput.length === 0, "artifact_control_failed");
      const text = output.toString("ascii");
      const value = JSON.parse(text);
      check(output.every((byte) => byte < 128) && `${canonicalArtifactJson(value)}\n` === text,
        "artifact_control_reply");
      return value;
    } finally {
      clearTimeout(timer);
      clearTimeout(escalation);
      active = undefined;
    }
  }

  async function exportAttachments(input) {
    await revalidate();
    requireExpectedSourceCommit(input.source.sourceCommit, expectedSourceCommit, "artifact_source_mismatch");
    requireExpectedSourceCommit(input.inspection.expected.sourceCommit, expectedSourceCommit, "artifact_source_mismatch");
    check(!workloadCleanupPending, "artifact_workload_cleanup_pending");
    const deadline = artifactExportDeadline(Date.now(), overallDeadline);
    budgetPhase = "export";
    phaseDeadline = deadline;
    artifactBinding = await artifactControl("host-init", artifactRequest({
      parentIdentity: artifactParent.parentIdentity,
      source: { ...input.source, harnessSha256, imageToolSourcesSha256 },
    }), Math.min(30_000, deadline - Date.now()));
    for (const artifact of Object.keys(ARTIFACT_CAPS)) {
      await copySelectedArtifact(input, artifact, artifactBinding, deadline, false);
    }
    check(Date.now() < deadline, "artifact_export_deadline");
    artifactManifest = await artifactControl("host-manifest", artifactRequest({ binding: artifactBinding,
      sha256s: input.sha256s, reader: input.reader, inspection: input.inspection }), deadline - Date.now());
    check(Date.now() < deadline, "artifact_export_deadline");
    budgetPhase = "prepare";
    phaseDeadline = null;
  }

  async function copySelectedArtifact(input, artifact, binding, deadline, failed) {
    await revalidate();
    requireExpectedSourceCommit(input.source.sourceCommit, expectedSourceCommit, "artifact_source_mismatch");
    check(await imageToolsetSha256() === imageToolSourcesSha256, "image_tool_source_changed");
    check(deadline > Date.now() && !interrupted, "artifact_cleanup_reserve");
    for (const tool of ["colima", "python"]) {
      const path = await realpath(CLI[tool]);
      assertApprovedCli(CLI[tool], path, metadata(await lstat(path)), uid);
    }
    const digest = failed ? input.descriptor.sha256 : input.sha256s[artifact];
    const request = artifactRequest(failed ? { binding } : { binding, artifact,
      expected: { reader: input.reader, owner: input.owner, sha256: digest } });
    const timeoutMs = Math.min(deadline - Date.now(), 15 * 60_000);
    check(timeoutMs > 0 && !interrupted, "artifact_cleanup_reserve");
    const admittedTimeout = commandBudget(timeoutMs);
    const writer = spawn(CLI.python, ["-I", "-S", "-B", ARTIFACT_HELPER,
      failed ? "host-failure-receive" : "host-receive", canonicalArtifactJson(request)],
    { cwd: REPO, env, stdio: ["pipe", "pipe", "pipe"] });
    const reader = spawn(CLI.colima, ["ssh", "--profile", profile, "--",
      ...guestReaderArguments("guest-read", artifactApproval, {
        artifact, approval: artifactApproval, reader: input.reader,
        sha256: digest, owner: input.owner,
        ...(failed ? { source: input.descriptor.source } : {}),
      })], { cwd: REPO, env, stdio: ["ignore", "pipe", "pipe"] });
    const copied = await copyArtifactStream({ reader, writer, artifact, timeoutMs: admittedTimeout,
      setActive: (value) => { active = value; } });
    check(copied.sha256 === digest && (!failed || copied.size === input.descriptor.size), "artifact_copy_digest");
    return copied;
  }

  async function exportFailedArchive(input) {
    archiveRetention = { version: 1, state: "incomplete", qualified: false, d2Authority: false,
      releaseAuthority: false, childName: null, manifestSha256: null, retentionFailure: null };
    try {
      await revalidate();
      requireExpectedSourceCommit(input.source.sourceCommit, expectedSourceCommit, "failed_archive_source_changed");
      check(!workloadCleanupPending, "artifact_workload_cleanup_pending");
      const deadline = artifactExportDeadline(Date.now(), overallDeadline);
      budgetPhase = "export";
      phaseDeadline = deadline;
      await assertFailedArchiveTools();
      requireExpectedSourceCommit((await run("git", ["rev-parse", "HEAD"])).stdout.trim(), expectedSourceCommit,
        "failed_archive_source_changed");
      failedArchiveBinding = await artifactControl("host-failure-init", artifactRequest({
        parentIdentity: artifactParent.parentIdentity,
        source: { ...input.source, harnessSha256, imageToolSourcesSha256 }, descriptor: input.descriptor,
      }), Math.min(30_000, deadline - Date.now()));
      archiveRetention = validateFailureArchiveRetention({ ...archiveRetention,
        childName: failedArchiveBinding.childName });
      const received = await copySelectedArtifact(input, "runner.oci.tar", failedArchiveBinding, deadline, true);
      await assertFailedArchiveTools();
      requireExpectedSourceCommit((await run("git", ["rev-parse", "HEAD"])).stdout.trim(), expectedSourceCommit,
        "failed_archive_source_changed");
      check(Date.now() < deadline, "artifact_export_deadline");
      failedArchivePending = { received };
      return false; // bytes staged only, not even a completed unqualified manifest yet
    } catch {
      failedArchivePending = undefined;
      archiveRetention.retentionFailure = "transfer_failed";
      return false; // original workload failure remains primary; partial bytes remain
    } finally {
      budgetPhase = "prepare";
      phaseDeadline = null;
    }
  }

  async function assertFailedArchiveTools() {
    check(sha256(await readFile(SELF)) === harnessSha256
      && sha256(await readFile(TEST)) === testSha256
      && sha256(await readFile(HOST_PROCESS_PRODUCER)) === producerSha256
      && await imageToolsetSha256() === imageToolSourcesSha256, "failed_archive_source_changed");
  }

  async function hostProcessSnapshot(pids) {
    check(Array.isArray(pids) && pids.length >= 1 && pids.length <= 4096
      && pids.every((pid, index) => Number.isInteger(pid) && pid > 1 && pid <= 2_147_483_647
        && (index === 0 || pids[index - 1] < pid)), "host_process_selection_pid");
    check(Buffer.isBuffer(producerSource)
      && sha256(await readFile(HOST_PROCESS_PRODUCER)) === producerSha256,
    "host_process_producer_changed");
    const observed = await run("python", ["-I", "-S", "-B", HOST_PROCESS_PRODUCER,
      "--snapshot-pids", JSON.stringify(pids)], {
      timeout: 30_000,
      maximum: MAX_HOST_PROCESS_OUTPUT,
      retainFailureDiagnostics: false,
    });
    check(observed.stderr === "", "host_process_snapshot_stderr");
    const snapshot = parseHostProcessSnapshot(observed.stdout);
    check(sha256(await readFile(HOST_PROCESS_PRODUCER)) === producerSha256,
      "host_process_producer_changed");
    return snapshot;
  }

  async function bracketedListenerSnapshot(knownPids = []) {
    const before = await listenerSnapshot();
    const processes = await hostProcessSnapshot(listenerSnapshotPids(before, knownPids));
    const after = await listenerSnapshot();
    assertListenerSnapshotsEqual(before, after);
    return { listeners: after, processes };
  }

  async function captureUsernetArtifacts() {
    for (const directory of [usernetNetworksDirectory, usernetDirectory]) {
      check(await realpath(directory) === directory, "usernet_directory_scope");
    }
    const directories = await Promise.all([
      ["networks-directory", usernetNetworksDirectory],
      ["user-v2-directory", usernetDirectory],
    ].map(async ([label, path]) =>
      kernelMetadata(await lstat(path, { bigint: true }), label)));
    check(await realpath(usernetPidFile) === usernetPidFile, "usernet_pid_file_scope");
    const pidBefore = await lstat(usernetPidFile, { bigint: true });
    const pidBytes = await readFile(usernetPidFile);
    const pidMiddle = await lstat(usernetPidFile, { bigint: true });
    const pidBytesAfter = await readFile(usernetPidFile);
    const pidAfter = await lstat(usernetPidFile, { bigint: true });
    const pidMetadata = kernelMetadata(pidBefore, "pid");
    check(JSON.stringify(pidMetadata) === JSON.stringify(kernelMetadata(pidMiddle, "pid"))
      && JSON.stringify(pidMetadata) === JSON.stringify(kernelMetadata(pidAfter, "pid"))
      && pidBytes.equals(pidBytesAfter) && Number(pidBefore.size) === pidBytes.length,
    "usernet_pid_file_changed");
    const sockets = [];
    for (const [label, path] of usernetSocketPaths) {
      check(await realpath(path) === path, "usernet_socket_scope");
      sockets.push(kernelMetadata(await lstat(path, { bigint: true }), label));
    }
    const value = { directories, pidFile: pidMetadata, sockets };
    return Object.freeze({
      pid: parseUsernetPid(pidBytes),
      artifactIdentitySha256: assertUsernetArtifactIdentity(value, uid),
      identity: Object.freeze(value),
    });
  }

  async function revalidateUsernetArtifacts(expected) {
    const priorAmbiguity = usernetIdentityAmbiguous;
    usernetIdentityAmbiguous = true;
    const current = await captureUsernetArtifacts();
    vmPids.add(current.pid);
    usernetIdentityAmbiguous = validateUsernetContinuity(expected, current, priorAmbiguity);
  }

  async function assertUsernetSocketPathsAbsent() {
    for (const [, path] of usernetSocketPaths) {
      let absent = false;
      try { await lstat(path); }
      catch (error) { if (error.code === "ENOENT") absent = true; else throw error; }
      check(absent, "usernet_socket_path_still_present");
    }
    usernetSocketsAbsent = true;
  }

  const inventory = async () => ({
    colima: parseInventory((await run("colima", ["list", "--json"])).stdout),
    lima: parseInventory((await run("lima", ["list", "--json", "--tty=false"])).stdout),
  });
  const listenerSnapshot = async () => {
    const capture = async (protocol) => {
      const selector = protocol === "tcp"
        ? ["-iTCP", "-sTCP:LISTEN"]
        : ["-iUDP"];
      const observed = await run("lsof", ["-nP", "-a", "-u", String(uid), ...selector,
        "-F", "pn"], { allowFailure: true });
      return parseListenerRecords(assertListenerCapture(observed));
    };
    return { tcp: await capture("tcp"), udp: await capture("udp") };
  };
  const pidExists = (pid) => {
    try { process.kill(pid, 0); return true; }
    catch (error) { if (error.code === "ESRCH") return false; throw error; }
  };
  async function assertCliPreflight() {
    for (const path of Object.values(CLI)) {
      const canonical = await realpath(path);
      assertApprovedCli(path, canonical, metadata(await lstat(canonical)), uid);
    }
    for (const name of ["colima", "lima", "qemu"]) {
      const args = name === "colima" ? ["version"] : ["--version"];
      const observed = await run(name, args);
      check(observed.stderr === "" || name === "qemu", `version_${name}_stderr`);
      assertVersionOutput(name, observed.stdout + observed.stderr);
    }
    const help = await run("colima", ["start", "--help"]);
    for (const flag of ["--arch", "--vm-type", "--mount", "--ssh-agent", "--ssh-config",
      "--activate", "--template", "--binfmt", "--port-forwarder"]) {
      check(help.stdout.includes(flag), "colima_start_option_missing");
    }
  }

  const cleanup = singleFlight(async () => {
    cleaning = true;
    await revalidate();
    if (startAttempted) {
      let usernetDeleteBlocked = usernetIdentityAmbiguous;
      if (usernetExpected) {
        try {
          const artifacts = await captureUsernetArtifacts();
          vmPids.add(artifacts.pid);
          if (usernetArtifactsAuthority) {
            const priorAmbiguity = usernetIdentityAmbiguous;
            usernetIdentityAmbiguous = true;
            usernetIdentityAmbiguous = validateUsernetContinuity(
              usernetArtifactsAuthority, artifacts, priorAmbiguity,
            );
          }
        } catch {
          // The exact private profile still receives stop/force-stop, but an
          // ambiguous helper identity cannot authorize state deletion.
          usernetDeleteBlocked = true;
          usernetIdentityAmbiguous = true;
        }
      }
      let beforeStop = Object.freeze({ deletionBlocked: true, recordedPids: Object.freeze([]) });
      try {
        const prior = await inventory();
        beforeStop = preStopEvidence(prior.colima, prior.lima, profile);
      } catch {
        // Inventory/JSON ambiguity must not prevent a bounded stop of this
        // exact marker-bound private profile, but it can never authorize delete.
      }
      for (const pid of beforeStop.recordedPids) vmPids.add(pid);
      // Even Starting/Installing/partial exact-owned inventory receives a
      // bounded stop attempt. Only the fresh strict stopped proof below can
      // authorize deletion.
      let stopped;
      try {
        stopped = await run("colima", cleanupArguments(profile, "stop"),
          { timeout: 90_000, allowFailure: true });
      } catch {
        stopped = { code: null, signal: "bounded_stop_failed" };
      }
      if (stopped.code !== 0 || stopped.signal !== null) {
        await run("colima", cleanupArguments(profile, "force-stop"), { timeout: 60_000 });
      }
      const postStop = await inventory();
      cleanupInventoryProof(postStop.colima, postStop.lima, profile, { requireStopped: true });
      check(!beforeStop.deletionBlocked, "prestop_inventory_ambiguous");
      assertRecordedPidsAbsent([...vmPids], pidExists);
      check(!usernetDeleteBlocked && !usernetIdentityAmbiguous, "usernet_identity_ambiguous");
      await revalidate();
      await run("colima", cleanupArguments(profile, "delete"), { timeout: 120_000 });
    }
    const final = await inventory();
    assertEmptyInventories(final.colima, final.lima);
    assertRecordedPidsAbsent([...vmPids], pidExists);
    let socketAbsent = false;
    try { await lstat(join(env.COLIMA_HOME, profile, "docker.sock")); }
    catch (error) { if (error.code === "ENOENT") socketAbsent = true; else throw error; }
    check(socketAbsent, "owned_docker_socket_still_present");
    cleanupProven = true;
  });

  const overallDuration = qualificationDuration(options);
  const overallDeadline = Date.now() + overallDuration;
  overall = setTimeout(stop, overallDuration);
  try {
    if (options.artifact) {
      artifactApproval = JSON.parse((await privateFile(options.artifact.approvalPath, uid, 16384)).toString("utf8"));
      validateReaderApproval(artifactApproval, sha256(await readFile(ARTIFACT_HELPER)));
      artifactParent = await artifactControl("host-check", artifactRequest());
    }
    assertUnixSocketBudget([
      join(env.COLIMA_HOME, profile, "docker.sock"),
      join(env.LIMA_HOME, `colima-${profile}`, "ssh.sock.1234567890123456"),
    ]);
    for (const path of [env.COLIMA_HOME, env.COLIMA_CACHE_HOME, env.LIMA_HOME,
      env.DOCKER_CONFIG, env.TMPDIR]) {
      await revalidate();
      await mkdir(path, { mode: 0o700 });
    }
    await revalidate();
    await mkdir(limaPolicyDirectory, { mode: 0o700 });
    const policyDirectory = await lstat(limaPolicyDirectory);
    await writeFile(limaPolicyFile, LIMA_PORT_FORWARD_OVERRIDE, { flag: "wx", mode: 0o600 });
    const policyFile = await lstat(limaPolicyFile);
    assertLimaOverrideIdentity(metadata(policyDirectory), metadata(policyFile), uid);
    limaPolicyDirectoryIdentity = identity(policyDirectory);
    limaPolicyFileIdentity = identity(policyFile);
    assertLimaPortForwardOverride(await privateFile(limaPolicyFile, uid,
      LIMA_PORT_FORWARD_OVERRIDE.length));
    limaPolicyEstablished = true;
    await revalidate();
    await assertCliPreflight();
    const limaCanonical = await realpath(CLI.lima);
    check(limaCanonical === KNOWN_LIMA_BINARY, "known_lima_binary_path");
    const limaExecutableIdentity = executableIdentitySha256(
      limaCanonical,
      kernelMetadata(await lstat(limaCanonical, { bigint: true })),
    );
    stage = "source_snapshot";
    const baseCommit = (await run("git", ["rev-parse", "HEAD"])).stdout.trim();
    requireExpectedSourceCommit(baseCommit, expectedSourceCommit);
    check(sha256(await readFile(SELF)) === harnessSha256
      && sha256(await readFile(TEST)) === testSha256
      && sha256(await readFile(HOST_PROCESS_PRODUCER)) === producerSha256,
    "harness_source_changed");
    const initial = await inventory();
    assertEmptyInventories(initial.colima, initial.lima);
    const baseline = await retryListenerObservation(() => bracketedListenerSnapshot());
    const processBaseline = baseline.processes;
    const listenerBaseline = baseline.listeners;
    const minimumUsernetBirthMicros = (BigInt(Date.now()) * 1_000n).toString();

    stage = "vm_start";
    startAttempted = true;
    usernetExpected = true;
    if (mode === "image") emitImageProgress("vm_starting");
    await run("colima", startArguments(profile), { timeout: 600_000, maximum: 4 * 1024 * 1024 });
    stage = "vm_identity";
    const live = await inventory();
    for (const pid of cleanupInventoryProof(live.colima, live.lima, profile)) vmPids.add(pid);
    const lima = live.lima[0];
    assertVmConfiguration(await readFile(join(env.COLIMA_HOME, profile, "colima.yaml"), "utf8"));
    const guest = (args, options) => run("colima", ["ssh", "--profile", profile, "--", ...args], options);
    const guestArchitecture = (await guest(["uname", "-m"])).stdout.trim();
    check(guestArchitecture === "x86_64", "guest_not_x86_64");
    check((await guest(["getconf", "_NPROCESSORS_ONLN"])).stdout.trim() === "4", "guest_cpu_bound");
    const memory = /^MemTotal:\s+([0-9]+) kB$/m.exec((await guest(["cat", "/proc/meminfo"])).stdout);
    const guestMemoryBytes = memory ? Number(memory[1]) * 1024 : 0;
    check(Number.isSafeInteger(guestMemoryBytes) && guestMemoryBytes >= 5 * GIB
      && guestMemoryBytes <= 6 * GIB, "guest_memory_bound");
    const mounts = (await guest(["cat", "/proc/mounts"])).stdout;
    check(!/\s(?:virtiofs|9p|fuse\.sshfs|sshfs)\s/.test(mounts), "guest_host_mount_detected");
    const disks = (await guest(["df", "-Pk", "/", "/var/lib/docker"])).stdout.trim()
      .split("\n").slice(1).map((line) => line.trim().split(/\s+/));
    check(disks.length === 2 && disks[0][0] !== disks[1][0], "guest_disk_topology");
    const diskBytes = disks.map((row) => Number(row[1]) * 1024);
    for (const [index, maximum] of [8, 16].entries()) {
      check(Number.isSafeInteger(diskBytes[index]) && diskBytes[index] > (maximum - 2) * GIB
        && diskBytes[index] <= maximum * GIB, "guest_disk_bound");
    }
    const engine = JSON.parse((await guest(["docker", "info", "--format", "{{json .}}"])).stdout);
    check(engine.OSType === "linux" && engine.Architecture === "x86_64" && engine.NCPU === 4
      && engine.MemTotal >= 5 * GIB && engine.MemTotal <= 6 * GIB,
    "engine_architecture_or_resources");
    const managementPort = qemuManagementPort(lima);

    let buildx;
    if (mode === "reader") {
      buildx = { status: "not_requested" }; // never published as compatibility/image qualification
    } else {
      stage = "buildx_observation";
      const buildxResult = await guest(["docker", "buildx", "version"], { allowFailure: true });
      if (buildxResult.code === 0 && buildxResult.signal === null) {
        const match = /^github\.com\/docker\/buildx\s+(v[0-9]+\.[0-9]+\.[0-9]+)(?:\s|$)/
          .exec(buildxResult.stdout.trim());
        check(match && buildxResult.stderr === "", "buildx_version_unknown");
        buildx = { status: "available", version: match[1] };
      } else {
        const diagnostic = `${buildxResult.stdout}\n${buildxResult.stderr}`;
        check(buildxResult.code === 1 && buildxResult.signal === null
          && /(?:is not a docker command|unknown command:?\s+["']?buildx)/i.test(diagnostic),
        "buildx_failure_ambiguous");
        buildx = { status: "unsupported_known" };
      }
    }

    stage = "usernet_identity";
    const priorUsernetAmbiguity = usernetIdentityAmbiguous;
    usernetIdentityAmbiguous = true;
    const usernetArtifacts = await captureUsernetArtifacts();
    usernetArtifactsAuthority = usernetArtifacts;
    vmPids.add(usernetArtifacts.pid);
    const initialUsernetBefore = await hostProcessSnapshot([usernetArtifacts.pid]);
    await revalidateUsernetArtifacts(usernetArtifacts);
    const initialUsernetAfter = await hostProcessSnapshot([usernetArtifacts.pid]);
    const usernetHelper = bindUsernetHelper({
      baseline: processBaseline,
      before: initialUsernetBefore,
      after: initialUsernetAfter,
      pid: usernetArtifacts.pid,
      uid,
      limaExecutableIdentitySha256: limaExecutableIdentity,
      minimumBirthMicros: minimumUsernetBirthMicros,
    });
    usernetIdentityAmbiguous = priorUsernetAmbiguity;
    stage = "listener_observation";
    const observeCurrentListeners = async () => {
      const captured = await retryListenerObservation(async () => {
      commandBudget(30_000); // exhaustion cannot start a new retry
      const listenerBefore = await listenerSnapshot();
      const processBeforeListeners = await hostProcessSnapshot(
        listenerSnapshotPids(listenerBefore, [...vmPids]),
      );
      const listenerAfter = await listenerSnapshot();
      assertListenerSnapshotsEqual(listenerBefore, listenerAfter);
      const priorListenerUsernetAmbiguity = usernetIdentityAmbiguous;
      usernetIdentityAmbiguous = true;
      await revalidateUsernetArtifacts(usernetArtifacts);
      const processAfterListeners = await hostProcessSnapshot(
        listenerSnapshotPids(listenerAfter, [...vmPids]),
      );
      const observedUsernetHelper = bindUsernetHelper({
        baseline: processBaseline,
        before: processBeforeListeners,
        after: processAfterListeners,
        pid: usernetArtifacts.pid,
        uid,
        limaExecutableIdentitySha256: limaExecutableIdentity,
        minimumBirthMicros: minimumUsernetBirthMicros,
      });
      check(canonicalJson(observedUsernetHelper) === canonicalJson(usernetHelper),
        "usernet_helper_identity_changed");
      usernetIdentityAmbiguous = priorListenerUsernetAmbiguity;
      assertListenerSnapshotsEqual(listenerAfter, await listenerSnapshot());
      return { listeners: listenerAfter, processBeforeListeners, processAfterListeners };
    });
    const listenerAfter = captured.listeners;
    const processBeforeListeners = captured.processBeforeListeners;
    const processAfterListeners = captured.processAfterListeners;
    const listenerInput = {
      tcpBefore: listenerBaseline.tcp,
      tcpAfter: listenerAfter.tcp,
      udpBefore: listenerBaseline.udp,
      udpAfter: listenerAfter.udp,
    };
    listenerFailureEvidence = listenerDeltaFailureEvidence(
      listenerInput,
      [...vmPids],
      usernetHelper.pid,
      { baselineProcesses: processBaseline, controllerPid: process.pid, parentPid: process.ppid,
        postProcessesAfter: processAfterListeners, postProcessesBefore: processBeforeListeners,
        usernetBinding: { artifactIdentitySha256: usernetArtifacts.artifactIdentitySha256,
          birthIdentitySha256: usernetHelper.birthIdentitySha256,
          executableIdentitySha256: usernetHelper.executableIdentitySha256, pid: usernetHelper.pid } },
    );
    processEvidence = hostProcessEvidence({
      artifactIdentitySha256: usernetArtifacts.artifactIdentitySha256,
      baselineSnapshotSha256: processBaseline.rawSha256,
      birthIdentitySha256: usernetHelper.birthIdentitySha256,
      boundaryAfterSnapshotSha256: processAfterListeners.rawSha256,
      boundaryBeforeSnapshotSha256: processBeforeListeners.rawSha256,
      executableIdentitySha256: usernetHelper.executableIdentitySha256,
      producerSha256,
    }, listenerFailureEvidence);
    const listenerEvidence = listenerDeltaPolicy(
      listenerInput,
      [...vmPids],
      managementPort,
      usernetHelper.pid,
      {
        baselineProcesses: processBaseline,
        controllerPid: process.pid,
        parentPid: process.ppid,
        postProcessesAfter: processAfterListeners,
        postProcessesBefore: processBeforeListeners,
        usernetBinding: {
          artifactIdentitySha256: usernetArtifacts.artifactIdentitySha256,
          birthIdentitySha256: usernetHelper.birthIdentitySha256,
          executableIdentitySha256: usernetHelper.executableIdentitySha256,
          pid: usernetHelper.pid,
        },
      },
    );
      commandBudget(1); // complete evidence arriving after the phase deadline is not a pass
      listenerFailureEvidence = null;
      return listenerEvidence;
    };
    let listenerEvidence = await observeCurrentListeners();
    stage = "host_agent_log_observation";
    const hostAgentLog = await boundedGeneratedFile(
      join(env.LIMA_HOME, `colima-${profile}`, "ha.stderr.log"), uid, MAX_HOST_AGENT_LOG);
    assertNoAutomaticForwardAttempts(hostAgentLog);

    if (mode === "image") {
      stage = "runner_image_workload";
      emitImageProgress("vm_isolation_checked");
      check(buildx.status === "available" && buildx.version === "v0.34.1",
        "image_buildx_version_required");
      check(await imageToolsetSha256() === imageToolSourcesSha256,
        "image_tool_source_changed");
      const epoch = (await run("git", ["show", "-s", "--format=%ct", baseCommit])).stdout.trim();
      check(/^[1-9][0-9]{8,11}$/.test(epoch), "image_source_epoch_invalid");
      const scratchRoot = join(env.TMPDIR, "image-workload");
      await revalidate();
      await mkdir(scratchRoot, { mode: 0o700 });
      const { runManagedRunnerImageWorkload, validateQualification } = await import("./managed-runner-image-workload.mjs");
      try {
        imageEvidence = await runManagedRunnerImageWorkload({
          guest: async (args, options) => (await guest(args, options)).stdout,
          host: async (tool, args, options) => {
            check(["git", "tar"].includes(tool), "image_host_tool_scope");
            return (await run(tool, args, options)).stdout;
          },
          diagnostic: async (combinedBuildLogTail) => {
            check(typeof combinedBuildLogTail === "string"
              && Buffer.byteLength(combinedBuildLogTail) <= MAX_DIAGNOSTIC,
            "image_diagnostic_bound");
            await writeDiagnostics("colima", Buffer.from(
              "Bounded private image-workload diagnostic; the source label follows.\n"),
            Buffer.from(combinedBuildLogTail));
          },
          progress: emitImageProgress,
          scratchRoot,
          sourceCommit: expectedSourceCommit,
          buildEpoch: Number(epoch),
          preservation: options.artifact ? { approval: artifactApproval, exportAttachments, exportFailedArchive,
            onPhase: workloadPhase,
            beforeBuild: async () => {
              workloadCleanupPending = true;
              commandBudget(30_000);
              const current = await artifactControl("host-check", artifactRequest());
              check(canonicalJson(current) === canonicalJson(artifactParent), "artifact_parent_changed");
              commandBudget(1);
            } } : null,
        });
        validateQualification(imageEvidence, expectedSourceCommit);
      } catch (error) {
        if (!failedArchivePending) throw error;
        deferredImageFailure = error;
      }
    } else if (mode === "reader") {
      stage = "reader_runtime_observation";
      check(await imageToolsetSha256() === imageToolSourcesSha256, "image_tool_source_changed");
      const before = await lstat(ARTIFACT_HELPER);
      check(before.isFile() && !before.isSymbolicLink() && before.uid === uid && before.nlink === 1
        && (before.mode & 0o022) === 0 && before.size > 0 && before.size <= 128 * 1024,
      "reader_observation_source_identity");
      const readerSource = await readFile(ARTIFACT_HELPER);
      readerSourceSha256 = sha256(readerSource);
      const archived = await run("tar", readerArchiveArguments(dirname(ARTIFACT_HELPER)),
        { binaryStdout: true, maximum: 1024 * 1024, readerOperation: "reader_archive_create" });
      check(archived.stderr === "", "reader_observation_archive_stderr");
      const after = await lstat(ARTIFACT_HELPER);
      check(identity(before) === identity(after) && before.size === after.size
        && before.uid === after.uid && before.mode === after.mode && after.nlink === 1
        && before.mtimeMs === after.mtimeMs && before.ctimeMs === after.ctimeMs
        && sha256(await readFile(ARTIFACT_HELPER)) === readerSourceSha256, "reader_observation_source_changed");
      validateReaderArchive(archived.stdout, readerSource);
      readerObservation = await observeGuestReaderRuntime(async (args, settings, operation) => {
        const observed = await guest(args, { ...settings, readerOperation: operation });
        check(observed.stderr === "", "reader_observation_stderr");
        return observed.stdout;
      }, archived.stdout, readerSourceSha256);
    }
    if (mode === "image" || mode === "reader") {
      if (budgeted) {
        budgetPhase = "postObservation";
        phaseDeadline = Math.min(Date.now() + ARTIFACT_POST_OBSERVATION_MS,
          overallDeadline - ARTIFACT_CLEANUP_RESERVE_MS - ARTIFACT_SOURCE_FINAL_MS);
        commandBudget(1);
      }
      stage = "post_image_listener_observation";
      listenerEvidence = await observeCurrentListeners();
      stage = "post_image_host_agent_log_observation";
      assertNoAutomaticForwardAttempts(await boundedGeneratedFile(
        join(env.LIMA_HOME, `colima-${profile}`, "ha.stderr.log"), uid, MAX_HOST_AGENT_LOG));
      check(await imageToolsetSha256() === imageToolSourcesSha256,
        "image_tool_source_changed");
      commandBudget(1);
    }

    stage = "source_final";
    if (budgeted) {
      budgetPhase = "sourceFinal";
      phaseDeadline = Math.min(Date.now() + ARTIFACT_SOURCE_FINAL_MS,
        overallDeadline - ARTIFACT_CLEANUP_RESERVE_MS);
    }
    requireExpectedSourceCommit((await run("git", ["rev-parse", "HEAD"])).stdout.trim(), expectedSourceCommit);
    check(sha256(await readFile(SELF)) === harnessSha256
      && sha256(await readFile(TEST)) === testSha256
      && sha256(await readFile(HOST_PROCESS_PRODUCER)) === producerSha256,
    "harness_source_changed");
    if (deferredImageFailure) {
      failedArchivePostChecks = true;
      throw deferredImageFailure;
    }
    result = {
      baseCommit,
      buildx,
      claims: { api: false, image: false, productionAuthority: false, runnerStartup: false },
      cleanup: undefined,
      emulated: true,
      harnessSha256,
      hostProcessEvidence: processEvidence,
      identities: {
        engineArchitecture: engine.Architecture,
        engineOs: engine.OSType,
        guestArchitecture,
        limaVersion: lima.limaVersion ?? lima.LimaVersion,
        vmType: lima.vmType ?? lima.VMType,
      },
      listenerEvidence,
      observations: {
        dataDiskBytes: diskBytes[1],
        engineCpuCount: engine.NCPU,
        engineMemoryBytes: engine.MemTotal,
        guestMemoryBytes,
        rootDiskBytes: diskBytes[0],
      },
      qualification: "isolated_linux_amd64_vm_compatibility_only",
      portForwardPolicySha256: sha256(LIMA_PORT_FORWARD_OVERRIDE),
      resources: { cpus: 4, memoryGiB: 6, rootDiskGiB: 8, dataDiskGiB: 16, freeBytes },
      testSha256,
      toolVersions: { ...KNOWN_VERSIONS },
      version: 1,
    };
  } catch (error) {
    primaryFailure = codeOf(deferredImageFailure ?? error, "closed_stage_failure");
  } finally {
    cleaning = true;
    if (budgeted) {
      budgetPhase = "outerCleanup";
      // Keep the full outer cleanup envelope, including its shared local-I/O
      // margin. Do not impose a shorter stop/delete phase or reset the clock.
      phaseDeadline = overallDeadline - 70_000;
    }
    try { await cleanup(); }
    catch { cleanupFailure = "cleanup_unproven_state_retained"; }
    if (cleanupProven && !cleanupFailure && !interrupted
        && ((!primaryFailure && result) || (failedArchivePending && failedArchivePostChecks))) {
      stage = "source_after_cleanup";
      if (budgeted) {
        cleaning = false;
        budgetPhase = "sourceAfter";
        phaseDeadline = Math.min(Date.now() + 35_000, overallDeadline - 155_000);
      }
      try {
        requireExpectedSourceCommit((await run("git", ["rev-parse", "HEAD"])).stdout.trim(), expectedSourceCommit);
        check(sha256(await readFile(SELF)) === harnessSha256
          && sha256(await readFile(TEST)) === testSha256
          && sha256(await readFile(HOST_PROCESS_PRODUCER)) === producerSha256,
        "harness_source_changed");
        if (mode === "image" || mode === "reader") {
          check(await imageToolsetSha256() === imageToolSourcesSha256,
            "image_tool_source_changed");
        }
        if (failedArchivePending) failedArchiveSourceAfter = true;
      } catch (error) {
        primaryFailure ??= codeOf(error, "closed_source_final_failure");
      }
    }
    if (!budgeted) {
      clearTimeout(overall);
      for (const signal of signals) process.off(signal, stop);
    }
    try {
      if (cleanupProven && (primaryFailure || interrupted)) {
        const names = await readdir(stateRoot);
        const removal = diagnosticRetentionPlan(names);
        for (const name of removal) {
          await revalidate();
          const path = join(stateRoot, name);
          const info = await lstat(path);
          check(info.isDirectory() && !info.isSymbolicLink() && info.uid === uid,
            "diagnostic_cleanup_child_identity");
          if (name === "l") limaPolicyEstablished = false;
          await rm(path, { recursive: true, force: false });
          if (name === "l") await assertUsernetSocketPathsAbsent();
        }
        const remaining = await readdir(stateRoot);
        check(diagnosticRetentionPlan(remaining).length === 0, "diagnostic_cleanup_incomplete");
        await revalidate();
        retainedDiagnosticsOnly = true;
      } else if (cleanupProven && !cleanupFailure) {
        await revalidate();
        limaPolicyEstablished = false;
        await rm(stateRoot, { recursive: true, force: false });
        let absent = false;
        try { await lstat(stateRoot); }
        catch (error) { if (error.code === "ENOENT") absent = true; else throw error; }
        check(absent, "private_state_root_not_removed");
        await assertUsernetSocketPathsAbsent();
        retained = false;
      }
    } catch {
      cleanupFailure = "cleanup_unproven_state_retained";
      retained = true;
    }
  }

  if (primaryFailure || cleanupFailure || interrupted || !result || retained) {
    if (archiveRetention && failedArchiveFinalizationAllowed({ pending: Boolean(failedArchivePending),
      postChecks: failedArchivePostChecks, cleanupProven, cleanupFailure,
      sourceUnchanged: failedArchiveSourceAfter, interrupted })) {
      try {
        cleaning = false;
        budgetPhase = "qualify";
        phaseDeadline = overallDeadline;
        await assertFailedArchiveTools();
        const saved = await artifactControl("host-failure-manifest", artifactRequest({
          binding: failedArchiveBinding, received: failedArchivePending.received,
          checks: { postObservation: true, sourceUnchanged: true, normalCleanup: true },
        }));
        check(saved.name === "failure-retention-manifest.json" && /^(?!0{64}$)[a-f0-9]{64}$/.test(saved.sha256),
          "failed_archive_manifest_reply");
        archiveRetention = { ...archiveRetention, state: "retained", manifestSha256: saved.sha256 };
      } catch { archiveRetention.retentionFailure = "finalization_failed"; }
    } else if (archiveRetention && archiveRetention.retentionFailure === null) {
      archiveRetention.retentionFailure = cleanupFailure || !cleanupProven ? "cleanup_failed"
        : !failedArchivePostChecks ? "post_checks_failed" : "source_failed";
    }
    clearTimeout(overall);
    for (const signal of signals) process.off(signal, stop);
    console.log(JSON.stringify(failureReport({
      stage: deferredImageFailure ? "runner_image_workload" : stage,
      primaryFailure, cleanupFailure, interrupted, retained, stateRoot, profile,
      cleanupProven, retainedDiagnosticsOnly, diagnostics, listenerFailureEvidence,
      hostProcessEvidence: processEvidence,
      archiveRetention,
    })));
    throw new Error("nonprod_managed_runner_vm_not_qualified");
  }

  result.cleanup = {
    privateInventoriesEmpty: true,
    ownedVmPidsAbsent: true,
    socketAbsent: true,
    stateAbsent: true,
    usernetSocketsAbsent,
  };
  if (mode === "reader") {
    try {
      check(!interrupted && Date.now() < overallDeadline, "reader_observation_deadline");
      const observedReport = readerRuntimeObservationReport({ sourceCommit: expectedSourceCommit,
        harnessSha256, toolSourcesSha256: imageToolSourcesSha256,
        observation: readerObservation, cleanup: result.cleanup }, readerSourceSha256, expectedSourceCommit);
      const bytes = Buffer.from(`${JSON.stringify(observedReport)}\n`, "utf8");
      check(bytes.length <= 64 * 1024, "reader_observation_report_bound");
      process.stdout.write(bytes);
    } finally {
      clearTimeout(overall);
      for (const signal of signals) process.off(signal, stop);
    }
    return;
  }
  const report = reportMode(result, expectedSourceCommit);
  if (mode === "image") {
    check(imageEvidence?.qualification === "isolated_measured_runner_image_only"
      && imageEvidence.sourceCommit === expectedSourceCommit
      && imageEvidence.cleanup?.containerRemoved === true
      && imageEvidence.cleanup?.builderRemoved === true,
    "image_workload_report_incomplete");
    const fullReport = {
      event: "managed_runner_vm_image_smoke_finished",
      version: 1,
      qualification: "isolated_measured_runner_image_only",
      compatibility: report,
      image: imageEvidence,
      imageToolSourcesSha256,
      claims: { normalRunnerStartup: false, managedAuthority: false,
        submission: false, production: false },
    };
    try {
      await emitImageReport(fullReport, options.artifact ? async (imageReportSha256) => {
        cleaning = false;
        budgetPhase = "qualify";
        phaseDeadline = overallDeadline;
        check(artifactBinding && artifactManifest && !interrupted && Date.now() < overallDeadline,
          "artifact_qualification_unavailable");
        await artifactControl("host-qualify", artifactRequest({ binding: artifactBinding,
          manifestSha256: artifactManifest.sha256,
          qualification: { sourceCommit: expectedSourceCommit,
            imageReportSha256, harnessSha256,
            toolSourcesSha256: imageToolSourcesSha256,
            postListenerChecked: true, sourceUnchanged: true, cleanup: result.cleanup } }));
      } : null);
    } finally {
      if (options.artifact) {
        clearTimeout(overall);
        for (const signal of signals) process.off(signal, stop);
      }
    }
  } else {
    console.log(JSON.stringify({ event: "managed_runner_vm_compatibility_finished", ...report }));
  }
}

if (process.argv[1] && resolve(process.argv[1]) === SELF) {
  await main().catch((error) => {
    if (error.message !== "nonprod_managed_runner_vm_not_qualified") {
      console.log(JSON.stringify({ event: "managed_runner_vm_preflight_failed", version: 1,
        code: codeOf(error) }));
    }
    process.exitCode = 1;
  });
}
