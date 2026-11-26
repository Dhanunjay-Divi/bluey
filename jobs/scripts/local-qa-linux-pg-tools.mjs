// Closed Linux-only PostgreSQL tool surface for the owned local-QA fixture.
// It never accepts caller-supplied SQL, paths, environment, or command args.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readlink, realpath } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";
import { composeLocalQaLinuxC2cPgCommands } from "./local-qa-linux-c2c-pg-command.mjs";
import { validateLocalQaLinuxC2cPgCapsulePlan } from "./local-qa-linux-c2c-pg-capsule.mjs";
import { LINUX_DURABLE_PG_IDENTITY_SQL } from "./local-qa-linux-postgres.mjs";
import { c2cCensusStatement, parseC2cCensusOutput, parseReviewFirstReceiptCensusOutput,
  requireC2cFixtureAccountId, reviewFirstReceiptCensusStatement } from "./local-qa-linux-c2c-census.mjs";

const REFUSAL = "BLUEY_LOCAL_QA_LINUX_PG_TOOLS_REFUSED";
const UNCLOSED = "BLUEY_LOCAL_QA_LINUX_PG_TOOLS_UNCLOSED";
const STATE = "bluey-jobs-qa-draft";
const DATA = "postgres";
const PG_AUTH_FILE = ".local-qa-postgres-password";
const MAX_BINARY = 256 * 1024 * 1024;
const MAX_OUTPUT = 64 * 1024;
const TIMEOUT = 15_000;
const U64 = /^[1-9][0-9]{0,19}$/;
const MANIFEST_BYTES = 240_820;

function refuse() { throw new Error(REFUSAL); }
function need(value) { if (!value) refuse(); }
function preserveUnclosed(error) { if (error?.message === UNCLOSED) throw error; }
function plain(value) { return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype; }
function keys(value, names) { return plain(value) && Object.keys(value).sort().join("|") === [...names].sort().join("|"); }
function stamp(value) { return [value.dev, value.ino, value.uid, value.mode, value.nlink, value.size, value.mtimeNs, value.ctimeNs].map(String).join(":"); }
function directoryStamp(value) { return [value.dev, value.ino, value.uid, value.mode].map(String).join(":"); }
function sha256(value) { return typeof value === "string" && value.length === 64 && /^[0-9a-f]+$/.test(value); }
function port(value) { need(Number.isSafeInteger(value) && value > 1024 && value <= 65535 && value !== 5432); return value; }
function secret(value) { need(sha256(value)); return value; }
function systemId(value) { need(typeof value === "string" && U64.test(value) && BigInt(value) <= 18_446_744_073_709_551_615n); return value; }

async function privateDirectory(path) {
  need(typeof path === "string" && resolve(path) === path && await realpath(path) === path);
  const info = await lstat(path, { bigint: true });
  need(info.isDirectory() && info.uid === BigInt(process.getuid()) && (info.mode & 0o7777n) === 0o700n);
  return directoryStamp(info);
}

async function directoryIdentity(path) {
  need(typeof path === "string" && resolve(path) === path && await realpath(path) === path);
  const info = await lstat(path, { bigint: true }); need(info.isDirectory());
  return directoryStamp(info);
}

async function privatePassword(path) {
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW);
  try {
    const info = await file.stat({ bigint: true }), named = await lstat(path, { bigint: true });
    need(info.isFile() && info.uid === BigInt(process.getuid()) && info.nlink === 1n && (info.mode & 0o7777n) === 0o600n
      && info.size === 65n && stamp(info) === stamp(named));
    return stamp(info);
  } finally { await file.close(); }
}

