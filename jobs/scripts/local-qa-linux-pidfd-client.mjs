// Closed Node-side protocol client for the one-postmaster Linux pidfd helper.
// It never signals a numeric PID: only the pinned helper can request shutdown.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readlink, realpath } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";
import { parseLinuxPostmasterStat } from "./local-qa-linux-postmaster-observer.mjs";

const REFUSAL = "BLUEY_LOCAL_QA_LINUX_PIDFD_REFUSED";
const UNCLOSED = "BLUEY_LOCAL_QA_LINUX_PIDFD_UNCLOSED";
const MAX_TOOL_BYTES = 256 * 1024 * 1024;
const MAX_PROC_BYTES = 64 * 1024;
const MAX_REQUEST_BYTES = 4096;
const MAX_RESPONSE_BYTES = 64;
const BIND_TIMEOUT_MS = 10_000;
const COMMAND_TIMEOUT_MS = 10_000;
const SHA256 = /^[0-9a-f]{64}$/;
const NETNS = /^net:\[[1-9][0-9]{0,19}\]$/;
const CGROUP = /^\/(?:[^\0\r\n]+)?$/;

function refuse() { throw new Error(REFUSAL); }
function need(value) { if (!value) refuse(); }
function plain(value) { return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype; }
function keys(value, expected) { return plain(value) && Object.keys(value).sort().join(",") === [...expected].sort().join(","); }
function stamp(info) {
  return [info.dev, info.ino, info.uid, info.gid, info.mode, info.nlink, info.size, info.mtimeNs, info.ctimeNs]
    .map(String).join(":");
}
function directoryStamp(info) { return [info.dev, info.ino, info.uid, info.gid, info.mode].map(String).join(":"); }
function safePid(value) { need(Number.isSafeInteger(value) && value > 1 && value <= 2_147_483_647); return value; }
function safePort(value) { need(Number.isSafeInteger(value) && value > 1024 && value <= 65535 && value !== 5432); return value; }
function safePath(value) { need(typeof value === "string" && resolve(value) === value && !/[\0\r\n]/.test(value)); return value; }

async function boundedBytes(path, maximum, flags = constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK) {
  const file = await open(path, flags);
  try {
    const pieces = [], buffer = Buffer.allocUnsafe(4096); let total = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
      if (bytesRead === 0) break;
      total += bytesRead; need(total <= maximum);
      pieces.push(Buffer.from(buffer.subarray(0, bytesRead)));
    }
    return Buffer.concat(pieces, total);
  } finally { await file.close(); }
}

async function pinnedTool(record, helper) {
  need(keys(record, ["path", "sha256"]) && safePath(record.path) && SHA256.test(record.sha256)
    && await realpath(record.path) === record.path);
  if (helper) need(basename(record.path) === "local-qa-linux-postmaster-pidfd.py");
  const file = await open(record.path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && (helper ? before.nlink === 1n : before.nlink >= 1n)
      && before.size > 0n && before.size <= BigInt(MAX_TOOL_BYTES)
      && (before.mode & 0o022n) === 0n && (before.mode & 0o7000n) === 0n
      && (before.uid === 0n || before.uid === BigInt(process.getuid())));
    if (!helper) need((before.mode & 0o111n) !== 0n);
    const named = await lstat(record.path, { bigint: true });
    need(stamp(named) === stamp(before));
    const hash = createHash("sha256"), buffer = Buffer.allocUnsafe(64 * 1024); let total = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
      if (bytesRead === 0) break;
      total += bytesRead; need(total <= MAX_TOOL_BYTES);
      hash.update(buffer.subarray(0, bytesRead));
    }
    const after = await file.stat({ bigint: true }), namedAfter = await lstat(record.path, { bigint: true });
    need(BigInt(total) === before.size && stamp(after) === stamp(before) && stamp(namedAfter) === stamp(before)
      && hash.digest("hex") === record.sha256);
    return Object.freeze({ path: record.path, sha256: record.sha256, stamp: stamp(before) });
  } finally { await file.close(); }
}

async function privateDirectory(path, mode = 0o700) {
  safePath(path); need(await realpath(path) === path);
  const info = await lstat(path, { bigint: true });
  need(info.isDirectory() && info.uid === BigInt(process.getuid()) && (info.mode & 0o7777n) === BigInt(mode));
  return directoryStamp(info);
}

