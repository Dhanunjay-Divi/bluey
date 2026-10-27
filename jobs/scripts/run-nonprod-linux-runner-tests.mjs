#!/usr/bin/env node
// Disposable Jobs-only construction/addon smoke, not a signed release or API run.
// Colima isolation: https://colima.run/docs/profiles/
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { lstat, mkdir, mkdtemp, readFile, readdir, realpath, rm, statfs, writeFile } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const GIB = 1024 ** 3;
export const CONSTRUCTION_BUILDER = "legacy-explicit";
const CLI = Object.freeze({ colima: "/opt/homebrew/bin/colima", docker: "/opt/homebrew/bin/docker",
  lima: "/opt/homebrew/bin/limactl", git: "/usr/bin/git" });
const PRODUCT = "jobs";
const OWNER_MARKER = ".phase634-owner.json";
const MAX_FAILURE_OUTPUT = 64 * 1024;
const MAX_FAILURE_COMMANDS = 8;
const MAX_FAILURE_DIAGNOSTICS = MAX_FAILURE_COMMANDS * 2;
const SELF = fileURLToPath(import.meta.url);
const REPO = resolve(dirname(SELF), "../..");
const check = (condition, code) => { if (!condition) throw new Error(code); };
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const inside = (parent, child) => child.startsWith(parent + "/");

export function boundedDiagnosticTail(previous, chunk) {
  check(Buffer.isBuffer(previous) && Buffer.isBuffer(chunk), "diagnostic_bytes_required");
  if (chunk.length >= MAX_FAILURE_OUTPUT) return Buffer.from(chunk.subarray(-MAX_FAILURE_OUTPUT));
  const keep = Math.min(previous.length, MAX_FAILURE_OUTPUT - chunk.length);
  return Buffer.concat([previous.subarray(previous.length - keep), chunk]);
}
function closedFailure(value, fallback) {
  return typeof value === "string" && /^[a-zA-Z0-9_]{1,80}$/.test(value) ? value : fallback;
}
export function failureReport({ stage, primaryFailure, cleanupFailure, interrupted, retained,
  stateRoot, profile, cleanupProven = false, retainedDiagnosticsOnly = false,
  diagnostics = [], diagnosticWriteFailed = false }) {
  check(Array.isArray(diagnostics) && diagnostics.length <= MAX_FAILURE_DIAGNOSTICS,
    "diagnostic_count_bound");
  const files = diagnostics.map((entry) => {
    check(/^command-failure-000[1-8]\.(?:stdout|stderr)$/.test(entry.file)
      && Object.hasOwn(CLI, entry.tool) && Number.isInteger(entry.bytes)
      && entry.bytes > 0 && entry.bytes <= MAX_FAILURE_OUTPUT, "diagnostic_report_shape");
    return { file: entry.file, tool: entry.tool, stage: closedFailure(entry.stage, "unknown_stage"), bytes: entry.bytes };
  });
  const primary = primaryFailure ? closedFailure(primaryFailure, "closed_stage_failure") : null;
  const cleanup = cleanupFailure ? closedFailure(cleanupFailure, "cleanup_unproven_state_retained") : null;
  return { event: "nonprod_linux_runner_failed", stage: closedFailure(stage, "unknown_stage"),
    code: primary || cleanup || (interrupted ? "interrupted" : "qualification_incomplete"),
    primaryFailure: primary, cleanupFailure: cleanup, cleanupProven,
    retainedDiagnosticsOnly: retained && cleanupProven && retainedDiagnosticsOnly,
    diagnostics: files, diagnosticWriteFailed,
    ...(retained ? { retainedState: stateRoot, profile } : {}) };
}
export function assertUnixSocketBudget({ dockerSocket, limaSshSocket }) {
  for (const path of [dockerSocket, limaSshSocket]) {
    // Darwin sockaddr_un.sun_path is104bytes including the terminating NUL.
    check(typeof path === "string" && path.startsWith("/") && !path.includes("\0")
      && Buffer.byteLength(path, "utf8") <= 103, "unix_socket_path_budget");
  }
}
export function diagnosticRetentionPlan(names) {
  check(Array.isArray(names) && names.length <= MAX_FAILURE_DIAGNOSTICS + 9
    && new Set(names).size === names.length && names.includes(OWNER_MARKER), "diagnostic_retention_shape");
  const removable = ["colima", "l", "lima", "cache", "context", "docker", "tmp", "image.id"];
  const diagnostics = names.filter((name) => /^command-failure-000[1-8]\.(?:stdout|stderr)$/.test(name));
  check(diagnostics.length <= MAX_FAILURE_DIAGNOSTICS && names.every((name) => name === OWNER_MARKER
    || removable.includes(name) || diagnostics.includes(name)), "diagnostic_retention_unknown_entry");
  return removable.filter((name) => names.includes(name));
}

