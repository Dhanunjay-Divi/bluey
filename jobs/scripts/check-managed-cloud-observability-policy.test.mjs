import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { verifyManagedCloudObservabilityPolicy } from "./check-managed-cloud-observability-policy.mjs";

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const workflow = ".github/workflows/observability-policy.yml";
const analyzer = "scripts/analyze-tracing-calls.py";
const yaml = Buffer.from("name: Observability Policy\n");
const py = Buffer.from("# reviewed fixture policy checker\n");
const fixtureFiles = [
  { path: workflow, mode: "100644", sha256: sha(yaml) },
  { path: analyzer, mode: "100755", sha256: sha(py) },
];
const config = {
  repo: "/owned/repository",
  sourceCommit: "1".repeat(40),
  approvedPolicySha256: sha(JSON.stringify({
    domain: "bluey-jobs-managed-cloud-observability-policy-v1",
    files: fixtureFiles,
  })),
};

const yamlObject = "2".repeat(40);
const pyObject = "3".repeat(40);
const yamlRecord = "100644 blob " + yamlObject + "\t" + workflow + "\0";
const pyRecord = "100755 blob " + pyObject + "\t" + analyzer + "\0";
function git({
  type = Buffer.from("commit\n"),
  tree = Buffer.from(yamlRecord + pyRecord),
  workflowBytes = yaml,
  analyzerBytes = py,
} = {}) {
  return (repo, args) => {
    assert.equal(repo, config.repo);
    if (args[0] === "ls-tree") {
      assert.deepEqual(args, ["ls-tree", "-z", config.sourceCommit, "--", workflow, analyzer]);
      return tree;
    }
    if (args[1] === "-t") {
      assert.deepEqual(args, ["cat-file", "-t", config.sourceCommit]);
      return type;
    }
    assert.deepEqual(args.slice(0, 2), ["cat-file", "blob"]);
    assert.equal(args.length, 3);
    if (args[2] === yamlObject) return workflowBytes;
    assert.equal(args[2], pyObject);
    return analyzerBytes;
  };
}

test("accepts only both committed blobs whose canonical policy digest is approved", () => {
  // Physical git listing order is not the policy order.
  const read = git({ tree: Buffer.from(pyRecord + yamlRecord) });
  assert.deepEqual(verifyManagedCloudObservabilityPolicy(config, read), {
    sourceCommit: config.sourceCommit,
    policySha256: config.approvedPolicySha256,
    files: fixtureFiles,
  });
});

test("rejects missing external approval or a symbolic revision before reading Git", () => {
  const unexpected = () => assert.fail("must not read Git");
  for (const approvedPolicySha256 of ["", null, "HEAD", "A".repeat(64)]) {
    assert.throws(() => verifyManagedCloudObservabilityPolicy({ ...config, approvedPolicySha256 }, unexpected),
      /independently approved Observability Policy SHA-256 is required/);
  }
  for (const sourceCommit of ["main", "HEAD", "1".repeat(39), config.sourceCommit + "^{tree}"]) {
    assert.throws(() => verifyManagedCloudObservabilityPolicy({ ...config, sourceCommit }, unexpected),
      /exact commit/);
  }
});

test("rejects a different policy script or workflow even when exact-commit metadata matches", () => {
  for (const diff of [
    { workflowBytes: Buffer.from("name: Observability Policy\n# different committed workflow\n") },
    { analyzerBytes: Buffer.from("# different committed analyzer\n") },
  ]) {
    assert.throws(() => verifyManagedCloudObservabilityPolicy(config, git(diff)),
      /does not match independently approved bytes/);
  }
  assert.throws(() => verifyManagedCloudObservabilityPolicy({ ...config, approvedPolicySha256: "0".repeat(64) }, git()),
    /does not match independently approved bytes/);
});

test("rejects non-commit, missing, duplicate, malformed, and wrong-mode tree entries", () => {
  assert.throws(() => verifyManagedCloudObservabilityPolicy(config, git({ type: Buffer.from("tag\n") })),
    /source object is not a commit/);
  for (const tree of [
    "",
    yamlRecord,
    yamlRecord + yamlRecord,
    yamlRecord + pyRecord.replace("100755", "120000"),
    yamlRecord + pyRecord.replace("100755", "100644"),
    yamlRecord + pyRecord.replace(analyzer, "scripts/unexpected.py"),
    (yamlRecord + pyRecord).slice(0, -1),
  ]) {
    assert.throws(() => verifyManagedCloudObservabilityPolicy(config, git({ tree: Buffer.from(tree) })),
      /committed Observability Policy/);
  }
});

test("rejects empty or oversized policy files without granting proof", () => {
  for (const diff of [
    { workflowBytes: Buffer.alloc(0) },
    { workflowBytes: Buffer.alloc(16_385) },
    { analyzerBytes: Buffer.alloc(65_537) },
  ]) {
    assert.throws(() => verifyManagedCloudObservabilityPolicy(config, git(diff)),
      /invalid committed Observability Policy file size/);
  }
});
