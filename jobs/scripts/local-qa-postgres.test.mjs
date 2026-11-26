import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { chmod, lstat, mkdtemp, readFile, readdir, realpath, rename, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initializeLocalQaState } from "./local-qa-state.mjs";
import { initializeLocalQaPostgres as initializePostgres, startLocalQaPostgres as startPostgres } from "./local-qa-postgres.mjs";

const observed = [], active = new Map();
const onLifecycle = event => {
  if (event.event === "started") {
    assert(!active.has(event.pid));
    const record = { pid: event.pid, dataDirectory: event.dataDirectory, exited: false };
    observed.push(record); active.set(event.pid, record);
  } else if (event.event === "exited") {
    const record = active.get(event.pid); assert(record && !record.exited);
    record.exited = true; active.delete(event.pid);
  }
  process.stdout.write(`LOCAL_QA_PG_EVENT ${JSON.stringify(event)}\n`);
};
const initializeLocalQaPostgres = options => initializePostgres({ ...options, onLifecycle });
const startLocalQaPostgres = options => startPostgres({ ...options, onLifecycle });

async function fixture(t) {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "local-qa-postgres-")));
  const stateDirectory = join(parent, "bluey-jobs-qa-postgres"), services = [];
  t.after(async () => {
    for (const service of services) await service.close();
    await assert.rejects(lstat(join(stateDirectory, "postgres", "postmaster.pid")), { code: "ENOENT" });
    for (const record of observed.filter(item => item.dataDirectory === join(stateDirectory, "postgres"))) {
      assert(record.exited, "owned_postgres_exit_not_observed");
      assert.throws(() => process.kill(record.pid, 0), error => error?.code === "ESRCH");
    }
    // This exact test-created parent is never a durable user state root.
    await rm(parent, { recursive: true, force: true });
  });
  const state = await initializeLocalQaState(stateDirectory);
  return { stateDirectory, state, services };
}
function sql(service, statement) {
  const url = new URL(service.env.BLUEY_DATABASE_URL);
  const result = spawnSync("/opt/homebrew/Cellar/postgresql@17/17.10/bin/psql",
    ["-X", "-w", "-h", url.hostname, "-p", url.port, "-U", url.username,
      "-d", url.pathname.slice(1), "-v", "ON_ERROR_STOP=1", "-A", "-t"],
    { input: statement + "\n", encoding: "utf8", timeout: 10000, maxBuffer: 16384,
      env: { PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", PGPASSWORD: url.password } });
  assert.equal(result.status, 0, "synthetic_pg_query_failed");
  assert.equal(result.signal, null); return result.stdout.trim();
}
async function start(f) {
  const service = await startLocalQaPostgres({ stateDirectory: f.stateDirectory });
  f.services.push(service); return service;
}

test("real PostgreSQL init, authenticated restart and saved synthetic row", async (t) => {
  const f = await fixture(t);
  const first = await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
  assert.match(first.systemIdentifier, /^[1-9][0-9]{0,19}$/);
  assert.deepEqual(await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory }), first);
  let service = await start(f);
  assert.equal(service.systemIdentifier, first.systemIdentifier);
  assert(!JSON.stringify(service).includes("postgresql://"));
  assert.equal(sql(service, "SHOW max_connections;"), "30");
  assert.equal(sql(service, "CREATE TABLE qa_restart (id integer PRIMARY KEY, note text NOT NULL); INSERT INTO qa_restart VALUES (1,'synthetic');"), "CREATE TABLE\nINSERT 0 1");
  await service.close();
  service = await start(f);
  assert.equal(service.systemIdentifier, first.systemIdentifier);
  assert.equal(sql(service, "SHOW max_connections;"), "30");
  assert.equal(sql(service, "SELECT note FROM qa_restart WHERE id=1;"), "synthetic");
  await service.close();
});

test("draft fixture capacity refuses outside the exact canonical draft state before acquiring a lock", async (t) => {
  const f = await fixture(t);
  const observedBefore = observed.length;
  for (const durableDraftFixture of [true, "true", 64]) {
    await assert.rejects(startLocalQaPostgres({ stateDirectory: f.stateDirectory, durableDraftFixture }),
      { message: "local_qa_postgres_start_failed" });
  }
  await assert.rejects(lstat(join(f.stateDirectory, ".local-qa-postgres.lock")), { code: "ENOENT" });
  assert.equal(observed.length, observedBefore);
});

test("wrong stored password refuses without rebuilding the cluster", async (t) => {
  const f = await fixture(t); await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
  const password = join(f.stateDirectory, ".local-qa-postgres-password");
  const original = await readFile(password, "utf8");
  await writeFile(password, (original[0] === "f" ? "e" : "f") + original.slice(1), { mode: 0o600 });
  await assert.rejects(startLocalQaPostgres({ stateDirectory: f.stateDirectory }), /local_qa_postgres_start_failed/);
  assert.equal((await readFile(password, "utf8")).length, 65);
});

test("missing or malformed marker, data replacement and config changes fail closed", async (t) => {
  for (const scenario of ["marker", "bad-marker", "directory", "config"]) {
    const f = await fixture(t); await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
    if (scenario === "marker") await rename(join(f.stateDirectory, ".local-qa-postgres-ready.json"), join(f.stateDirectory, "saved-ready"));
    if (scenario === "bad-marker") await writeFile(join(f.stateDirectory, ".local-qa-postgres-ready.json"), "{}\n", { mode: 0o600 });
    if (scenario === "directory") await rename(f.state.postgresDirectory, join(f.stateDirectory, "old-postgres"));
    if (scenario === "config") await writeFile(join(f.state.postgresDirectory, "pg_hba.conf"), "# changed\n", { mode: 0o600 });
    await assert.rejects(startLocalQaPostgres({ stateDirectory: f.stateDirectory }), /local_qa_postgres_start_failed/);
  }
});

test("password mode change and concurrent starts refuse", async (t) => {
  const f = await fixture(t); await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
  const password = join(f.stateDirectory, ".local-qa-postgres-password");
  await chmod(password, 0o640);
  await assert.rejects(startLocalQaPostgres({ stateDirectory: f.stateDirectory }), /local_qa_postgres_start_failed/);
  // A refused start retains an ambiguous lock; use a fresh owned cluster for concurrency.
  const fresh = await fixture(t); await initializeLocalQaPostgres({ stateDirectory: fresh.stateDirectory });
  const service = await start(fresh);
  await assert.rejects(startLocalQaPostgres({ stateDirectory: fresh.stateDirectory }), /local_qa_postgres_start_failed/);
  assert.equal(sql(service, "SELECT 1;"), "1"); await service.close();
});

test("unexpected tool directory refuses before PostgreSQL lock or process creation", async (t) => {
  const f = await fixture(t);
  const stateDirectory = f.stateDirectory;
  const lock = join(stateDirectory, ".local-qa-postgres.lock");
  const before = (await readdir(stateDirectory)).sort();
  const observedBefore = observed.length;
  await assert.rejects(initializeLocalQaPostgres({ stateDirectory, binDirectory: "/unexpected" }),
    { message: "local_qa_postgres_init_failed_arguments" });
  await assert.rejects(lstat(lock), { code: "ENOENT" });
  await assert.rejects(startLocalQaPostgres({ stateDirectory, binDirectory: "/unexpected" }),
    { message: "local_qa_postgres_start_failed" });
  await assert.rejects(lstat(lock), { code: "ENOENT" });
  assert.equal(observed.length, observedBefore);
  assert.deepEqual((await readdir(stateDirectory)).sort(), before);
  assert.deepEqual(await readdir(f.state.postgresDirectory), []);
});

test("occupied loopback port reaches an owned PostgreSQL child that exits without readiness", async (t) => {
  const f = await fixture(t);
  await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
  const lock = join(f.stateDirectory, ".local-qa-postgres.lock");
  await assert.rejects(lstat(lock), { code: "ENOENT" });
  const occupied = createServer();
  await new Promise((resolve, reject) => { occupied.once("error", reject); occupied.listen(0, "127.0.0.1", resolve); });
  try {
    const address = occupied.address(); assert(address && typeof address !== "string");
    const probe = createServer();
    await assert.rejects(new Promise((resolve, reject) => {
      probe.once("error", reject);
      probe.listen(address.port, "127.0.0.1", () => probe.close(resolve));
    }), { code: "EADDRINUSE" });
    const events = [];
    await assert.rejects(startPostgres({
      stateDirectory: f.stateDirectory, port: address.port,
      onLifecycle(event) { onLifecycle(event); events.push(event); },
    }), { message: "local_qa_postgres_start_failed" });
    assert.equal(events.length, 2);
    const [started, exited] = events;
    assert.equal(started.event, "started");
    assert.equal(started.dataDirectory, f.state.postgresDirectory);
    assert.equal(started.port, address.port);
    assert.deepEqual(exited, { event: "exited", pid: started.pid, exitCode: 1, signal: null });
    assert.deepEqual(occupied.address(), address);
    await assert.rejects(lstat(join(f.state.postgresDirectory, "postmaster.pid")), { code: "ENOENT" });
    assert((await lstat(lock)).isFile(), "failed launch retains its lock for inspection");
  } finally { await new Promise((resolve, reject) => occupied.close(error => error ? reject(error) : resolve())); }
});

test("claimed parent refuses init after complete child artifact loss", async (t) => {
  const f = await fixture(t); await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
  for (const name of await readdir(f.state.postgresDirectory)) {
    await rm(join(f.state.postgresDirectory, name), { recursive: true, force: true });
  }
  for (const name of [".local-qa-postgres-ready.json", ".local-qa-postgres-password"]) {
    await rm(join(f.stateDirectory, name));
  }
  await assert.rejects(initializeLocalQaPostgres({ stateDirectory: f.stateDirectory }), /local_qa_postgres_init_failed/);
  assert.deepEqual(await readdir(f.state.postgresDirectory), []);
  await assert.rejects(lstat(join(f.stateDirectory, ".local-qa-postgres-password")), { code: "ENOENT" });
  await assert.rejects(lstat(join(f.stateDirectory, ".local-qa-postgres-ready.json")), { code: "ENOENT" });
});

test("changed live marker fails close but reaps the owned postmaster and retains the lock", async (t) => {
  const f = await fixture(t); await initializeLocalQaPostgres({ stateDirectory: f.stateDirectory });
  const service = await start(f);
  await writeFile(join(f.stateDirectory, ".local-qa-postgres-ready.json"), "{}\n", { mode: 0o600 });
  await assert.rejects(service.close(), /local_qa_postgres_close_failed/);
  f.services.splice(f.services.indexOf(service), 1);
  await assert.rejects(lstat(join(f.state.postgresDirectory, "postmaster.pid")), { code: "ENOENT" });
  assert((await lstat(join(f.stateDirectory, ".local-qa-postgres.lock"))).isFile());
});
