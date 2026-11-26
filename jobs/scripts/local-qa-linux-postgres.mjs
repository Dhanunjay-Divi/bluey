// Linux-only, read-only attachment to an already started durable local-QA
// PostgreSQL instance.  This is deliberately not a PostgreSQL launcher,
// credential provider, or general SQL client.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";

const STATE = "bluey-jobs-qa-draft";
const ROOT_MARKER = ".bluey-test-run-v2";
const STATE_MARKER = ".bluey-local-qa.json";
const READY = ".local-qa-postgres-ready.json";
const PG_AUTH_FILE = ".local-qa-postgres-password";
const BOOTSTRAP = ".local-qa-bootstrap.json";
const OBJECT_MARKER = ".local-qa-object-store.json";
const CONFIG = Object.freeze(["postgresql.conf", "postgresql.auto.conf", "pg_hba.conf", "pg_ident.conf", "PG_VERSION"]);
const SHA256 = /^[0-9a-f]{64}$/;
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const U64 = /^[1-9][0-9]{0,19}$/;
const REFUSAL = "BLUEY_LOCAL_QA_LINUX_POSTGRES_REFUSED";

// The callback is the sole SQL executor.  It must issue precisely this query,
// using the already authenticated connection it owns.
export const LINUX_DURABLE_PG_IDENTITY_SQL = "SELECT current_setting('data_directory') AS \"dataDirectory\", host(inet_server_addr()) AS address, inet_server_port() AS port, current_database() AS database, current_user AS role, system_identifier::text AS \"systemIdentifier\" FROM pg_control_system()";

function refuse() { throw new Error(REFUSAL); }
function need(value) { if (!value) refuse(); }
function plain(value) { return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype; }
function keys(value, names) { return plain(value) && Object.keys(value).sort().join("|") === [...names].sort().join("|"); }
function sha(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
function identity(stat) { return [stat.dev, stat.ino, stat.uid, stat.mode].map(String).join(":"); }
function fileIdentity(stat) { return [stat.dev, stat.ino, stat.uid, stat.mode, stat.nlink, stat.size, stat.mtimeNs, stat.ctimeNs].map(String).join(":"); }
function systemIdentifier(value) { return typeof value === "string" && U64.test(value) && BigInt(value) <= 18_446_744_073_709_551_615n; }
function canonicalJson(value, bytes, pretty = false) {
  return Buffer.from((pretty ? JSON.stringify(value, null, 2) : JSON.stringify(value)) + "\n").equals(bytes);
}

async function privateDirectory(path) {
  need(typeof path === "string" && resolve(path) === path && await realpath(path) === path);
  const stat = await lstat(path, { bigint: true });
  need(stat.isDirectory() && stat.uid === BigInt(process.getuid()) && (stat.mode & 0o7777n) === 0o700n);
  return Object.freeze({ path, identity: identity(stat), fileIdentity: fileIdentity(stat) });
}

async function privateFile(path, maximum) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o600n && before.size > 0n && before.size <= BigInt(maximum));
    const buffer = Buffer.alloc(maximum + 1);
    let length = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, length, buffer.length - length, null);
      length += bytesRead;
      need(length <= maximum);
      if (bytesRead === 0) break;
    }
    const bytes = buffer.subarray(0, length);
    const after = await file.stat({ bigint: true });
    const named = await lstat(path, { bigint: true });
    need(BigInt(bytes.length) === before.size && fileIdentity(before) === fileIdentity(after)
      && fileIdentity(before) === fileIdentity(named));
    return Object.freeze({ path, bytes, identity: fileIdentity(before) });
  } finally { await file.close(); }
}

function json(file, maximum, pretty = false) {
  need(file.bytes.length <= maximum);
  let value;
  try { value = JSON.parse(file.bytes.toString("utf8")); } catch { refuse(); }
  need(canonicalJson(value, file.bytes, pretty));
  return value;
}

function parseDatabaseUrl(value) {
  need(typeof value === "string" && value.length > 0 && value.length <= 1024 && !/[\0\r\n]/.test(value));
  let url; try { url = new URL(value); } catch { refuse(); }
  const port = Number(url.port);
  need(url.protocol === "postgresql:" && url.hostname === "127.0.0.1" && Number.isInteger(port)
    && port > 1024 && port <= 65535 && port !== 5432 && url.username === "bluey_qa_local"
    && SHA256.test(url.password) && url.pathname === "/bluey_jobs_qa" && url.search === "?sslmode=disable" && url.hash === "");
  return Object.freeze({ port, password: url.password });
}

