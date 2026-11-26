// One browser for the sealed Linux C2C test. No executable discovery, default
// browser download, untrusted launch args, network fallback or stored login.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, opendir, realpath, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { assertLocalQaLinuxPostgresRunRoot } from "./local-qa-linux-postgres.mjs";

const REFUSED = "BLUEY_LOCAL_QA_LINUX_C2C_CHROMIUM_REFUSED";
const UNCLOSED = "BLUEY_LOCAL_QA_LINUX_C2C_CHROMIUM_UNCLOSED";
const PW = "receipt-prep-browser/node_modules/playwright-core";
const CR = "receipt-prep-browser/chromium";
const DRIVER = `${PW}/index.mjs`;
const CHROME = `${CR}/chrome`;
const NODE = "receipt-journey-node/toolchain/bin/node";
const INVENTORY = "inventory.json";
const MAX_INVENTORY = 1024 * 1024;
// The receipt-pinned Linux Chrome executable is 278,568,152 bytes.
const MAX_FILE = 320 * 1024 * 1024;
const MAX_BYTES = 600 * 1024 * 1024;
const MAX_FILES = 768;
const MANDATORY = [DRIVER, CHROME, NODE, `${PW}/package.json`, `${PW}/browsers.json`];
const whole = (re, value) => typeof value === "string" && re.exec(value)?.[0] === value;
const keys = (value, expected) => value !== null && typeof value === "object"
  && Object.getPrototypeOf(value) === Object.prototype && Object.keys(value).sort().join(",") === expected;
const stamp = s => [s.dev, s.ino, s.uid, s.mode, s.nlink, s.size, s.mtimeNs, s.ctimeNs].join(":");
function need(value) { if (!value) throw new Error(REFUSED); }
function pathSafe(value) {
  return whole(/^[A-Za-z0-9._@+/-]{1,240}$/, value)
    && value.split("/").every(part => part && part !== "." && part !== ".." && part !== ".git")
    && (value === NODE || value.startsWith(`${PW}/`) || value.startsWith(`${CR}/`));
}

export function parseLocalQaLinuxC2cChromiumInventory(raw) {
  try {
    need(Buffer.isBuffer(raw) && raw.length > 0 && raw.length <= MAX_INVENTORY);
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(raw));
    need(keys(value, "files,version") && Buffer.from(JSON.stringify(value) + "\n").equals(raw)
      && value.version === 1 && Array.isArray(value.files) && value.files.length <= MAX_FILES);
    let last = "", total = 0;
    const files = value.files.map(row => {
      need(keys(row, "mode,path,sha256,size") && pathSafe(row.path) && row.path > last
        && whole(/^[0-9a-f]{64}$/, row.sha256) && Number.isSafeInteger(row.size) && row.size >= 0 && row.size <= MAX_FILE
        && (row.mode === 0o444 || row.mode === 0o555));
      last = row.path;
      total += row.size;
      need(total <= MAX_BYTES);
      return Object.freeze({ ...row });
    });
    for (const path of MANDATORY) {
      const row = files.find(item => item.path === path);
      need(row && row.size > 0 && row.mode === ([CHROME, NODE].includes(path) ? 0o555 : 0o444));
    }
    const names = new Set(files.map(row => row.path));
    for (const path of names) {
      for (let parent = dirname(path); parent !== "."; parent = dirname(parent)) need(!names.has(parent));
    }
    return Object.freeze({ version: 1, files: Object.freeze(files) });
  } catch { throw new Error(REFUSED); }
}

async function readPinnedFile(path, max, row, keepBytes = false) {
  const fd = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await fd.stat({ bigint: true });
    need(before.isFile() && before.nlink === 1n && before.size >= 0n && before.size <= BigInt(max)
      && before.uid === 0n
      && Number(before.mode & 0o7777n) === (row?.mode ?? 0o444) && (!row || before.size === BigInt(row.size)));
    const hash = createHash("sha256"), buffer = Buffer.allocUnsafe(64 * 1024), pieces = [];
    let size = 0;
    for (;;) {
      const { bytesRead } = await fd.read(buffer, 0, buffer.length, null);
      if (!bytesRead) break;
      size += bytesRead;
      need(size <= max);
      const bytes = buffer.subarray(0, bytesRead);
      hash.update(bytes);
      if (keepBytes) pieces.push(Buffer.from(bytes));
    }
    const sha256 = hash.digest("hex"), identity = stamp(before);
    need(BigInt(size) === before.size && stamp(await fd.stat({ bigint: true })) === identity
      && stamp(await lstat(path, { bigint: true })) === identity && (!row || row.sha256 === sha256));
    return Object.freeze({ identity, sha256, bytes: keepBytes ? Buffer.concat(pieces) : undefined });
  } finally { await fd.close(); }
}

