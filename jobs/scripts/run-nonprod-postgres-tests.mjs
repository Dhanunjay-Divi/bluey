#!/usr/bin/env node

// Run only inside scripts/run-bluey-tests.sh. This script creates its own
// password-authenticated loopback cluster; it never accepts a database URL.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  realpathSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import net from "node:net";
import { join, resolve, sep } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const args = process.argv.slice(2);
assert.equal(args.shift(), "--pg-bin", "explicit PostgreSQL binary directory required");
const bin = realpathSync(args.shift());
assert(statSync(bin).isDirectory(), "PostgreSQL binary path must be a directory");
const caseFilters = [];
while (args[0] === "--case-filter") {
  args.shift();
  const filter = args.shift();
  assert.match(filter || "", /^[A-Za-z_][A-Za-z0-9_:]{0,255}$/, "invalid Cargo test filter");
  caseFilters.push(filter);
  assert(caseFilters.length <= 32, "too many Cargo test filters");
}
assert.equal(args.shift(), "--", "command separator required (omit command for cluster smoke)");
if (caseFilters.length) {
  assert.equal(args[0], "cargo", "isolated cases require a Cargo test command");
  assert(args.includes("test") && !args.includes("--"), "supply common Cargo test arguments only");
}
const root = realpathSync(process.env.BLUEY_TEST_RUN_ROOT || "/missing-owned-test-root");
assert(root.startsWith("/private/tmp/bluey-test-runs-") || root.startsWith("/tmp/bluey-test-runs-"));
assert.equal(statSync(root).uid, process.getuid());
assert.equal(statSync(root).mode & 0o777, 0o700);
assert.equal(resolve(process.env.CARGO_TARGET_DIR), join(root, "cargo-target"));
assert.equal(process.env.BLUEY_SERVER_DB_BACKEND, "sqlite");
assert.equal(process.env.BLUEY_DATABASE_URL, undefined, "owner database must not be inherited");
assert.equal(process.env.BLUEY_TEST_POSTGRES_URL, undefined, "test URL must be generated here");

// Do not pass provider/mailbox/production credentials or inherited feature flags
// to either the new server or Cargo. Preserve normal compiler/cache locations.
const allowed = new Set([
  "PATH", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "SDKROOT", "DEVELOPER_DIR",
  "CARGO_TARGET_DIR", "CARGO_INCREMENTAL", "TMPDIR", "TMP", "TEMP",
  "SQLITE_TMPDIR", "BLUEY_TEST_RUN_ROOT", "BLUEY_DB_PATH", "BLUEY_SERVER_DB_BACKEND",
  "BLUEY_DATA_DIR", "BLUEY_CONFIG_DIR", "BLUEY_RUNTIME_DIR", "BLUEY_LOG_DIR",
  "CUE_DATA_DIR", "CUE_CONFIG_DIR", "CUE_RUNTIME_DIR", "CUE_LOG_DIR",
]);
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) =>
  allowed.has(key) || /^CARGO_PROFILE_(DEV|TEST)_/.test(key)));
const work = join(root, "nonprod-postgres");
assert(!existsSync(work), "cluster directory must be fresh");
mkdirSync(work, { mode: 0o700 });
// Only these child processes get an empty home; the owner's environment and
// account files are never changed. Reuse owned Cargo source caches/toolchains, not
// .netrc, cloud profiles, Cargo credentials, or a coding-agent account home.
const isolatedHome = join(work, "child-home");
const isolatedCargo = join(isolatedHome, ".cargo");
mkdirSync(isolatedHome, { mode: 0o700 });
mkdirSync(isolatedCargo, { mode: 0o700 });
const originalCargo = realpathSync(process.env.CARGO_HOME || join(process.env.HOME, ".cargo"));
for (const directory of ["registry", "git"]) {
  const cached = join(originalCargo, directory);
  if (existsSync(cached)) {
    const resolved = realpathSync(cached);
    assert(statSync(resolved).isDirectory());
    assert.equal(statSync(resolved).uid, process.getuid());
    symlinkSync(resolved, join(isolatedCargo, directory), "dir");
  }
}
env.HOME = isolatedHome;
env.CARGO_HOME = isolatedCargo;
env.RUSTUP_HOME = realpathSync(process.env.RUSTUP_HOME || join(process.env.HOME, ".rustup"));
assert(statSync(env.RUSTUP_HOME).isDirectory());
assert.equal(statSync(env.RUSTUP_HOME).uid, process.getuid());
assert.equal(statSync(env.RUSTUP_HOME).mode & 0o022, 0);
const data = join(work, "data");
const log = join(work, "postgres.log");
const passwordFile = join(work, "password");
const password = randomBytes(32).toString("hex");
writeFileSync(passwordFile, password + "\n", { flag: "wx", mode: 0o600 });
const user = "bluey_jobs_test";
const database = "bluey_jobs_nonprod";
let server;
let command;
let signal;
let stopping = false;
const completions = new WeakMap();

