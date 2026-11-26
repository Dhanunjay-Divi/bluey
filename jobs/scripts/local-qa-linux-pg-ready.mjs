// Linux-only durable local-QA PostgreSQL ready-marker writer.  The caller owns
// initdb, postgres, SQL, and process lifetime; this module only binds the
// current portable local-QA state to a stopped PostgreSQL data directory.
import { createHash, randomBytes } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readdir, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { claimLocalQaPostgresInitialization, inspectLocalQaState } from "./local-qa-state.mjs";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";

const STATE_NAME = "bluey-jobs-qa-draft";
const READY = ".local-qa-postgres-ready.json";
const PG_AUTH_FILE = ".local-qa-postgres-password";
const PID = "postmaster.pid";
const CONFIG = Object.freeze([
  "postgresql.conf",
  "postgresql.auto.conf",
  "pg_hba.conf",
  "pg_ident.conf",
  "PG_VERSION",
]);
const SHA256 = /^[0-9a-f]{64}$/;
const U64 = /^[1-9][0-9]{0,19}$/;
const REFUSAL = "local_qa_linux_pg_ready_refused";

function refuse() { throw new Error(REFUSAL); }
function need(value) { if (!value) refuse(); }
function digest(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
function directoryIdentity(stat) {
  return [stat.dev, stat.ino, stat.uid, stat.mode].map(String).join(":");
}
function fileIdentity(stat) {
  return [stat.dev, stat.ino, stat.uid, stat.mode, stat.nlink, stat.size, stat.mtimeNs, stat.ctimeNs]
    .map(String).join(":");
}
function exact(value) { return `${JSON.stringify(value)}\n`; }
function validSystemIdentifier(value) {
  return typeof value === "string" && U64.test(value) && BigInt(value) <= 18_446_744_073_709_551_615n;
}
function exactKeys(value, names) {
  return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).join(",") === names.join(",");
}

async function absent(path) {
  try { await lstat(path); return false; }
  catch (error) { if (error?.code === "ENOENT") return true; throw error; }
}

async function privateDirectory(path) {
  need(typeof path === "string" && resolve(path) === path && await realpath(path) === path);
  const stat = await lstat(path, { bigint: true });
  need(stat.isDirectory() && stat.uid === BigInt(process.getuid()) && (stat.mode & 0o7777n) === 0o700n);
  return Object.freeze({ path, identity: directoryIdentity(stat) });
}

async function privateFile(path, maximum) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o600n && before.size > 0n && before.size <= BigInt(maximum));
    const bytes = Buffer.alloc(Number(before.size));
    let used = 0;
    while (used < bytes.length) {
      const result = await file.read(bytes, used, bytes.length - used, null);
      need(result.bytesRead >= 0 && used + result.bytesRead <= bytes.length);
      used += result.bytesRead;
      if (result.bytesRead === 0) break;
    }
    const after = await file.stat({ bigint: true });
    const named = await lstat(path, { bigint: true });
    need(used === bytes.length && BigInt(bytes.length) === before.size && fileIdentity(before) === fileIdentity(after)
      && fileIdentity(before) === fileIdentity(named));
    return Object.freeze({ path, bytes, identity: fileIdentity(before) });
  } finally {
    await file.close();
  }
}