function validateRootMarker(bytes, root) {
  const text = bytes.toString("utf8"), lines = text.split("\n");
  need(text.endsWith("\n") && lines.length === 10 && lines[9] === "" && lines[0] === "magic=bluey-test-run-v2");
  const fields = lines.slice(1, 9).map(line => /^([^=]+)=([^=]+)$/.exec(line)).map(match => match?.slice(1));
  need(fields.every(field => field !== undefined) && fields.map(([key]) => key).join("|")
    === "run_root|run_name|owner_uid|owner_pid|process_group_id|launcher_group_id|created_epoch|state");
  const value = name => fields.find(([key]) => key === name)?.[1];
  const name = value("run_name"), owner = value("owner_pid"), epoch = value("created_epoch");
  need(value("run_root") === root && name === root.slice(root.lastIndexOf("/") + 1)
    && value("owner_uid") === String(process.getuid()) && U64.test(owner) && BigInt(owner) > 1n
    && U64.test(value("process_group_id")) && BigInt(value("process_group_id")) > 1n
    && U64.test(value("launcher_group_id")) && BigInt(value("launcher_group_id")) > 1n
    && value("process_group_id") !== value("launcher_group_id") && U64.test(epoch) && value("state") === "running");
  const match = /^run\.([1-9][0-9]{0,19})\.([1-9][0-9]{0,9})\.([A-Za-z0-9]{1,64})$/.exec(name);
  need(match !== null && match[1] === epoch && match[2] === owner);
}

export async function assertLocalQaLinuxPostgresRunRoot(root) {
  try {
    need(process.platform === "linux" && typeof process.getuid === "function" && process.getuid() > 0
      && root === process.env.BLUEY_TEST_RUN_ROOT && isDurableQaFixtureRoot(root));
    const parent = await privateDirectory(dirname(root)), run = await privateDirectory(root);
    const marker = await privateFile(join(root, ROOT_MARKER), 4096);
    validateRootMarker(marker.bytes, root);
    return [parent.identity, run.identity, marker.identity].join("|");
  } catch { refuse(); }
}

function durableDataKeyHash(value) {
  need(typeof value === "string" && value === value.trim());
  let decoded;
  if (/^[0-9a-fA-F]{64}$/.test(value)) decoded = Buffer.from(value, "hex");
  else if (/^[A-Za-z0-9+/]{43}=$/.test(value)) decoded = Buffer.from(value, "base64");
  else { refuse(); }
  need(decoded.length === 32 && decoded.some(byte => byte !== decoded[0])
    && !decoded.equals(createHash("sha256").update("bluey-jobs-development-only-key").digest()));
  return sha(Buffer.from(value + "\n"));
}

function readPostmaster(file, expectedData, expectedPort) {
  const lines = file.bytes.toString("utf8").split("\n");
  need(lines.length >= 5 && /^[1-9][0-9]{0,9}$/.test(lines[0]) && Number(lines[0]) > 1
    && Number(lines[0]) <= 2_147_483_647 && lines[1] === expectedData && lines[3] === String(expectedPort));
  return Number(lines[0]);
}

// PostgreSQL 17's PM_STATUS_READY is the eighth line of postmaster.pid.
export function isLocalQaLinuxPostmasterReady(bytes, { pid, dataDirectory, port }) {
  need(Buffer.isBuffer(bytes) && bytes.length > 0 && bytes.length <= 8192 && !bytes.includes(0));
  const lines = bytes.toString("utf8").split("\n");
  if (lines.length < 5) return false; // Initial lockfile construction.
  need(readPostmaster({ bytes }, dataDirectory, port) === pid);
  return lines.length === 9 && lines[4] === "" && lines[5] === "127.0.0.1"
    && lines[7] === "ready   " && lines[8] === "";
}

export async function observeLocalQaLinuxPostmasterReady({ root, pid, dataDirectory, port }) {
  await assertLocalQaLinuxPostgresRunRoot(root);
  need(dataDirectory === join(root, STATE, "postgres"));
  try {
    const file = await privateFile(join(dataDirectory, "postmaster.pid"), 8192);
    return isLocalQaLinuxPostmasterReady(file.bytes, { pid, dataDirectory, port });
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    refuse();
  }
}

/** Validate the closed physical identity shape returned by queryIdentity. */
export function validateLinuxDurablePgIdentity(observation, expected) {
  const names = ["address", "port", "database", "role", "dataDirectory", "systemIdentifier"];
  need(keys(observation, names) && keys(expected, names));
  need(observation.address === "127.0.0.1" && Number.isSafeInteger(observation.port)
    && observation.port > 1024 && observation.port <= 65535 && observation.port !== 5432 && observation.database === "bluey_jobs_qa"
    && observation.role === "bluey_qa_local" && typeof observation.dataDirectory === "string"
    && systemIdentifier(observation.systemIdentifier));
  for (const name of names) need(observation[name] === expected[name]);
  return Object.freeze(Object.fromEntries(names.map(name => [name, observation[name]])));
}

