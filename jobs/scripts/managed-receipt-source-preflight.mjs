import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, lstatSync, openSync, readSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { managedReceiptCases } from "./managed-receipt-postgres-proof.mjs";
import {
  MANAGED_RECEIPT_CARGO_COMMAND, renderManagedReceiptPostgresWrapper,
} from "./render-managed-receipt-postgres-wrapper.mjs";

const SCOPE = ["jobs", "server", "crates/cue-core", "infra", ".github", "scripts", "Cargo.toml", "Cargo.lock"];
// Warmup pairs the API and portal; the portal imports these shared assets
// and validates the favicon at prerender. Do not copy the whole web tree.
const PREP_WEB_ASSETS = ["web/assets/bluey-logo.svg", "web/assets/bluey-wordmark.svg", "web/assets/favicon-512.png"];
const FIXTURE = "server/src/jobs_managed_request_start_postgres_test.rs";
const DRIVER = "jobs/runner/tests/managed-receipt-journey-driver.mjs";
const BROWSER_SUPPORT = [
  ["DRIVER", "jobs/portal/nonprod/interview-prep-fixture-driver.mjs"],
  ["GATEWAY", "jobs/portal/nonprod/interview-prep-fixture-gateway.mjs"],
  ["WITNESS", "jobs/portal/nonprod/interview-prep-witness.mjs"],
];
const WRAPPER = "jobs/scripts/run-nonprod-postgres-tests.mjs";
const NPM_LOCK = "jobs/package-lock.json";
const CARGO_LOCK = "server/Cargo.lock";
const MAX_FILE = 16 * 1024 * 1024;
const MAX_MANIFEST = 4 * 1024 * 1024;
const WARMUP_PROFILES = new Set(["interview-prep", "interview-prep-browser"]);
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const identity = (s) => [s.dev, s.ino, s.uid, s.mode, s.nlink, s.size, s.mtimeNs, s.ctimeNs];
const isWarmupProfile = (profile) => WARMUP_PROFILES.has(profile);

export function verifyReceiptSourceBindings({ fixture, driver, registry, dispatch, browserSupport }, profile = "baseline") {
  const pins = [...fixture.matchAll(/^const JOURNEY_DRIVER_SHA256: &str =\s*"([a-f0-9]{64})";$/gm)];
  assert.equal(pins.length, 1, "expected one driver pin");
  const driverSha256 = digest(driver);
  assert.equal(pins[0][1], driverSha256, "receipt driver does not match fixture pin");
  if (profile === "interview-prep-browser") {
    for (const [key] of BROWSER_SUPPORT) {
      const entry = new RegExp(`^const PREP_BROWSER_${key}_SHA256: &str =\\s*"([a-f0-9]{64})";$`, "gm");
      const binding = [...fixture.matchAll(entry)];
      assert.equal(binding.length, 1, "expected one browser support pin");
      assert(Buffer.isBuffer(browserSupport?.[key]), "browser support source missing");
      assert.equal(binding[0][1], digest(browserSupport[key]), "browser support does not match fixture pin");
    }
  }
  const feature = MANAGED_RECEIPT_CARGO_COMMAND[MANAGED_RECEIPT_CARGO_COMMAND.indexOf("--features") + 1];
  assert(dispatch.includes(`#[cfg(all(test, debug_assertions, feature = "${feature}"))]\n#[path = "jobs_managed_request_start_postgres_test.rs"]`),
    "receipt command does not select the fixture feature");
  for (const { name, witnesses } of managedReceiptCases(profile)) {
    assert.equal(registry.split(`"${name}"`).length, 2, "receipt case not uniquely registered");
    assert.equal(fixture.split(`"${name}"`).length, 2, "receipt fixture selector missing or duplicated");
    assert.equal(fixture.split(`async fn ${name.split("::").at(-1)}()`).length, 2, "receipt case body missing or duplicated");
    if (isWarmupProfile(profile)) {
      assert.equal(fixture.split(`"${witnesses.at(-1)}"`).length, 2, "receipt warmup witness not uniquely registered");
    }
  }
  return driverSha256;
}

export function readBoundFile(root, relative) {
  assert(relative.split("/").every((part) => part !== "." && part !== ".." && /^[A-Za-z0-9_.@+-]+$/.test(part)),
    "unexpected source path");
  const path = join(root, relative);
  assert.equal(realpathSync(path), path, "source path must be physical");
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = fstatSync(fd, { bigint: true });
    assert(before.isFile() && before.nlink === 1n && before.uid === BigInt(process.getuid())
      && before.size <= BigInt(MAX_FILE) && (before.mode & 0o022n) === 0n, "source file custody");
    const bytes = Buffer.alloc(Number(before.size));
    let read = 0;
    while (read < bytes.length) {
      const size = readSync(fd, bytes, read, bytes.length - read, read);
      assert(size > 0, "source file truncated");
      read += size;
    }
    assert.deepEqual(identity(fstatSync(fd, { bigint: true })), identity(before));
    assert.deepEqual(identity(lstatSync(path, { bigint: true })), identity(before));
    assert.equal(realpathSync(path), path, "source path changed");
    return { bytes, mode: Number(before.mode & 0o7777n), sha256: digest(bytes) };
  } finally {
    closeSync(fd);
  }
}

export function renderBoundReceiptWrapper(bytes, profile = "baseline") {
  const source = bytes.toString("utf8");
  assert(Buffer.from(source).equals(bytes), "wrapper source is not valid UTF-8");
  return Buffer.from(renderManagedReceiptPostgresWrapper(source, profile));
}

