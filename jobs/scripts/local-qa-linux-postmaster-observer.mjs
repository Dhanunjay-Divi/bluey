// Linux-only, read-only process proof for the already-running local-QA
// PostgreSQL postmaster.  This deliberately does not launch, signal, or scan
// processes: every /proc path below is derived from the caller's pinned PID.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, opendir, readlink, realpath, stat } from "node:fs/promises";
import { join, resolve } from "node:path";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";

const REFUSAL = "BLUEY_LOCAL_QA_LINUX_POSTMASTER_REFUSED";
const MAX_PROC_TEXT = 64 * 1024;
const MAX_FDS = 512;
const MAX_EXECUTABLE = 256 * 1024 * 1024;
const PID = /^[1-9][0-9]{0,9}$/;
const DECIMAL = /^(?:0|[1-9][0-9]{0,19})$/;
const SHA256 = /^[0-9a-f]{64}$/;
const SOCKET = /^socket:\[([1-9][0-9]{0,19})\]$/;
const NETNS = /^net:\[([1-9][0-9]{0,19})\]$/;

function refuse() { throw new Error(REFUSAL); }
function need(value) { if (!value) refuse(); }
function canonicalPid(value) {
  need(Number.isSafeInteger(value) && value > 1 && value <= 2_147_483_647);
  return String(value);
}
function processIdText(value, minimum = 2) {
  need(PID.test(value));
  const number = Number(value);
  need(Number.isSafeInteger(number) && number >= minimum && number <= 2_147_483_647);
  return number;
}
function positiveU64(value) {
  need(DECIMAL.test(value) && value !== "0" && BigInt(value) <= 18_446_744_073_709_551_615n);
  return value;
}
function canonicalPort(value) {
  need(Number.isSafeInteger(value) && value > 1024 && value <= 65535 && value !== 5432);
  return value;
}
function canonicalRoot(root) {
  need(typeof root === "string" && resolve(root) === root && root === process.env.BLUEY_TEST_RUN_ROOT
    && isDurableQaFixtureRoot(root));
  return root;
}
function stamp(value) {
  return [value.dev, value.ino, value.uid, value.mode, value.nlink, value.size, value.mtimeNs, value.ctimeNs]
    .map(String).join(":");
}

async function boundedText(path, maximum = MAX_PROC_TEXT) {
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK);
  try {
    const pieces = [], buffer = Buffer.allocUnsafe(4096);
    let length = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
      if (bytesRead === 0) break;
      length += bytesRead;
      need(length <= maximum);
      pieces.push(Buffer.from(buffer.subarray(0, bytesRead)));
    }
    const bytes = Buffer.concat(pieces, length);
    need(!bytes.includes(0));
    return bytes.toString("utf8");
  } finally { await file.close(); }
}

async function boundedSha256(path) {
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.size > 0n && before.size <= BigInt(MAX_EXECUTABLE));
    const hash = createHash("sha256"), buffer = Buffer.allocUnsafe(64 * 1024);
    let total = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
      if (bytesRead === 0) break;
      total += bytesRead; need(total <= MAX_EXECUTABLE);
      hash.update(buffer.subarray(0, bytesRead));
    }
    const after = await file.stat({ bigint: true });
    need(BigInt(total) === before.size && stamp(before) === stamp(after));
    return Object.freeze({ stamp: stamp(before), sha256: hash.digest("hex") });
  } finally { await file.close(); }
}

/** Parse the minimal stable postmaster identity from one bounded /proc/PID/stat value. */
function parseLinuxProcessStat(text, expectedPid, parentMinimum) {
  try {
    const pid = canonicalPid(expectedPid);
    need(typeof text === "string" && text.length > 1 && text.length <= MAX_PROC_TEXT && text.endsWith("\n")
      && !/[\0\r]/.test(text) && !text.slice(0, -1).includes("\n"));
    text = text.slice(0, -1);
    const close = text.lastIndexOf(") ");
    need(close > pid.length && text.startsWith(`${pid} (`));
    const fields = text.slice(close + 2).split(" ");
    // state, ppid, pgrp, session ... starttime (field 22, index 19 after state)
    need(fields.length >= 20 && /^[A-Z]$/.test(fields[0]));
    return Object.freeze({ pid: Number(pid), ppid: processIdText(fields[1], parentMinimum),
      pgrp: processIdText(fields[2]), startTime: positiveU64(fields[19]) });
  } catch { refuse(); }
}

export const parseLinuxPostmasterStat = (text, pid) => parseLinuxProcessStat(text, pid, 2);
export const parseLinuxUnitMainStat = (text, pid) => parseLinuxProcessStat(text, pid, 1);

/** Require the real UID line of a bounded /proc/PID/status value to be exactly this user. */
export function requireLinuxPostmasterUid(statusText, uid) {
  try {
    need(Number.isSafeInteger(uid) && uid >= 0 && typeof statusText === "string"
      && statusText.length > 0 && statusText.length <= MAX_PROC_TEXT && !statusText.includes("\0"));
    const matches = statusText.match(/^Uid:\s*([0-9]+)\s+([0-9]+)\s+([0-9]+)\s+([0-9]+)\s*$/m);
    need(matches !== null && (statusText.match(/^Uid:/gm) ?? []).length === 1
      && matches.slice(1).every(value => value === String(uid)));
    return true;
  } catch { refuse(); }
}

