import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { closeSync, constants, fchmodSync, lstatSync, mkdirSync, openSync,
  readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import {
  preflightManagedReceiptSource, readBoundFile, renderBoundReceiptWrapper,
} from "./managed-receipt-source-preflight.mjs";

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const WRAPPER = "jobs/scripts/run-nonprod-postgres-tests.mjs";
const STAGED = "managed-receipt-source";
const RUN_ROOT = /^\/(?:private\/)?tmp\/bluey-test-runs-[A-Za-z0-9_-]+\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/;

function privateDirectory(path) {
  assert.equal(realpathSync(path), path, "staging directory must be physical");
  const stat = lstatSync(path);
  assert(stat.isDirectory() && stat.uid === process.getuid() && (stat.mode & 0o7777) === 0o700,
    "staging directory must be private and owned");
}

function observe(root, relative) {
  const file = readBoundFile(root, relative);
  return { file, row: [relative, file.sha256, file.mode, file.bytes.length] };
}

// Stages source only. The caller must separately bind the launcher, manifest
// pin and this module before evaluation and own the canonical root's cleanup.
// A partial tree on failure is never accepted or reused.
export function stageManagedReceiptSource({ checkout, manifestBytes, expectedSha256, profile = "baseline" }) {
  assert(Buffer.isBuffer(manifestBytes) && manifestBytes.length <= 4 * 1024 * 1024, "manifest byte bound");
  assert.match(expectedSha256, /^[a-f0-9]{64}$/);
  assert.equal(sha(manifestBytes), expectedSha256, "manifest pin mismatch");
  const manifestText = manifestBytes.toString("utf8");
  const accepted = JSON.parse(manifestText);
  assert.equal(manifestText, JSON.stringify(accepted) + "\n", "noncanonical manifest");
  assert.equal(accepted.profile, profile === "baseline" ? undefined : profile, "receipt manifest profile mismatch");
  const canonical = () => JSON.stringify(preflightManagedReceiptSource(checkout, profile)) + "\n";
  assert.equal(sha(canonical()), expectedSha256, "source no longer matches manifest");

  const root = process.env.BLUEY_TEST_RUN_ROOT;
  assert(typeof root === "string" && RUN_ROOT.test(root), "canonical launcher root required");
  privateDirectory(dirname(root));
  privateDirectory(root);
  const target = join(root, STAGED);
  assert.equal(accepted.stagedWrapper[0], WRAPPER, "unexpected wrapper overlay");
  const expected = accepted.rows.map((row) => row[0] === WRAPPER ? accepted.stagedWrapper : row);
  assert.equal(sha(JSON.stringify(expected)), accepted.stagedSourceSha256, "staged projection mismatch");
  mkdirSync(target, { mode: 0o700 });
  privateDirectory(target);
  const directories = new Set([""]);
  for (const [i, source] of accepted.rows.entries()) {
    const relative = source[0];
    const { file, row } = observe(checkout, relative);
    assert.deepEqual(row, source, "source changed before copy");
    const bytes = relative === WRAPPER ? renderBoundReceiptWrapper(file.bytes, profile) : file.bytes;
    const output = expected[i];
    assert.deepEqual([relative, sha(bytes), file.mode, bytes.length], output, "staged bytes disagree");
    const parts = relative.split("/");
    for (let j = 1; j < parts.length; j += 1) {
      const dir = parts.slice(0, j).join("/");
      if (!directories.has(dir)) {
        mkdirSync(join(target, dir), { mode: 0o700 });
        directories.add(dir);
      }
      privateDirectory(join(target, dir));
    }
    const fd = openSync(join(target, relative),
      constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
    try {
      writeFileSync(fd, bytes);
      fchmodSync(fd, output[2]);
    } finally {
      closeSync(fd);
    }
  }
  const stagedRows = [];
  const files = new Set(expected.map(([name]) => name));
  const walk = (dir) => {
    privateDirectory(join(target, dir));
    for (const entry of readdirSync(join(target, dir), { withFileTypes: true })) {
      const relative = dir ? `${dir}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        assert(directories.has(relative), "unexpected staged directory");
        walk(relative);
      } else {
        assert(entry.isFile() && files.has(relative), "unexpected staged entry");
        stagedRows.push(observe(target, relative).row);
      }
    }
  };
  walk("");
  stagedRows.sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
  assert.deepEqual(stagedRows, expected, "staged tree readback mismatch");
  assert.equal(sha(canonical()), expectedSha256, "source changed during staging");
  return { ...(profile !== "baseline" && { profile }), target,
    sourceRows: stagedRows.length, stagedSourceSha256: accepted.stagedSourceSha256,
    manifestSha256: expectedSha256, runtimeAdmitted: false };
}
