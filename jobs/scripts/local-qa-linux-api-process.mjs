// Closed Linux local-QA API process controller.  It owns neither HTTP nor a
// numeric-PID fallback; its only shutdown authority is the supplied pidfd bind.
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { constants } from "node:fs";
import { lstat, open, readlink, realpath, stat } from "node:fs/promises";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { parseLocalQaBuildBinding } from "./local-qa-build-binding.mjs";
import { isDurableQaFixtureRoot, parseDurableQaPostgresUrl } from "./local-qa-durable-posting-seeder.mjs";
import {
  assertLinuxTcpPortVacant, observeOwnedLinuxLoopbackListener, parseLinuxPostmasterStat, requireLinuxPostmasterUid,
} from "./local-qa-linux-postmaster-observer.mjs";
import { LOCAL_QA_LINUX_EFFECT_FLAGS } from "./local-qa-linux-c2c-session.mjs";

const REFUSAL = "BLUEY_LOCAL_QA_LINUX_API_PROCESS_REFUSED";
const UNCLOSED = "BLUEY_LOCAL_QA_LINUX_API_PROCESS_UNCLOSED";
// Match the PID descriptor helper's executable admission ceiling.
const MAX_ARTIFACT = 256 * 1024 * 1024, MAX_MANIFEST = 65_536, OUTPUT = 1024 * 1024;
const ENV_KEYS = Object.freeze([
  "PATH", "LANG", "LC_ALL", "TZ", "HOME", "TMPDIR", "RUST_LOG", "BLUEY_API_HOST", "BLUEY_PORT", "BLUEY_PUBLIC_URL",
  "BLUEY_SERVER_DB_BACKEND", "BLUEY_DATABASE_URL", "BLUEY_JWT_SECRET", "BLUEY_JOBS_DATA_KEY", "BLUEY_JOBS_BETA_ENABLED",
  "BLUEY_ADMIN_EMAILS", "BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID", "BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY",
  "BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID", "BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON", "BLUEY_OBJECT_ENDPOINT_URL",
  "BLUEY_OBJECT_BUCKET", "BLUEY_OBJECT_ACCESS_KEY_ID", "BLUEY_OBJECT_SECRET_ACCESS_KEY", "BLUEY_OBJECT_REGION",
  "BLUEY_OBJECT_KEY_PREFIX", "BLUEY_OBJECT_MAX_BYTES", "BLUEY_MAIL_TRANSPORT", "BLUEY_SMTP_HOST", "BLUEY_SMTP_PORT",
  "BLUEY_SMTP_STARTTLS", "BLUEY_SMTP_FROM", "BLUEY_DEV_LOG_AUTH_LINKS", ...LOCAL_QA_LINUX_EFFECT_FLAGS,
].sort().join(","));
const SHA = value => typeof value === "string" && value.length === 64 && /^[0-9a-f]+$/.test(value);
const need = value => { if (!value) throw new Error(REFUSAL); };
const stamp = value => [value.dev, value.ino, value.uid, value.mode, value.nlink, value.size, value.mtimeNs, value.ctimeNs].map(String).join(":");
const port = value => { need(Number.isSafeInteger(value) && value > 1024 && value <= 65535 && value !== 5432); return value; };
const privateDir = async path => {
  need(typeof path === "string" && resolve(path) === path && await realpath(path) === path);
  const s = await lstat(path, { bigint: true });
  need(s.isDirectory() && s.uid === BigInt(process.getuid()) && (s.mode & 0o7777n) === 0o700n);
  return [s.dev, s.ino, s.uid, s.mode].join(":");
};
async function procText(path) {
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW);
  try {
    const buffer = Buffer.alloc(MAX_MANIFEST + 1);
    let size = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, size, buffer.length - size, null);
      size += bytesRead;
      need(size <= MAX_MANIFEST);
      if (bytesRead === 0) break;
    }
    const bytes = buffer.subarray(0, size);
    need(!bytes.includes(0));
    return bytes.toString("utf8");
  } finally { await file.close(); }
}

async function pin(path, expected, executable = false) {
  need(typeof path === "string" && resolve(path) === path && SHA(expected) && await realpath(path) === path);
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true }), limit = executable ? MAX_ARTIFACT : MAX_MANIFEST;
    need(before.isFile() && before.nlink === 1n && before.size > 0n && before.size <= BigInt(limit)
      && (before.uid === 0n || before.uid === BigInt(process.getuid())) && (before.mode & 0o7022n) === 0n
      && (!executable || (before.mode & 0o111n) !== 0n));
    const hash = createHash("sha256"), b = Buffer.allocUnsafe(65536); let size = 0;
    for (;;) { const { bytesRead } = await file.read(b, 0, b.length, null); if (!bytesRead) break; size += bytesRead; need(size <= limit); hash.update(b.subarray(0, bytesRead)); }
    const after = await file.stat({ bigint: true }), named = await lstat(path, { bigint: true });
    need(BigInt(size) === before.size && stamp(before) === stamp(after) && stamp(before) === stamp(named) && hash.digest("hex") === expected);
    return stamp(before);
  } finally { await file.close(); }
}