async function scope(root, stateDirectory) {
  need(process.platform === "linux" && typeof process.getuid === "function" && process.getuid() !== 0
    && root === process.env.BLUEY_TEST_RUN_ROOT && isDurableQaFixtureRoot(root, "linux")
    && stateDirectory === join(root, "bluey-jobs-qa-draft"));
  const parent = dirname(root), dataDirectory = join(stateDirectory, "postgres");
  safePath(parent); need(await realpath(parent) === parent);
  const parentIdentity = await privateDirectory(parent);
  return Object.freeze({ parent: parentIdentity, root: await privateDirectory(root), state: await privateDirectory(stateDirectory),
    data: await privateDirectory(dataDirectory), dataDirectory });
}

async function currentProcStat(pid) {
  const raw = await boundedBytes(`/proc/${pid}/stat`, MAX_PROC_BYTES);
  need(!raw.includes(0));
  return parseLinuxPostmasterStat(raw.toString("utf8"), pid);
}

async function assertSelfNetwork(networkNamespace, unitCgroup) {
  need(NETNS.test(networkNamespace) && CGROUP.test(unitCgroup));
  need(await readlink("/proc/self/ns/net") === networkNamespace);
  const raw = await boundedBytes("/proc/self/cgroup", 4096);
  need(raw.toString("utf8") === `0::${unitCgroup}\n`);
}

function inputShape(input, binding, role) {
  const postgres = role === "postgres";
  const expected = postgres
    ? ["dataDirectory", "networkNamespace", "pid", "port", "postgresPath", "postgresSha256", "root", "stateDirectory", "unitCgroup"]
    : ["apiPath", "apiSha256", "networkNamespace", "pid", "port", "root", "stateDirectory", "unitCgroup"];
  need(keys(input, expected));
  need(input.root === binding.root && input.stateDirectory === binding.stateDirectory && input.unitCgroup === binding.unitCgroup
    && input.networkNamespace === binding.networkNamespace);
  if (postgres) need(input.dataDirectory === binding.dataDirectory && safePath(input.postgresPath) && SHA256.test(input.postgresSha256));
  else need(safePath(input.apiPath) && SHA256.test(input.apiSha256));
  return Object.freeze({ ...input, pid: safePid(input.pid), port: safePort(input.port) });
}

/** Pure finite-state response gate for pidfd client tests and framing. */
export function nextLocalQaLinuxPidfdState(state, response) {
  const expected = Object.freeze({ await_bound: "bound\n", stop_requested: "stopped\n", dispose_requested: "disposed\n" });
  if (typeof state !== "string" || typeof response !== "string" || response !== expected[state]) throw new Error(REFUSAL);
  return Object.freeze({ await_bound: "bound", stop_requested: "stopped", dispose_requested: "disposed" }[state]);
}

function childTerminal(child) {
  let result;
  const terminal = new Promise(resolveTerminal => child.once("close", (code, signal) => {
    result = Object.freeze({ code, signal }); resolveTerminal(result);
  }));
  return Object.freeze({ terminal, get result() { return result; } });
}

function waitTerminal(watched, timeoutMs) {
  return new Promise(resolveTerminal => {
    let settled = false;
    const finish = value => {
      if (settled) return; settled = true; clearTimeout(timer); resolveTerminal(value);
    };
    const timer = setTimeout(() => finish(undefined), timeoutMs);
    watched.terminal.then(finish, () => finish(undefined));
  });
}

function closeIpc(child) {
  try { child.stdin?.end(); } catch {}
  try { child.stdout?.destroy(); } catch {}
}

