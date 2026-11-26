// Owned PostgreSQL lifecycle for the disposable Linux C2C QA unit only.
import { constants } from "node:fs";
import { lstat, open, readlink, unlink } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { createLocalQaLinuxPgReady } from "./local-qa-linux-pg-ready.mjs";
import { createLocalQaLinuxPgTools } from "./local-qa-linux-pg-tools.mjs";
import {
  expectedLocalQaLinuxC2cPgCapsule, validateLocalQaLinuxC2cPgCapsulePlan,
} from "./local-qa-linux-c2c-pg-capsule.mjs";
import { createLocalQaLinuxPidfdBinder } from "./local-qa-linux-pidfd-client.mjs";
import { createLocalQaLinuxPostmasterObserver } from "./local-qa-linux-postmaster-observer.mjs";
import {
  assertLocalQaLinuxPostgresRunRoot, LINUX_DURABLE_PG_IDENTITY_SQL, observeLocalQaLinuxPostmasterReady,
  validateLinuxDurablePgIdentity,
} from "./local-qa-linux-postgres.mjs";
import { requireLocalQaLinuxC2cEnvironment } from "./local-qa-linux-c2c-session.mjs";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";

const LOCK = ".local-qa-postgres.lock";
const REFUSAL = "local_qa_linux_pg_runtime_refused";
const UNCLOSED = "local_qa_linux_pg_runtime_unclosed";
const TOOL_REFUSED = "BLUEY_LOCAL_QA_LINUX_PG_TOOLS_REFUSED";
const TOOL_UNCLOSED = "BLUEY_LOCAL_QA_LINUX_PG_TOOLS_UNCLOSED";
const PIDFD_UNCLOSED = "BLUEY_LOCAL_QA_LINUX_PIDFD_UNCLOSED";
const MAX_READY_MS = 20_000;
const MAX_STOP_MS = 15_000;
const runtimeKeys = ["assertOwnedUnit", "pidfdToolchain", "networkNamespace", "pgCapsule", "port", "root",
  "stateDirectory", "unitCgroup"].sort().join(",");
const identityKeys = "dataDirectory,databaseUrl,pid,port,root,sql";
const whole = (re, value) => typeof value === "string" && re.exec(value)?.[0] === value;
const stamp = s => [s.dev, s.ino, s.uid, s.mode, s.nlink, s.size, s.mtimeNs, s.ctimeNs].map(String).join(":");
function need(value) { if (!value) throw new Error(REFUSAL); }
const safeError = error => new Error(
  [UNCLOSED, TOOL_UNCLOSED, PIDFD_UNCLOSED].includes(error?.message) ? UNCLOSED : REFUSAL,
);
function hasKeys(value, keys) {
  return value && Object.getPrototypeOf(value) === Object.prototype && Object.keys(value).sort().join(",") === keys;
}

function freezePins(pins, keys) {
  need(hasKeys(pins, keys));
  return Object.freeze(Object.fromEntries(Object.entries(pins).map(([name, tool]) => {
    need(hasKeys(tool, "path,sha256") && typeof tool.path === "string" && resolve(tool.path) === tool.path
      && whole(/^[0-9a-f]{64}$/, tool.sha256) && !/[\0\r\n]/.test(tool.path));
    return [name, Object.freeze({ path: tool.path, sha256: tool.sha256 })];
  })));
}

export function validateLocalQaLinuxPgRuntimeBinding(value) {
  need(hasKeys(value, runtimeKeys));
  need(isDurableQaFixtureRoot(value.root, "linux") && resolve(value.root) === value.root
    && value.stateDirectory === join(value.root, "bluey-jobs-qa-draft"));
  need(Number.isSafeInteger(value.port) && value.port > 1024 && value.port <= 65535 && value.port !== 5432
    && typeof value.assertOwnedUnit === "function");
  need(whole(/^net:\[[1-9][0-9]{0,19}\]$/, value.networkNamespace)
    && whole(/^\/[^\0\r\n]{1,512}$/, value.unitCgroup) && resolve(value.unitCgroup) === value.unitCgroup);
  try { validateLocalQaLinuxC2cPgCapsulePlan(value.pgCapsule); } catch { throw new Error(REFUSAL); }
  const pgCapsule = expectedLocalQaLinuxC2cPgCapsule(value.pgCapsule.run);
  need(value.unitCgroup === `/system.slice/bluey-jobs-qa-${pgCapsule.run}.service`);
  const pidfdToolchain = freezePins(value.pidfdToolchain, "helper,python");
  need(basename(pidfdToolchain.helper.path) === "local-qa-linux-postmaster-pidfd.py"
    && pidfdToolchain.helper.path !== pidfdToolchain.python.path);
  return Object.freeze({ ...value, pgCapsule, pidfdToolchain });
}