export function validateLocalQaLinuxApiEnvironment(env, root, stateDirectory, selected) {
  need(env && Object.getPrototypeOf(env) === Object.prototype && Object.keys(env).sort().join(",") === ENV_KEYS
    && Object.values(env).every(v => typeof v === "string" && v.length <= 8192 && !/[\0\r\n]/.test(v)));
  need(env.BLUEY_API_HOST === "127.0.0.1" && env.BLUEY_PORT === String(selected) && env.HOME === stateDirectory
    && env.TMPDIR === join(root, "tmp") && env.PATH === "/usr/bin:/bin" && env.LC_ALL === "C"
    && env.BLUEY_PUBLIC_URL === `http://127.0.0.1:${selected}` && env.BLUEY_JOBS_BETA_ENABLED === "1"
    && LOCAL_QA_LINUX_EFFECT_FLAGS.every(key => env[key] === "0")
    && env.LANG === "C" && env.TZ === "UTC" && env.RUST_LOG === "warn"
    && env.BLUEY_SERVER_DB_BACKEND === "postgres" && env.BLUEY_ADMIN_EMAILS === "operator@bluey-qa.test"
    && env.BLUEY_MAIL_TRANSPORT === "smtp" && env.BLUEY_SMTP_HOST === "127.0.0.1" && env.BLUEY_SMTP_STARTTLS === "false"
    && env.BLUEY_DEV_LOG_AUTH_LINKS === "0");
  const db = parseDurableQaPostgresUrl(env.BLUEY_DATABASE_URL);
  const objectPort = /^http:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})$/.exec(env.BLUEY_OBJECT_ENDPOINT_URL)?.[1];
  const smtpPort = Number(env.BLUEY_SMTP_PORT);
  need(port(smtpPort) && env.BLUEY_SMTP_PORT === String(smtpPort) && port(Number(objectPort))
    && new Set([smtpPort, Number(objectPort), db.port, selected]).size === 4
    && /^qa-[0-9a-f]{32}$/.test(env.BLUEY_OBJECT_BUCKET) && /^qa-[0-9a-f]{32}$/.test(env.BLUEY_OBJECT_KEY_PREFIX));
  return Object.freeze({ ...env });
}

async function bounded(result) {
  const timer = new AbortController();
  try {
    const value = await Promise.race([result, delay(15_000, undefined, { signal: timer.signal })]);
    if (value === undefined) throw new Error(UNCLOSED);
    return value;
  } finally { timer.abort(); }
}