async function verifySnapshot(root, stateDirectory, dataDirectory, snapshot) {
  const parent = await privateDirectory(dirname(root)), run = await privateDirectory(root);
  const state = await privateDirectory(stateDirectory), data = await privateDirectory(dataDirectory);
  const objects = await privateDirectory(join(stateDirectory, "objects")), logs = await privateDirectory(join(stateDirectory, "logs"));
  need(parent.identity === snapshot.parentIdentity && run.identity === snapshot.rootIdentity
    && state.identity === snapshot.stateIdentity && data.identity === snapshot.dataIdentity
    && objects.identity === snapshot.objectsIdentity && logs.identity === snapshot.logsIdentity);
  for (const [path, expectedIdentity] of snapshot.files) {
    const current = await privateFile(path, 1024 * 1024);
    need(current.identity === expectedIdentity);
  }
}

async function observe({ root, stateDirectory, dataDirectory, expected, assertOwnedPostmaster, queryIdentity, snapshot }) {
  await verifySnapshot(root, stateDirectory, dataDirectory, snapshot);
  const pidFile = await privateFile(join(dataDirectory, "postmaster.pid"), 8192);
  const pid = readPostmaster(pidFile, dataDirectory, expected.port);
  const ownerInput = Object.freeze({ pid, port: expected.port, dataDirectory, root });
  need(await assertOwnedPostmaster(ownerInput) === true);
  const sqlInput = Object.freeze({ pid, port: expected.port, dataDirectory, root, databaseUrl: snapshot.databaseUrl,
    sql: LINUX_DURABLE_PG_IDENTITY_SQL });
  const observed = validateLinuxDurablePgIdentity(await queryIdentity(sqlInput), expected);
  const after = await privateFile(join(dataDirectory, "postmaster.pid"), 8192);
  need(after.identity === pidFile.identity && readPostmaster(after, dataDirectory, expected.port) === pid);
  need(await assertOwnedPostmaster(ownerInput) === true);
  await verifySnapshot(root, stateDirectory, dataDirectory, snapshot);
  return Object.freeze({ pid, pidFileIdentity: pidFile.identity, observed });
}

/**
 * Attach only to a current, owned Linux QA database.  It creates nothing and
 * returns no credentials.  Call assertCurrent around every privileged fixture
 * operation to re-bind the durable files, owned postmaster and SQL identity.
 */
