// Read the policy from a committed tree, not from the candidate worktree.
// Approval comes from the caller's protected policy source, not from that tree.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { isAbsolute, normalize } from "node:path";
import { pathToFileURL } from "node:url";

const POLICY_FILES = [
  { path: ".github/workflows/observability-policy.yml", mode: "100644", maxBytes: 16_384 },
  { path: "scripts/analyze-tracing-calls.py", mode: "100755", maxBytes: 65_536 },
];
const SHA1 = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const MAX_BYTES = 65_536;
const DOMAIN = "bluey-jobs-managed-cloud-observability-policy-v1";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function requirePolicy(condition, message) {
  if (!condition) throw new Error(message);
}

function localGit(repo, args) {
  const reply = spawnSync("/usr/bin/git", [
    "--no-optional-locks", "--no-replace-objects", "-c", "protocol.allow=never", "-C", repo, ...args,
  ], {
    env: {
      PATH: "/usr/bin:/bin",
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_NO_LAZY_FETCH: "1",
      GIT_TERMINAL_PROMPT: "0",
      LC_ALL: "C",
    },
    maxBuffer: MAX_BYTES,
    timeout: 5_000,
    stdio: ["pipe", "pipe", "pipe"],
  });
  requirePolicy(!reply.error && reply.status === 0 && reply.signal === null,
    "could not read the exact committed Observability Policy");
  return reply.stdout;
}

/** Trust the repo as a local Git object reader, not as a source of approval. */
export function verifyManagedCloudObservabilityPolicy({
  repo,
  sourceCommit,
  approvedPolicySha256,
}, readGit = localGit) {
  requirePolicy(typeof repo === "string" && isAbsolute(repo) && normalize(repo) === repo,
    "policy source repository must use an exact absolute path");
  requirePolicy(typeof sourceCommit === "string" && SHA1.test(sourceCommit),
    "policy source must be an exact commit");
  requirePolicy(typeof approvedPolicySha256 === "string" && SHA256.test(approvedPolicySha256),
    "an independently approved Observability Policy SHA-256 is required");

  const type = readGit(repo, ["cat-file", "-t", sourceCommit]);
  requirePolicy(Buffer.isBuffer(type) && type.equals(Buffer.from("commit\n")),
    "Observability Policy source object is not a commit");
  const listed = readGit(repo, [
    "ls-tree", "-z", sourceCommit, "--", ...POLICY_FILES.map(({ path }) => path),
  ]);
  requirePolicy(Buffer.isBuffer(listed) && listed.length > 0 && listed.length <= 1024,
    "invalid committed Observability Policy tree listing");
  const records = listed.toString("utf8").split("\0");
  requirePolicy(records.length === POLICY_FILES.length + 1 && records.pop() === "",
    "missing or unexpected committed Observability Policy file");
  const byPath = new Map();
  for (const line of records) {
    const match = /^(100644|100755) blob ([0-9a-f]{40})\t([a-zA-Z0-9._/-]+)$/.exec(line);
    requirePolicy(match && !byPath.has(match[3]), "invalid committed Observability Policy path or mode");
    byPath.set(match[3], { mode: match[1], objectId: match[2] });
  }

  const files = POLICY_FILES.map(({ path, mode, maxBytes }) => {
    const entry = byPath.get(path);
    requirePolicy(entry && entry.mode === mode, "missing committed Observability Policy file or wrong mode");
    const bytes = readGit(repo, ["cat-file", "blob", entry.objectId]);
    requirePolicy(Buffer.isBuffer(bytes) && bytes.length > 0 && bytes.length <= maxBytes,
      "invalid committed Observability Policy file size");
    return { path, mode, sha256: sha256(bytes) };
  });

  const policySha256 = sha256(JSON.stringify({ domain: DOMAIN, files }));
  requirePolicy(policySha256 === approvedPolicySha256,
    "committed Observability Policy does not match independently approved bytes");
  return { sourceCommit, policySha256, files };
}

function run(argv) {
  const allowed = new Map([
    ["--repo", "repo"],
    ["--source-commit", "sourceCommit"],
    ["--approved-policy-sha256", "approvedPolicySha256"],
  ]);
  requirePolicy(argv.length === allowed.size * 2, "exact Observability Policy CLI options required");
  const options = Object.create(null);
  for (let index = 0; index < argv.length; index += 2) {
    const key = allowed.get(argv[index]);
    requirePolicy(key && !Object.hasOwn(options, key), "unknown or duplicate Observability Policy option");
    options[key] = argv[index + 1];
  }
  console.log(JSON.stringify(verifyManagedCloudObservabilityPolicy(options)));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    run(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