export function assertResources({ cpus, memoryGiB, rootDiskGiB, dataDiskGiB, freeBytes }) {
  check(cpus === 4 && memoryGiB === 6 && rootDiskGiB === 8 && dataDiskGiB === 16,
    "resource_bounds");
  check(Number.isFinite(freeBytes) && freeBytes >= 32 * GIB, "host_capacity");
}
export function assertPrivateRoot(path, canonical, info, uid, kind) {
  check(path === canonical && path === resolve(path), "root_canonical");
  check(info.directory && !info.symlink && info.uid === uid && (info.mode & 0o7777) === 0o700,
    "root_private_identity");
  if (kind === "state") {
    check(/^\/(?:private\/)?tmp\/bluey-jobs-linux-p634-[A-Za-z0-9]{6}$/.test(path), "state_root_scope");
  } else {
    check(/^\/(?:private\/)?tmp\/bluey-test-runs-[^/]+\/run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$/.test(path),
      "launcher_root_scope");
  }
}
export function isolatedEnvironment(stateRoot, canonicalRoot, home) {
  check(!inside(canonicalRoot, stateRoot) && stateRoot !== canonicalRoot, "vm_state_must_be_separate");
  check(inside(canonicalRoot, home), "private_home_required");
  return Object.freeze({
    PATH: "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin", HOME: home,
    LANG: "C", LC_ALL: "C", TZ: "UTC", TMPDIR: join(stateRoot, "tmp"),
    TMP: join(stateRoot, "tmp"), TEMP: join(stateRoot, "tmp"),
    COLIMA_HOME: join(stateRoot, "colima"), COLIMA_CACHE_HOME: join(stateRoot, "cache"),
    LIMA_HOME: join(stateRoot, "l"), DOCKER_CONFIG: join(stateRoot, "docker"),
    DOCKER_BUILDKIT: "0", DOCKER_SCAN_SUGGEST: "false",
  });
}
function assertProfile(profile) { check(/^p634-[a-f0-9]{20}$/.test(profile), "owned_profile_required"); }
export function startArguments(profile) {
  assertProfile(profile);
  return ["start", "--profile", profile, "--activate=false", "--template=false", "--runtime", "docker",
    "--arch", "aarch64", "--vm-type", "vz", "--cpus", "4", "--memory", "6", "--root-disk", "8",
    "--disk", "16", "--mount", "none", "--ssh-agent=false", "--ssh-config=false",
    "--binfmt=false", "--vz-rosetta=false", "--port-forwarder", "none"];
}
export function cleanupArguments(profile, operation) {
  assertProfile(profile);
  if (operation === "stop") return ["stop", "--profile", profile];
  if (operation === "force-stop") return ["stop", "--profile", profile, "--force"];
  check(operation === "delete", "cleanup_operation");
  return ["delete", "--profile", profile, "--force", "--data"];
}
export function assertEmptyInventories(colima, lima) {
  check(Array.isArray(colima) && Array.isArray(lima) && colima.length === 0 && lima.length === 0,
    "private_vm_inventory_not_empty");
}
function limaPidFields(row) {
  let host = Object.hasOwn(row, "hostAgentPID") ? row.hostAgentPID : row.HostAgentPID;
  let driver = Object.hasOwn(row, "driverPID") ? row.driverPID : row.DriverPID;
  const documentedProtocol = row.limaVersion === "2.2.1" && row.vmType === "vz" && row.arch === "aarch64"
    && ["Running", "Stopped"].includes(row.status);
  const noErrors = row.errors === undefined || (Array.isArray(row.errors) && row.errors.length === 0);
  if (documentedProtocol) {
    check(noErrors && (row.status !== "Running" || (Number.isInteger(host) && host > 1)),
      "lima_known_protocol_live_host_pid_required");
  }
  if (host === undefined || driver === undefined) {
    // Lima2.2.1 Instance.MarshalJSON preserves json:"...,omitempty" on these
    // integer fields. Missing means zero ONLY for this exact reviewed protocol:
    // https://github.com/lima-vm/lima/blob/v2.2.1/pkg/limatype/lima_instance.go
    check(documentedProtocol && noErrors, "lima_pid_omission_protocol_unsupported");
    if (host === undefined) host = 0;
    if (driver === undefined) driver = 0;
  }
  return [host, driver];
}
export function ownedLimaPids(row, profile, { allowStopped = false } = {}) {
  assertProfile(profile);
  check((row?.name ?? row?.Name) === `colima-${profile}`, "foreign_lima_instance");
  const fields = limaPidFields(row);
  const errors = row.errors ?? row.Errors;
  const knownStopped = allowStopped && ["Stopped", "stopped"].includes(row.status ?? row.Status)
    && fields.every((pid) => pid === 0) && (errors === undefined || (Array.isArray(errors) && errors.length === 0));
  check(fields.every((pid) => Number.isInteger(pid) && pid >= 0 && pid <= 2_147_483_647)
    && (fields.some((pid) => pid > 1) || knownStopped), "lima_pid_observation_unsupported");
  return [...new Set(fields.filter((pid) => pid > 1))];
}
export function cleanupInventoryProof(colima, lima, profile, { requireStopped = false } = {}) {
  assertProfile(profile);
  check(Array.isArray(colima) && colima.length === 1 && Array.isArray(lima) && lima.length === 1
    && (colima[0]?.name ?? colima[0]?.Name) === profile
    && (lima[0]?.name ?? lima[0]?.Name) === `colima-${profile}`, "complete_owned_vm_inventory_required");
  const states = [colima[0].status ?? colima[0].Status, lima[0].status ?? lima[0].Status];
  check(states.every((state) => ["Running", "running", "Starting", "starting", "Stopped", "stopped"].includes(state)),
    "owned_vm_status_unknown");
  const errors = lima[0].errors ?? lima[0].Errors;
  check(errors === undefined || (Array.isArray(errors) && errors.length === 0), "owned_vm_status_errors");
  const pids = ownedLimaPids(lima[0], profile, { allowStopped: true });
  if (states.some((state) => ["Running", "running", "Starting", "starting"].includes(state))) {
    check(pids.length > 0, "live_owned_vm_pid_required");
  }
  if (requireStopped) {
    check(states.every((state) => ["Stopped", "stopped"].includes(state)) && pids.length === 0,
      "owned_vm_not_proven_stopped_before_delete");
  }
  return pids;
}
export function assertRecordedPidsAbsent(pids, exists) {
  check(Array.isArray(pids) && pids.length <= 8 && pids.every((pid) => Number.isInteger(pid)
    && pid > 1 && pid <= 2_147_483_647), "recorded_vm_pids_invalid");
  for (const pid of pids) {
    let absent = false;
    try { absent = exists(pid) === false; } catch { /* Inspection ambiguity retains state. */ }
    check(absent, "owned_vm_pid_still_present_or_ambiguous");
  }
}
export function assertRuntimeMeasurement(value, sourceCommit, buildId) {
  check(value?.version === 1 && value.audience === "bluey-jobs-managed-cloud-runtime-measurement-v1"
    && value.sourceCommit === sourceCommit && value.buildId === buildId && value.componentId === "jobs-runner"
    && JSON.stringify(value.roles) === '["managed_runner"]'
    && ["configSchemaSha256", "migrationSetSha256", "protocolSetSha256"]
      .every((field) => value[field] === "0".repeat(64)), "runtime_measurement_identity");
}
export function admittedContextPath(path) {
  if (typeof path !== "string" || path.includes("\\") || path.includes("\0")
    || path.split("/").some((part) => part === ".." || part === "")) return false;
  if (path !== "jobs/.dockerignore" && path.split("/").some((part) => part.startsWith("."))) return false;
  if (["jobs/.dockerignore", "jobs/package.json", "jobs/package-lock.json", "jobs/runner/Dockerfile",
    "jobs/scripts/managed-cloud-release-gate.mjs", "jobs/browser/package.json", "jobs/portal/package.json",
    "jobs/runner/native-storage/Cargo.toml", "jobs/runner/native-storage/Cargo.lock",
    "jobs/runner/native-storage/build.rs"].includes(path)) return true;
  return /^jobs\/(?:automation|runner|workflows)\/(?:package\.json|tsconfig\.json|src\/.+)$/.test(path)
    || /^jobs\/runner\/native-storage\/src\/.+$/.test(path);
}
export function smokeArguments(image, uid, gid, name) {
  check(/^sha256:[a-f0-9]{64}$/.test(image), "exact_image_required");
  check(/^p634-smoke-[a-f0-9]{20}$/.test(name), "owned_container_required");
  check([uid, gid].every((id) => Number.isInteger(id) && id >= 1000 && id <= 65535), "safe_image_uid");
  return ["run", "--name", name, "--interactive", "--network", "none", "--read-only", "--cap-drop", "ALL",
    "--security-opt", "no-new-privileges", "--pids-limit", "64", "--memory", "512m", "--cpus", "1",
    "--user", `${uid}:${gid}`, "--tmpfs", "/tmp:rw,noexec,nosuid,nodev,size=32m,mode=1777",
    "--tmpfs", `/var/lib/bluey-jobs-runner:rw,noexec,nosuid,nodev,size=16m,mode=0700,uid=${uid},gid=${gid}`,
    "--env", "BLUEY_JOBS_RUNNER_NATIVE_SMOKE_ROOT=/var/lib/bluey-jobs-runner", "--workdir", "/app/runner",
    "--entrypoint", "/usr/local/bin/node", image, "--input-type=module"];
}
export function parseInventory(raw) {
  const text = raw.trim();
  if (!text) return [];
  let rows;
  try { const value = JSON.parse(text); rows = Array.isArray(value) ? value : [value]; }
  catch { rows = text.split("\n").map((line) => JSON.parse(line)); }
  check(rows.length <= 8 && rows.every((row) => row && typeof row === "object" && !Array.isArray(row)),
    "inventory_shape");
  return rows;
}
export function assertSmokeInspection(container, image, uid, gid) {
  const config = container?.HostConfig;
  check(container?.Image === image && container?.Config?.User === `${uid}:${gid}`
    && container.Config.WorkingDir === "/app/runner" && container.State?.Running === false
    && container.State?.ExitCode === 0 && config?.NetworkMode === "none"
    && config.ReadonlyRootfs === true && config.Privileged === false
    && JSON.stringify(config.CapDrop) === '["ALL"]' && !config.CapAdd?.length
    && config.SecurityOpt?.includes("no-new-privileges") && config.PidsLimit === 64
    && config.Memory === 512 * 1024 * 1024 && config.NanoCpus === 1_000_000_000
    && !config.Binds?.length && Object.keys(config.PortBindings || {}).length === 0
    && Object.values(container.NetworkSettings?.Ports || {}).every((binding) => binding === null),
  "smoke_runtime_isolation");
  const expected = { "/tmp": "rw,noexec,nosuid,nodev,size=32m,mode=1777",
    "/var/lib/bluey-jobs-runner": `rw,noexec,nosuid,nodev,size=16m,mode=0700,uid=${uid},gid=${gid}` };
  check(JSON.stringify(config.Tmpfs) === JSON.stringify(expected), "smoke_tmpfs_bounds");
  check((container.Mounts || []).every((mount) => mount.Type === "tmpfs"
    && Object.hasOwn(expected, mount.Destination)), "smoke_host_mount");
}
function yamlValue(raw, key) {
  const lines = raw.split("\n");
  const found = lines.flatMap((line, index) => line.startsWith(`${key}:`) ? [index] : []);
  check(found.length <= 1, "vm_config_duplicate");
  if (!found.length) return undefined;
  const index = found[0];
  let value = lines[index].slice(key.length + 1).split("#")[0].trim();
  for (let next = index + 1; next < lines.length && !/^[A-Za-z]/.test(lines[next]); next += 1) {
    const extra = lines[next].split("#")[0].trim();
    if (extra) value += ` ${extra}`;
  }
  return value.trim().replace(/^"([^"\n]*)"$/, "$1");
}
export function assertVmConfiguration(raw) {
  check(typeof raw === "string" && raw.length < 128 * 1024, "vm_config_size");
  for (const [key, expected] of Object.entries({ cpu: "4", memory: "6", rootDisk: "8", disk: "16",
    arch: "aarch64", vmType: "vz", runtime: "docker", forwardAgent: "false", sshConfig: "false",
    autoActivate: "false", rosetta: "false", binfmt: "false", portForwarder: "none" })) {
    check(yamlValue(raw, key) === expected, `vm_config_${key}`);
  }
  // In Colima's schema an explicit [] means mount HOME; nil means no mounts.
  check([undefined, "", "null", "~"].includes(yamlValue(raw, "mounts")), "vm_host_mounts");
}
function info(metadata) { return { directory: metadata.isDirectory(), symlink: metadata.isSymbolicLink(),
  uid: metadata.uid, mode: metadata.mode }; }