async function pinnedRegular(record, executable) {
  need(plain(record) && typeof record.path === "string" && resolve(record.path) === record.path
    && typeof record.bytes === "number" && Number.isSafeInteger(record.bytes) && record.bytes > 0
    && sha256(record.sha256) && Number.isSafeInteger(record.mode)
    && record.mode === (executable ? 0o555 : 0o444) && await realpath(record.path) === record.path);
  const file = await open(record.path, constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === 0n && before.nlink === 1n
      && before.size === BigInt(record.bytes) && before.size <= BigInt(MAX_BINARY)
      && (before.mode & 0o7777n) === BigInt(record.mode));
    const hash = createHash("sha256"), buffer = Buffer.allocUnsafe(64 * 1024); let count = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
      if (bytesRead === 0) break;
      count += bytesRead; need(count <= MAX_BINARY); hash.update(buffer.subarray(0, bytesRead));
    }
    const after = await file.stat({ bigint: true }), named = await lstat(record.path, { bigint: true });
    need(BigInt(count) === before.size && stamp(before) === stamp(after) && stamp(before) === stamp(named));
    need(hash.digest("hex") === record.sha256);
    return Object.freeze({ path: record.path, sha256: record.sha256, stamp: stamp(before) });
  } finally { await file.close(); }
}

async function stableRegular(record, expected, executable) {
  need(plain(expected) && typeof expected.stamp === "string" && typeof expected.sha256 === "string");
  need(plain(record) && typeof record.path === "string" && resolve(record.path) === record.path
    && typeof record.bytes === "number" && Number.isSafeInteger(record.bytes) && record.bytes > 0
    && sha256(record.sha256) && Number.isSafeInteger(record.mode)
    && record.mode === (executable ? 0o555 : 0o444) && await realpath(record.path) === record.path);
  const info = await lstat(record.path, { bigint: true });
  need(info.isFile() && info.uid === 0n && info.nlink === 1n && info.size === BigInt(record.bytes)
    && (info.mode & 0o7777n) === BigInt(record.mode) && stamp(info) === expected.stamp);
}

async function protectedRoot(path, capsuleRoot) {
  for (let current = path;; current = dirname(current)) {
    need(await realpath(current) === current);
    const info = await lstat(current, { bigint: true });
    need(info.isDirectory() && info.uid === 0n && (info.mode & 0o022n) === 0n);
    if (current === capsuleRoot || current.startsWith(`${capsuleRoot}/`)) {
      need((info.mode & 0o7777n) === 0o555n);
    }
    if (current === "/") return;
  }
}

async function pinnedLink(record) {
  need(keys(record, ["mode", "path", "target", "type"]) && record.type === "link"
    && record.mode === 0o777 && typeof record.path === "string" && resolve(record.path) === record.path
    && typeof record.target === "string" && /^[A-Za-z0-9_.+-]{1,128}$/.test(record.target));
  const info = await lstat(record.path, { bigint: true });
  need(info.isSymbolicLink() && info.uid === 0n && info.nlink === 1n
    && (info.mode & 0o7777n) === BigInt(record.mode)
    && await readlink(record.path) === record.target);
  return Object.freeze({ stamp: stamp(info) });
}

async function stableLink(record, expected) {
  need(plain(expected) && typeof expected.stamp === "string");
  const pinned = await pinnedLink(record);
  need(pinned.stamp === expected.stamp);
}

function capsulePins(plan) {
  const tools = Object.freeze(Object.fromEntries(Object.entries(plan.tools).map(([name, value]) =>
    [name, Object.freeze({ ...value })])));
  const libraries = Object.freeze(Object.fromEntries(Object.entries(plan.libraries).map(([name, value]) =>
    [name, Object.freeze({ ...value })])));
  const manifest = Object.freeze({
    path: join(plan.capsuleRoot, "files.json"),
    bytes: MANIFEST_BYTES,
    sha256: plan.manifestSha256,
    mode: 0o444,
  });
  return Object.freeze({ capsuleRoot: plan.capsuleRoot, tools, libraries, manifest });
}