async function observeTree(root, rows, baseline) {
  need(await realpath(root) === root);
  // Pathname imports are safe only if the unprivileged actor cannot replace
  // files or ancestors (an actor-owned 0444 file can be made writable).
  for (let parent = dirname(root);; parent = dirname(parent)) {
    const info = await lstat(parent, { bigint: true });
    need(info.isDirectory() && info.uid === 0n && (info.mode & 0o022n) === 0n);
    if (parent === "/") break;
  }
  const tree = new Map(), names = new Map(rows.map(row => [row.path, row])), dirs = new Set([""]);
  names.set(INVENTORY, undefined);
  for (const path of names.keys()) {
    for (let parent = dirname(path); parent !== "."; parent = dirname(parent)) dirs.add(parent);
  }
  async function walk(relative) {
    const path = relative ? join(root, relative) : root;
    const directory = await lstat(path, { bigint: true });
    need(directory.isDirectory() && directory.uid === 0n && (directory.mode & 0o7777n) === 0o555n);
    const before = stamp(directory);
    let count = 0;
    for await (const entry of await opendir(path)) {
      count++;
      need(count <= MAX_FILES);
      const child = relative ? `${relative}/${entry.name}` : entry.name;
      if (dirs.has(child)) {
        need(entry.isDirectory());
        await walk(child);
      } else {
        need(entry.isFile() && names.has(child));
        const row = names.get(child);
        const observed = baseline ? stamp(await lstat(join(root, child), { bigint: true }))
          : (await readPinnedFile(join(root, child), row ? MAX_FILE : MAX_INVENTORY, row)).identity;
        tree.set(child, observed);
      }
    }
    need(stamp(await lstat(path, { bigint: true })) === before);
    tree.set(relative, before);
  }
  await walk("");
  need(tree.size === names.size + dirs.size);
  if (baseline) for (const [key, identity] of tree) need(baseline.get(key) === identity);
  return tree;
}

async function bounded(promise, timeout) {
  const timer = new AbortController();
  try {
    return await Promise.race([promise, delay(timeout, undefined, { signal: timer.signal }).then(() => { throw new Error(UNCLOSED); })]);
  } finally { timer.abort(); }
}

export async function createLocalQaLinuxC2cChromium({ root, closureRoot, inventoryFile, inventorySha256, assertOwnedUnit }) {
  try {
    need(process.platform === "linux" && process.arch === "x64" && process.getuid?.() > 0
      && typeof root === "string" && root === process.env.BLUEY_TEST_RUN_ROOT && typeof assertOwnedUnit === "function"
      && typeof closureRoot === "string" && resolve(closureRoot) === closureRoot && closureRoot !== "/");
    const writable = dirname(root);
    need(closureRoot !== writable && !closureRoot.startsWith(`${writable}/`) && !writable.startsWith(`${closureRoot}/`)
      && inventoryFile === join(closureRoot, INVENTORY) && whole(/^[0-9a-f]{64}$/, inventorySha256));
    const launcher = await assertLocalQaLinuxPostgresRunRoot(root);
    const verifyUnit = async () => {
      need(await assertLocalQaLinuxPostgresRunRoot(root) === launcher && await assertOwnedUnit() === true);
    };
    await verifyUnit();
    need(await realpath(closureRoot) === closureRoot && await realpath(inventoryFile) === inventoryFile);
    const seed = await readPinnedFile(inventoryFile, MAX_INVENTORY, undefined, true);
    need(seed.sha256 === inventorySha256);
    const inventory = parseLocalQaLinuxC2cChromiumInventory(seed.bytes);
    const pinned = await observeTree(closureRoot, inventory.files);
    need(pinned.get(INVENTORY) === seed.identity && await realpath(process.execPath) === join(closureRoot, NODE)
      && stamp(await stat("/proc/self/exe", { bigint: true })) === pinned.get(NODE));
    const verify = async () => {
      await verifyUnit();
      await observeTree(closureRoot, inventory.files, pinned);
      await verifyUnit();
    };
    await verify();
    let browser, context, used = false, closing, contextClosed = false;
    const close = () => closing ??= (async () => {
      let uncertain = false;
      try {
        if (context && !contextClosed) await bounded(context.close(), 15_000);
      } catch { uncertain = true; }
      try { if (browser && browser.isConnected()) await bounded(browser.close(), 15_000); }
      catch { uncertain = true; }
      try {
        need(!uncertain);
        need((!context || contextClosed) && (!browser || !browser.isConnected()));
        await verify();
        return true;
      } catch { throw new Error(UNCLOSED); }
    })();
    return Object.freeze({
      async openContext({ origin, serviceWorkers, signal }) {
        try {
          need(!used && !closing && (signal === undefined || signal instanceof AbortSignal)
            && whole(/^http:\/\/127\.0\.0\.1:[1-9][0-9]{3,4}$/, origin) && serviceWorkers === "block");
          const address = new URL(origin);
          need(address.origin === origin && Number(address.port) > 1024 && Number(address.port) !== 5187);
          used = true;
          signal?.throwIfAborted();
          await verify();
          const mod = await import(pathToFileURL(join(closureRoot, DRIVER)).href);
          await verify();
          signal?.throwIfAborted();
          browser = await mod.chromium.launch({ executablePath: join(closureRoot, CHROME), headless: true,
            chromiumSandbox: true, timeout: 30_000,
            env: { HOME: join(root, "tmp"), TMPDIR: join(root, "tmp"), PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", TZ: "UTC" } });
          context = await bounded(browser.newContext({ acceptDownloads: false, serviceWorkers: "block" }), 15_000);
          context.once("close", () => { contextClosed = true; });
          need(browser.isConnected() && context.pages().length === 0);
          await verify();
          signal?.throwIfAborted();
          return context;
        } catch (error) {
          try { if (used) await close(); } catch { throw new Error(UNCLOSED); }
          throw new Error(error?.message === UNCLOSED ? UNCLOSED : REFUSED);
        }
      },
      close,
    });
  } catch (error) { throw new Error(error?.message === UNCLOSED ? UNCLOSED : REFUSED); }
}