export async function attachLocalQaLinuxPostgres({ root, databaseUrl, dataKey, expectedSystemIdentifier,
  expectedCandidateId, assertOwnedPostmaster, queryIdentity }) {
  try {
    need(process.platform === "linux" && typeof process.getuid === "function" && root === process.env.BLUEY_TEST_RUN_ROOT
      && isDurableQaFixtureRoot(root));
    need(process.env.BLUEY_DATABASE_URL === undefined && process.env.BLUEY_TEST_POSTGRES_URL === undefined);
    need(typeof assertOwnedPostmaster === "function" && typeof queryIdentity === "function"
      && systemIdentifier(expectedSystemIdentifier) && UUID_V4.test(expectedCandidateId));
    const database = parseDatabaseUrl(databaseUrl), parent = await privateDirectory(dirname(root)), run = await privateDirectory(root);
    need(parent.path === dirname(root));
    const rootMarker = await privateFile(join(root, ROOT_MARKER), 4096); validateRootMarker(rootMarker.bytes, root);
    const stateDirectory = join(root, STATE), dataDirectory = join(stateDirectory, "postgres");
    const state = await privateDirectory(stateDirectory), data = await privateDirectory(dataDirectory);
    const stateFile = await privateFile(join(stateDirectory, STATE_MARKER), 8192), marker = json(stateFile, 8192, true);
    need(keys(marker, ["schemaVersion", "kind", "qaId", "ownerUid", "createdAt", "bucket", "prefix", "secretHashes", "rootIdentity", "directoryIdentities", "objectStoreMarkerSha256", "postgresInitialization"])
      && marker.schemaVersion === 1 && marker.kind === "bluey_jobs_durable_local_qa" && UUID_V4.test(marker.qaId)
      && marker.ownerUid === process.getuid() && Number.isFinite(Date.parse(marker.createdAt))
      && new Date(marker.createdAt).toISOString() === marker.createdAt && /^qa-[0-9a-f]{32}$/.test(marker.bucket)
      && /^qa-[0-9a-f]{32}$/.test(marker.prefix) && marker.rootIdentity === state.identity
      && marker.postgresInitialization === "claimed" && keys(marker.secretHashes, ["jwtSecret", "jobsDataKey", "objectAccessKey", "objectSecret"])
      && Object.values(marker.secretHashes).every(value => typeof value === "string" && SHA256.test(value))
      && keys(marker.directoryIdentities, ["objects", "postgres", "logs"]));
    need(marker.secretHashes.jobsDataKey === durableDataKeyHash(dataKey) && marker.directoryIdentities.postgres === data.identity);
    const objects = await privateDirectory(join(stateDirectory, "objects")), logs = await privateDirectory(join(stateDirectory, "logs"));
    need(marker.directoryIdentities.objects === objects.identity && marker.directoryIdentities.logs === logs.identity);
    const objectMarker = await privateFile(join(stateDirectory, "objects", OBJECT_MARKER), 8192);
    need(marker.objectStoreMarkerSha256 === sha(objectMarker.bytes));
    const readyFile = await privateFile(join(stateDirectory, READY), 8192), ready = json(readyFile, 8192);
    need(keys(ready, ["version", "qaId", "rootIdentity", "dataIdentity", "systemIdentifier", "passwordSha256", "configHashes"])
      && ready.version === 1 && ready.qaId === marker.qaId && ready.rootIdentity === marker.rootIdentity
      && ready.dataIdentity === data.identity && ready.systemIdentifier === expectedSystemIdentifier
      && systemIdentifier(ready.systemIdentifier) && SHA256.test(ready.passwordSha256) && keys(ready.configHashes, CONFIG));
    const password = await privateFile(join(stateDirectory, PG_AUTH_FILE), 128);
    need(/^[0-9a-f]{64}\n$/.test(password.bytes.toString("utf8")) && sha(password.bytes) === ready.passwordSha256
      && password.bytes.subarray(0, 64).toString("utf8") === database.password);
    password.bytes.fill(0);
    const bootstrapFile = await privateFile(join(stateDirectory, BOOTSTRAP), 8192), bootstrap = json(bootstrapFile, 8192);
    need(keys(bootstrap, ["version", "qaId", "root", "configSha256", "intentSha256", "systemIdentifier", "operatorId", "candidateId"])
      && bootstrap.version === 1 && bootstrap.qaId === marker.qaId && bootstrap.root === marker.rootIdentity
      && SHA256.test(bootstrap.configSha256) && SHA256.test(bootstrap.intentSha256)
      && bootstrap.systemIdentifier === ready.systemIdentifier && UUID_V4.test(bootstrap.operatorId)
      && bootstrap.candidateId === expectedCandidateId && bootstrap.operatorId !== bootstrap.candidateId);
    const files = new Map([[join(root, ROOT_MARKER), rootMarker.identity], [join(stateDirectory, STATE_MARKER), stateFile.identity],
      [join(stateDirectory, READY), readyFile.identity], [join(stateDirectory, PG_AUTH_FILE), password.identity],
      [join(stateDirectory, BOOTSTRAP), bootstrapFile.identity], [join(stateDirectory, "objects", OBJECT_MARKER), objectMarker.identity]]);
    for (const name of CONFIG) {
      const file = await privateFile(join(dataDirectory, name), 1024 * 1024);
      need(ready.configHashes[name] === sha(file.bytes)); files.set(file.path, file.identity);
    }
    const expected = Object.freeze({ address: "127.0.0.1", port: database.port, database: "bluey_jobs_qa",
      role: "bluey_qa_local", dataDirectory, systemIdentifier: expectedSystemIdentifier });
    const snapshot = Object.freeze({ parentIdentity: parent.identity, rootIdentity: run.identity,
      stateIdentity: state.identity, dataIdentity: data.identity, objectsIdentity: objects.identity,
      logsIdentity: logs.identity, files, databaseUrl });
    const initial = await observe({ root, stateDirectory, dataDirectory, expected, assertOwnedPostmaster, queryIdentity, snapshot });
    const assertCurrent = async () => {
      try {
        const current = await observe({ root, stateDirectory, dataDirectory, expected, assertOwnedPostmaster, queryIdentity, snapshot });
        need(current.pid === initial.pid && current.pidFileIdentity === initial.pidFileIdentity);
      }
      catch { refuse(); }
    };
    return Object.freeze({ root, stateDirectory, systemIdentifier: expectedSystemIdentifier, assertCurrent });
  } catch { refuse(); }
}
