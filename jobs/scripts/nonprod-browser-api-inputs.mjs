// Parent-side readback for the explicit inputs of the private browser test.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, lstatSync, openSync, readSync, readdirSync, realpathSync } from "node:fs";
import { isAbsolute, join } from "node:path";

// The inline Vite build disables vite.config and dotenv discovery, but
// still resolves the portal TypeScript config and copies public assets.
export function browserPortalSourcePaths(jobsRoot) {
  assert(isAbsolute(jobsRoot), "browser Jobs input root must be absolute");
  return [
    "portal/src",
    "portal/index.html",
    "portal/package.json",
    "portal/public",
    "portal/tsconfig.json",
    "package.json",
    "package-lock.json",
  ].map((path) => join(jobsRoot, path));
}

// Watch API sources used by the fixture, not only the fixture entrypoint.
// This detects drift during a run; build provenance must be verified separately.
export function browserApiSourcePaths(repositoryRoot) {
  assert(isAbsolute(repositoryRoot), "browser API source root must be absolute");
  return [
    "Cargo.toml",
    "Cargo.lock",
    "server/Cargo.toml",
    "server/Cargo.lock",
    "server/src",
    "server/examples/jobs-nonprod-browser-fixture.rs",
    "crates/cue-core/Cargo.toml",
    "crates/cue-core/src",
    "infra/postgres/server-runtime",
    "infra/sqlite/server-runtime",
  ].map((path) => join(repositoryRoot, path));
}

function identity(stat) {
  return [stat.dev, stat.ino, stat.mode, stat.nlink, stat.uid, stat.gid, stat.size,
    stat.mtimeNs, stat.ctimeNs].map(String);
}

export function snapshotBrowserInputs(roots, { executables = false } = {}) {
  const entries = new Map();
  const buffer = Buffer.alloc(64 * 1024);
  function visit(path) {
    assert(!entries.has(path), "overlapping browser input paths");
    const stat = lstatSync(path, { bigint: true });
    const id = identity(stat);
    assert(!stat.isSymbolicLink() && realpathSync(path) === path, "noncanonical browser input");
    if (executables) {
      assert(stat.isFile() && stat.uid === BigInt(process.getuid()) &&
        (stat.mode & 0o122n) === 0o100n, "browser artifact must be an owned executable");
    }
    if (stat.isDirectory()) {
      entries.set(path, { kind: "directory", identity: id });
      for (const name of readdirSync(path).sort()) visit(join(path, name));
    } else {
      assert(stat.isFile(), "unsupported browser input type");
      const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
      try {
        assert.deepEqual(identity(fstatSync(fd, { bigint: true })), id, "browser input changed before read");
        const hash = createHash("sha256");
        let remaining = stat.size;
        while (remaining > 0n) {
          const max = remaining > BigInt(buffer.length) ? buffer.length : Number(remaining);
          const read = readSync(fd, buffer, 0, max, null);
          assert(read > 0, "browser input truncated during read");
          hash.update(buffer.subarray(0, read));
          remaining -= BigInt(read);
        }
        assert.deepEqual(identity(fstatSync(fd, { bigint: true })), id, "browser input changed during read");
        entries.set(path, { kind: "file", identity: id, sha256: hash.digest("hex") });
      } finally { closeSync(fd); }
    }
    assert.deepEqual(identity(lstatSync(path, { bigint: true })), id, "browser input path changed during read");
  }
  for (const path of roots) visit(path);
  return entries;
}

export function assertBrowserInputsUnchanged(expected, actual) {
  assert(expected.size === actual.size, "browser input inventory changed");
  for (const [path, entry] of expected) {
    assert(actual.get(path)?.kind === entry.kind, "browser input inventory changed");
  }
  for (const [path, entry] of expected) {
    const observed = actual.get(path);
    assert(observed.sha256 === entry.sha256 && observed.identity.length === entry.identity.length &&
      observed.identity.every((value, i) => value === entry.identity[i]),
      "browser input changed since snapshot");
  }
}
