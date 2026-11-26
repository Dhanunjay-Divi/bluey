// Darwin-only local-QA AF_UNIX peer attestation.  This module is deliberately
// unconnected to a launcher: callers must inject the returned connector into
// the candidate-only gateway after they have admitted the API process.
import { createHash } from "node:crypto";
import { open, lstat, realpath } from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import { connect, Socket } from "node:net";
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";

const MAX_HELPER_BYTES = 8 * 1024;
const MAX_INTERPRETER_BYTES = 64 * 1024 * 1024;
const VERIFIER_TIMEOUT_MS = 8_000;

function refused() { throw new Error("local_qa_peer_refused"); }
function isRecord(value, keys) {
  return value && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).sort().join(",") === keys;
}
function canonicalPositive(value, minimum = 1) {
  return Number.isSafeInteger(value) && value >= minimum;
}
function canonicalPath(value) { return typeof value === "string" && resolve(value) === value; }
function canonicalDigest(value) { return typeof value === "string" && /^[a-f0-9]{64}$/.test(value); }
function hash(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
function check(condition) { if (!condition) refused(); }

async function checkedUnixBinding(binding) {
  check(isRecord(binding, "dev,ino,mode,path,pid,root,uid"));
  check(canonicalPath(binding.path) && canonicalPath(binding.root)
    && dirname(binding.path) === binding.root
    && canonicalPositive(binding.pid, 2) && canonicalPositive(binding.uid)
    && canonicalPositive(binding.dev, 0) && canonicalPositive(binding.ino, 1)
    && binding.mode === 0o600);
  check(typeof process.getuid === "function" && binding.uid === process.getuid());
  try {
    check(await realpath(binding.root) === binding.root);
    const root = await lstat(binding.root, { bigint: true });
    check(root.isDirectory() && root.uid === BigInt(binding.uid) && (root.mode & 0o7777n) === 0o700n);
    const socket = await lstat(binding.path, { bigint: true });
    check(socket.isSocket() && socket.uid === BigInt(binding.uid)
      && Number(socket.mode & 0o7777n) === binding.mode
      && Number(socket.dev) === binding.dev && Number(socket.ino) === binding.ino);
  } catch { refused(); }
  return Object.freeze({ ...binding });
}

async function immutableRootOwnedAncestors(path) {
  for (let current = dirname(path); ; current = dirname(current)) {
    const directory = await lstat(current, { bigint: true });
    check(directory.isDirectory() && directory.uid === 0n && (directory.mode & 0o022n) === 0n);
    if (current === "/") return;
  }
}

async function sealedFile(path, digest, { rootOwned, maximumBytes }) {
  check(canonicalPath(path) && canonicalDigest(digest));
  let file;
  try {
    // O_NOFOLLOW is an additional fence; the checked bytes, not this pathname,
    // are later passed to Python with -c.
    check(await realpath(path) === path);
    if (rootOwned) await immutableRootOwnedAncestors(path);
    file = await open(path, fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW | fsConstants.O_NONBLOCK);
    const before = await file.stat({ bigint: true });
    check(before.isFile() && before.size > 0n && before.size <= BigInt(maximumBytes));
    if (rootOwned) check(before.uid === 0n && (before.mode & 0o022n) === 0n);
    const limit = Number(before.size);
    const checkedBytes = Buffer.alloc(limit + 1);
    let length = 0;
    while (length < checkedBytes.length) {
      const result = await file.read(checkedBytes, length, checkedBytes.length - length, length);
      if (!result.bytesRead) break;
      length += result.bytesRead;
    }
    const bytes = checkedBytes.subarray(0, length);
    const after = await file.stat({ bigint: true });
    check(before.dev === after.dev && before.ino === after.ino && before.size === after.size
      && bytes.length === Number(before.size) && hash(bytes) === digest);
    return bytes;
  } catch { refused(); }
  finally { await file?.close(); }
}

function boundedSignal(signal) {
  check(signal === undefined || (signal && typeof signal.aborted === "boolean"
    && typeof signal.addEventListener === "function" && typeof signal.removeEventListener === "function"));
  const timeout = AbortSignal.timeout(VERIFIER_TIMEOUT_MS);
  return signal ? AbortSignal.any([signal, timeout]) : timeout;
}

async function checkedVerifier(verifier) {
  check(isRecord(verifier, "assertOwner,helperPath,helperSha256,interpreterPath,interpreterSha256"));
  check(typeof verifier.assertOwner === "function"
    && canonicalPath(verifier.interpreterPath) && canonicalPath(verifier.helperPath)
    && canonicalDigest(verifier.interpreterSha256) && canonicalDigest(verifier.helperSha256));
  // The interpreter is a sealed, root-owned CLT executable.  The helper is
  // converted to sealed text below, so spawn never opens its mutable pathname.
  const [interpreterBytes, helperBytes] = await Promise.all([
    sealedFile(verifier.interpreterPath, verifier.interpreterSha256,
      { rootOwned: true, maximumBytes: MAX_INTERPRETER_BYTES }),
    sealedFile(verifier.helperPath, verifier.helperSha256,
      { rootOwned: false, maximumBytes: MAX_HELPER_BYTES }),
  ]);
  let helperText;
  try { helperText = new TextDecoder("utf-8", { fatal: true }).decode(helperBytes); } catch { refused(); }
  check(interpreterBytes.length > 0 && helperText.length > 0);
  return Object.freeze({ ...verifier, helperText });
}

async function assertCurrentOwner(assertOwner, binding, phase, signal) {
  check(!signal.aborted);
  let rejectAbort;
  const aborted = new Promise((_, reject) => { rejectAbort = reject; });
  const onAbort = () => rejectAbort(new Error("aborted"));
  signal.addEventListener("abort", onAbort, { once: true });
  try {
    const accepted = await Promise.race([
      Promise.resolve().then(() => assertOwner(binding, phase)),
      aborted,
    ]);
    check(accepted === true && !signal.aborted);
  } catch { refused(); }
  finally { signal.removeEventListener("abort", onAbort); }
}

function connectPath(path, signal) {
  return new Promise((resolveSocket, reject) => {
    if (signal?.aborted) return reject(new Error("local_qa_peer_refused"));
    const socket = connect({ path }); // Deliberately no host, port, or DNS fallback.
    let settled = false;
    const finish = (error) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", abort);
      if (error) { socket.destroy(); reject(error); } else resolveSocket(socket);
    };
    const abort = () => finish(new Error("local_qa_peer_refused"));
    socket.once("connect", () => finish());
    socket.once("error", () => finish(new Error("local_qa_peer_refused")));
    signal?.addEventListener("abort", abort, { once: true });
  });
}