// Read-only observation of Git-selected working bytes, not a sealed snapshot,
// full filesystem inventory, or permission to launch a test.
export function preflightManagedReceiptSource(root, profile = "baseline") {
  const selected = managedReceiptCases(profile);
  const scope = isWarmupProfile(profile) ? [...SCOPE, ...PREP_WEB_ASSETS] : SCOPE;
  assert.equal(realpathSync(root), root, "physical checkout required");
  const git = (...args) => execFileSync("/usr/bin/git", [
    "--no-optional-locks", "-c", "core.fsmonitor=false", "-c", "core.excludesFile=/dev/null", ...args,
  ], {
    cwd: root, encoding: "utf8", timeout: 10_000, maxBuffer: 1024 * 1024,
    env: {
      PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", GIT_CONFIG_NOSYSTEM: "1",
      GIT_CONFIG_GLOBAL: "/dev/null", GIT_NO_REPLACE_OBJECTS: "1",
    },
  });
  assert.equal(git("rev-parse", "--show-toplevel"), `${root}\n`, "checkout root required");
  const head = git("rev-parse", "HEAD").trim();
  assert.match(head, /^[a-f0-9]{40}$/);
  const list = () => git("ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", ...scope)
    .split("\0").filter(Boolean).sort();
  const names = list();
  assert(names.length <= 10_000 && new Set(names).size === names.length, "invalid source inventory");
  if (isWarmupProfile(profile)) {
    assert(PREP_WEB_ASSETS.every((path) => names.includes(path)), "warmup portal brand assets missing");
  }
  const bindings = new Map();
  const needed = new Set([FIXTURE, DRIVER, WRAPPER, NPM_LOCK, CARGO_LOCK, "server/src/db/owned_postgres_test.rs",
    "server/src/jobs_workflow_dispatch.rs", "jobs/scripts/nonprod-postgres-identity.mjs"]);
  if (profile === "interview-prep-browser") {
    for (const [, name] of BROWSER_SUPPORT) needed.add(name);
  }
  const measure = (retain) => {
    let total = 0;
    return names.map((name) => {
      const { bytes, sha256, mode } = readBoundFile(root, name);
      total += bytes.length;
      assert(total <= 256 * 1024 * 1024, "source inventory byte limit");
      if (retain && needed.has(name)) bindings.set(name, bytes);
      return [name, sha256, mode, bytes.length];
    });
  };
  const rows = measure(true);
  assert.deepEqual(measure(false), rows, "source drift during preflight");
  assert.deepEqual(list(), names, "source inventory drift during preflight");
  assert.equal(git("rev-parse", "HEAD"), `${head}\n`, "HEAD changed during preflight");
  const bytes = (name) => {
    assert(bindings.has(name), `missing receipt input: ${name}`);
    return bindings.get(name);
  };
  const driverSha256 = verifyReceiptSourceBindings({
    fixture: bytes(FIXTURE).toString("utf8"), driver: bytes(DRIVER),
    registry: bytes("server/src/db/owned_postgres_test.rs").toString("utf8"),
    dispatch: bytes("server/src/jobs_workflow_dispatch.rs").toString("utf8"),
    ...(profile === "interview-prep-browser" && {
      browserSupport: Object.fromEntries(BROWSER_SUPPORT.map(([key, name]) => [key, bytes(name)])),
    }),
  }, profile);
  bytes("jobs/scripts/nonprod-postgres-identity.mjs");
  const rendered = renderBoundReceiptWrapper(bytes(WRAPPER), profile);
  const renderedWrapperSha256 = digest(rendered);
  const staged = rows.map((row) => row[0] === WRAPPER
    ? [WRAPPER, renderedWrapperSha256, row[2], Buffer.byteLength(rendered)]
    : row);
  const jobs = rows.filter(([path]) => path.startsWith("jobs/"));
  return {
    schema: "bluey_managed_receipt_source_preflight_v2", runtimeAdmitted: false, head,
    ...(profile !== "baseline" && { profile }),
    sourceRows: rows.length, sourceSha256: digest(JSON.stringify(rows)),
    jobsRows: jobs.length, jobsSha256: digest(JSON.stringify(jobs)),
    npmLockSha256: digest(bytes(NPM_LOCK)), cargoLockSha256: digest(bytes(CARGO_LOCK)),
    driverSha256, renderedWrapperSha256, rows,
    stagedSourceSha256: digest(JSON.stringify(staged)),
    stagedWrapper: staged.find(([path]) => path === WRAPPER),
    selectors: selected.map(({ name }) => name), command: MANAGED_RECEIPT_CARGO_COMMAND,
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const options = process.argv.slice(2);
  const profileFlags = new Map([
    ["--interview-prep", "interview-prep"],
    ["--interview-prep-browser", "interview-prep-browser"],
  ]);
  const profile = profileFlags.get(options[0]) ?? "baseline";
  if (profile !== "baseline") options.shift();
  const full = options.length === 1 && options[0] === "--manifest";
  assert(options.length === 0 || full,
    "read-only options: [--interview-prep|--interview-prep-browser] [--manifest]");
  const root = fileURLToPath(new URL("../..", import.meta.url)).replace(/\/$/, "");
  const result = preflightManagedReceiptSource(root, profile);
  const { rows, ...summary } = result;
  const output = JSON.stringify(full ? result : summary);
  assert(Buffer.byteLength(output) + 1 <= MAX_MANIFEST, "source manifest exceeds its byte bound");
  process.stdout.write(`${output}\n`);
}
