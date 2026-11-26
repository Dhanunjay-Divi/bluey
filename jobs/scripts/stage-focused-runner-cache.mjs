import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, lstatSync, mkdirSync, openSync,
  readSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const MANIFEST = "480a336791ebe6dd4ad38ebbfd7d59c5c5a990e7b00821b0862a664821839462";
const MANIFEST_BYTES = 1_050_449;
const TREE = "a94366a302e199786c44f62bbb65916578d805f2d4f36b6de56c448fadef717f";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const identity = (stat) => Object.fromEntries(
  ["dev", "ino", "uid", "mode", "nlink", "size", "mtimeNs", "ctimeNs"].map((key) => [key, String(stat[key])]),
);
const stat = (path) => lstatSync(path, { bigint: true });

function privateDirectory(path) {
  assert.equal(realpathSync(path), path, "runner_cache_directory_path");
  const observed = stat(path);
  assert(observed.isDirectory() && observed.uid === BigInt(process.getuid()) && (observed.mode & 0o7777n) === 0o700n,
    "runner_cache_directory_mode");
  return observed;
}

function readCacheFile(path, limit, mode, expectedIdentity) {
  assert.equal(realpathSync(path), path, "runner_cache_file_path");
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = fstatSync(fd, { bigint: true });
    assert(before.isFile() && before.nlink === 1n && before.uid === BigInt(process.getuid())
      && (before.mode & 0o7777n) === mode && before.size <= BigInt(limit), "runner_cache_file_custody");
    if (expectedIdentity) assert.deepEqual(identity(before), expectedIdentity, "runner_cache_source_identity");
    const bytes = Buffer.alloc(Number(before.size));
    for (let pos = 0; pos < bytes.length;) {
      const count = readSync(fd, bytes, pos, bytes.length - pos, pos);
      assert(count > 0, "runner_cache_short_read");
      pos += count;
    }
    assert.deepEqual(identity(fstatSync(fd, { bigint: true })), identity(before));
    assert.deepEqual(identity(stat(path)), identity(before));
    return bytes;
  } finally {
    closeSync(fd);
  }
}

// Source-only operation. The caller owns scheduler admission for installation,
// fencing, and exact-root cleanup. The retained cache is never a write target.
export function stageFocusedRunnerCache(resource) {
  const root = process.env.BLUEY_TEST_RUN_ROOT;
  assert(typeof root === "string"
    && /^\/private\/tmp\/bluey-test-runs-[A-Za-z0-9_-]+\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/.test(root),
  "runner_cache_root");
  privateDirectory(dirname(root));
  privateDirectory(root);
  assert(!resource.startsWith(root + "/"), "runner_cache_retained_source");
  privateDirectory(resource);
  const manifestPath = join(resource, "CACHE-MANIFEST.json");
  const manifest = readCacheFile(manifestPath, MANIFEST_BYTES, 0o400n);
  assert.equal(sha(manifest), MANIFEST, "runner_cache_manifest_pin");
  const exported = JSON.parse(manifest).exportedCacheTree;
  assert.equal(sha(JSON.stringify(exported.rows)), TREE, "runner_cache_tree_pin");
  assert.equal(exported.rows.length, 1752);
  const source = join(resource, "cache");
  const target = join(root, "runner-gate-cache");
  const rows = new Map(exported.rows.map((row) => [row.path, row]));
  assert.equal(rows.size, 1752);
  for (const row of rows.values()) {
    assert(row.path === "" || row.path.split("/").every((part) =>
      /^[A-Za-z0-9_.-]+$/.test(part) && part !== "." && part !== ".."), "runner_cache_relative_path");
  }
  let count = 0;
  let total = 0;
  function visit(relative, copy) {
    const row = rows.get(relative);
    assert(row, "runner_cache_unexpected_entry");
    count += 1;
    const from = join(copy ? source : target, relative);
    const to = join(target, relative);
    if (row.type === "dir") {
      const before = privateDirectory(from);
      if (copy) {
        assert.deepEqual(identity(before), row.identity, "runner_cache_directory_identity");
        mkdirSync(to, { mode: 0o700 });
      }
      for (const child of readdirSync(from)) visit(relative ? relative + "/" + child : child, copy);
      assert.deepEqual(identity(privateDirectory(from)), identity(before));
    } else {
      assert.equal(row.type, "file");
      const bytes = readCacheFile(from, 64 * 1024 * 1024, copy ? 0o400n : 0o600n, copy ? row.identity : undefined);
      assert.equal(bytes.length, row.bytes);
      assert.equal(sha(bytes), row.sha256, "runner_cache_content_pin");
      total += bytes.length;
      if (copy) writeFileSync(to, bytes, { flag: "wx", mode: 0o600 });
    }
  }
  for (const copy of [true, false]) {
    count = 0;
    total = 0;
    visit("", copy);
    assert.equal(count, rows.size);
    assert.equal(total, 148354670);
  }
  for (const row of rows.values()) assert.deepEqual(identity(stat(join(source, row.path))), row.identity);
  assert.equal(sha(readCacheFile(manifestPath, MANIFEST_BYTES, 0o400n)), MANIFEST);
  return { target, entries: count, bytes: total, retainedTreeSha256: TREE };
}
