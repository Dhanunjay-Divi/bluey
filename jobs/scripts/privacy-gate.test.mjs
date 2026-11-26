import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { runPrivacyGate, scanTextForSecrets } from "./privacy-gate.mjs";

function opaqueFixture(seed) {
  return [seed.toUpperCase(), "8kVz", "4mQa", "29Wt"].join("");
}

const assignments = [
  `BLUEY_JOBS_RUNNER_TOKEN=${opaqueFixture("tz")}`,
  `BLUEY_PROVIDER_CLIENT_SECRET = "${opaqueFixture("bf")}"`,
  `export BLUEY_SMTP_PASSWORD='${opaqueFixture("xr")}' # check assignment only`,
  "BLUEY_SIGNING_PRIVATE_KEY=replace-with-jobs-signing-key",
  "SQUARE_SANDBOX_ACCESS_TOKEN=EAAA_sandbox_xxxxx",
  "SQUARE_PRODUCTION_ACCESS_TOKEN=EAAA_production_xxxxx",
  "BLUEY_RETRY_TOKEN=${RUNNER_RETRY_TOKEN}",
].join("\n");

const rejected = [
  { line: 1, kind: "raw bluey_jobs_runner_token assignment" },
  { line: 2, kind: "raw bluey_provider_client_secret assignment" },
  { line: 3, kind: "raw bluey_smtp_password assignment" },
];

test("environment templates reject both quoted and unquoted opaque assignments", () => {
  assert.deepEqual(scanTextForSecrets("ops/runner.env.example", assignments), rejected);
});

test("template placeholders must match the value, not a word in an opaque credential", () => {
  assert.deepEqual(scanTextForSecrets(".env.example", assignments.split("\n").slice(3).join("\n")), []);
  const opaque = [opaqueFixture("jk"), "fake", opaqueFixture("lm")].join("-");
  assert.deepEqual(scanTextForSecrets(".env.example", `APP_ACCESS_TOKEN=${opaque}`), [
    { line: 1, kind: "raw app_access_token assignment" },
  ]);
});

test("template credentials with punctuation or key-specific names are not overlooked", () => {
  const punctuated = [opaqueFixture("qp"), "@.", opaqueFixture("mb"), "!"].join("");
  const keys = [
    "AWS_SECRET_ACCESS_KEY", "BLUEY_SERVER_SECRET_KEY", "BLUEY_ASSET_SIGNING_KEY",
    "BLUEY_SESSION_DATA_KEY", "BLUEY_RUNNER_CAPABILITY_KEY", "BLUEY_WEBHOOK_SIGNATURE_KEY",
  ];
  const text = keys.map((key, i) => `${key}=${i % 2 ? JSON.stringify(punctuated + "#suffix") : punctuated}`).join("\n");
  assert.deepEqual(
    scanTextForSecrets("ops/runtime.ENV.EXAMPLE", text),
    keys.map((key, i) => ({ line: i + 1, kind: `raw ${key.toLowerCase()} assignment` })),
  );
});

test("vendor placeholder exceptions are exact-name-bound and do not mask malformed assignments", () => {
  const text = [
    "OTHER_ACCESS_TOKEN=EAAA_sandbox_xxxxx",
    "SQUARE_PRODUCTION_ACCESS_TOKEN=EAAA_sandbox_xxxxx",
    "SQUARE_SANDBOX_ACCESS_TOKEN=EAAA_SANDBOX_XXXXX",
    "RUNNER_SIGNING_KEY=Replace-with-runner-key",
    `JOBS_WEBHOOK_SECRET="${opaqueFixture("hd")}`,
    'EMPTY_SECRET=""',
    "FRESH_ACCESS_TOKEN= # secret injected outside the template",
  ].join("\n");
  assert.deepEqual(scanTextForSecrets(".env.example", text), [
    { line: 1, kind: "raw other_access_token assignment" },
    { line: 2, kind: "raw square_production_access_token assignment" },
    { line: 3, kind: "raw square_sandbox_access_token assignment" },
    { line: 4, kind: "raw runner_signing_key assignment" },
    { line: 5, kind: "unreadable template credential assignment" },
  ]);
});

test("plural API and signing key lists cannot bypass template checks", () => {
  const keyList = [opaqueFixture("vz"), opaqueFixture("td")].join(",");
  const text = [
    `GEMINI_API_KEYS=${keyList}`,
    `BLUEY_SIGNING_KEYS="${keyList}"`,
    "OPENAI_API_KEYS=",
    "DEEPGRAM_API_KEYS=replace-with-deepgram-api-keys",
    "ANTHROPIC_API_KEYS_SOURCE=read-from-runtime-config",
  ].join("\n");
  assert.deepEqual(scanTextForSecrets("ops/bluey-api.env.example", text), [
    { line: 1, kind: "raw gemini_api_keys assignment" },
    { line: 2, kind: "raw bluey_signing_keys assignment" },
  ]);
});

test("the indexed gate scans dotenv template paths without expanding by blob alias", () => {
  const root = mkdtempSync(path.join(tmpdir(), "jobs-privacy-template-"));
  const inheritedGit = Object.entries(process.env).filter(([key]) => key.startsWith("GIT_"));
  const localGit = { GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null", GIT_TERMINAL_PROMPT: "0" };
  const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "pipe" });
  try {
    for (const [key] of inheritedGit) delete process.env[key];
    Object.assign(process.env, localGit);
    git("init", "-q");
    mkdirSync(path.join(root, "ops"));
    writeFileSync(path.join(root, ".env.example"), "BLUEY_API_ORIGIN=https://example.invalid\n");
    writeFileSync(path.join(root, "ops/runner.env.example"), assignments);
    writeFileSync(path.join(root, "opaque.example"), assignments);
    git("add", "-f", "--", ".env.example", "ops/runner.env.example", "opaque.example");
    assert.deepEqual(runPrivacyGate(root), {
      trackedPaths: 3,
      scannedTextFiles: 2,
      issues: rejected.map((issue) => ({ filePath: "ops/runner.env.example", ...issue })),
    });
  } finally {
    for (const key of Object.keys(localGit)) delete process.env[key];
    for (const [key, value] of inheritedGit) process.env[key] = value;
    rmSync(root, { recursive: true });
  }
});