async function syncDirectory(path) {
  const directory = await open(path, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try { await directory.sync(); }
  finally { await directory.close(); }
}

async function createPrivate(path, bytes) {
  const file = await open(path,
    constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
  try {
    await file.writeFile(bytes);
    await file.sync();
    await file.chmod(0o600);
    const stat = await file.stat({ bigint: true });
    need(stat.isFile() && stat.uid === BigInt(process.getuid()) && stat.nlink === 1n
      && (stat.mode & 0o7777n) === 0o600n && stat.size === BigInt(bytes.length));
  } finally {
    await file.close();
  }
  const readback = await privateFile(path, 8192);
  need(readback.bytes.equals(bytes));
  return readback;
}

export function parseLocalQaLinuxPgReadyMarker(bytes, { qaId, rootIdentity, dataIdentity }) {
  need(Buffer.isBuffer(bytes) && typeof qaId === "string" && typeof rootIdentity === "string"
    && typeof dataIdentity === "string");
  let marker;
  try { marker = JSON.parse(bytes.toString("utf8")); }
  catch { refuse(); }
  need(exactKeys(marker, [
    "version", "qaId", "rootIdentity", "dataIdentity", "systemIdentifier", "passwordSha256", "configHashes",
  ]) && exact(marker) === bytes.toString("utf8"));
  need(marker.version === 1 && marker.qaId === qaId && marker.rootIdentity === rootIdentity
    && marker.dataIdentity === dataIdentity && validSystemIdentifier(marker.systemIdentifier)
    && typeof marker.passwordSha256 === "string" && SHA256.test(marker.passwordSha256)
    && exactKeys(marker.configHashes, CONFIG));
  for (const name of CONFIG) need(typeof marker.configHashes[name] === "string" && SHA256.test(marker.configHashes[name]));
  return Object.freeze(marker);
}

/**
 * Bind one fresh current local-QA state directory to its Linux-owned PostgreSQL
 * ready marker.  This module never starts a process or sends SQL.
 */
export async function createLocalQaLinuxPgReady({ root, stateDirectory, assertUnit }) {
  try {
    need(process.platform === "linux" && typeof process.getuid === "function" && process.getuid() !== 0
      && typeof assertUnit === "function" && root === process.env.BLUEY_TEST_RUN_ROOT
      && isDurableQaFixtureRoot(root) && stateDirectory === join(root, STATE_NAME));

    async function unit() { need(await assertUnit() === true); }
    async function inspectBound(expectedInitialization) {
      await unit();
      const parent = await privateDirectory(dirname(root));
      const run = await privateDirectory(root);
      const state = await privateDirectory(stateDirectory);
      const current = await inspectLocalQaState(stateDirectory);
      need(current.kind === "bluey_jobs_durable_local_qa" && current.stateDirectory === stateDirectory
        && current.postgresDirectory === join(stateDirectory, "postgres")
        && current.postgresInitialization === expectedInitialization);
      const data = await privateDirectory(current.postgresDirectory);
      return Object.freeze({ parent, run, state, data, current });
    }

    await unit();
    const initialState = await inspectLocalQaState(stateDirectory);
    need(initialState.postgresInitialization === "unclaimed" || initialState.postgresInitialization === "claimed");
    const initial = await inspectBound(initialState.postgresInitialization);
    const initialQaId = initial.current.qaId;
    const initialParentIdentity = initial.parent.identity;
    const initialRootIdentity = initial.run.identity;
    const initialStateIdentity = initial.state.identity;
    const initialDataIdentity = initial.data.identity;
    let prepared = initialState.postgresInitialization === "claimed";
    let committed = prepared;
    let passwordIdentity;
    let passwordValue;
    let readyIdentity;
    let committedReady;

    async function assertState() {
      const bound = await inspectBound(prepared ? "claimed" : "unclaimed");
      need(bound.current.qaId === initialQaId && bound.parent.identity === initialParentIdentity
        && bound.run.identity === initialRootIdentity && bound.state.identity === initialStateIdentity
        && bound.data.identity === initialDataIdentity);
      return Object.freeze({ qaId: bound.current.qaId, postgresInitialization: bound.current.postgresInitialization });
    }

    async function assertStopped(bound) {
      need(await absent(join(bound.data.path, PID)));
    }

    async function currentConfig(bound) {
      const hashes = {};
      for (const name of CONFIG) {
        const file = await privateFile(join(bound.data.path, name), 1024 * 1024);
        if (name === "PG_VERSION") need(file.bytes.equals(Buffer.from("17\n")));
        hashes[name] = digest(file.bytes);
      }
      return Object.freeze(hashes);
    }

    async function bindStoppedReady() {
      const bound = await assertState();
      const current = await inspectBound("claimed");
      need(bound.qaId === initialQaId && bound.postgresInitialization === "claimed");
      await assertStopped(current);
      const readyFile = await privateFile(join(current.state.path, READY), 8192);
      const ready = parseLocalQaLinuxPgReadyMarker(readyFile.bytes, {
        qaId: initialQaId, rootIdentity: initialStateIdentity, dataIdentity: initialDataIdentity,
      });
      const password = await privateFile(join(current.state.path, PG_AUTH_FILE), 128);
      need(/^[0-9a-f]{64}\n$/.test(password.bytes.toString("utf8"))
        && digest(password.bytes) === ready.passwordSha256);
      const hashes = await currentConfig(current);
      for (const name of CONFIG) need(hashes[name] === ready.configHashes[name]);
      await assertStopped(current);
      await assertState();
      return Object.freeze({ bound: current, readyFile, ready, password, hashes });
    }

    if (prepared) {
      const existing = await bindStoppedReady();
      passwordIdentity = existing.password.identity;
      passwordValue = existing.password.bytes.subarray(0, 64).toString("utf8");
      existing.password.bytes.fill(0);
      readyIdentity = existing.readyFile.identity;
      committedReady = existing.ready;
    }

    async function prepareInitialPassword() {
      await unit();
      need(!prepared && !committed);
      await assertState();
      const bound = await inspectBound("unclaimed");
      need(await absent(join(bound.state.path, READY)) && await absent(join(bound.state.path, PG_AUTH_FILE)));
      await assertStopped(bound);
      need((await readdir(bound.data.path)).length === 0);
      const claimed = await claimLocalQaPostgresInitialization(stateDirectory);
      need(claimed.qaId === initialQaId && claimed.postgresInitialization === "claimed");
      prepared = true;
      await assertState();
      const bytes = Buffer.from(`${randomBytes(32).toString("hex")}\n`);
      const file = await createPrivate(join(stateDirectory, PG_AUTH_FILE), bytes);
      await syncDirectory(stateDirectory);
      passwordIdentity = file.identity;
      passwordValue = bytes.subarray(0, 64).toString("utf8");
      bytes.fill(0);
      return passwordValue;
    }

    async function snapshot() {
      await unit();
      need(prepared);
      await assertState();
      const bound = await inspectBound("claimed");
      await assertStopped(bound);
      const hashes = await currentConfig(bound);
      await assertStopped(bound);
      await assertState();
      return hashes;
    }

    async function commit({ password, systemIdentifier, configHashes }) {
      await unit();
      need(prepared && !committed && typeof password === "string" && /^[0-9a-f]{64}$/.test(password)
        && password === passwordValue && validSystemIdentifier(systemIdentifier) && exactKeys(configHashes, CONFIG));
      await assertState();
      const bound = await inspectBound("claimed");
      await assertStopped(bound);
      need(await absent(join(bound.state.path, READY)));
      const passwordFile = await privateFile(join(bound.state.path, PG_AUTH_FILE), 128);
      need(passwordFile.identity === passwordIdentity && passwordFile.bytes.equals(Buffer.from(`${password}\n`)));
      const actual = await currentConfig(bound);
      for (const name of CONFIG) need(typeof configHashes[name] === "string" && SHA256.test(configHashes[name])
        && configHashes[name] === actual[name]);
      const marker = {
        version: 1,
        qaId: initialQaId,
        rootIdentity: initialStateIdentity,
        dataIdentity: initialDataIdentity,
        systemIdentifier,
        passwordSha256: digest(passwordFile.bytes),
        configHashes: Object.fromEntries(CONFIG.map(name => [name, actual[name]])),
      };
      const bytes = Buffer.from(exact(marker));
      const file = await createPrivate(join(bound.state.path, READY), bytes);
      await syncDirectory(bound.state.path);
      const reread = await bindStoppedReady();
      need(file.identity === reread.readyFile.identity && reread.ready.systemIdentifier === systemIdentifier
        && reread.ready.passwordSha256 === marker.passwordSha256);
      bytes.fill(0);
      readyIdentity = reread.readyFile.identity;
      committedReady = reread.ready;
      committed = true;
      return Object.freeze({ systemIdentifier: reread.ready.systemIdentifier,
        configHashes: Object.freeze({ ...reread.ready.configHashes }) });
    }

    async function read() {
      await unit();
      need(prepared && committed && committedReady !== undefined);
      const current = await bindStoppedReady();
      need(current.readyFile.identity === readyIdentity
        && current.ready.systemIdentifier === committedReady.systemIdentifier);
      need(current.password.identity === passwordIdentity);
      const value = current.password.bytes.subarray(0, 64).toString("utf8");
      current.password.bytes.fill(0);
      return Object.freeze({ systemIdentifier: current.ready.systemIdentifier, password: value,
        configHashes: Object.freeze({ ...current.hashes }) });
    }

    return Object.freeze({ dataDirectory: initial.data.path, assertState, prepareInitialPassword, snapshot, commit, read });
  } catch {
    refuse();
  }
}
