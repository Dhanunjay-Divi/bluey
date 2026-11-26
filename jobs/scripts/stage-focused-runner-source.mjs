import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { closeSync, constants, fchmodSync, lstatSync, mkdirSync, openSync,
  readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { readBoundFile } from "./managed-receipt-source-preflight.mjs";

const HEAD = "e1108052e60f19fbd5ef5af534190c52978eb51f";
const SCOPE = ["jobs", ".github/workflows/jobs-ci.yml", ".github/workflows/release.yml",
  "ops/bluey-jobs-runner.env.example"];
const sha = (value) => createHash("sha256").update(value).digest("hex");

function privateDirectory(path) {
  const metadata = lstatSync(path);
  assert.equal(realpathSync(path), path, "runner_stage_directory_path");
  assert(metadata.isDirectory() && metadata.uid === process.getuid()
    && (metadata.mode & 0o7777) === 0o700, "runner_stage_directory_mode");
}

// This only copies a pinned source projection. The outer launcher owns
// scheduler admission, tool provenance and removal of its exact run root.
export function stageFocusedRunnerSource({ checkout, expectedSha256 }) {
  assert.match(expectedSha256, /^[0-9a-f]{64}$/);
  assert.equal(realpathSync(checkout), checkout, "runner_stage_checkout_path");
  const root = process.env.BLUEY_TEST_RUN_ROOT;
  assert(typeof root === "string"
    && /^\/private\/tmp\/bluey-test-runs-[A-Za-z0-9_-]+\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/.test(root),
    "runner_stage_owned_root");
  privateDirectory(dirname(root));
  privateDirectory(root);
  const git = (...args) => execFileSync("/Library/Developer/CommandLineTools/usr/bin/git", [
    "--no-optional-locks", "-c", "core.fsmonitor=false", "-c", "core.excludesFile=/dev/null", ...args,
  ], {
    cwd: checkout, encoding: "utf8", timeout: 10000, maxBuffer: 1024 * 1024,
    env: { PATH: "/usr/bin:/bin", LC_ALL: "C", GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1" },
  });
  const list = () => git("ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", ...SCOPE)
    .split("\0").filter(Boolean).sort();
  const checkGit = () => {
    assert.equal(git("rev-parse", "--show-toplevel"), `${checkout}\n`);
    assert.equal(git("rev-parse", "HEAD"), `${HEAD}\n`);
    git("diff", "--cached", "--quiet");
  };
  checkGit();
  const names = list();
  assert(names.length > 0 && names.length <= 2048 && new Set(names).size === names.length,
    "runner_stage_inventory");
  assert(names.every((name) => name.startsWith("jobs/") || SCOPE.includes(name)), "runner_stage_scope");
  assert(!names.some((name) => name.split("/").some((part) => ["node_modules", "dist", ".env"].includes(part))),
    "runner_stage_generated_input");
  const observe = (base, path) => {
    const file = readBoundFile(base, path);
    return { file, row: [path, file.sha256, file.mode, file.bytes.length] };
  };
  let total = 0;
  const rows = names.map((name) => {
    const { row } = observe(checkout, name);
    total += row[3];
    assert(total <= 64 * 1024 * 1024, "runner_stage_source_bound");
    return row;
  });
  assert.equal(sha(JSON.stringify(rows)), expectedSha256, "runner_stage_source_pin");
  const target = join(root, "runner-gate-source");
  mkdirSync(target, { mode: 0o700 });
  privateDirectory(target);
  for (const expected of rows) {
    const [relative] = expected;
    const { file, row } = observe(checkout, relative);
    assert.deepEqual(row, expected, "runner_stage_source_drift");
    const output = join(target, relative);
    mkdirSync(dirname(output), { recursive: true, mode: 0o700 });
    privateDirectory(dirname(output));
    const fd = openSync(output, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
    try {
      writeFileSync(fd, file.bytes);
      fchmodSync(fd, file.mode);
    } finally {
      closeSync(fd);
    }
  }
  const seen = [];
  const expectedFiles = new Set(names);
  const expectedDirs = new Set(rows.flatMap(([path]) => {
    const parts = path.split("/");
    return parts.slice(1).map((_, i) => parts.slice(0, i + 1).join("/"));
  }));
  function walk(dir) {
    privateDirectory(join(target, dir));
    for (const entry of readdirSync(join(target, dir), { withFileTypes: true })) {
      const relative = dir ? `${dir}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        assert(expectedDirs.has(relative), "runner_stage_unexpected_directory");
        walk(relative);
      } else {
        assert(entry.isFile() && expectedFiles.has(relative), "runner_stage_unexpected_entry");
        seen.push(observe(target, relative).row);
      }
    }
  }
  walk("");
  seen.sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
  assert.deepEqual(seen, rows, "runner_stage_readback_drift");
  for (const expected of rows) {
    assert.deepEqual(observe(checkout, expected[0]).row, expected, "runner_stage_source_drift");
  }
  assert.deepEqual(list(), names, "runner_stage_inventory_drift");
  checkGit();
  return { target, files: rows.length, sourceSha256: expectedSha256 };
}
