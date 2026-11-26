// Private, candidate-only local-QA login handoff. This module is inert: it has
// no CLI, opens no listener and never reads the local-QA session configuration.
import { randomUUID } from "node:crypto";
import { constants } from "node:fs";
import { link, lstat, open, realpath, unlink } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";

const FILE_NAME = "candidate-login.json";
const KIND = "bluey_jobs_local_qa_candidate_login";
const MAX_LIFETIME_MS = 2 * 60 * 60 * 1000;
const OPTION_KEYS = ["directory", "email", "expiresAtMs", "origin", "password"];
const publications = new WeakMap();

function refuse() {
  throw new Error("local_qa_candidate_handoff_refused");
}

function need(value) {
  if (!value) refuse();
}

function directoryIdentity(value) {
  return [value.dev, value.ino, value.uid, value.mode].map(String).join(":");
}

function fileIdentity(value) {
  return [fileContentsIdentity(value), value.nlink, value.ctimeNs].map(String).join(":");
}

// Linking and unlinking an alias intentionally changes nlink and ctime, not bytes.
function fileContentsIdentity(value) {
  return [value.dev, value.ino, value.uid, value.mode, value.size, value.mtimeNs]
    .map(String).join(":");
}

async function removeBoundPublication(path, directory, directoryBefore, created,
  { exact = false, links = 1n } = {}) {
  const sameDirectory = async () => need(
    directoryIdentity(await privateDirectory(directory)) === directoryIdentity(directoryBefore));
  await sameDirectory();
  const current = await lstat(path, { bigint: true });
  need(created && current.isFile() && current.nlink === links
    && current.dev === created.dev && current.ino === created.ino
    && current.uid === BigInt(process.getuid()) && current.mode === created.mode
    && (!exact || fileIdentity(current) === fileIdentity(created)));
  await unlink(path);
  await syncDirectory(directory);
  await sameDirectory();
  try { await lstat(path); } catch (error) { if (error?.code === "ENOENT") return; throw error; }
  refuse();
}

async function verifiedAlias(path, staged, bytes, links) {
  const before = await lstat(path, { bigint: true });
  need(before.isFile() && before.nlink === links
    && fileContentsIdentity(before) === fileContentsIdentity(staged));
  const reader = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  const readback = Buffer.alloc(bytes.length + 1);
  try {
    need(fileIdentity(await reader.stat({ bigint: true })) === fileIdentity(before));
    let size = 0;
    for (;;) {
      const result = await reader.read(readback, size, readback.length - size, null);
      size += result.bytesRead;
      need(size <= bytes.length);
      if (!result.bytesRead) break;
    }
    need(size === bytes.length && readback.subarray(0, size).equals(bytes)
      && fileIdentity(await reader.stat({ bigint: true })) === fileIdentity(before)
      && fileIdentity(await lstat(path, { bigint: true })) === fileIdentity(before));
    return before;
  } finally {
    readback.fill(0);
    await reader.close();
  }
}

function validateOrigin(value) {
  need(typeof value === "string" && value.length <= 64 && !/[\0\r\n]/.test(value));
  let parsed;
  try { parsed = new URL(value); } catch { refuse(); }
  need(parsed.protocol === "http:" && parsed.hostname === "127.0.0.1"
    && /^[1-9][0-9]{3,4}$/.test(parsed.port)
    && Number(parsed.port) > 1024 && Number(parsed.port) <= 65535
    && parsed.username === "" && parsed.password === "" && parsed.pathname === "/"
    && parsed.search === "" && parsed.hash === "" && parsed.origin === value);
}

async function privateDirectory(path) {
  need(typeof path === "string" && isAbsolute(path) && resolve(path) === path
    && !/[\0\r\n]/.test(path) && !path.endsWith("/") && await realpath(path) === path);
  const value = await lstat(path, { bigint: true });
  need(value.isDirectory() && value.uid === BigInt(process.getuid())
    && (value.mode & 0o7777n) === 0o700n);
  return value;
}