async function run(path, args, options) {
  return await new Promise((resolveRun, rejectRun) => {
    let output = 0, timedOut = false, failed = false, stdout = "", stderr = "", settled = false, timer, reapTimer;
    const child = spawn(path, args, { cwd: options.cwd, env: options.env, stdio: ["pipe", "pipe", "pipe"], detached: false });
    const stop = () => { if (!child.killed) child.kill("SIGKILL"); };
    const settle = (value, failure) => {
      if (settled) return; settled = true; clearTimeout(timer); clearTimeout(reapTimer);
      if (failure) rejectRun(new Error(REFUSAL)); else resolveRun(value);
    };
    const receive = (chunk, target) => {
      output += chunk.length;
      if (output > MAX_OUTPUT) { failed = true; stop(); return; }
      if (target === "stdout") stdout += chunk.toString("utf8"); else stderr += chunk.toString("utf8");
    };
    child.stdout.on("data", chunk => receive(chunk, "stdout")); child.stderr.on("data", chunk => receive(chunk, "stderr"));
    child.once("error", () => { if (!child.pid) settle(undefined, true); else failed = true; });
    child.stdin.once("error", () => { failed = true; stop(); });
    const timeoutMs = options.timeoutMs ?? TIMEOUT;
    need(Number.isSafeInteger(timeoutMs) && timeoutMs >= 250 && timeoutMs <= TIMEOUT);
    timer = setTimeout(() => {
      timedOut = true; stop();
      reapTimer = setTimeout(() => {
        if (settled) return; settled = true; clearTimeout(timer);
        rejectRun(new Error(UNCLOSED));
      }, 1_000);
    }, timeoutMs);
    child.once("close", code => {
      if (failed || timedOut || code !== 0 || /\0/.test(stdout) || /\0/.test(stderr)) settle(undefined, true);
      else settle(stdout, false);
    });
    try { child.stdin.end(options.stdin ?? ""); } catch { failed = true; stop(); }
  });
}