function tcpRows(text, family) {
  need(typeof text === "string" && text.length > 0 && text.length <= MAX_PROC_TEXT && !text.includes("\0"));
  const lines = text.split("\n");
  const remoteHeader = family === 4 ? "rem_address" : "remote_address";
  need(lines.length >= 2 && lines.length <= 1025
    && new RegExp(`^\\s*sl\\s+local_address\\s+${remoteHeader}\\s+st\\s+tx_queue(?::|\\s+)rx_queue\\s+tr\\s+tm->when\\s+retrnsmt\\s+uid\\s+timeout\\s+inode\\s*$`).test(lines[0]));
  const rows = [];
  for (const line of lines.slice(1)) {
    if (line === "") continue;
    const values = line.trim().split(/\s+/);
    const width = family === 4 ? 8 : 32;
    const endpoint = new RegExp(`^[0-9A-F]{${width}}:[0-9A-F]{4}$`);
    need(values.length >= 10 && /^[0-9]+:$/.test(values[0]) && endpoint.test(values[1])
      && endpoint.test(values[2]) && /^[0-9A-F]{2}$/.test(values[3])
      && DECIMAL.test(values[9]));
    const [address, port] = values[1].split(":");
    need(address.length === (family === 4 ? 8 : 32));
    rows.push(Object.freeze({ address, port: Number.parseInt(port, 16), state: values[3], inode: values[9] }));
  }
  return rows;
}

/**
 * Require one owned IPv4 127.0.0.1 listener and no owned extra listener.
 * The two /proc/PID/net tables are namespace-wide, so unowned rows are used
 * only to reject an endpoint collision; ownership is derived solely from the
 * postmaster's own descriptor inode set.
 */
export function requireOwnedLinuxLoopbackListener(tcp4Text, tcp6Text, socketInodes, port, allowAbsent = false) {
  try {
    canonicalPort(port);
    need((allowAbsent === true || allowAbsent === false) && socketInodes instanceof Set
      && (allowAbsent || socketInodes.size > 0) && socketInodes.size <= MAX_FDS
      && [...socketInodes].every(value => typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value)));
    const four = tcpRows(tcp4Text, 4), six = tcpRows(tcp6Text, 6);
    const listening = [...four.map(row => ({ ...row, family: 4 })), ...six.map(row => ({ ...row, family: 6 }))]
      .filter(row => row.state === "0A");
    const owned = listening.filter(row => socketInodes.has(row.inode));
    if (allowAbsent && owned.length === 0 && !listening.some(row => row.port === port)) return null;
    need(owned.length === 1);
    const listener = owned[0];
    need(listener.family === 4 && listener.address === "0100007F" && listener.port === port);
    // Any wildcard or IPv6 listener on the requested port can receive the same
    // traffic.  A second 127.0.0.1 listener is also a collision, regardless of owner.
    need(!listening.some(row => row !== listener && row.port === port));
    return listener.inode;
  } catch { refuse(); }
}

// Kept as the PostgreSQL-specific public contract.  The API process reuses the
// identical closed listener proof through the generic name above.
export const requireOwnedLinuxPgListener = (tcp4, tcp6, inodes, port) =>
  requireOwnedLinuxLoopbackListener(tcp4, tcp6, inodes, port);

async function socketInodesForPid(proc, allowEmpty = false) {
  const directory = await opendir(join(proc, "fd"));
  const values = new Set(); let count = 0;
  try {
    for await (const entry of directory) {
      count += 1; need(count <= MAX_FDS && entry.isSymbolicLink() && /^[0-9]+$/.test(entry.name));
      const target = await readlink(join(proc, "fd", entry.name));
      const match = SOCKET.exec(target); if (match) values.add(match[1]);
    }
  } finally { await directory.close().catch(() => {}); }
  need((allowEmpty || values.size > 0) && values.size <= MAX_FDS);
  return values;
}

/** Read only descriptors belonging to this PID, then prove its loopback listener. */
export async function observeOwnedLinuxLoopbackListener(proc, port, allowAbsent = false) {
  try {
    return requireOwnedLinuxLoopbackListener(await boundedText(join(proc, "net", "tcp")),
      await boundedText(join(proc, "net", "tcp6")), await socketInodesForPid(proc, allowAbsent), port, allowAbsent);
  } catch { refuse(); }
}

export async function assertLinuxTcpPortVacant(port) {
  canonicalPort(port);
  const tcp4 = tcpRows(await boundedText("/proc/self/net/tcp"), 4);
  const tcp6 = tcpRows(await boundedText("/proc/self/net/tcp6"), 6);
  need(![...tcp4, ...tcp6].some(row => row.state === "0A" && row.port === port));
}