async function syncDirectory(path) {
  const handle = await open(path,
    constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try { await handle.sync(); } finally { await handle.close(); }
}

function validateOptions(options) {
  need(options && typeof options === "object" && !Array.isArray(options)
    && Object.getPrototypeOf(options) === Object.prototype
    && Object.keys(options).sort().join(",") === OPTION_KEYS.join(","));
  const { directory, origin, email, password, expiresAtMs } = options;
  validateOrigin(origin);
  need(email === "candidate@bluey-qa.test" && typeof password === "string"
    && /^Qa-[0-9a-f]{48}$/.test(password));
  const issuedAtMs = Date.now();
  need(Number.isSafeInteger(expiresAtMs) && expiresAtMs > issuedAtMs
    && expiresAtMs - issuedAtMs <= MAX_LIFETIME_MS);
  return { directory, origin, email, password, issuedAtMs, expiresAtMs };
}

export async function writeLocalQaCandidateHandoff(options) {
  const { directory, origin, email, password, issuedAtMs, expiresAtMs } = validateOptions(options);
  const directoryBefore = await privateDirectory(directory);
  const path = join(directory, FILE_NAME);
  const stagedPath = join(directory, `.candidate-login-${randomUUID()}.tmp`);
  const sameDirectory = async () => need(
    directoryIdentity(await privateDirectory(directory)) === directoryIdentity(directoryBefore));
  const value = { version: 1, kind: KIND, origin, email, password, issuedAtMs,
    expiresAtMs };
  const bytes = Buffer.from(`${JSON.stringify(value)}\n`);
  need(bytes.length <= 2048);

  let handle, created, staged, ownsStage = false, ownsFinal = false;
  try {
    handle = await open(stagedPath, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL
      | constants.O_NOFOLLOW, 0o600);
    ownsStage = true;
    created = await handle.stat({ bigint: true });
    need(created.isFile() && created.uid === BigInt(process.getuid()) && created.nlink === 1n
      && (created.mode & 0o7777n) === 0o600n && created.size === 0n);
    await handle.writeFile(bytes);
    await handle.sync();
    staged = await handle.stat({ bigint: true });
    need(staged.isFile() && staged.dev === created.dev && staged.ino === created.ino
      && staged.uid === created.uid && staged.mode === created.mode && staged.nlink === 1n
      && staged.size === BigInt(bytes.length));
    await handle.close();
    handle = undefined;

    const checkedStage = await verifiedAlias(stagedPath, staged, bytes, 1n);
    await sameDirectory();
    need(fileIdentity(checkedStage) === fileIdentity(staged)
      && fileIdentity(await lstat(stagedPath, { bigint: true })) === fileIdentity(checkedStage)
      && Date.now() < expiresAtMs);
    // Node rename would replace an existing handoff. A same-directory link
    // makes only already verified bytes visible and never replaces a name.
    await link(stagedPath, path);
    ownsFinal = true;
    await sameDirectory();
    const linked = await lstat(path, { bigint: true });
    need(linked.nlink === 2n && fileContentsIdentity(linked) === fileContentsIdentity(staged)
      && fileIdentity(await lstat(stagedPath, { bigint: true })) === fileIdentity(linked));
    await unlink(stagedPath);
    ownsStage = false;

    await syncDirectory(directory);
    await sameDirectory();
    const written = await verifiedAlias(path, staged, bytes, 1n);
    await sameDirectory();
    need(fileIdentity(await lstat(path, { bigint: true })) === fileIdentity(written)
      && Date.now() < expiresAtMs);
    const published = Object.freeze({ path, expiresAtMs });
    publications.set(published, { directory, directoryBefore, written });
    return published;
  } catch {
    try {
      if (ownsFinal) {
        await sameDirectory();
        const links = ownsStage ? 2n : 1n;
        const current = await verifiedAlias(path, staged, bytes, links);
        await removeBoundPublication(path, directory, directoryBefore, current, { exact: true, links });
      }
      if (ownsStage) {
        await sameDirectory();
        const current = staged ? await verifiedAlias(stagedPath, staged, bytes, 1n) : created;
        await removeBoundPublication(stagedPath, directory, directoryBefore, current, { exact: Boolean(staged) });
      }
    } catch { throw new Error("local_qa_candidate_handoff_cleanup_unproven"); }
    refuse();
  } finally {
    if (handle) await handle.close().catch(() => {});
    bytes.fill(0);
  }
}

// Only the process which successfully published this exact file can revoke it.
// The directory is separately owned/canonical under the local QA launcher.
export async function revokeLocalQaCandidateHandoff(publication) {
  const binding = publications.get(publication);
  need(binding);
  publications.delete(publication);
  const { directory, directoryBefore, written } = binding;
  const { path } = publication;
  try {
    await removeBoundPublication(path, directory, directoryBefore, written, { exact: true });
  } catch {
    throw new Error("local_qa_candidate_handoff_cleanup_unproven");
  }
}

// Session-owned lifecycle gate around the exact, capability-bound writer
// above. It adds no path or credential authority and deliberately has one
// publication attempt: a caller must create a fresh owner after uncertainty.
export function createLocalQaCandidateHandoffOwner(input) {
  need(input && Object.getPrototypeOf(input) === Object.prototype
    && Object.keys(input).sort().join(",") === "isActive,prepare"
    && typeof input.isActive === "function" && typeof input.prepare === "function");
  const { isActive, prepare } = input;
  let revoked = false, publication, pending, cleanupPromise, revokePromise;

  function currentlyActive() {
    try { return isActive() === true; } catch { return false; }
  }

  function cleanupOwned(value) {
    if (cleanupPromise !== undefined) return cleanupPromise;
    cleanupPromise = revokeLocalQaCandidateHandoff(value);
    return cleanupPromise;
  }

  async function publish() {
    need(!pending && !revoked);
    // Install the one-attempt promise before calling even the trusted hooks.
    // Retain its terminal outcome so late teardown cannot miss an ambiguous write.
    pending = Promise.resolve().then(async () => {
      need(!revoked && currentlyActive());
      let options;
      try { options = await prepare(); } catch (error) {
        if (error?.message === "local_qa_candidate_handoff_cleanup_unproven") throw error;
        refuse();
      }
      need(!revoked && currentlyActive());
      publication = await writeLocalQaCandidateHandoff(options);
      if (revoked || !currentlyActive()) {
        await cleanupOwned(publication);
        refuse();
      }
      return Object.freeze({ path: publication.path, expiresAtMs: publication.expiresAtMs });
    });
    return pending;
  }

  function revoke() {
    if (revokePromise !== undefined) return revokePromise;
    // Disarm before waiting so an in-flight publication observes revocation.
    revoked = true;
    revokePromise = (async () => {
      try { await pending; } catch (error) {
        if (error?.message === "local_qa_candidate_handoff_cleanup_unproven") throw error;
      }
      if (publication !== undefined) await cleanupOwned(publication);
    })();
    return revokePromise;
  }

  return Object.freeze({ publish, revoke });
}