export function requireLocalQaLinuxPgIdentityRequest(request, proof, databaseUrl) {
  need(hasKeys(request, identityKeys) && request.sql === LINUX_DURABLE_PG_IDENTITY_SQL
    && request.databaseUrl === databaseUrl && request.root === proof.root && request.dataDirectory === proof.dataDirectory
    && request.port === proof.port && request.pid === proof.pid);
}

async function missing(path) {
  try { await lstat(path); return false; }
  catch (error) { if (error.code === "ENOENT") return true; throw error; }
}

async function flushDirectory(path) {
  const dir = await open(path, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try { await dir.sync(); } finally { await dir.close(); }
}

async function acquireLock(path, assertState) {
  await assertState();
  const lockPath = join(path, LOCK);
  const file = await open(lockPath, constants.O_CREAT | constants.O_EXCL
    | constants.O_WRONLY | constants.O_NOFOLLOW, 0o600);
  let identity;
  try {
    await file.sync();
    identity = stamp(await file.stat({ bigint: true }));
  } finally { await file.close(); }
  await flushDirectory(path);
  const check = async () => {
    await assertState();
    const entry = await lstat(lockPath, { bigint: true });
    need(entry.isFile() && entry.uid === BigInt(process.getuid()) && entry.nlink === 1n
      && (entry.mode & 0o7777n) === 0o600n && stamp(entry) === identity);
  };
  await check();
  return Object.freeze({ check, async release() {
    await check();
    await unlink(lockPath);
    await flushDirectory(path);
  } });
}

function watchChild(child) {
  let result;
  // Attach before the next await. "close" follows spawn error or process
  // exit and confirms the ChildProcess and stdio lifecycle is finished.
  let failed = false;
  child.once("error", () => { failed = true; });
  const terminal = new Promise(resolve => child.once("close", (code, signal) => {
    result = Object.freeze({ code, signal, failed });
    resolve(result);
  }));
  return Object.freeze({ terminal, get result() { return result; } });
}

async function withTimeout(promise, limit) {
  const timer = new AbortController();
  try { return await Promise.race([promise, delay(limit, undefined, { signal: timer.signal })]); }
  finally { timer.abort(); }
}

function expectedIdentity(dataDirectory, port, systemIdentifier) {
  return Object.freeze({ address: "127.0.0.1", port, database: "bluey_jobs_qa", role: "bluey_qa_local",
    dataDirectory, systemIdentifier });
}

async function assertSelfCgroup(expected) {
  const file = await open("/proc/self/cgroup", constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const buffer = Buffer.alloc(1024);
    const { bytesRead } = await file.read(buffer);
    need(bytesRead < buffer.length && buffer.subarray(0, bytesRead).toString("utf8") === `0::${expected}\n`);
    need((await file.read(buffer)).bytesRead === 0);
  } finally { await file.close(); }
}

/**
 * Create an owner-bound controller, not a general PostgreSQL launcher.
 * assertOwnedUnit is the independent unit/boot/isolated-net proof.
 * The pinned helper binds the direct child to a pidfd and handles stop/dispose.
 * Only the enclosing unit owner may reap the scope if binding or close is
 * uncertain. Pins come from the independently sealed unit, not API input.
 */
export async function createLocalQaLinuxPgRuntime(input) {
  try {
    const binding = validateLocalQaLinuxPgRuntimeBinding(input);
    const { root, stateDirectory, port, networkNamespace, unitCgroup } = binding;
    need(process.platform === "linux" && typeof process.getuid === "function" && process.getuid() !== 0
      && root === process.env.BLUEY_TEST_RUN_ROOT && isDurableQaFixtureRoot(root));
    const rootIdentity = await assertLocalQaLinuxPostgresRunRoot(root);
    const unit = Object.freeze({ root, stateDirectory, networkNamespace, unitCgroup });
    const assertOwnedUnit = async observed => {
      requireLocalQaLinuxC2cEnvironment(process.env);
      const checkSelf = async () => {
        need(await readlink("/proc/self/ns/net") === networkNamespace);
        await assertSelfCgroup(unitCgroup);
        need(await assertLocalQaLinuxPostgresRunRoot(root) === rootIdentity);
      };
      await checkSelf();
      need(await binding.assertOwnedUnit(Object.freeze({ ...observed, ...unit })) === true);
      await checkSelf();
      return true;
    };
    await assertOwnedUnit();
    const ready = await createLocalQaLinuxPgReady({ root, stateDirectory, assertUnit: assertOwnedUnit });
    const tools = await createLocalQaLinuxPgTools({ root, stateDirectory, assertUnit: assertOwnedUnit,
      pgCapsule: binding.pgCapsule, port });
    const bindPostmasterStop = await createLocalQaLinuxPidfdBinder({
      ...unit, assertOwnedUnit, pidfdToolchain: binding.pidfdToolchain,
    });
    const { dataDirectory } = ready;
    const pidFile = join(dataDirectory, "postmaster.pid");
    const postgresRuntime = Object.freeze({ postgresPath: tools.postgresPath, postgresSha256: tools.postgresSha256,
      unitCgroup, networkNamespace, assertOwnedUnit });

    async function launch({ password, systemIdentifier }, lock, signal) {
      signal?.throwIfAborted();
      await lock.check();
      need(await missing(pidFile));
      const observe = await createLocalQaLinuxPostmasterObserver({ root, ...postgresRuntime });
      const log = await open("/dev/null", constants.O_WRONLY | constants.O_NOFOLLOW);
      let child, observedChild, stopper, closing;
      const close = () => closing ??= (async () => {
        if (!child) return;
        // Delay alone does not make a signal safe. Only the directly bound
        // pidfd closure can request shutdown; no fallback numeric PID kill.
        try {
          if (!observedChild.result) {
            if (!stopper) throw new Error(UNCLOSED);
            await lock.check();
            if (!observedChild.result && await withTimeout(stopper.stop(), MAX_STOP_MS) !== true) throw new Error(UNCLOSED);
          }
          const terminal = observedChild.result ?? await withTimeout(observedChild.terminal, MAX_STOP_MS);
          if (!terminal || terminal.failed || terminal.code !== 0 || terminal.signal !== null) throw new Error(UNCLOSED);
          await lock.check();
          if (!(await missing(pidFile))) throw new Error(UNCLOSED);
        } finally {
          if (stopper && await withTimeout(stopper.dispose(), MAX_STOP_MS) !== true) throw new Error(UNCLOSED);
        }
      })();
      try {
        try {
          child = await tools.spawnPostmaster({ port, logFd: log.fd });
          observedChild = watchChild(child);
        } finally { await log.close(); }
        need(Number.isInteger(child.pid) && child.pid > 1);
        const handle = await bindPostmasterStop(Object.freeze({ ...unit, pid: child.pid, port, dataDirectory,
          postgresPath: tools.postgresPath, postgresSha256: tools.postgresSha256 }));
        need(hasKeys(handle, "dispose,stop") && typeof handle.stop === "function" && typeof handle.dispose === "function");
        stopper = Object.freeze({ ...handle });
        const proof = Object.freeze({ root, pid: child.pid, port, dataDirectory });
        const assertPostmaster = async () => {
          signal?.throwIfAborted();
          need(!observedChild.result);
          await lock.check();
          need(await observe(proof) === true);
        };
        const end = performance.now() + MAX_READY_MS;
        for (;;) {
          signal?.throwIfAborted();
          need(!observedChild.result && performance.now() < end);
          await lock.check();
          if (await observeLocalQaLinuxPostmasterReady(proof)) {
            await assertPostmaster();
            // A db-side timeout/refusal cannot be mistaken for ownership.
            // The owned listener plus lock must both revalidate on any retry.
            const timeoutMs = Math.min(2_000, Math.floor(end - performance.now()));
            need(timeoutMs >= 250);
            let accepting = false;
            try { accepting = await tools.probe({ port, password, timeoutMs }) === true; }
            catch (error) {
              // Never retry a tool whose process could not be reaped.
              if (error?.message !== TOOL_REFUSED) throw safeError(error);
            }
            await assertPostmaster();
            if (accepting) break;
          }
          await delay(150, undefined, { signal });
        }
        await assertPostmaster();
        const withPostmaster = async operation => {
          await assertPostmaster();
          const outcome = await operation({ port, password });
          await assertPostmaster();
          return outcome;
        };
        const query = () => withPostmaster(async connection =>
          validateLinuxDurablePgIdentity(await tools.queryIdentity(connection),
            expectedIdentity(dataDirectory, port, systemIdentifier)));
        return Object.freeze({ bootstrap: () => withPostmaster(connection => tools.bootstrapDatabase({
          ...connection, assertReady: async () => { await assertPostmaster(); return true; },
        })),
          checkDatabase: () => withPostmaster(tools.checkDatabase),
          readC2cCensus: accountId => withPostmaster(connection => tools.readC2cCensus({ ...connection, accountId })),
          readReviewFirstReceiptCensus: (accountId, applicationId, phase, runId) =>
            withPostmaster(connection => tools.readReviewFirstReceiptCensus({
              ...connection, accountId, applicationId, phase, runId,
            })),
          query, proof, close });
      } catch (error) {
        try { await close(); } catch { throw new Error(UNCLOSED); }
        throw safeError(error);
      }
    }

    async function verifyRunning(runtime) {
      await runtime.query();
      need(await runtime.checkDatabase() === true);
    }

    // Fresh-only. Marker publication is after a successful SQL check, clean
    // PostgreSQL shutdown, and unchanged stopped physical identity.
    async function initialize({ signal } = {}) {
      try {
        need(signal === undefined || signal instanceof AbortSignal);
        signal?.throwIfAborted();
        const lock = await acquireLock(stateDirectory, ready.assertState);
        const password = await ready.prepareInitialPassword();
        await lock.check();
        await tools.initialize();
        const configHashes = await ready.snapshot();
        const systemIdentifier = await tools.control();
        const runtime = await launch({ password, systemIdentifier }, lock, signal);
        try { await runtime.bootstrap(); await verifyRunning(runtime); }
        finally { await runtime.close(); }
        signal?.throwIfAborted();
        need(await tools.control() === systemIdentifier);
        await ready.commit({ password, systemIdentifier, configHashes });
        await lock.release();
        return Object.freeze({ systemIdentifier });
      } catch (error) {
        // Lock/partial state stays at a failed attempt; never retry or
        // manufacture a ready marker from the retained directory.
        throw safeError(error);
      }
    }

    // Refuses an uninitialized or modified database, without repair.
    async function start({ signal } = {}) {
      let runtime;
      try {
        need(signal === undefined || signal instanceof AbortSignal);
        signal?.throwIfAborted();
        const lock = await acquireLock(stateDirectory, ready.assertState);
        const { systemIdentifier, password } = await ready.read();
        need(await tools.control() === systemIdentifier);
        runtime = await launch({ password, systemIdentifier }, lock, signal);
        await verifyRunning(runtime);
        const databaseUrl = `postgresql://bluey_qa_local:${password}@127.0.0.1:${port}/bluey_jobs_qa?sslmode=disable`;
        let closing;
        return Object.freeze({ port, systemIdentifier, databaseUrl, postgresRuntime,
          async assertCurrent() {
            need(!closing);
            await runtime.query();
            return true;
          },
          async queryIdentity(request) {
            need(!closing);
            requireLocalQaLinuxPgIdentityRequest(request, runtime.proof, databaseUrl);
            return runtime.query();
          },
          async readC2cCensus(accountId) {
            need(!closing);
            return runtime.readC2cCensus(accountId);
          },
          async readReviewFirstReceiptCensus(accountId, applicationId, phase, runId) {
            need(!closing);
            return runtime.readReviewFirstReceiptCensus(accountId, applicationId, phase, runId);
          },
          close() { return closing ??= (async () => {
            await runtime.close();
            need(await tools.control() === systemIdentifier);
            const stopped = await ready.read();
            need(stopped.systemIdentifier === systemIdentifier && stopped.password === password);
            await lock.release();
          })(); } });
      } catch (error) {
        try { await runtime?.close(); } catch { throw new Error(UNCLOSED); }
        throw safeError(error);
      }
    }
    return Object.freeze({ initialize, start });
  } catch {
    throw new Error(REFUSAL);
  }
}
