// Private durable local-QA state, not a production configuration or test cleanup root.
// No reset/delete operation: incomplete or changed state is retained and refused.
import { constants } from "node:fs";
import { lstat, realpath, mkdir, open, rename } from "node:fs/promises";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { dirname, basename, join, resolve, isAbsolute } from "node:path";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import { initializeLocalQaObjectDirectory, LOCAL_QA_OBJECT_MARKER } from "./local-qa-object-store.mjs";

const MARKER = ".bluey-local-qa.json";
const KIND = "bluey_jobs_durable_local_qa";
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SHA = /^[0-9a-f]{64}$/;
const KEYS = Object.freeze({
  jwtSecret: "jwt.key", jobsDataKey: "jobs-data.key",
  objectAccessKey: "object-access.key", objectSecret: "object-secret.key",
});
const DIRECTORIES = ["objects", "postgres", "logs"];
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const json = value => JSON.stringify(value, null, 2) + "\n";
function requireThat(value, code) {
  if (!value) throw new Error(`local_qa_state_${code}`);
}
function uid() {
  requireThat(typeof process.getuid === "function", "platform_unsupported");
  return process.getuid();
}
const dirIdentity = stat => [stat.dev, stat.ino, stat.uid, stat.mode].map(String).join(":");
const fileIdentity = stat => [stat.dev, stat.ino, stat.uid, stat.mode, stat.nlink,
  stat.size, stat.mtimeNs, stat.ctimeNs].map(String).join(":");