function attest(socket, binding, verifier, signal) {
  return new Promise((resolveAttested, reject) => {
    if (signal?.aborted) return reject(new Error("local_qa_peer_refused"));
    const child = spawn(verifier.interpreterPath, ["-I", "-S", "-B", "-c", verifier.helperText,
      "--pid", String(binding.pid), "--uid", String(binding.uid)], {
      cwd: binding.root,
      env: { LANG: "C", LC_ALL: "C", PATH: "/usr/bin:/bin", TZ: "UTC" },
      shell: false,
      stdio: [socket, "pipe", "pipe"],
    });
    let settled = false, failure = null, killTimer = null;
    const timer = setTimeout(() => fail(new Error("local_qa_peer_refused")), VERIFIER_TIMEOUT_MS);
    const settle = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer); clearTimeout(killTimer);
      signal?.removeEventListener("abort", abort);
      if (error) reject(error); else resolveAttested();
    };
    const fail = (error) => {
      if (failure || settled) return;
      failure = error;
      child.kill("SIGTERM");
      killTimer = setTimeout(() => child.kill("SIGKILL"), 250);
    };
    const abort = () => fail(new Error("local_qa_peer_refused"));
    child.once("error", () => fail(new Error("local_qa_peer_refused")));
    // The verifier is a pure inherited-FD inspector: any output is a protocol breach.
    child.stdout.once("data", () => fail(new Error("local_qa_peer_refused")));
    child.stderr.once("data", () => fail(new Error("local_qa_peer_refused")));
    child.once("close", (code, childSignal) => {
      if (failure) settle(failure);
      else if (code === 0 && childSignal === null) settle();
      else settle(new Error("local_qa_peer_refused"));
    });
    signal?.addEventListener("abort", abort, { once: true });
  });
}

/**
 * Connect one admitted local Unix socket and attest its kernel peer identity.
 * `assertOwner` is required before connection and before the held stream is
 * returned, because PID identity alone is not a process-lifetime witness.
 */
export async function connectOwnedUnix(binding, verifier, signal) {
  check(process.platform === "darwin");
  const deadline = boundedSignal(signal);
  const [checkedBinding, checked] = await Promise.all([checkedUnixBinding(binding), checkedVerifier(verifier)]);
  await assertCurrentOwner(checked.assertOwner, checkedBinding, "before_connect", deadline);
  const socket = await connectPath(checkedBinding.path, deadline);
  try {
    await checkedUnixBinding(checkedBinding);
    await attest(socket, checkedBinding, checked, deadline);
    await assertCurrentOwner(checked.assertOwner, checkedBinding, "before_release", deadline);
    if (deadline.aborted || !(socket instanceof Socket) || socket.readyState !== "open") refused();
    return socket;
  } catch (error) {
    socket.destroy();
    throw error;
  }
}

// Adapts sealed verifier configuration to the two-argument connector shape.
// The caller still has to bind its separately admitted PID into `binding`; the
// gateway's pathname-only target intentionally cannot satisfy that authority.
export function createConnectOwnedUnix(verifier) {
  return async (binding, signal) => connectOwnedUnix(binding, verifier, signal);
}