function executable(name) {
  const path = realpathSync(join(bin, name));
  assert(path.startsWith(bin + sep), "PostgreSQL executable escaped pinned directory");
  const info = statSync(path);
  assert(info.isFile() && (info.mode & 0o111) !== 0, "PostgreSQL executable must be executable");
  assert.equal(info.mode & 0o022, 0, "PostgreSQL executable must not be group/world writable");
  return path;
}

function run(name, argv, extraEnv = {}) {
  const result = spawnSync(executable(name), argv, {
    env: { ...env, ...extraEnv }, encoding: "utf8", timeout: 30_000,
    maxBuffer: 2 * 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(`${name} failed (${result.status ?? "spawn error"})`);
  return result.stdout.trim();
}

function exited(child) {
  if (!completions.has(child)) {
    const completion = new Promise((resolveClose) => {
      let error;
      child.once("error", (value) => { error = value; });
      child.once("close", (code, signal) => resolveClose({ code, signal, error }));
    });
    completions.set(child, completion);
  }
  return completions.get(child);
}

async function runChild(argv, childEnv, captureStdout = false) {
  command = spawn(argv[0], argv.slice(1), {
    env: childEnv,
    stdio: captureStdout ? ["ignore", "pipe", "inherit"] : "inherit",
  });
  const completion = exited(command);
  const chunks = [];
  let capturedBytes = 0;
  let overflow = false;
  if (captureStdout) {
    command.stdout.on("data", (chunk) => {
      capturedBytes += chunk.length;
      if (capturedBytes > 1024 * 1024) {
        overflow = true;
        command.kill("SIGTERM");
      } else chunks.push(chunk);
    });
  }
  const result = await completion;
  assert(!overflow, "Cargo test discovery exceeded its output bound");
  return { ...result, stdout: Buffer.concat(chunks).toString("utf8") };
}

function verifyPostmasterIdentity() {
  const [pid, directory] = readFileSync(join(data, "postmaster.pid"), "utf8").split("\n");
  assert.equal(Number(pid), server.pid, "refuse substituted postmaster PID");
  assert.equal(realpathSync(directory), realpathSync(data));
}

for (const name of ["SIGHUP", "SIGINT", "SIGTERM"]) {
  process.on(name, () => {
    signal = name;
    command?.kill("SIGTERM");
    server?.kill("SIGINT");
  });
}

async function stopServer() {
  if (!server) return;
  stopping = true;
  if (!server.pid) { await exited(server); return; }
  if (server.exitCode === null && server.signalCode === null) {
    const pidFile = join(data, "postmaster.pid");
    if (existsSync(pidFile)) verifyPostmasterIdentity();
    server.kill("SIGINT"); // PostgreSQL fast, rollback-safe shutdown.
    const finished = await Promise.race([exited(server).then(() => true), delay(5000, false, { ref: false })]);
    if (!finished) {
      server.kill("SIGQUIT");
      const stopped = await Promise.race([exited(server).then(() => true), delay(5000, false, { ref: false })]);
      assert(stopped, "owned PostgreSQL did not stop; cleanup is not a pass");
      throw new Error("owned PostgreSQL required forced shutdown; validation is not a pass");
    }
  }
  const stopped = await exited(server);
  assert.equal(stopped.code, 0, "owned PostgreSQL shutdown must exit cleanly");
  assert.equal(stopped.signal, null, "owned PostgreSQL shutdown must not be signal termination");
  assert(!existsSync(join(data, "postmaster.pid")), "postmaster identity remains after shutdown");
}

let status = 1;
try {
  assert.match(run("postgres", ["--version"]), /^postgres \(PostgreSQL\) 17\./,
    "validation requires PostgreSQL major version 17");
  run("initdb", ["-D", data, "-U", user, "--auth-local=scram-sha-256",
    "--auth-host=scram-sha-256", "--pwfile", passwordFile, "--encoding=UTF8", "--no-locale"]);
  const listener = net.createServer();
  await new Promise((resolveListen, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolveListen);
  });
  const port = listener.address().port;
  await new Promise((resolveClose) => listener.close(resolveClose));
  assert(port > 1024 && port <= 65535);
  const logFd = openSync(log, "wx", 0o600);
  try {
    server = spawn(executable("postgres"), ["-D", data, "-h", "127.0.0.1", "-p", String(port),
      "-k", "", "-c", "max_connections=80", "-c", "shared_buffers=128MB"],
    { env, stdio: ["ignore", logFd, logFd] });
    exited(server); // Register close/error completion before either event can fire.
  } finally { closeSync(logFd); }
  let serverError;
  server.once("error", (error) => { serverError = error; });
  const connectionArgs = ["-X", "-w", "-h", "127.0.0.1", "-p", String(port),
    "-U", user, "-v", "ON_ERROR_STOP=1", "-A", "-t"];
  const sql = (db, query) => run("psql", [...connectionArgs, "-d", db, "-c", query], { PGPASSWORD: password });
  let ready = false;
  for (let attempt = 0; attempt < 100 && !signal; attempt += 1) {
    if (serverError || server.exitCode !== null || server.signalCode !== null) {
      throw new Error("owned PostgreSQL exited before readiness");
    }
    const probe = spawnSync(executable("pg_isready"), ["-h", "127.0.0.1", "-p", String(port), "-t", "1"],
      { env, stdio: "ignore", timeout: 2000 });
    if (probe.status === 0) { ready = true; break; }
    await delay(100);
  }
  assert(ready && !signal, "owned PostgreSQL did not become ready");
  verifyPostmasterIdentity();
  assert.equal(sql("postgres", "SHOW data_directory"), realpathSync(data));
  const serverVersion = Number(sql("postgres", "SHOW server_version_num"));
  assert(serverVersion >= 170000 && serverVersion < 180000, "connected server must be PostgreSQL 17");
  assert.equal(sql("postgres", "SELECT host(inet_server_addr())"), "127.0.0.1");
  assert.equal(Number(sql("postgres", "SELECT inet_server_port()")), port);
  sql("postgres", `CREATE DATABASE ${database}`);
  sql(database, "CREATE EXTENSION vector");
  assert.equal(sql(database, "SELECT '[1,0,0]'::vector <=> '[1,0,0]'::vector"), "0");
  assert.equal(sql(database, "SELECT current_database()"), database);
  console.log(JSON.stringify({ event: "nonprod_postgres_ready", address: "127.0.0.1", port,
    database, dataDirectory: realpathSync(data), version: sql(database, "SELECT version()"),
    vector: sql(database, "SELECT extversion FROM pg_extension WHERE extname='vector'") }));
  if (args.length) {
    const childEnv = { ...env,
      BLUEY_TEST_POSTGRES_URL: `postgresql://${user}:${password}@127.0.0.1:${port}/${database}?sslmode=disable`,
      BLUEY_JOBS_REQUIRE_POSTGRES_TESTS: "1",
    };
    if (caseFilters.length) {
      // Runtime authority is deliberately global within a database. Independent
      // v2/v3 fixtures must not inherit another case's live activation/head.
      // Keep one cluster/compiler root, but allocate one fresh DB per exact test.
      const names = new Set();
      for (const filter of caseFilters) {
        const listed = await runChild([...args, filter, "--", "--list"], childEnv, true);
        assert.equal(listed.code, 0, "Cargo test discovery failed");
        const found = listed.stdout.split("\n").flatMap((line) => {
          const match = /^([A-Za-z_][A-Za-z0-9_:]*): test$/.exec(line.trim());
          return match ? [match[1]] : [];
        });
        assert(found.length > 0, `Cargo filter matched no tests: ${filter}`);
        for (const name of found) {
          assert(!names.has(name), `Cargo filters overlap: ${name}`);
          names.add(name);
          assert(names.size <= 128, "isolated PostgreSQL case count exceeds its bound");
        }
      }
      let passed = 0;
      const failed = [];
      for (const name of names) {
        assert(!signal && server.exitCode === null && server.signalCode === null,
          "validation/database interrupted between cases");
        verifyPostmasterIdentity();
        const caseDatabase = `bluey_jobs_case_${randomBytes(12).toString("hex")}`;
        sql("postgres", `CREATE DATABASE ${caseDatabase}`);
        sql(caseDatabase, "CREATE EXTENSION vector");
        assert.equal(sql(caseDatabase, "SELECT current_database()"), caseDatabase);
        const caseUrl = new URL(childEnv.BLUEY_TEST_POSTGRES_URL);
        caseUrl.pathname = "/" + caseDatabase;
        console.log(JSON.stringify({ event: "nonprod_postgres_case_started", test: name,
          database: caseDatabase }));
        const result = await runChild([
          ...args, name, "--", "--exact", "--nocapture", "--test-threads=1",
        ], { ...childEnv, BLUEY_TEST_POSTGRES_URL: caseUrl.href });
        if (result.code === 0 && result.signal === null && !result.error) passed += 1;
        else failed.push(name);
        console.log(JSON.stringify({ event: "nonprod_postgres_case_finished", test: name,
          exitCode: result.code, signal: result.signal }));
      }
      status = failed.length ? 1 : 0;
      console.log(JSON.stringify({ event: "nonprod_postgres_cases_finished",
        tests: names.size, passed, failed }));
    } else {
      status = (await runChild(args, childEnv)).code ?? 1;
    }
  } else status = 0;
  assert.equal(server.exitCode, null, "database exited during test command");
  assert.equal(server.signalCode, null, "database was killed during test command");
  assert(!serverError && !signal, "validation interrupted");
} catch (error) {
  console.error(`Non-production PostgreSQL validation failed: ${error.message}`);
  status = 1;
} finally {
  try { await stopServer(); }
  catch (error) { console.error(`Non-production PostgreSQL cleanup failed: ${error.message}`); status = 1; }
}
console.log(JSON.stringify({ event: "nonprod_postgres_finished", status, shutdownAttempted: stopping,
  serverExited: Boolean(server && (server.exitCode !== null || server.signalCode !== null)) }));
process.exitCode = status;