export function createLocalQaLinuxPidfdProtocol(child) {
  let pending = null, failed = false, ended = false, finalFrame = false, complete = false, bytes = Buffer.alloc(0);
  const fail = () => {
    if (failed) return;
    failed = true;
    if (pending) { const { reject, timer } = pending; pending = null; clearTimeout(timer); reject(new Error(REFUSAL)); }
    closeIpc(child);
  };
  const data = chunk => {
    if (!Buffer.isBuffer(chunk) || pending === null || finalFrame || chunk.length > MAX_RESPONSE_BYTES - bytes.length) return fail();
    bytes = Buffer.concat([bytes, chunk], bytes.length + chunk.length);
    const newline = bytes.indexOf(0x0a);
    if (newline < 0) return;
    // Every helper reply is exactly one short LF-terminated frame. Any tail,
    // including a second frame, is out-of-phase protocol output.
    if (newline !== bytes.length - 1 || bytes.indexOf(0x0a, newline + 1) >= 0 || bytes.includes(0)) return fail();
    const frame = bytes.toString("utf8"), active = pending;
    bytes = Buffer.alloc(0); pending = null; clearTimeout(active.timer); active.resolve(frame);
    if (active.final) finalFrame = true;
  };
  child.stdout.on("data", data);
  child.stdout.on("error", fail);
  child.stdout.on("end", () => {
    ended = true;
    if (finalFrame && pending === null && bytes.length === 0 && !failed) complete = true;
    else fail();
  });
  // Persistent error sinks are required: stdin EPIPE can occur after a helper
  // refusal, long after the corresponding write call returned.
  child.stdin.on("error", fail);
  return Object.freeze({
    expect(timeoutMs, final = false) {
      return new Promise((resolveFrame, rejectFrame) => {
        if (failed || ended || finalFrame || pending !== null || bytes.length !== 0 || typeof final !== "boolean") { rejectFrame(new Error(REFUSAL)); return; }
        const timer = setTimeout(() => { fail(); }, timeoutMs);
        pending = Object.freeze({ resolve: resolveFrame, reject: rejectFrame, timer, final });
      });
    },
    close() { fail(); },
    get complete() { return complete && !failed; },
  });
}

async function absentSamePostmaster(pid, startTime) {
  try {
    const current = await currentProcStat(pid);
    need(current.startTime !== startTime); // PID reuse is not the original process, but still uncertain.
    refuse();
  } catch (error) {
    if (error?.code === "ENOENT") return true;
    throw error;
  }
}

/**
 * Bind a pinned Python pidfd helper to one direct postmaster. The returned
 * closure accepts exactly one postmaster identity and exposes no numeric kill.
 */
