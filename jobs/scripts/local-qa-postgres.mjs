// Dedicated persistent local QA PostgreSQL; not a production service manager.
// The private directory is trusted to its OS owner: path APIs are not openat
// and do not promise protection against a hostile process with the same UID.
import { createHash, randomBytes } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { constants, closeSync, fstatSync, lstatSync, openSync, readFileSync, realpathSync } from "node:fs";
import { open, lstat, realpath, readdir, unlink } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { claimLocalQaPostgresInitialization, inspectLocalQaState } from "./local-qa-state.mjs";
import { parseReviewFirstReceiptCensusOutput, reviewFirstReceiptCensusStatement } from "./local-qa-linux-c2c-census.mjs";

const BIN = "/opt/homebrew/Cellar/postgresql@17/17.10/bin";
const DEFAULT_CONNECTION_LIMIT = 30;
// The API and one-shot fixture can each reserve 16 primary + 8 lifecycle
// connections. Leave room for the identity probe and PostgreSQL reserves.
const DURABLE_DRAFT_CONNECTION_LIMIT = 64;
const DURABLE_DRAFT_RUN_ROOT = /^\/private\/tmp\/bluey-test-runs-[a-f0-9]{12}\/run\.\d+\.\d+\.\d+$/;
const VECTOR = "/opt/homebrew/Cellar/pgvector/0.8.3";
const PYTHON = "/Library/Developer/CommandLineTools/usr/bin/python3";
const PYROOT = "/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9";
const PROCESS_HELPER = join(dirname(fileURLToPath(import.meta.url)), "local-qa-process-info.py");
const PROCESS_NATIVE = join(dirname(PROCESS_HELPER), "managed-runner-host-processes.py");
const INSPECTOR_PINS = Object.freeze({
  [`${PYROOT}/bin/python3.9`]: "6e7ae61f68a3838094fc56590f84c52069a97d7816f6bb79e0e85995b340e464",
  [`${PYROOT}/Resources/Python.app/Contents/MacOS/Python`]: "295b2fd03b2b05f12b6f4887d70dbf43e5358173f8eec0909c121c9bdec2756f",
  [PROCESS_HELPER]: "28c1e2260197e3fcb6bee2adddeaa75a7f17c3a88180c370b541578d04074c8a",
  [PROCESS_NATIVE]: "bac1d707b8d077bde9ed6dd787d801550b198fefc908c82e25edb4cae3f6481a",
});
const PINS = Object.freeze({
  [`${BIN}/postgres`]: "38c50336c63ae8b73b799a8c0ebc4f948e311eee882a633c8286d1ca4110b502",
  [`${BIN}/initdb`]: "43f864de4746e096246a2a9f9784ccbd7364b35b2301b82aeeb6d2a12c1912c4",
  [`${BIN}/psql`]: "bb4ef660720948d242d8c4e32450ae5607241510dc8f06cf685048e4c493447d",
  [`${BIN}/pg_isready`]: "a28e468c1ac200dee1d56f02d1436dfe2e799371a8c4448d5a2540efeb80dabb",
  [`${BIN}/pg_controldata`]: "61f1ee105f5b8e44ef443bfa01a4460ff3406560c15add70f5953d375085f487",
  [`${VECTOR}/share/postgresql@17/extension/vector.control`]: "db4ad0045e6622228471acdcb8fc9aeeec0cd6cf9666569d26f723751ea95fcb",
  [`${VECTOR}/share/postgresql@17/extension/vector--0.8.3.sql`]: "7fb5bb279ef83bf9204bfac7405bb5c9a05e49f5ac7d64d1eb4464d103b80f32",
  [`${VECTOR}/lib/postgresql@17/vector.dylib`]: "c3834e81f74c3a3f78706691d283b7077d17cb47501daf74e6066532fcadd70e",
});
const PG_AUTH_FILE = ".local-qa-postgres-password";
const READY = ".local-qa-postgres-ready.json";
const LOCK = ".local-qa-postgres.lock";
const USER = "bluey_qa_local", DATABASE = "bluey_jobs_qa";
const CONFIG = ["postgresql.conf", "postgresql.auto.conf", "pg_hba.conf", "pg_ident.conf", "PG_VERSION"];
const hash = value => createHash("sha256").update(value).digest("hex");
const exact = value => JSON.stringify(value) + "\n";
const meta = value => [value.dev, value.ino, value.uid, value.mode, value.nlink, value.size,
  value.mtimeNs, value.ctimeNs].map(String).join(":");