async function cgroupForPid(proc, expected) {
  need(typeof expected === "string" && /^\/(?:[^\0\r\n]+)?$/.test(expected));
  const lines = (await boundedText(join(proc, "cgroup"))).split("\n");
  need(lines.length === 2 && lines[1] === "" && lines[0] === `0::${expected}`);
}

async function namespaceForPid(proc, expected) {
  need(typeof expected === "string" && NETNS.test(expected));
  need(await readlink(join(proc, "ns", "net")) === expected);
}

async function executableForPid(proc, postgresPath, expectedSha, pathStamp) {
  const named = await lstat(postgresPath, { bigint: true });
  need(named.isFile() && stamp(named) === pathStamp);
  const namedDigest = await boundedSha256(postgresPath);
  need(namedDigest.stamp === pathStamp && namedDigest.sha256 === expectedSha);
  need(await readlink(join(proc, "exe")) === postgresPath);
  const executable = await stat(join(proc, "exe"), { bigint: true });
  need(executable.isFile() && stamp(executable) === pathStamp);
  const actual = await boundedSha256(join(proc, "exe"));
  need(actual.stamp === pathStamp && actual.sha256 === expectedSha);
}

/**
 * Build a read-only assertion callback for attachLocalQaLinuxPostgres.  The
 * supplied unit assertion is intentionally external: only the caller that
 * created the audited unit can prove its systemd ownership.
 */
export async function createLocalQaLinuxPostmasterObserver({ root, postgresPath, postgresSha256,
  unitCgroup, networkNamespace, assertOwnedUnit }) {
  try {
    canonicalRoot(root);
    need(process.platform === "linux" && typeof process.getuid === "function" && typeof process.pid === "number"
      && typeof postgresPath === "string" && resolve(postgresPath) === postgresPath && SHA256.test(postgresSha256)
      && typeof assertOwnedUnit === "function" && await realpath(postgresPath) === postgresPath);
    // The observer itself is the QA worker.  Bind its namespace before using a
    // PID-relative network table, otherwise a PID namespace coincidence could
    // make a postmaster appear to own a socket from another network namespace.
    need(typeof networkNamespace === "string" && NETNS.test(networkNamespace)
      && await readlink("/proc/self/ns/net") === networkNamespace);
    const pinned = await boundedSha256(postgresPath);
    need(pinned.sha256 === postgresSha256);
    const pathStat = await lstat(postgresPath, { bigint: true });
    // Package-managed PostgreSQL is normally root-owned.  Its authority here
    // is the canonical path, stable inode metadata and pinned digest, not a
    // user-writable ownership bit.
    need(pathStat.isFile() && stamp(pathStat) === pinned.stamp);
    const pinnedStamp = pinned.stamp;
    // This closure is per attachment, never global process state.  A caller
    // constructs a new observer after a legitimate postmaster restart.
    let attached = null;
    return async function assertOwnedPostmaster({ pid, port, dataDirectory, root: observedRoot }) {
      try {
        const pidText = canonicalPid(pid); canonicalPort(port); canonicalRoot(observedRoot);
        need(observedRoot === root && dataDirectory === join(root, "bluey-jobs-qa-draft", "postgres")
          && await realpath(dataDirectory) === dataDirectory);
        await namespaceForPid("/proc/self", networkNamespace);
        const proc = join("/proc", pidText);
        const before = parseLinuxPostmasterStat(await boundedText(join(proc, "stat")), pid);
        need(before.ppid === process.pid);
        requireLinuxPostmasterUid(await boundedText(join(proc, "status")), process.getuid());
        await cgroupForPid(proc, unitCgroup); await namespaceForPid(proc, networkNamespace);
        need(await assertOwnedUnit(Object.freeze({ pid, port, root, dataDirectory, unitCgroup, networkNamespace })) === true);
        await executableForPid(proc, postgresPath, postgresSha256, pinnedStamp);
        const inode = await observeOwnedLinuxLoopbackListener(proc, port);
        await cgroupForPid(proc, unitCgroup); await namespaceForPid(proc, networkNamespace);
        need(await assertOwnedUnit(Object.freeze({ pid, port, root, dataDirectory, unitCgroup, networkNamespace })) === true);
        await executableForPid(proc, postgresPath, postgresSha256, pinnedStamp);
        const inodeAfter = await observeOwnedLinuxLoopbackListener(proc, port);
        const after = parseLinuxPostmasterStat(await boundedText(join(proc, "stat")), pid);
        requireLinuxPostmasterUid(await boundedText(join(proc, "status")), process.getuid());
        need(after.ppid === process.pid && after.pgrp === before.pgrp
          && after.startTime === before.startTime && inodeAfter === inode && inode.length > 0);
        await namespaceForPid("/proc/self", networkNamespace);
        const current = Object.freeze({ pid, startTime: after.startTime, pgrp: after.pgrp, inode, port });
        if (attached === null) attached = current;
        else need(attached.pid === current.pid && attached.startTime === current.startTime
          && attached.pgrp === current.pgrp && attached.inode === current.inode && attached.port === current.port);
        return true;
      } catch { refuse(); }
    };
  } catch { refuse(); }
}
