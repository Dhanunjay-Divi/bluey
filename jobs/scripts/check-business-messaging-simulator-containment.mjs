import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function assertAbsent(source, patterns, label) {
  for (const pattern of patterns) {
    assert.doesNotMatch(source, pattern, `${label} must not match ${pattern}`);
  }
}

function walk(relativeRoot, predicate) {
  const root = path.join(repoRoot, relativeRoot);
  const results = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const relativePath = path.join(relativeRoot, entry.name);
    if (entry.isDirectory()) {
      results.push(...walk(relativePath, predicate));
    } else if (predicate(relativePath)) {
      results.push(relativePath);
    }
  }
  return results;
}

export function checkBusinessMessagingSimulatorContainment() {
  const tsPath =
    "jobs/automation/tests/support/business-messaging-simulator.ts";
  const fixturePath =
    "jobs/automation/tests/fixtures/business-messaging-simulator-v1.json";
  const rustPath = "server/src/jobs_business_messaging_simulator.rs";
  const rustTestPath = "server/tests/business_messaging_simulator.rs";
  const requiredPaths = [tsPath, fixturePath, rustPath, rustTestPath];
  for (const relativePath of requiredPaths) {
    assert(
      fs.existsSync(path.join(repoRoot, relativePath)),
      `${relativePath} must exist`,
    );
  }

  const tsSource = read(tsPath);
  const fixture = read(fixturePath);
  const rustSource = read(rustPath);
  const rustTest = read(rustTestPath);
  const automationIndex = read("jobs/automation/src/index.ts");
  const automationPackage = JSON.parse(read("jobs/automation/package.json"));
  const automationTsconfig = JSON.parse(
    read("jobs/automation/tsconfig.json"),
  );
  const cargo = read("server/Cargo.toml");
  const library = read("server/src/lib.rs").replace(/\s+/g, " ").trim();

  assert.deepEqual(
    tsSource.match(/^[ \t]*import .*$/gm) ?? [],
    ['import { createHash } from "node:crypto";'],
    `${tsPath} may statically import only node:crypto createHash`,
  );
  assert.equal(
    (tsSource.match(/^[ \t]*import\b/gm) ?? []).length,
    1,
    `${tsPath} must not contain multiline or additional static imports`,
  );
  assert.deepEqual(
    rustSource.match(/^[ \t]*use .*;$/gm) ?? [],
    [
      "use serde::{Deserialize, Serialize};",
      "use serde_json::Value;",
      "use sha2::{Digest, Sha256};",
      "use std::fmt;",
      "    use super::*;",
    ],
    `${rustPath} may use only deterministic serialization, hashing, and formatting`,
  );
  assert.equal(
    (rustSource.match(/^[ \t]*use\b/gm) ?? []).length,
    5,
    `${rustPath} must not contain multiline or additional use declarations`,
  );

  assert.doesNotMatch(
    automationIndex,
    /business-messaging-simulator/,
    "the simulator must not be exported from the automation package root",
  );
  assert.doesNotMatch(
    JSON.stringify({
      main: automationPackage.main,
      types: automationPackage.types,
      files: automationPackage.files,
      exports: automationPackage.exports,
    }),
    /business-messaging-simulator/,
    "the simulator must not be a published package export",
  );
  assert.deepEqual(
    automationTsconfig.include,
    ["src/**/*.ts"],
    "the production automation compiler must include only src/**/*.ts",
  );
  for (const compiledPath of [
    "jobs/automation/dist/business-messaging-simulator.js",
    "jobs/automation/dist/business-messaging-simulator.d.ts",
  ]) {
    assert(
      !fs.existsSync(path.join(repoRoot, compiledPath)),
      `${compiledPath} must not exist in production automation output`,
    );
  }
  for (const runtimeRoot of [
    "jobs/automation/src",
    "jobs/browser/src",
    "jobs/runner/src",
    "jobs/workflows/src",
    "jobs/portal/src",
  ]) {
    for (const relativePath of walk(runtimeRoot, (entry) =>
      /\.(?:js|mjs|ts|tsx)$/.test(entry),
    )) {
      if (relativePath === tsPath) continue;
      assert.doesNotMatch(
        read(relativePath),
        /business-messaging-simulator/,
        `${relativePath} must not import the test-only simulator`,
      );
    }
  }

  assertAbsent(
    tsSource,
    [
      /["']node:(?:http|https|http2|net|tls|dns|dgram|child_process|fs)["']/,
      /\b(?:import|require)\s*\(/,
      /\bfetch\s*\(/,
      /\b(?:XMLHttpRequest|WebSocket|EventSource)\b/,
      /\b(?:process\.|Deno\.|Bun\.|navigator\.)/,
      /\b(?:exec|execFile|spawn|fork)\s*\(/,
      /\b(?:Date\.now|Math\.random|randomUUID|getRandomValues)\s*\(/,
      /\b(?:OAuth|PKCE|accessToken|refreshToken|clientSecret)\b/,
    ],
    tsPath,
  );

  assertAbsent(
    rustSource,
    [
      /\b(?:reqwest|hyper|tokio|tokio_tungstenite|lettre)\b/,
      /\bstd::(?:env|net|process|fs)\b/,
      /\b(?:env|net|process|fs)::/,
      /\b(?:DbPool|rusqlite|postgres|Redis|MailboxConnection|JobsProviderCredential)\b/,
      /\b(?:jobs_communication_dispatch|jobs_provider_auth)\b/,
      /\b(?:prepare_application_draft|save_candidate_event|upsert_posting)\b/,
      /\b(?:INSERT\s+INTO|UPDATE\s+jobs_|DELETE\s+FROM)\b/i,
      /\b(?:Command|SocketAddr|TcpListener|TcpStream|ToSocketAddrs|UdpSocket)\b/,
    ],
    rustPath,
  );

  assertAbsent(
    fixture,
    [
      /https?:\/\//i,
      /\bBearer\s+/i,
      /\b(?:sk_live_|sk-ant-|AKIA)[A-Za-z0-9_-]+/,
      /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/,
      /\+\d{7,}/,
    ],
    fixturePath,
  );

  assert.match(
    cargo,
    /^business-messaging-simulator-test-support = \[\]$/m,
  );
  assert.match(
    cargo,
    /\[\[test\]\]\s+name = "business_messaging_simulator"\s+path = "tests\/business_messaging_simulator\.rs"\s+required-features = \["business-messaging-simulator-test-support"\]/,
  );
  assert.match(
    library,
    /#\[cfg\(all\(\s*feature = "business-messaging-simulator-test-support",\s*not\(debug_assertions\)\s*\)\)\] compile_error!\("business-messaging-simulator-test-support must never be enabled in release builds"\);/,
  );
  assert.match(
    library,
    /#\[cfg\(feature = "business-messaging-simulator-test-support"\)\] pub mod jobs_business_messaging_simulator;/,
  );

  assert.match(
    rustTest,
    /business_messaging_simulator_v1\.json/,
    "the Rust verifier must consume the shared fixture",
  );

  const runtimeFiles = walk("server/src", (entry) => entry.endsWith(".rs"));
  for (const relativePath of runtimeFiles) {
    if (relativePath === rustPath || relativePath === "server/src/lib.rs") {
      continue;
    }
    assert.doesNotMatch(
      read(relativePath),
      /jobs_business_messaging_simulator/,
      `${relativePath} must not reference the test-only simulator`,
    );
  }

  for (const productionPath of [
    "server/Dockerfile.jobs",
    ".github/workflows/jobs-managed-cloud-release.yml",
  ]) {
    assert.doesNotMatch(
      read(productionPath),
      /business-messaging-simulator-test-support/,
      `${productionPath} must not compile simulator test support`,
    );
  }

  for (const artifactConsumerPath of [
    "jobs/runner/Dockerfile",
    "jobs/workflows/Dockerfile",
    ".github/workflows/jobs-browser-release.yml",
  ]) {
    const artifactConsumer = read(artifactConsumerPath);
    assert.doesNotMatch(
      artifactConsumer,
      /automation\/tests|business-messaging-simulator/,
      `${artifactConsumerPath} must not package simulator test support`,
    );
  }

  const guardCommand =
    "node jobs/scripts/check-business-messaging-simulator-containment.mjs";
  const featureClippy =
    "cargo clippy --manifest-path server/Cargo.toml --locked --no-default-features " +
    "--features business-messaging-simulator-test-support " +
    "--test business_messaging_simulator -- -D warnings";
  const featureTest =
    "cargo test --manifest-path server/Cargo.toml --locked --no-default-features " +
    "--features business-messaging-simulator-test-support " +
    "--test business_messaging_simulator";
  for (const workflowPath of [
    ".github/workflows/jobs-ci.yml",
    ".github/workflows/release.yml",
  ]) {
    const workflow = read(workflowPath).replace(/\s+/g, " ").trim();
    assert.equal(workflow.split(guardCommand).length - 1, 1);
    assert.equal(workflow.split(featureClippy).length - 1, 1);
    assert.equal(workflow.split(featureTest).length - 1, 1);
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  checkBusinessMessagingSimulatorContainment();
  console.log(
    "Business messaging simulator containment passed (test-only, no route, no egress).",
  );
}