const dirId = value => [value.dev, value.ino, value.uid, value.mode].map(String).join(":");
function requireThat(value) { if (!value) throw new Error("local_qa_postgres_refused"); }
async function absent(path) { try { await lstat(path); return false; } catch (error) { if (error.code === "ENOENT") return true; throw error; } }
async function readPrivate(path, max = 65536) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    requireThat(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n &&
      (before.mode & 0o7777n) === 0o600n && before.size <= BigInt(max));
    const bytes = await file.readFile();
    requireThat(BigInt(bytes.length) === before.size && meta(await file.stat({ bigint: true })) === meta(before) &&
      meta(await lstat(path, { bigint: true })) === meta(before));
    return bytes;
  } finally { await file.close(); }
}
async function directory(path) {
  requireThat(await realpath(path) === path);
  const stat = await lstat(path, { bigint: true });
  requireThat(stat.isDirectory() && stat.uid === BigInt(process.getuid()) && (stat.mode & 0o7777n) === 0o700n);
  return dirId(stat);
}
async function syncDir(path) { const fd = await open(path, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW); try { await fd.sync(); } finally { await fd.close(); } }
async function createPrivate(path, bytes) {
  const file = await open(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
  try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
}
async function verifyTools() {
  for (const [path, expected] of Object.entries({ ...PINS, ...INSPECTOR_PINS })) {
    requireThat(await realpath(path) === path);
    const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const before = await file.stat({ bigint: true });
      requireThat(before.isFile() && before.nlink > 0n && (before.mode & 0o022n) === 0n && before.size <= 32n * 1024n * 1024n);
      const bytes = await file.readFile();
      requireThat(BigInt(bytes.length) === before.size && hash(bytes) === expected &&
        meta(await file.stat({ bigint: true })) === meta(before) && meta(await lstat(path, { bigint: true })) === meta(before));
    } finally { await file.close(); }
  }
  requireThat(await realpath("/opt/homebrew/opt/postgresql@17/bin") === BIN);
  requireThat(await realpath("/opt/homebrew/share/postgresql@17/extension/vector.control") === `${VECTOR}/share/postgresql@17/extension/vector.control`);
  requireThat(await realpath("/opt/homebrew/share/postgresql@17/extension/vector--0.8.3.sql") === `${VECTOR}/share/postgresql@17/extension/vector--0.8.3.sql`);
  requireThat(await realpath("/opt/homebrew/lib/postgresql@17/vector.dylib") === `${VECTOR}/lib/postgresql@17/vector.dylib`);
  requireThat(await realpath(PYTHON) === `${PYROOT}/bin/python3.9`);
}
function command(path, args, env, input = "") {
  const result = spawnSync(path, args, { env, input, encoding: "utf8", timeout: 30000, maxBuffer: 1024 * 1024, stdio: ["pipe", "pipe", "pipe"] });
  if (result.error || result.status !== 0 || result.signal !== null) {
    const name = basename(path);
    const safeName = ["initdb", "pg_controldata", "psql"].includes(name) ? name : "other";
    const outcome = result.error?.code === "ETIMEDOUT" ? "timeout"
      : result.error?.code === "ENOBUFS" ? "output_limit"
        : result.error ? "spawn_error" : result.signal ? "signal" : "nonzero";
    throw Object.assign(new Error("local_qa_postgres_command_failed"), { safeName, outcome });
  }
  return result.stdout.trim();
}
function systemId(value) {
  requireThat(typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value) && BigInt(value) <= 18446744073709551615n);
  return value;
}
function verifyInspectorFiles() {
  requireThat(realpathSync(PYTHON) === `${PYROOT}/bin/python3.9`);
  for (const [path, expected] of Object.entries(INSPECTOR_PINS)) {
    requireThat(realpathSync(path) === path);
    const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const before = fstatSync(fd, { bigint: true });
      requireThat(before.isFile() && before.nlink > 0n && (before.uid === 0n || before.uid === BigInt(process.getuid())) &&
        (before.mode & 0o022n) === 0n && before.size <= 32n * 1024n * 1024n);
      const bytes = readFileSync(fd);
      requireThat(BigInt(bytes.length) === before.size && hash(bytes) === expected &&
        meta(fstatSync(fd, { bigint: true })) === meta(before) && meta(lstatSync(path, { bigint: true })) === meta(before));
    } finally { closeSync(fd); }
  }
}
function processInfo(pid) {
  requireThat(Number.isSafeInteger(pid) && pid > 1 && pid <= 2147483647);
  verifyInspectorFiles();
  const result = spawnSync(PYTHON, ["-I", "-S", "-B", PROCESS_HELPER, String(pid)],
    { encoding: "utf8", timeout: 5000, maxBuffer: 8192, env: { PATH: "/usr/bin:/bin", LC_ALL: "C" } });
  requireThat(!result.error && result.status === 0 && result.signal === null && result.stderr === "");
  verifyInspectorFiles();
  let info; try { info = JSON.parse(result.stdout); } catch { throw new Error("local_qa_postgres_refused"); }
  requireThat(info && Object.keys(info).join(",") === "pid,ppid,uid,ruid,pgid,birthSec,birthUsec,executable,executableDevice,executableInode" &&
    JSON.stringify(info) + "\n" === result.stdout &&
    info.pid === pid && info.uid === process.getuid() && info.ruid === process.getuid() &&
    Number.isSafeInteger(info.ppid) && info.ppid >= 0 && info.ppid <= 2147483647 &&
    Number.isSafeInteger(info.pgid) && info.pgid > 1 && info.pgid <= 2147483647 &&
    typeof info.birthSec === "string" && /^[1-9][0-9]{0,19}$/.test(info.birthSec) && BigInt(info.birthSec) <= 18446744073709551615n &&
    Number.isSafeInteger(info.birthUsec) && info.birthUsec >= 0 && info.birthUsec < 1000000 &&
    typeof info.executable === "string" && /^\/[\x20-\x7e]*$/.test(info.executable) &&
    typeof info.executableDevice === "string" && /^(0|[1-9][0-9]{0,19})$/.test(info.executableDevice) &&
    BigInt(info.executableDevice) <= 18446744073709551615n &&
    typeof info.executableInode === "string" && /^[1-9][0-9]{0,19}$/.test(info.executableInode) &&
    BigInt(info.executableInode) <= 18446744073709551615n);
  return info;
}
function group(pid) {
  const info = processInfo(pid);
  if (pid === process.pid) {
    const path = realpathSync(process.execPath), stat = lstatSync(path, { bigint: true });
    requireThat(info.executable === path && info.executableDevice === String(stat.dev) && info.executableInode === String(stat.ino));
  }
  return info.pgid;
}
async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", resolve); });
  const address = server.address(); requireThat(address && typeof address !== "string" && address.port > 1024);
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); return address.port;
}
async function configHashes(data) {
  const result = {};
  for (const name of CONFIG) result[name] = hash(await readPrivate(join(data, name), 1024 * 1024));
  return result;
}
function controlId(data, env) {
  const output = command(`${BIN}/pg_controldata`, [data], env);
  const rows = [...output.matchAll(/^Database system identifier:\s*([0-9]+)\s*$/gm)];
  requireThat(rows.length === 1); return systemId(rows[0][1]);
}
async function acquire(stateDirectory) {
  const state = await inspectLocalQaState(stateDirectory);
  const rootId = await directory(stateDirectory), dataId = await directory(state.postgresDirectory);
  const lockPath = join(stateDirectory, LOCK);
  const lock = await open(lockPath, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
  let lockId;
  try { await lock.sync(); lockId = dirId(await lock.stat({ bigint: true })); } finally { await lock.close(); }
  await syncDir(stateDirectory);
  async function check() {
    requireThat((await inspectLocalQaState(stateDirectory)).qaId === state.qaId &&
      await directory(stateDirectory) === rootId && await directory(state.postgresDirectory) === dataId);
    const stat = await lstat(lockPath, { bigint: true });
    requireThat(stat.isFile() && stat.uid === BigInt(process.getuid()) && stat.nlink === 1n &&
      (stat.mode & 0o7777n) === 0o600n && dirId(stat) === lockId);
  }
  async function release() { await check(); await unlink(lockPath); await syncDir(stateDirectory); }
  await check(); return { state, rootId, dataId, check, release };
}
async function readReady(context) {
  const root = context.state.stateDirectory, bytes = await readPrivate(join(root, READY));
  let marker; try { marker = JSON.parse(bytes.toString("utf8")); } catch { throw new Error("local_qa_postgres_refused"); }
  requireThat(marker && Object.keys(marker).join(",") === "version,qaId,rootIdentity,dataIdentity,systemIdentifier,passwordSha256,configHashes" &&
    (await inspectLocalQaState(root)).postgresInitialization === "claimed" &&
    marker.version === 1 && marker.qaId === context.state.qaId && marker.rootIdentity === context.rootId &&
    marker.dataIdentity === context.dataId && exact(marker) === bytes.toString("utf8") &&
    /^[0-9a-f]{64}$/.test(marker.passwordSha256) && marker.configHashes &&
    Object.keys(marker.configHashes).join(",") === CONFIG.join(","));
  systemId(marker.systemIdentifier);
  const passwordBytes = await readPrivate(join(root, PG_AUTH_FILE), 128), password = passwordBytes.toString("utf8");
  requireThat(/^[0-9a-f]{64}\n$/.test(password) && hash(passwordBytes) === marker.passwordSha256);
  const config = await configHashes(context.state.postgresDirectory);
  requireThat(exact(config) === exact(marker.configHashes));
  await context.check(); await verifyTools();
  return { marker, markerBytes: bytes, password: password.trim() };
}
function closedEnv(state) { return { PATH: `${BIN}:/usr/bin:/bin`, HOME: state.stateDirectory, LANG: "C", LC_ALL: "C", TZ: "UTC" }; }
async function launch(context, password, expectedSystemId, port, onLifecycle,
  connectionLimit = DEFAULT_CONNECTION_LIMIT) {
  const state = context.state, data = state.postgresDirectory, env = closedEnv(state);
  const selected = port === 0 ? await freePort() : port;
  requireThat(Number.isInteger(selected) && selected > 1024 && selected <= 65535);
  const args = ["-D", data, "-h", "127.0.0.1", "-p", String(selected),
    "-c", "unix_socket_directories=", "-c", `max_connections=${connectionLimit}`, "-c", "shared_buffers=128MB"];
  const callerGroup = group(process.pid);
  const child = spawn(`${BIN}/postgres`, args, { env, stdio: ["ignore", "pipe", "pipe"] });
  let ended, error, outputBytes = 0, overflow = false, observerFailed = false, childBinding;
  const observe = event => {
    try {
      const result = onLifecycle?.(Object.freeze(event));
      if (result && typeof result.then === "function") {
        observerFailed = true; Promise.resolve(result).catch(() => {});
      }
    } catch { observerFailed = true; }
  };
  if (child.pid) observe({ event: "started", pid: child.pid, callerPGID: callerGroup, port: selected, dataDirectory: data });
  const terminal = new Promise(resolve => { child.once("error", value => { error = value; });
    child.once("close", (code, signal) => {
      ended = { code, signal };
      if (child.pid) observe({ event: "exited", pid: child.pid, exitCode: code, signal });
      resolve(ended);
    }); });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => {
    outputBytes += chunk.length; if (outputBytes > 1024 * 1024) overflow = true;
  });
  const pidPath = join(data, "postmaster.pid");
  async function checkChild(requirePid = true) {
    requireThat(!ended && !error && group(process.pid) === callerGroup);
    const info = processInfo(child.pid), executable = lstatSync(`${BIN}/postgres`, { bigint: true });
    requireThat(info.ppid === process.pid && info.pgid === callerGroup && info.executable === `${BIN}/postgres` &&
      info.executableDevice === String(executable.dev) && info.executableInode === String(executable.ino));
    const binding = [info.pid, info.ppid, info.uid, info.ruid, info.pgid, info.birthSec, info.birthUsec,
      info.executable, info.executableDevice, info.executableInode];
    if (childBinding) requireThat(exact(binding) === exact(childBinding)); else childBinding = binding;
    if (!requirePid) return;
    const text = (await readPrivate(pidPath, 8192)).toString("utf8").split("\n");
    requireThat(/^[1-9][0-9]*$/.test(text[0]) && Number(text[0]) === child.pid && text[1] === data &&
      /^[1-9][0-9]*$/.test(text[3]) && Number(text[3]) === selected);
  }
  const sql = (database, statement) => command(`${BIN}/psql`, ["-X", "-w", "-h", "127.0.0.1", "-p", String(selected),
    "-U", USER, "-d", database, "-v", "ON_ERROR_STOP=1", "-A", "-t"], { ...env, PGPASSWORD: password }, statement + "\n");
  async function stop(requirePid) {
    if (ended) throw new Error("local_qa_postgres_refused");
    await checkChild(requirePid); child.kill("SIGINT");
    if (!await Promise.race([terminal.then(() => true), delay(5000, false)])) {
      await checkChild(requirePid || !(await absent(pidPath))); child.kill("SIGQUIT"); await Promise.race([terminal, delay(5000)]);
      throw new Error("local_qa_postgres_refused");
    }
    requireThat(ended.code === 0 && ended.signal === null && !overflow && !observerFailed && await absent(pidPath));
  }
  const close = () => stop(true);
  try {
    let ready = false;
    for (let i = 0; i < 100 && !ready; i++) {
      if (ended || error || overflow) break;
      if (!(await absent(pidPath))) {
        const probe = spawnSync(`${BIN}/pg_isready`, ["-h", "127.0.0.1", "-p", String(selected), "-t", "1"],
          { env, stdio: "ignore", timeout: 2000 });
        if (!probe.error && probe.status === 0) { await checkChild(); ready = true; }
      }
      if (!ready) await delay(100);
    }
    requireThat(ready); await checkChild();
    const identity = sql("postgres", "SELECT current_setting('data_directory'),current_setting('server_version_num'),host(inet_server_addr()),inet_server_port(),current_setting('listen_addresses'),current_setting('unix_socket_directories'),system_identifier::text FROM pg_control_system();").split("|");
    requireThat(identity.length === 7 && identity[0] === data && identity[1] === "170010" && identity[2] === "127.0.0.1" &&
      identity[3] === String(selected) && identity[4] === "127.0.0.1" && identity[5] === "");
    const id = systemId(identity[6]); if (expectedSystemId) requireThat(id === expectedSystemId);
    await checkChild(); await context.check(); await verifyTools();
    observe({ event: "identity_verified", pid: child.pid, callerPGID: callerGroup, port: selected,
      dataDirectory: data, systemIdentifier: id });
    requireThat(!observerFailed);
    const checkedSql = async (database, statement) => {
      await checkChild(); await verifyTools();
      const output = sql(database, statement);
      await checkChild(); await verifyTools();
      return output;
    };
    return { port: selected, systemIdentifier: id, sql, checkedSql, close };
  } catch {
    // Before readiness the directly spawned child still needs PS uid/comm and
    // process-group identity; a PID file, when present, also binds.
    let cleanupFailed = false;
    try { if (!ended && !error) await stop(!(await absent(pidPath))); } catch { cleanupFailed = true; }
    if (!ended && !error) await Promise.race([terminal, delay(5000)]);
    if (cleanupFailed || (!ended && !error)) throw new Error("local_qa_postgres_cleanup_failed");
    throw new Error("local_qa_postgres_refused");
  }
}
export async function initializeLocalQaPostgres({ stateDirectory, port = 0, binDirectory = BIN, onLifecycle }) {
  let context, service, releasable = false, stage = "arguments";
  try {
    requireThat(binDirectory === BIN && (onLifecycle === undefined || typeof onLifecycle === "function"));
    stage = "acquire"; context = await acquire(stateDirectory);
    stage = "verify_tools"; await verifyTools();
    const root = context.state.stateDirectory, data = context.state.postgresDirectory;
    stage = "ready_lookup";
    if (!(await absent(join(root, READY)))) {
      stage = "ready_read";
      const { marker, password } = await readReady(context);
      stage = "stopped_identity";
      requireThat(await absent(join(data, "postmaster.pid")) && controlId(data, closedEnv(context.state)) === marker.systemIdentifier);
      // Existing marker is not evidence that the database still exists. Verify
      // it by an authenticated transient start, without creating or repairing.
      stage = "existing_launch"; service = await launch(context, password, marker.systemIdentifier, port, onLifecycle);
      stage = "existing_verify";
      requireThat(service.sql(DATABASE, "SELECT current_database();") === DATABASE &&
        service.sql(DATABASE, "SELECT extversion FROM pg_extension WHERE extname='vector';") === "0.8.3" &&
        service.sql(DATABASE, "SELECT '[1,0,0]'::vector <=> '[1,0,0]'::vector;") === "0");
      stage = "existing_shutdown"; await service.close(); service = undefined;
      releasable = true; return Object.freeze({ initialized: true, systemIdentifier: marker.systemIdentifier });
    }
    stage = "claim_precheck";
    requireThat((await inspectLocalQaState(root)).postgresInitialization === "unclaimed" &&
      await absent(join(root, PG_AUTH_FILE)) && (await readdir(data)).length === 0);
    stage = "claim"; const claimed = await claimLocalQaPostgresInitialization(root);
    requireThat(claimed.qaId === context.state.qaId && claimed.postgresInitialization === "claimed");
    await context.check();
    stage = "password_write";
    const password = randomBytes(32).toString("hex"), passwordBytes = Buffer.from(password + "\n");
    await createPrivate(join(root, PG_AUTH_FILE), passwordBytes); await syncDir(root); await context.check();
    stage = "initdb"; command(`${BIN}/initdb`, ["-D", data, "-U", USER, "--auth-local=scram-sha-256", "--auth-host=scram-sha-256",
      "--pwfile", join(root, PG_AUTH_FILE), "--encoding=UTF8", "--no-locale"], closedEnv(context.state));
    stage = "config_hashes"; const hashes = await configHashes(data);
    stage = "control_id"; const id = controlId(data, closedEnv(context.state));
    stage = "launch"; service = await launch(context, password, id, port, onLifecycle);
    stage = "database_create"; service.sql("postgres", `CREATE DATABASE ${DATABASE};`);
    stage = "vector_create"; service.sql(DATABASE, "CREATE EXTENSION vector;");
    stage = "vector_verify";
    requireThat(service.sql(DATABASE, "SELECT extversion FROM pg_extension WHERE extname='vector';") === "0.8.3");
    requireThat(service.sql(DATABASE, "SELECT '[1,0,0]'::vector <=> '[1,0,0]'::vector;") === "0");
    stage = "shutdown"; await service.close(); service = undefined;
    stage = "post_shutdown_verify";
    await context.check(); requireThat(await absent(join(data, "postmaster.pid")) && controlId(data, closedEnv(context.state)) === id);
    const marker = { version: 1, qaId: context.state.qaId, rootIdentity: context.rootId, dataIdentity: context.dataId,
      systemIdentifier: id, passwordSha256: hash(passwordBytes), configHashes: hashes };
    stage = "ready_commit"; passwordBytes.fill(0); await createPrivate(join(root, READY), Buffer.from(exact(marker))); await syncDir(root);
    stage = "ready_verify"; await readReady(context); releasable = true; return Object.freeze({ initialized: true, systemIdentifier: id });
  } catch (error) {
    if (error?.message === "local_qa_postgres_cleanup_failed") throw new Error("local_qa_postgres_cleanup_failed");
    const command = error?.message === "local_qa_postgres_command_failed" &&
      ["initdb", "pg_controldata", "psql", "other"].includes(error.safeName) &&
      ["timeout", "output_limit", "spawn_error", "signal", "nonzero"].includes(error.outcome)
      ? `_${error.safeName}_${error.outcome}` : "";
    throw new Error(`local_qa_postgres_init_failed_${stage}${command}`);
  }
  finally {
    if (service) { try { await service.close(); } catch { throw new Error("local_qa_postgres_cleanup_failed"); } }
    if (context && releasable) {
      try { await context.release(); } catch { throw new Error("local_qa_postgres_init_failed"); }
    }
  }
}
export async function startLocalQaPostgres({ stateDirectory, port = 0, binDirectory = BIN, onLifecycle,
  durableDraftFixture = false }) {
  let context, service;
  try {
    requireThat(binDirectory === BIN && (onLifecycle === undefined || typeof onLifecycle === "function"));
    requireThat(typeof durableDraftFixture === "boolean");
    if (durableDraftFixture) {
      const root = process.env.BLUEY_TEST_RUN_ROOT;
      requireThat(typeof root === "string" && DURABLE_DRAFT_RUN_ROOT.test(root) &&
        stateDirectory === join(root, "bluey-jobs-qa-draft") && await realpath(root) === root);
    }
    context = await acquire(stateDirectory); const ready = await readReady(context);
    requireThat(await absent(join(context.state.postgresDirectory, "postmaster.pid")) &&
      controlId(context.state.postgresDirectory, closedEnv(context.state)) === ready.marker.systemIdentifier);
    const connectionLimit = durableDraftFixture ? DURABLE_DRAFT_CONNECTION_LIMIT : DEFAULT_CONNECTION_LIMIT;
    service = await launch(context, ready.password, ready.marker.systemIdentifier, port, onLifecycle, connectionLimit);
    requireThat(service.sql(DATABASE, "SELECT current_database();") === DATABASE &&
      service.sql(DATABASE, "SHOW max_connections;") === String(connectionLimit) &&
      service.sql(DATABASE, "SELECT extversion FROM pg_extension WHERE extname='vector';") === "0.8.3" &&
      service.sql(DATABASE, "SELECT '[1,0,0]'::vector <=> '[1,0,0]'::vector;") === "0");
    let closing;
    const close = () => closing ||= (async () => {
      let authorized = true;
      try {
        const now = await readReady(context); requireThat(now.markerBytes.equals(ready.markerBytes));
      } catch { authorized = false; }
      try { await service.close(); } catch { throw new Error("local_qa_postgres_cleanup_failed"); }
      if (!authorized) throw new Error("local_qa_postgres_close_failed");
      try { await context.release(); } catch { throw new Error("local_qa_postgres_close_failed"); }
    })();
    const readReviewFirstReceiptCensus = async (accountId, applicationId, phase, runId) => {
      try {
        requireThat(durableDraftFixture && closing === undefined);
        const statement = reviewFirstReceiptCensusStatement(accountId, applicationId, phase, runId);
        await context.check();
        const output = await service.checkedSql(DATABASE, statement);
        await context.check();
        return parseReviewFirstReceiptCensusOutput(output, phase);
      } catch {
        throw new Error("local_qa_postgres_receipt_census_refused");
      }
    };
    const result = { host: "127.0.0.1", port: service.port, database: DATABASE,
      systemIdentifier: service.systemIdentifier, close };
    Object.defineProperty(result, "env", { enumerable: false, value: Object.freeze({
      BLUEY_SERVER_DB_BACKEND: "postgres", BLUEY_DATABASE_URL: `postgresql://${USER}:${ready.password}@127.0.0.1:${service.port}/${DATABASE}?sslmode=disable`,
    }) });
    if (durableDraftFixture) {
      Object.defineProperty(result, "readReviewFirstReceiptCensus", {
        enumerable: false, value: readReviewFirstReceiptCensus,
      });
    }
    return Object.freeze(result);
  } catch (error) {
    // A failed or ambiguous start retains its lock and data for inspection.
    if (service) { try { await service.close(); } catch { throw new Error("local_qa_postgres_cleanup_failed"); } }
    if (error?.message === "local_qa_postgres_cleanup_failed") throw error;
    throw new Error("local_qa_postgres_start_failed");
  }
}