/** Build the closed, pinned tool surface used by the Linux local-QA lifecycle. */
export async function createLocalQaLinuxPgTools({ root, stateDirectory, pgCapsule, port: selectedPort, assertUnit }) {
  try {
    need(process.platform === "linux" && typeof process.getuid === "function" && process.getuid() !== 0
      && typeof root === "string" && root === process.env.BLUEY_TEST_RUN_ROOT && isDurableQaFixtureRoot(root)
      && stateDirectory === join(root, STATE) && typeof assertUnit === "function");
    port(selectedPort);
    validateLocalQaLinuxC2cPgCapsulePlan(pgCapsule);
    const commands = composeLocalQaLinuxC2cPgCommands({
      capsulePlan: pgCapsule,
      state: {
        home: stateDirectory,
        dataDirectory: join(stateDirectory, DATA),
        passwordFile: join(stateDirectory, PG_AUTH_FILE),
      },
      port: selectedPort,
    });
    const capsule = capsulePins(pgCapsule);
    const capsuleDirectories = new Set([capsule.capsuleRoot,
      ...Object.values(capsule.tools).map(record => dirname(record.path)),
      ...Object.values(capsule.libraries).map(record => dirname(record.path))]);
    async function verifyCapsuleDirectories() {
      for (const path of capsuleDirectories) await protectedRoot(path, capsule.capsuleRoot);
    }
    const parentDirectory = dirname(root), rootIdentity = await privateDirectory(root);
    const parentIdentity = await directoryIdentity(parentDirectory), stateIdentity = await privateDirectory(stateDirectory);
    const dataDirectory = join(stateDirectory, DATA), dataIdentity = await privateDirectory(dataDirectory);
    need(commands.vectorVersion === "0.8.7" && commands.runtimeQualified === false
      && commands.initdb.path === capsule.tools.initdb.path
      && commands.pgControldata.path === capsule.tools.pg_controldata.path
      && commands.postgres.path === capsule.tools.postgres.path
      && commands.psql.postgres.path === capsule.tools.psql.path
      && commands.psql.bluey_jobs_qa.path === capsule.tools.psql.path);
    async function initialCapsule() {
      await verifyCapsuleDirectories();
      const tools = Object.freeze(Object.fromEntries(await Promise.all(
        Object.entries(capsule.tools).map(async ([name, record]) => [name, await pinnedRegular(record, true)]),
      )));
      const libraries = Object.freeze(Object.fromEntries(await Promise.all(Object.entries(capsule.libraries)
        .map(async ([name, record]) => [name, record.type === "link"
          ? await pinnedLink(record) : await pinnedRegular(record, false)]))));
      const manifest = await pinnedRegular(capsule.manifest, false);
      return Object.freeze({ tools, libraries, manifest });
    }
    const capsuleBaseline = await initialCapsule();
    async function verifyCapsule(executable) {
      need(Object.hasOwn(capsule.tools, executable));
      await verifyCapsuleDirectories();
      const manifest = await pinnedRegular(capsule.manifest, false);
      need(manifest.stamp === capsuleBaseline.manifest.stamp
        && manifest.sha256 === capsuleBaseline.manifest.sha256);
      for (const [name, record] of Object.entries(capsule.tools)) {
        if (name === executable) {
          const current = await pinnedRegular(record, true);
          need(current.stamp === capsuleBaseline.tools[name].stamp
            && current.sha256 === capsuleBaseline.tools[name].sha256);
        } else {
          await stableRegular(record, capsuleBaseline.tools[name], true);
        }
      }
      const hashLibraries = executable === "postgres"
        ? new Set(Object.keys(capsule.libraries))
        : new Set(["libpq", "libpqLink"]);
      for (const [name, record] of Object.entries(capsule.libraries)) {
        if (record.type === "link") {
          await stableLink(record, capsuleBaseline.libraries[name]);
        } else if (hashLibraries.has(name)) {
          const current = await pinnedRegular(record, false);
          need(current.stamp === capsuleBaseline.libraries[name].stamp
            && current.sha256 === capsuleBaseline.libraries[name].sha256);
        } else {
          await stableRegular(record, capsuleBaseline.libraries[name], false);
        }
      }
      return capsuleBaseline.tools[executable];
    }
    async function verifyScope() {
      need(await directoryIdentity(parentDirectory) === parentIdentity && await privateDirectory(root) === rootIdentity
        && await privateDirectory(stateDirectory) === stateIdentity && await privateDirectory(dataDirectory) === dataIdentity);
    }
    async function verify(name) {
      await verifyScope(); need(await assertUnit() === true);
      await verifyCapsule(name);
    }
    async function verifyAfter(name) { await verify(name); }
    async function initialize() {
      try {
        const passwordFile = join(stateDirectory, PG_AUTH_FILE); const passwordIdentity = await privatePassword(passwordFile);
        await verify("initdb"); await verify("postgres");
        await run(commands.initdb.path, commands.initdb.argv,
          { cwd: commands.initdb.cwd, env: commands.initdb.env });
        need(await privatePassword(passwordFile) === passwordIdentity); await verifyAfter("initdb"); await verifyAfter("postgres");
        return true;
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function control() {
      try {
        await verify("pg_controldata");
        const output = await run(commands.pgControldata.path, commands.pgControldata.argv,
          { cwd: commands.pgControldata.cwd, env: commands.pgControldata.env });
        const matches = output.match(/^Database system identifier:\s*([1-9][0-9]{0,19})\s*$/m);
        const state = output.match(/^Database cluster state:\s*(.+?)\s*$/m);
        need(matches !== null && (output.match(/^Database system identifier:/gm) ?? []).length === 1
          && state !== null && (output.match(/^Database cluster state:/gm) ?? []).length === 1 && state[1] === "shut down");
        await verifyAfter("pg_controldata");
        return systemId(matches[1]);
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function spawnPostmaster({ port: selected, logFd }) {
      try {
        need(selected === selectedPort && Number.isInteger(logFd) && logFd >= 0); await verify("postgres");
        const child = spawn(commands.postgres.path, commands.postgres.argv, { cwd: commands.postgres.cwd, env: commands.postgres.env,
          stdio: ["ignore", logFd, logFd], detached: false });
        // Force Node to attach both terminal handlers immediately.  The caller
        // remains responsible for lifecycle policy and all signals.
        child.once("error", () => {}); child.once("close", () => {});
        need(Number.isSafeInteger(child.pid) && child.pid > 1);
        return child;
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function psql(database, selected, password, statement, timeoutMs) {
      need((database === "postgres" || database === "bluey_jobs_qa") && selected === selectedPort);
      await verify("psql");
      const command = commands.psql[database];
      const output = await run(command.path, command.argv,
        { cwd: command.cwd, env: { ...command.env, PGPASSWORD: secret(password) }, stdin: `${statement}\n`, timeoutMs });
      await verifyAfter("psql"); return output;
    }
    async function bootstrapDatabase({ port: selected, password, assertReady }) {
      try {
        secret(password); need(selected === selectedPort && typeof assertReady === "function");
        need((await psql("postgres", selected, password, "CREATE DATABASE bluey_jobs_qa;")).trim() === "");
        need(await assertReady() === true);
        need((await psql("bluey_jobs_qa", selected, password, "CREATE EXTENSION vector;")).trim() === "");
        return true;
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function probe({ port: selected, password, timeoutMs = 2_000 }) {
      try {
        secret(password); need(selected === selectedPort);
        need(Number.isSafeInteger(timeoutMs) && timeoutMs >= 250 && timeoutMs <= 5_000
          && (await psql("postgres", selected, password, "SELECT 1;", timeoutMs)).trim() === "1");
        return true;
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function checkDatabase({ port: selected, password }) {
      try {
        secret(password); need(selected === selectedPort);
        const result = (await psql("bluey_jobs_qa", selected, password,
          "SELECT extversion || '|' || ('[1,0,0]'::vector <=> '[1,0,0]'::vector)::text FROM pg_extension WHERE extname='vector';")).trim();
        need(result === "0.8.7|0"); return true;
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function queryIdentity({ port: selected, password }) {
      try {
        secret(password); need(selected === selectedPort);
        const statement = `SELECT row_to_json(identity)::text FROM (${LINUX_DURABLE_PG_IDENTITY_SQL}) AS identity;`;
        const text = (await psql("bluey_jobs_qa", selected, password, statement)).trim();
        const value = JSON.parse(text);
        need(keys(value, ["address", "port", "database", "role", "dataDirectory", "systemIdentifier"])
          && value.address === "127.0.0.1" && value.port === selected && value.database === "bluey_jobs_qa"
          && value.role === "bluey_qa_local" && value.dataDirectory === dataDirectory);
        systemId(value.systemIdentifier);
        return Object.freeze({ address: value.address, port: value.port, database: value.database, role: value.role,
          dataDirectory: value.dataDirectory, systemIdentifier: value.systemIdentifier });
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function readC2cCensus({ port: selected, password, accountId }) {
      try {
        secret(password); need(selected === selectedPort); requireC2cFixtureAccountId(accountId);
        const output = await psql("bluey_jobs_qa", selected, password, c2cCensusStatement(accountId));
        return parseC2cCensusOutput(output);
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    async function readReviewFirstReceiptCensus({ port: selected, password, accountId, applicationId, phase, runId }) {
      try {
        secret(password);
        need(selected === selectedPort);
        const statement = reviewFirstReceiptCensusStatement(accountId, applicationId, phase, runId);
        const output = await psql("bluey_jobs_qa", selected, password, statement);
        return parseReviewFirstReceiptCensusOutput(output, phase);
      } catch (error) { preserveUnclosed(error); refuse(); }
    }
    return Object.freeze({ postgresPath: capsuleBaseline.tools.postgres.path,
      postgresSha256: capsuleBaseline.tools.postgres.sha256,
      initialize, control, spawnPostmaster, probe, checkDatabase, bootstrapDatabase, queryIdentity,
      readC2cCensus, readReviewFirstReceiptCensus });
  } catch { refuse(); }
}