async function createLocalQaLinuxPidfdBinderForRole(role, { root, stateDirectory, unitCgroup, networkNamespace,
  pidfdToolchain, assertOwnedUnit }) {
  try {
    need(role === "postgres" || role === "api");
    need(keys(pidfdToolchain, ["helper", "python"]) && typeof assertOwnedUnit === "function");
    const initialScope = await scope(root, stateDirectory);
    need(typeof unitCgroup === "string" && CGROUP.test(unitCgroup) && typeof networkNamespace === "string" && NETNS.test(networkNamespace));
    const python = await pinnedTool(pidfdToolchain.python, false);
    const helper = await pinnedTool(pidfdToolchain.helper, true);
    const binding = Object.freeze({ root, stateDirectory, unitCgroup, networkNamespace, dataDirectory: initialScope.dataDirectory,
      initialScope, python, helper, assertOwnedUnit });
    const checkScope = async () => {
      const now = await scope(root, stateDirectory);
      need(now.parent === initialScope.parent && now.root === initialScope.root && now.state === initialScope.state && now.data === initialScope.data);
      await assertSelfNetwork(networkNamespace, unitCgroup);
    };
    return async function bindPostmasterStop(input) {
      let child, watched, protocol, state = "await_bound", stopUsed = false, disposeUsed = false, startTime, closed = false;
      const close = async () => {
        if (closed) return; closed = true; closeIpc(child);
        protocol?.close();
        if (watched && !watched.result && !(await waitTerminal(watched, COMMAND_TIMEOUT_MS))) {
          throw new Error(UNCLOSED);
        }
        // The caller treats an uncertain close as an uncertain lifetime, but
        // still re-check both executable paths after closing our IPC ends.
        if (child) {
          try {
            await pinnedTool(pidfdToolchain.python, false);
            await pinnedTool(pidfdToolchain.helper, true);
          } catch {}
        }
      };
      try {
        const request = inputShape(input, binding, role);
        const apiBinary = role === "api" ? await pinnedTool(Object.freeze({ path: request.apiPath, sha256: request.apiSha256 }), false) : undefined;
        const observed = Object.freeze(role === "postgres"
          ? { pid: request.pid, port: request.port, root, dataDirectory: request.dataDirectory, unitCgroup, networkNamespace }
          : { pid: request.pid, port: request.port, root, stateDirectory, apiPath: request.apiPath, unitCgroup, networkNamespace });
        await checkScope();
        need(await binding.assertOwnedUnit(observed) === true);
        const first = await currentProcStat(request.pid);
        need(first.ppid === process.pid); startTime = first.startTime;
        await checkScope();
        need(await binding.assertOwnedUnit(observed) === true);
        const beforePython = await pinnedTool(pidfdToolchain.python, false);
        const beforeHelper = await pinnedTool(pidfdToolchain.helper, true);
        need(beforePython.stamp === binding.python.stamp && beforeHelper.stamp === binding.helper.stamp);
        const frameBody = role === "postgres"
          ? { root, pid: request.pid, startTime, port: request.port, dataDirectory: request.dataDirectory,
            postgresPath: request.postgresPath, postgresSha256: request.postgresSha256, unitCgroup, networkNamespace }
          : { root, pid: request.pid, startTime, port: request.port, stateDirectory, apiPath: request.apiPath,
            apiSha256: request.apiSha256, unitCgroup, networkNamespace };
        const frame = Buffer.from(`${JSON.stringify(Object.freeze(frameBody))}\n`, "utf8");
        need(frame.length <= MAX_REQUEST_BYTES);
        child = spawn(binding.python.path, ["-I", "-S", "-B", binding.helper.path], {
          cwd: root, detached: false, env: Object.freeze({ BLUEY_TEST_RUN_ROOT: root, PATH: "/usr/bin:/bin", LC_ALL: "C", LANG: "C", TZ: "UTC" }),
          stdio: ["pipe", "pipe", "ignore"],
        });
        watched = childTerminal(child);
        protocol = createLocalQaLinuxPidfdProtocol(child);
        child.once("error", () => { protocol.close(); closeIpc(child); });
        // Keep this anonymous pipe open: after the JSON bind frame, the helper
        // accepts exactly `stop` then `dispose` over the same stream.
        const bound = protocol.expect(BIND_TIMEOUT_MS);
        child.stdin.write(frame);
        state = nextLocalQaLinuxPidfdState(state, await bound);
        need(!watched.result);
        const boundPython = await pinnedTool(pidfdToolchain.python, false);
        const boundHelper = await pinnedTool(pidfdToolchain.helper, true);
        need(boundPython.stamp === binding.python.stamp && boundHelper.stamp === binding.helper.stamp);
        if (apiBinary) need((await pinnedTool(Object.freeze({ path: request.apiPath, sha256: request.apiSha256 }), false)).stamp === apiBinary.stamp);
        const after = await currentProcStat(request.pid);
        need(after.startTime === startTime && after.ppid === process.pid);
        await checkScope();
        need(await binding.assertOwnedUnit(observed) === true);
        return Object.freeze({
          async stop() {
            try {
              need(state === "bound" && !stopUsed && !disposeUsed && !closed); stopUsed = true;
              await checkScope();
              need(await binding.assertOwnedUnit(observed) === true);
              const stopped = protocol.expect(COMMAND_TIMEOUT_MS);
              child.stdin.write("stop\n");
              state = nextLocalQaLinuxPidfdState("stop_requested", await stopped);
              need(state === "stopped" && !watched.result); return true;
            } catch { await close(); refuse(); }
          },
          async dispose() {
            try {
              // A postmaster can exit naturally before the runtime reaches its
              // close path. The helper permits dispose after that terminal
              // observation without a preceding stop request.
              need((state === "bound" || state === "stopped") && !disposeUsed && !closed); disposeUsed = true;
              await absentSamePostmaster(request.pid, startTime);
              const disposed = protocol.expect(COMMAND_TIMEOUT_MS, true);
              child.stdin.write("dispose\n");
              state = nextLocalQaLinuxPidfdState("dispose_requested", await disposed);
              const terminal = await waitTerminal(watched, COMMAND_TIMEOUT_MS);
              need(terminal && terminal.code === 0 && terminal.signal === null && state === "disposed" && protocol.complete === true);
              const finalPython = await pinnedTool(pidfdToolchain.python, false);
              const finalHelper = await pinnedTool(pidfdToolchain.helper, true);
              need(finalPython.stamp === binding.python.stamp && finalHelper.stamp === binding.helper.stamp);
              if (apiBinary) need((await pinnedTool(Object.freeze({ path: request.apiPath, sha256: request.apiSha256 }), false)).stamp === apiBinary.stamp);
              closed = true; return true;
            } catch { await close(); refuse(); }
          },
        });
      } catch { await close(); refuse(); }
    };
  } catch { refuse(); }
}

/** Bind the exact direct PostgreSQL child; its packet and observer shape remain unchanged. */
export async function createLocalQaLinuxPidfdBinder(input) {
  return createLocalQaLinuxPidfdBinderForRole("postgres", input);
}

/** Bind the exact direct API child. Listener ownership remains the API owner's separate proof. */
export async function createLocalQaLinuxApiPidfdBinder(input) {
  return createLocalQaLinuxPidfdBinderForRole("api", input);
}