async function absent(path) {
  try { await lstat(path); return false; }
  catch (error) { if (error.code === "ENOENT") return true; throw error; }
}
async function privateDirectory(path) {
  requireThat(await realpath(path) === path, "directory_indirection");
  const stat = await lstat(path, { bigint: true });
  requireThat(stat.isDirectory() && stat.uid === BigInt(uid())
    && (stat.mode & 0o7777n) === 0o700n, "directory_permissions");
  return dirIdentity(stat);
}
async function requireDirectory(path, expected) {
  requireThat(await privateDirectory(path) === expected, "directory_changed");
}
async function validateLocation(path) {
  uid();
  requireThat(typeof path === "string" && isAbsolute(path) && resolve(path) === path
    && !/[\0\r\n]/.test(path) && !path.endsWith("/"), "path_invalid");
  requireThat(/^bluey-jobs-qa-[a-z0-9][a-z0-9-]{0,63}$/.test(basename(path)), "name_invalid");
  requireThat(!path.startsWith("/Volumes/") && !path.split("/").includes(".git"), "path_forbidden");
  // An existing private parent is required; never chmod or recursively create
  // owner paths. Refuse worktrees and Git metadata before creating anything.
  const parent = dirname(path);
  const identity = await privateDirectory(parent);
  for (let current = parent;; current = dirname(current)) {
    requireThat(await absent(join(current, ".git")), "git_overlap");
    const bare = !(await absent(join(current, "HEAD")))
      && !(await absent(join(current, "objects"))) && !(await absent(join(current, "refs")));
    requireThat(!bare, "git_overlap");
    if (current === "/") break;
  }
  await requireDirectory(parent, identity);
  return { parent, identity };
}
async function readPrivate(path, maximum) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    requireThat(before.isFile() && before.uid === BigInt(uid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o600n && before.size <= BigInt(maximum), "file_permissions");
    const bytes = Buffer.alloc(maximum + 1);
    let size = 0;
    for (;;) {
      const result = await file.read(bytes, size, bytes.length - size, null);
      size += result.bytesRead;
      requireThat(size <= maximum, "file_size");
      if (!result.bytesRead) break;
    }
    requireThat(BigInt(size) === before.size
      && fileIdentity(await file.stat({ bigint: true })) === fileIdentity(before)
      && fileIdentity(await lstat(path, { bigint: true })) === fileIdentity(before), "file_changed");
    return bytes.subarray(0, size);
  } finally { await file.close(); }
}
async function syncDirectory(path) {
  const file = await open(path, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try { await file.sync(); } finally { await file.close(); }
}
async function writePrivate(path, bytes, root, rootIdentity) {
  await requireDirectory(root, rootIdentity);
  const file = await open(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL
    | constants.O_NOFOLLOW, 0o600);
  try {
    await file.writeFile(bytes);
    await file.sync();
    const metadata = await file.stat({ bigint: true });
    requireThat(metadata.isFile() && metadata.uid === BigInt(uid()) && metadata.nlink === 1n
      && (metadata.mode & 0o7777n) === 0o600n && metadata.size === BigInt(bytes.length)
      && fileIdentity(await lstat(path, { bigint: true })) === fileIdentity(metadata), "file_changed");
  } finally { await file.close(); }
  await requireDirectory(root, rootIdentity);
}
function validateMarker(marker, bytes) {
  const names = ["schemaVersion", "kind", "qaId", "ownerUid", "createdAt", "bucket", "prefix",
    "secretHashes", "rootIdentity", "directoryIdentities", "objectStoreMarkerSha256", "postgresInitialization"];
  requireThat(marker && typeof marker === "object" && !Array.isArray(marker)
    && Object.keys(marker).sort().join("|") === names.sort().join("|"), "marker_shape");
  requireThat(marker.schemaVersion === 1 && marker.kind === KIND && UUID.test(marker.qaId)
    && marker.ownerUid === uid() && typeof marker.createdAt === "string"
    && Number.isFinite(Date.parse(marker.createdAt))
    && new Date(marker.createdAt).toISOString() === marker.createdAt
    && /^qa-[0-9a-f]{32}$/.test(marker.bucket) && /^qa-[0-9a-f]{32}$/.test(marker.prefix), "marker_fields");
  requireThat(marker.secretHashes && Object.keys(marker.secretHashes).sort().join("|")
    === Object.keys(KEYS).sort().join("|")
    && Object.values(marker.secretHashes).every(value => typeof value === "string" && SHA.test(value)), "marker_hashes");
  requireThat(typeof marker.rootIdentity === "string" && marker.directoryIdentities
    && Object.keys(marker.directoryIdentities).sort().join("|") === [...DIRECTORIES].sort().join("|")
    && Object.values(marker.directoryIdentities).every(value => typeof value === "string"), "marker_directories");
  requireThat(typeof marker.objectStoreMarkerSha256 === "string"
    && SHA.test(marker.objectStoreMarkerSha256), "marker_object_store");
  requireThat(["unclaimed", "claimed"].includes(marker.postgresInitialization), "marker_postgres");
  requireThat(Buffer.from(json(marker)).equals(bytes), "marker_encoding");
}
function validateSecret(name, bytes) {
  const text = bytes.toString("utf8");
  requireThat(text.endsWith("\n") && !text.slice(0, -1).includes("\n"), "secret_encoding");
  const value = text.slice(0, -1);
  const valid = name === "jobsDataKey"
    ? /^[A-Za-z0-9+/]{43}=$/.test(value) && Buffer.from(value, "base64").length === 32
      && Buffer.from(value, "base64").toString("base64") === value
    : name === "objectAccessKey" ? /^qa[0-9a-f]{32}$/.test(value) : SHA.test(value);
  requireThat(valid, "secret_encoding");
  return value;
}
async function readState(path, includeSecrets) {
  const parent = await validateLocation(path);
  const rootIdentity = await privateDirectory(path);
  const markerBytes = await readPrivate(join(path, MARKER), 4096);
  let marker;
  try { marker = JSON.parse(markerBytes.toString("utf8")); }
  catch { throw new Error("local_qa_state_marker_encoding"); }
  validateMarker(marker, markerBytes);
  requireThat(marker.rootIdentity === rootIdentity, "directory_changed");
  const values = {};
  for (const [name, filename] of Object.entries(KEYS)) {
    const bytes = await readPrivate(join(path, filename), 128);
    requireThat(digest(bytes) === marker.secretHashes[name], "secret_changed");
    values[name] = validateSecret(name, bytes);
    bytes.fill(0);
  }
  for (const directory of DIRECTORIES) {
    requireThat(await privateDirectory(join(path, directory)) === marker.directoryIdentities[directory],
      "directory_changed");
  }
  const objectMarker = await readPrivate(join(path, "objects", LOCAL_QA_OBJECT_MARKER), 4096);
  requireThat(digest(objectMarker) === marker.objectStoreMarkerSha256, "object_store_changed");
  await requireDirectory(path, rootIdentity);
  await requireDirectory(parent.parent, parent.identity);
  const description = Object.freeze({ schemaVersion: 1, kind: KIND, qaId: marker.qaId,
    createdAt: marker.createdAt, stateDirectory: path, bucket: marker.bucket, prefix: marker.prefix,
    objectDirectory: join(path, "objects"), postgresDirectory: join(path, "postgres"),
    logDirectory: join(path, "logs"), postgresInitialization: marker.postgresInitialization });
  if (!includeSecrets) return description;
  const secrets = {};
  for (const [name, value] of Object.entries(values)) {
    Object.defineProperty(secrets, name, { value, enumerable: false });
  }
  Object.defineProperty(secrets, "toJSON", { value: () => "[Local QA secrets redacted]" });
  Object.defineProperty(secrets, inspect.custom, { value: () => "[Local QA secrets redacted]" });
  return Object.freeze(secrets);
}
export async function inspectLocalQaState(path) { return readState(path, false); }
export async function loadLocalQaSecrets(path) { return readState(path, true); }
// One-way, durable intent BEFORE initdb or database credentials are created.
// Loss of every PostgreSQL child artifact cannot make an established parent
// look like first use. Caller holds its exclusive PG lifecycle lock.
export async function claimLocalQaPostgresInitialization(path) {
  const state = await inspectLocalQaState(path);
  requireThat(state.postgresInitialization === "unclaimed", "postgres_already_claimed");
  const rootIdentity = await privateDirectory(path), markerPath = join(path, MARKER);
  const original = await readPrivate(markerPath, 4096);
  const markerIdentity = fileIdentity(await lstat(markerPath, { bigint: true }));
  const marker = JSON.parse(original.toString("utf8")); validateMarker(marker, original);
  requireThat(marker.postgresInitialization === "unclaimed", "postgres_already_claimed");
  marker.postgresInitialization = "claimed";
  const bytes = Buffer.from(json(marker)), pending = join(path, ".postgres-claim.pending");
  await writePrivate(pending, bytes, path, rootIdentity);
  const pendingIdentity = fileIdentity(await lstat(pending, { bigint: true }));
  requireThat((await readPrivate(markerPath, 4096)).equals(original)
    && fileIdentity(await lstat(markerPath, { bigint: true })) === markerIdentity
    && (await readPrivate(pending, 4096)).equals(bytes)
    && fileIdentity(await lstat(pending, { bigint: true })) === pendingIdentity, "marker_changed");
  await requireDirectory(path, rootIdentity);
  await rename(pending, markerPath); await syncDirectory(path);
  requireThat((await readPrivate(markerPath, 4096)).equals(bytes), "marker_changed");
  return inspectLocalQaState(path);
}
export async function initializeLocalQaState(path) {
  const parent = await validateLocation(path);
  try { await mkdir(path, { mode: 0o700 }); }
  catch (error) {
    if (error.code === "EEXIST") return inspectLocalQaState(path);
    throw error;
  }
  // Commit marker LAST. Partial initialization is never silently repaired or
  // deleted. The private-parent/current-user boundary is not hostile-owner race proof.
  const rootIdentity = await privateDirectory(path);
  await requireDirectory(parent.parent, parent.identity);
  await syncDirectory(parent.parent);
  const qaId = randomUUID();
  const secretValues = { jwtSecret: randomBytes(32).toString("hex"),
    jobsDataKey: randomBytes(32).toString("base64"),
    objectAccessKey: "qa" + randomBytes(16).toString("hex"), objectSecret: randomBytes(32).toString("hex") };
  const secretHashes = {};
  for (const [name, filename] of Object.entries(KEYS)) {
    const bytes = Buffer.from(secretValues[name] + "\n");
    secretHashes[name] = digest(bytes);
    await writePrivate(join(path, filename), bytes, path, rootIdentity);
    bytes.fill(0);
  }
  const directoryIdentities = {};
  for (const directory of DIRECTORIES) {
    await requireDirectory(path, rootIdentity);
    await mkdir(join(path, directory), { mode: 0o700 });
    directoryIdentities[directory] = await privateDirectory(join(path, directory));
  }
  await syncDirectory(path);
  const marker = { schemaVersion: 1, kind: KIND, qaId, ownerUid: uid(), createdAt: new Date().toISOString(),
    bucket: "qa-" + qaId.replaceAll("-", ""), prefix: "qa-" + randomBytes(16).toString("hex"),
    secretHashes, rootIdentity, directoryIdentities };
  const objectStore = initializeLocalQaObjectDirectory({ directory: join(path, "objects"), qaId,
    bucket: marker.bucket, prefix: marker.prefix,
    accessKey: secretValues.objectAccessKey, secret: secretValues.objectSecret });
  marker.objectStoreMarkerSha256 = objectStore.markerSha256;
  marker.postgresInitialization = "unclaimed";
  await writePrivate(join(path, MARKER), Buffer.from(json(marker)), path, rootIdentity);
  await syncDirectory(path);
  await syncDirectory(parent.parent);
  return inspectLocalQaState(path);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [action, flag, path, ...extra] = process.argv.slice(2);
  try {
    requireThat(["init", "status"].includes(action) && flag === "--state-dir" && path && !extra.length, "arguments");
    const result = action === "init" ? await initializeLocalQaState(path) : await inspectLocalQaState(path);
    process.stdout.write(json(result));
  } catch (error) {
    const code = /^local_qa_state_[a-z_]+$/.test(error.message) ? error.message : "local_qa_state_refused";
    process.stderr.write(code + "\n"); process.exitCode = 1;
  }
}