function identity(metadata) { return `${metadata.dev}:${metadata.ino}`; }
async function privateDirectory(path, kind) {
  const metadata = await lstat(path);
  assertPrivateRoot(path, await realpath(path), info(metadata), process.getuid(), kind);
  return metadata;
}
async function privateFile(path, maximum) {
  const metadata = await lstat(path);
  check(metadata.isFile() && !metadata.isSymbolicLink() && metadata.uid === process.getuid()
    && metadata.nlink === 1 && (metadata.mode & 0o7777) === 0o600
    && metadata.size > 0 && metadata.size <= maximum, "private_file_identity");
  return readFile(path);
}

export async function main() {
  check(process.argv.length === 2, "arguments_forbidden");
  check(process.platform === "darwin" && process.arch === "arm64", "native_darwin_arm64_required");
  check(process.env.BLUEY_DATABASE_URL === undefined && process.env.BLUEY_TEST_POSTGRES_URL === undefined,
    "database_environment_forbidden");
  const canonicalRoot = process.env.BLUEY_TEST_RUN_ROOT;
  check(typeof canonicalRoot === "string", "launcher_root_required");
  await privateDirectory(canonicalRoot, "launcher");
  const launcherMarker = await privateFile(join(canonicalRoot, ".bluey-test-run-v2"), 4096);
  const markerLines = launcherMarker.toString("utf8").trimEnd().split("\n");
  check(markerLines.length === 9 && markerLines[0] === "magic=bluey-test-run-v2"
    && markerLines[1] === `run_root=${canonicalRoot}` && markerLines[2] === `run_name=${basename(canonicalRoot)}`
    && markerLines[3] === `owner_uid=${process.getuid()}` && markerLines[8] === "state=running", "launcher_marker");
  const home = process.env.HOME;
  check(typeof home === "string" && inside(canonicalRoot, home) && await realpath(home) === home, "private_home_required");
  const homeInfo = await lstat(home);
  check(homeInfo.isDirectory() && !homeInfo.isSymbolicLink() && homeInfo.uid === process.getuid()
    && (homeInfo.mode & 0o7777) === 0o700, "private_home_identity");
  const beforeFs = await statfs(canonicalRoot);
  const freeBefore = beforeFs.bavail * beforeFs.bsize;
  assertResources({ cpus: 4, memoryGiB: 6, rootDiskGiB: 8, dataDiskGiB: 16, freeBytes: freeBefore });
  const stateRoot = await mkdtemp(join(await realpath("/tmp"), "bluey-jobs-linux-p634-"));
  const stateIdentity = identity(await privateDirectory(stateRoot, "state"));
  const profile = `p634-${randomBytes(10).toString("hex")}`;
  const env = isolatedEnvironment(stateRoot, canonicalRoot, home);
  const marker = Buffer.from(`${JSON.stringify({ version: 1, root: stateRoot, profile, uid: process.getuid(),
    nonce: randomBytes(24).toString("hex") })}\n`);
  await writeFile(join(stateRoot, OWNER_MARKER), marker, { flag: "wx", mode: 0o600 });
  let active;
  let interrupted = false;
  let cleaning = false;
  let startAttempted = false;
  let stage = "preflight";
  let failure;
  let cleanupFailure;
  let cleanupProven = false;
  let retainedDiagnosticsOnly = false;
  let result;
  let retained = true;
  const vmPids = new Set();
  const diagnostics = [];
  let failureSequence = 0;
  let diagnosticWriteFailed = false;
  const event = (value) => console.log(JSON.stringify(value));
  const signals = ["SIGHUP", "SIGINT", "SIGTERM"];
  const stop = () => { interrupted = true; if (active && !cleaning) active.terminate(); };
  for (const signal of signals) process.on(signal, stop);
  const overall = setTimeout(stop, 45 * 60_000);

  async function revalidate() {
    check(identity(await privateDirectory(stateRoot, "state")) === stateIdentity, "state_root_changed");
    check((await privateFile(join(stateRoot, OWNER_MARKER), 4096)).equals(marker), "state_marker_changed");
  }
  async function command(tool, args, { input, timeout = 30_000, maximum = 2 * 1024 * 1024,
    allowFailure = false, binary = false } = {}) {
    check(!interrupted || cleaning, "interrupted");
    await revalidate();
    const child = spawn(CLI[tool], args, { cwd: REPO, env, stdio: ["pipe", "pipe", "pipe"] });
    let stdout = [], stdoutTail = Buffer.alloc(0), stderr = Buffer.alloc(0), outputBytes = 0, denied = false, escalation;
    const terminate = () => {
      if (denied) return;
      denied = true;
      child.kill("SIGTERM");
      escalation = setTimeout(() => child.kill("SIGKILL"), 5000);
    };
    active = { child, terminate };
    const collect = (stream, save) => stream.on("data", (chunk) => {
      if (save) stdoutTail = boundedDiagnosticTail(stdoutTail, chunk);
      else stderr = boundedDiagnosticTail(stderr, chunk);
      outputBytes += chunk.length;
      if (outputBytes > maximum) { if (!denied) terminate(); return; }
      if (save) stdout.push(chunk);
    });
    collect(child.stdout, true); collect(child.stderr, false);
    child.stdin.on("error", () => {});
    child.stdin.end(input);
    const timer = setTimeout(terminate, timeout);
    const closed = await new Promise((resolveClose) => {
      child.once("error", () => resolveClose({ code: null, signal: "spawn_error" }));
      child.once("close", (code, signal) => resolveClose({ code, signal }));
    });
    clearTimeout(timer); clearTimeout(escalation); active = undefined;
    if ((denied || closed.code !== 0 || closed.signal !== null) && failureSequence < MAX_FAILURE_COMMANDS) {
      // Raw output is private-only: fixed filenames, owned inode-bound root,
      // bounded tails, create-new0600. Never include content/argv in reports.
      failureSequence += 1;
      try {
        await revalidate();
        for (const [stream, tail] of [["stdout", stdoutTail], ["stderr", stderr]]) {
          const file = `command-failure-${String(failureSequence).padStart(4, "0")}.${stream}`;
          const bytes = tail.length ? tail : Buffer.from(`No ${stream} captured.\n`);
          await writeFile(join(stateRoot, file), bytes, { flag: "wx", mode: 0o600 });
          const observed = await privateFile(join(stateRoot, file), MAX_FAILURE_OUTPUT);
          check(observed.equals(bytes), "failure_diagnostic_changed");
          diagnostics.push({ file, tool, stage: cleaning ? "cleanup" : stage, bytes: bytes.length });
        }
      } catch { diagnosticWriteFailed = true; }
    }
    check(!denied && (!interrupted || cleaning), "command_timeout_or_interrupted");
    if (!allowFailure) check(closed.code === 0 && closed.signal === null, `command_${tool}_failed`);
    const bytes = Buffer.concat(stdout);
    stdout = [];
    return { ...closed, stdout: binary ? bytes : bytes.toString("utf8") };
  }
  const dockerHost = `unix://${env.COLIMA_HOME}/${profile}/docker.sock`;
  const docker = (args, options) => command("docker", ["--host", dockerHost, ...args], options);
  const inventory = async () => ({
    colima: parseInventory((await command("colima", ["list", "--json"])).stdout),
    lima: parseInventory((await command("lima", ["list", "--json", "--tty=false"])).stdout),
  });
  const pidExists = (pid) => {
    try { process.kill(pid, 0); return true; } catch (error) {
      if (error.code === "ESRCH") return false;
      throw error;
    }
  };
  try {
    assertUnixSocketBudget({ dockerSocket: join(env.COLIMA_HOME, profile, "docker.sock"),
      limaSshSocket: join(env.LIMA_HOME, `colima-${profile}`, "ssh.sock.1234567890123456") });
    for (const directory of new Set([env.COLIMA_HOME, env.COLIMA_CACHE_HOME, env.LIMA_HOME, env.DOCKER_CONFIG, env.TMPDIR])) {
      await mkdir(directory, { mode: 0o700 });
    }
    for (const path of Object.values(CLI)) {
      const actual = await realpath(path); const metadata = await lstat(actual);
      check(metadata.isFile() && !metadata.isSymbolicLink() && (metadata.uid === 0 || metadata.uid === process.getuid())
        && (metadata.mode & 0o022) === 0 && (metadata.mode & 0o111) !== 0, "approved_cli_identity");
    }
    // One explicit, reviewed construction mode: no BuildKit-equivalence claim,
    // automatic fallback, plugin installation or owner CLI-plugin directory.
    check(env.DOCKER_BUILDKIT === "0", "explicit_legacy_builder_required");
    stage = "source_snapshot";
    const head = (await command("git", ["rev-parse", "HEAD"])).stdout.trim();
    check(/^[a-f0-9]{40}$/.test(head), "source_commit");
    async function unchanged() {
      check((await command("git", ["rev-parse", "HEAD"])).stdout.trim() === head, "source_commit_changed");
      check((await command("git", ["status", "--porcelain=v1", "-z", "--untracked-files=all", "--", PRODUCT])).stdout === "", "product_source_dirty");
    }
    await unchanged();
    const epoch = (await command("git", ["show", "-s", "--format=%ct", head])).stdout.trim();
    check(/^[1-9][0-9]{8,11}$/.test(epoch), "source_epoch");
    const tree = (await command("git", ["ls-tree", "-r", "-z", head, "--", PRODUCT])).stdout;
    const entries = tree.split("\0").filter(Boolean).map((entry) => {
      const parsed = /^(100644|100755) blob ([a-f0-9]{40})\t(.+)$/.exec(entry);
      check(parsed !== null, "tracked_product_symlink_or_special_file");
      return parsed[3];
    }).filter(admittedContextPath);
    check(entries.length > 20 && entries.length <= 3000, "context_entry_bound");
    const context = join(stateRoot, "context"); await mkdir(context, { mode: 0o700 });
    let sourceBytes = 0;
    for (const path of entries) {
      const bytes = (await command("git", ["show", `${head}:${path}`], { binary: true, maximum: 16 * 1024 * 1024 })).stdout;
      sourceBytes += bytes.length; check(sourceBytes <= 64 * 1024 * 1024, "context_byte_bound");
      const livePath = join(REPO, path); const liveInfo = await lstat(livePath);
      check(liveInfo.isFile() && !liveInfo.isSymbolicLink() && await realpath(livePath) === livePath
        && hash(await readFile(livePath)) === hash(bytes), "context_source_changed");
      const destination = join(context, path.slice(5));
      await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
      await writeFile(destination, bytes, { flag: "wx", mode: 0o444 });
    }
    const smoke = (await command("git", ["show", `${head}:jobs/scripts/native-runner-addon-smoke.mjs`], { binary: true })).stdout;
    check(smoke.equals(await readFile(join(REPO, "jobs/scripts/native-runner-addon-smoke.mjs"))), "smoke_source_changed");
    const initial = await inventory(); assertEmptyInventories(initial.colima, initial.lima);
    stage = "vm_start"; event({ event: "linux_runner_stage", stage }); startAttempted = true;
    await command("colima", startArguments(profile), { timeout: 600_000 });
    stage = "vm_identity";
    const live = await inventory();
    check(live.colima.length === 1 && (live.colima[0].name ?? live.colima[0].Name) === profile
      && live.lima.length === 1 && (live.lima[0].name ?? live.lima[0].Name) === `colima-${profile}`, "owned_vm_inventory");
    for (const pid of ownedLimaPids(live.lima[0], profile)) vmPids.add(pid);
    assertVmConfiguration((await readFile(join(env.COLIMA_HOME, profile, "colima.yaml"))).toString("utf8"));
    const guest = (args) => command("colima", ["ssh", "--profile", profile, "--", ...args]);
    check((await guest(["uname", "-m"])).stdout.trim() === "aarch64", "guest_not_native_arm64");
    check((await guest(["getconf", "_NPROCESSORS_ONLN"])).stdout.trim() === "4", "guest_cpu_bound");
    const memory = /^MemTotal:\s+([0-9]+) kB$/m.exec((await guest(["cat", "/proc/meminfo"])).stdout);
    check(memory && Number(memory[1]) * 1024 <= 6 * GIB && Number(memory[1]) * 1024 >= 5 * GIB, "guest_memory_bound");
    const mounts = (await guest(["cat", "/proc/mounts"])).stdout;
    check(!/\s(?:virtiofs|9p|fuse\.sshfs|sshfs)\s/.test(mounts), "guest_host_mount_detected");
    const disks = (await guest(["df", "-Pk", "/", "/var/lib/docker"])).stdout.trim().split("\n").slice(1)
      .map((line) => line.trim().split(/\s+/));
    check(disks.length === 2 && disks[0][0] !== disks[1][0], "guest_disk_topology");
    for (const [index, maximum] of [8, 16].entries()) {
      const bytes = Number(disks[index][1]) * 1024;
      check(Number.isFinite(bytes) && bytes > (maximum - 2) * GIB && bytes <= maximum * GIB, "guest_disk_bound");
    }
    const socketPath = join(env.COLIMA_HOME, profile, "docker.sock");
    const socket = await lstat(socketPath);
    check(socket.isSocket() && !socket.isSymbolicLink() && socket.uid === process.getuid()
      && await realpath(socketPath) === socketPath, "owned_docker_socket");
    const engine = JSON.parse((await docker(["info", "--format", "{{json .}}"])).stdout);
    check(engine.OSType === "linux" && ["aarch64", "arm64"].includes(engine.Architecture)
      && engine.NCPU === 4 && engine.MemTotal <= 6 * GIB, "docker_native_resource_identity");
    stage = "image_build"; event({ event: "linux_runner_stage", stage });
    const tag = `bluey-jobs-nonprod-p634:${profile.slice(5)}`;
    const zeros = "0".repeat(64);
    await docker(["build", "--pull", "--platform", "linux/arm64", "--file", join(context, "runner/Dockerfile"),
      "--tag", tag, "--iidfile", join(stateRoot, "image.id"), "--build-arg", `SOURCE_DATE_EPOCH=${epoch}`,
      "--build-arg", `BLUEY_JOBS_BUILD_ID=nonprod-${profile}`, "--build-arg", `BLUEY_JOBS_SOURCE_COMMIT=${head}`,
      ...["CONFIG_SCHEMA", "MIGRATION_SET", "PROTOCOL_SET"].flatMap((name) => ["--build-arg", `BLUEY_JOBS_${name}_SHA256=${zeros}`]),
      context], { timeout: 1_800_000, maximum: 4 * 1024 * 1024 });
    const imageId = (await readFile(join(stateRoot, "image.id"), "utf8")).trim();
    check(/^sha256:[a-f0-9]{64}$/.test(imageId), "image_id");
    const images = JSON.parse((await docker(["image", "inspect", tag])).stdout);
    check(images.length === 1 && images[0].Id === imageId && images[0].Os === "linux"
      && images[0].Architecture === "arm64" && images[0].Config.User === "pwuser", "exact_image_platform_user");
    const measurementBytes = (await docker(["run", "--rm", "--network", "none", "--read-only", "--cap-drop", "ALL",
      "--security-opt", "no-new-privileges", "--pids-limit", "32", "--memory", "128m", "--entrypoint",
      "/usr/local/bin/node", imageId, "-e", "process.stdout.write(require('node:fs').readFileSync('/app/.bluey/managed-cloud-runtime-measurement.json'))"],
    { maximum: 512 * 1024, binary: true })).stdout;
    assertRuntimeMeasurement(JSON.parse(measurementBytes), head, `nonprod-${profile}`);
    const queryArgs = ["run", "--rm", "--network", "none", "--read-only", "--cap-drop", "ALL", "--security-opt",
      "no-new-privileges", "--pids-limit", "32", "--memory", "128m", "--entrypoint", "/usr/bin/id", imageId];
    const uidText = (await docker([...queryArgs, "-u", "pwuser"])).stdout.trim();
    const gidText = (await docker([...queryArgs, "-g", "pwuser"])).stdout.trim();
    check(/^[1-9][0-9]{3,4}$/.test(uidText) && /^[1-9][0-9]{3,4}$/.test(gidText), "image_uid_query");
    stage = "native_addon_smoke"; event({ event: "linux_runner_stage", stage });
    const smokeName = `p634-smoke-${profile.slice(5)}`;
    const output = await docker(smokeArguments(imageId, Number(uidText), Number(gidText), smokeName), { input: smoke, timeout: 60_000 });
    check(output.stdout === "Bluey Jobs native runner addon smoke passed.\n", "native_smoke_output");
    const containers = JSON.parse((await docker(["container", "inspect", smokeName])).stdout);
    check(containers.length === 1, "exact_smoke_container");
    assertSmokeInspection(containers[0], imageId, Number(uidText), Number(gidText));
    await docker(["container", "rm", smokeName]);
    check((await docker(["ps", "--all", "--quiet"])).stdout.trim() === "", "owned_containers_remain");
    await unchanged();
    result = { version: 1, qualification: "nonproduction_image_native_addon_only", sourceCommit: head,
      imageId, platform: "linux/arm64", builder: CONSTRUCTION_BUILDER, tests: 1, passed: 1, failed: [],
      runtimeMeasurementSha256: hash(measurementBytes),
      resources: { cpus: 4, memoryGiB: 6, rootDiskGiB: 8, dataDiskGiB: 16 },
      context: { files: entries.length, bytes: sourceBytes }, freeBeforeBytes: freeBefore };
  } catch (error) { failure = /^[a-zA-Z0-9_]{1,80}$/.test(error.message) ? error.message : "closed_stage_failure"; }
  finally {
    cleaning = true;
    try {
      await revalidate();
      if (startAttempted) {
        const prior = await inventory();
        // An attempted start with absent/partial inventory is not proof that no
        // detached VM exists. Retain its separately owned disks in that case.
        for (const pid of cleanupInventoryProof(prior.colima, prior.lima, profile)) vmPids.add(pid);
        let stopped;
        try { stopped = await command("colima", cleanupArguments(profile, "stop"), { timeout: 90_000, allowFailure: true }); }
        catch { stopped = { code: null, signal: "stop_timeout" }; }
        if (stopped.code !== 0 || stopped.signal !== null) {
          await command("colima", cleanupArguments(profile, "force-stop"), { timeout: 60_000 });
        }
        // Stop success (including --force) is never permission to unlink disks.
        // Require observed stopped inventory AND old process absence first.
        const postStop = await inventory();
        cleanupInventoryProof(postStop.colima, postStop.lima, profile, { requireStopped: true });
        assertRecordedPidsAbsent([...vmPids], pidExists);
        await revalidate();
        await command("colima", cleanupArguments(profile, "delete"), { timeout: 120_000 });
      }
      const final = await inventory(); assertEmptyInventories(final.colima, final.lima);
      assertRecordedPidsAbsent([...vmPids], pidExists);
      await assert.rejects(lstat(join(env.COLIMA_HOME, profile, "docker.sock")), { code: "ENOENT" });
      await revalidate();
      cleanupProven = true;
      if (failure || interrupted) {
        // Keep only bounded diagnostics+owner marker after ALL VM/disk safety
        // proofs. Unknown cleanup never enters this destructive reduction.
        const names = await readdir(stateRoot);
        const plan = diagnosticRetentionPlan(names);
        for (const name of names) {
          if (name !== OWNER_MARKER && !plan.includes(name)) await privateFile(join(stateRoot, name), MAX_FAILURE_OUTPUT);
        }
        const children = new Map();
        for (const name of plan) {
          const path = join(stateRoot, name); const metadata = await lstat(path);
          check(!metadata.isSymbolicLink() && metadata.uid === process.getuid()
            && (name === "image.id" ? metadata.isFile() : metadata.isDirectory())
            && await realpath(path) === path, "diagnostic_cleanup_child_identity");
          children.set(name, identity(metadata));
        }
        for (const name of plan) {
          await revalidate();
          const path = join(stateRoot, name);
          check(identity(await lstat(path)) === children.get(name), "diagnostic_cleanup_child_changed");
          await rm(path, { recursive: name !== "image.id", force: false });
        }
        const remaining = await readdir(stateRoot);
        check(diagnosticRetentionPlan(remaining).length === 0, "diagnostic_cleanup_incomplete");
        for (const name of remaining) if (name !== OWNER_MARKER) await privateFile(join(stateRoot, name), MAX_FAILURE_OUTPUT);
        await revalidate();
        retainedDiagnosticsOnly = true;
      } else {
        await rm(stateRoot, { recursive: true, force: false });
        await assert.rejects(lstat(stateRoot), { code: "ENOENT" });
        retained = false;
      }
    } catch { cleanupFailure = "cleanup_unproven_state_retained"; }
    clearTimeout(overall);
    for (const signal of signals) process.off(signal, stop);
  }
  if (failure || cleanupFailure || interrupted || !result || retained) {
    event(failureReport({ stage, primaryFailure: failure, cleanupFailure, interrupted, retained,
      stateRoot, profile, cleanupProven, retainedDiagnosticsOnly, diagnostics, diagnosticWriteFailed }));
    throw new Error("nonprod_linux_runner_not_qualified");
  }
  const after = await statfs(canonicalRoot);
  result.freeAfterBytes = after.bavail * after.bsize;
  result.freeDeltaBytes = result.freeAfterBytes - freeBefore;
  result.cleanup = { privateVmInventoriesEmpty: true, ownedVmPids: [...vmPids], ownedVmPidsAbsent: true,
    dockerSocketAbsent: true, stateAbsent: true };
  await writeFile(join(canonicalRoot, "linux-runner-result.json"), `${JSON.stringify(result)}\n`, { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify({ event: "nonprod_linux_runner_finished", ...result }));
}

if (process.argv[1] && resolve(process.argv[1]) === SELF) {
  await main().catch((error) => {
    if (error.message !== "nonprod_linux_runner_not_qualified") {
      const code = /^[a-zA-Z0-9_]{1,80}$/.test(error.message) ? error.message : "closed_preflight_failure";
      console.log(JSON.stringify({ event: "nonprod_linux_runner_failed", stage: "preflight", code }));
    }
    process.exitCode = 1;
  });
}