export async function prepareLocalQaLinuxApiProcess({ root, stateDirectory, port: selected, buildBinding,
  unitCgroup, networkNamespace, assertOwnedUnit, bindApiStop }) {
  try {
    port(selected); const binding = parseLocalQaBuildBinding(buildBinding);
    need(process.platform === "linux" && typeof process.getuid === "function" && process.getuid() !== 0
      && root === process.env.BLUEY_TEST_RUN_ROOT && isDurableQaFixtureRoot(root) && stateDirectory === join(root, "bluey-jobs-qa-draft")
      && typeof assertOwnedUnit === "function" && typeof bindApiStop === "function" && /^\/(?:[^\0\r\n]+)?$/.test(unitCgroup)
      && /^net:\[[1-9][0-9]{0,19}\]$/.test(networkNamespace));
    // Artifact custody is established before touching fixture state.
    const apiStamp = await pin(binding.artifactPath, binding.artifactSha256, true);
    const manifestStamp = await pin(binding.manifestFile, binding.manifestSha256);
    const parent = root.slice(0, root.lastIndexOf("/"));
    const rootScope = [await privateDir(parent), await privateDir(root)];
    let stateScope, attempted = false;
    async function checkScope() { need(JSON.stringify([await privateDir(parent), await privateDir(root)]) === JSON.stringify(rootScope)); if (stateScope) need(await privateDir(stateDirectory) === stateScope); }
    async function checkSelf() {
      await checkScope();
      need(await readlink("/proc/self/ns/net") === networkNamespace
        && await procText("/proc/self/cgroup") === `0::${unitCgroup}\n`);
    }
    async function unit(observed) {
      await checkSelf();
      need(await assertOwnedUnit(Object.freeze({ root, stateDirectory, unitCgroup, networkNamespace, ...observed })) === true);
      await checkSelf();
    }
    await unit({});
    return Object.freeze({ async start({ env, signal } = {}) {
      let child, terminal, exit, pidfd, output = 0, closePromise, attached;
      const portProof = () => ({ pid: child.pid, port: selected, apiPath: binding.artifactPath });
      const close = () => closePromise ??= (async () => {
        if (!child) return;
        let error;
        try {
          if (!exit) {
            need(pidfd);
            await unit(portProof());
            if (!exit) need(await bounded(pidfd.stop()) === true);
          }
          const observed = exit ?? await bounded(terminal);
          need(observed.code === 0 && observed.signal === null && !observed.failed);
          await unit({});
          await assertLinuxTcpPortVacant(selected);
          need(await pin(binding.artifactPath, binding.artifactSha256, true) === apiStamp
            && await pin(binding.manifestFile, binding.manifestSha256) === manifestStamp);
        } catch { error = new Error(UNCLOSED); }
        finally {
          try { if (pidfd) need(await bounded(pidfd.dispose()) === true); }
          catch { error = new Error(UNCLOSED); }
        }
        if (error) throw error;
      })();
      try {
        need(!attempted && (signal === undefined || signal instanceof AbortSignal)); attempted = true;
        signal?.throwIfAborted();
        const closed = validateLocalQaLinuxApiEnvironment(env, root, stateDirectory, selected);
        stateScope = await privateDir(stateDirectory);
        await unit({});
        need(await pin(binding.artifactPath, binding.artifactSha256, true) === apiStamp
          && await pin(binding.manifestFile, binding.manifestSha256) === manifestStamp);
        await assertLinuxTcpPortVacant(selected);
        child = spawn(binding.artifactPath, [], { cwd: stateDirectory, env: closed, detached: false, stdio: ["ignore", "pipe", "pipe"] });
        const drain = chunk => { output = Math.min(OUTPUT + 1, output + chunk.length); };
        let failed = false;
        child.stdout.on("data", drain); child.stderr.on("data", drain); child.once("error", () => { failed = true; });
        child.stdout.on("error", () => { failed = true; }); child.stderr.on("error", () => { failed = true; });
        terminal = new Promise(resolveTerminal => child.once("close", (code, exitSignal) => {
          exit = Object.freeze({ code, signal: exitSignal, failed });
          resolveTerminal(exit);
        }));
        need(Number.isSafeInteger(child.pid) && child.pid > 1);
        const proc = `/proc/${child.pid}`;
        const processIdentity = async () => {
          const observation = parseLinuxPostmasterStat(await procText(join(proc, "stat")), child.pid);
          need(observation.ppid === process.pid);
          requireLinuxPostmasterUid(await procText(join(proc, "status")), process.getuid());
          need(await procText(join(proc, "cgroup")) === `0::${unitCgroup}\n`
            && await readlink(join(proc, "ns", "net")) === networkNamespace
            && await readlink(join(proc, "cwd")) === stateDirectory
            && await readlink(join(proc, "exe")) === binding.artifactPath
            && stamp(await stat(join(proc, "exe"), { bigint: true })) === apiStamp
            && stamp(await lstat(binding.artifactPath, { bigint: true })) === apiStamp);
          return `${observation.pid}:${observation.ppid}:${observation.pgrp}:${observation.startTime}`;
        };
        const prove = async allowAbsent => {
          signal?.throwIfAborted();
          need(!closePromise && !exit && !failed && output <= OUTPUT);
          await unit(portProof());
          const before = await processIdentity();
          const inode = await observeOwnedLinuxLoopbackListener(proc, selected, allowAbsent);
          await unit(portProof());
          need(await processIdentity() === before && !exit && !failed && output <= OUTPUT);
          if (inode === null) { need(allowAbsent && !attached); return false; }
          need(await observeOwnedLinuxLoopbackListener(proc, selected) === inode);
          const identity = `${before}:${inode}`;
          if (attached === undefined) attached = identity;
          else need(attached === identity);
          return true;
        };
        pidfd = await bindApiStop(Object.freeze({ root, stateDirectory, unitCgroup, networkNamespace, pid: child.pid, port: selected, apiPath: binding.artifactPath, apiSha256: binding.artifactSha256 }));
        need(pidfd && Object.keys(pidfd).sort().join(",") === "dispose,stop" && typeof pidfd.stop === "function" && typeof pidfd.dispose === "function");
        const readyUntil = performance.now() + 60_000;
        while (!await prove(true)) {
          need(performance.now() < readyUntil);
          await delay(100, undefined, { signal });
        }
        return Object.freeze({ port: selected, assertCurrent: () => prove(false), close });
      } catch (error) {
        try { await close(); } catch { throw new Error(UNCLOSED); }
        throw new Error(/unclosed$/i.test(error?.message ?? "") ? UNCLOSED : REFUSAL);
      }
    } });
  } catch { throw new Error(REFUSAL); }
}
