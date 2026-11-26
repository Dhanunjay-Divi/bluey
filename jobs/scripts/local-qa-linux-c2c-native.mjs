// Native synthetic C2C actor. A separate administrator-owned launcher stages
// the sealed input and provisions the dedicated unit. This actor never does.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { requireLocalQaLinuxC2cEnvironment } from "./local-qa-linux-c2c-session.mjs";
import { createLocalQaLinuxC2cChromium } from "./local-qa-linux-c2c-chromium.mjs";
import { validateLocalQaLinuxC2cBrowserBinding } from "./local-qa-linux-c2c-restart-gateway.mjs";
import { createLocalQaLinuxC2cUnitAssertion } from "./local-qa-linux-c2c-unit.mjs";
import { runLocalQaLinuxOwnedBrowserSignup, runLocalQaLinuxOwnedC2cJourney,
  validateLocalQaLinuxOwnedApiBinding } from "./local-qa-linux-owned-api-session.mjs";
import { assertLocalQaLinuxPostgresRunRoot } from "./local-qa-linux-postgres.mjs";

const REFUSED = "local_qa_linux_c2c_native_refused";
const UNCLOSED = "local_qa_linux_c2c_native_unclosed";
const SPEC_NAME = ".local-qa-linux-c2c-native.json";
const MAX_SPEC = 65_536;
const MAX_WALL_MS = 480_000;
const SHA = /^[0-9a-f]{64}$/;
const HASH32 = /^[0-9a-f]{32}$/;
const exact = (value, keys) => value && Object.getPrototypeOf(value) === Object.prototype
  && Object.keys(value).sort().join(",") === keys;
const whole = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;
const closedPath = value => typeof value === "string" && value !== "/" && resolve(value) === value
  && !/[\0\r\n]/.test(value);
const stamp = entry => [entry.dev, entry.ino, entry.uid, entry.mode, entry.nlink, entry.size, entry.mtimeNs,
  entry.ctimeNs].join(":");
function need(value) { if (!value) throw new Error(REFUSED); }

export function validateLocalQaLinuxC2cNativeSpec(spec) {
  need(exact(spec, "binding,browser,fixture,sourceInventorySha256,sourceTree,unit,version") && spec.version === 1
    && whole(/^[0-9a-f]{40}$/, spec.sourceTree) && whole(SHA, spec.sourceInventorySha256));
  const { binding, browser, fixture, unit } = spec;
  need(exact(binding, "apiPort,buildBinding,networkNamespace,objectPort,pgCapsule,pidfdToolchain,postgresPort,root,smtpPort,unitCgroup"));
  // This verifies the same nested pins, ports, and root grammar as the actual
  // owner, without granting an execution-capable assertion before admission.
  const { pgBinding, buildBinding, apiPort, objectPort, smtpPort } = validateLocalQaLinuxOwnedApiBinding({
    ...binding, assertOwnedUnit: () => false,
  });
  const { root, unitCgroup, networkNamespace, pgCapsule, pidfdToolchain, port: postgresPort } = pgBinding;
  need(buildBinding.sourceSha256 === spec.sourceInventorySha256);
  need(exact(unit, "bootId,invocationId,mainPid,systemctl,unitName") && whole(HASH32, unit.bootId)
    && whole(HASH32, unit.invocationId) && Number.isSafeInteger(unit.mainPid) && unit.mainPid > 1
    && unit.mainPid <= 2_147_483_647 && unit.unitName === `bluey-jobs-qa-${pgCapsule.run}.service`
    && unitCgroup === `/system.slice/${unit.unitName}`
    && exact(unit.systemctl, "path,sha256") && unit.systemctl.path === "/usr/bin/systemctl"
    && whole(SHA, unit.systemctl.sha256));
  need(exact(fixture, "artifactPath,artifactSha256") && closedPath(fixture.artifactPath)
    && fixture.artifactPath.startsWith(`${root}/`) && fixture.artifactPath !== buildBinding.artifactPath
    && whole(SHA, fixture.artifactSha256));
  const writable = dirname(root);
  const outsideWritable = value => closedPath(value) && value !== writable
    && !value.startsWith(`${writable}/`) && !writable.startsWith(`${value}/`);
  need(exact(browser, "closureRoot,inventoryFile,inventorySha256,port,webDirectory")
    && outsideWritable(browser.closureRoot) && outsideWritable(browser.inventoryFile)
    && whole(SHA, browser.inventorySha256) && outsideWritable(browser.webDirectory)
    && outsideWritable(buildBinding.artifactPath) && outsideWritable(buildBinding.manifestFile));
  validateLocalQaLinuxC2cBrowserBinding({ port: browser.port, webDirectory: browser.webDirectory,
    openContext: () => { throw new Error(REFUSED); } }, [apiPort, objectPort, smtpPort, postgresPort]);
  return Object.freeze({ version: 1, sourceTree: spec.sourceTree, sourceInventorySha256: spec.sourceInventorySha256,
    binding: Object.freeze({ root, unitCgroup, networkNamespace, pgCapsule, pidfdToolchain, buildBinding,
      apiPort, objectPort, smtpPort, postgresPort }),
    unit: Object.freeze({ ...unit, systemctl: Object.freeze({ ...unit.systemctl }) }),
    fixture: Object.freeze({ ...fixture }), browser: Object.freeze({ ...browser }),
  });
}

export function parseLocalQaLinuxC2cNativeSpec(bytes, expectedSha256) {
  try {
    need(Buffer.isBuffer(bytes) && bytes.length > 0 && bytes.length <= MAX_SPEC && whole(SHA, expectedSha256)
      && createHash("sha256").update(bytes).digest("hex") === expectedSha256);
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    need(Buffer.from(JSON.stringify(value) + "\n").equals(bytes)); // No duplicate keys or implicit syntax normalization.
    return validateLocalQaLinuxC2cNativeSpec(value);
  } catch { throw new Error(REFUSED); }
}

async function loadSpec(root, path, expected) {
  need(path === join(root, SPEC_NAME) && await realpath(path) === path);
  const fd = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await fd.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o600n && before.size > 0n && before.size <= BigInt(MAX_SPEC));
    const buffer = Buffer.alloc(MAX_SPEC + 1);
    let length = 0;
    for (;;) {
      const { bytesRead } = await fd.read(buffer, length, buffer.length - length, null);
      length += bytesRead;
      need(length <= MAX_SPEC);
      if (!bytesRead) break;
    }
    need(BigInt(length) === before.size && stamp(before) === stamp(await fd.stat({ bigint: true }))
      && stamp(before) === stamp(await lstat(path, { bigint: true })));
    const spec = parseLocalQaLinuxC2cNativeSpec(buffer.subarray(0, length), expected);
    need(spec.binding.root === root);
    return spec;
  } finally { await fd.close(); }
}

async function requireRootOwnedPath(path, directory = false) {
  need(await realpath(path) === path);
  const leaf = await lstat(path, { bigint: true });
  need(leaf.uid === 0n && (leaf.mode & 0o022n) === 0n
    && (directory ? leaf.isDirectory() : leaf.isFile() && (leaf.mode & 0o222n) === 0n));
  for (let parent = dirname(path);; parent = dirname(parent)) {
    const row = await lstat(parent, { bigint: true });
    need(row.isDirectory() && row.uid === 0n && (row.mode & 0o022n) === 0n);
    if (parent === "/") break;
  }
}

export const runLocalQaLinuxC2cNative = options => runNativeFixture(options, "saved_c2c");

// Separate caller; the saved C2C command line and receipt contract stay unchanged.
export const runLocalQaLinuxBrowserSignupNative = options => runNativeFixture(options, "signup");
export const runLocalQaLinuxBrowserSampleResumeNative = options => runNativeFixture(options, "signup_sample_resume");
export const runLocalQaLinuxBrowserSamplePrepareNative = options => runNativeFixture(options, "signup_sample_prepare");
export const runLocalQaLinuxBrowserSampleMockReceiptNative = options =>
  runNativeFixture(options, "signup_sample_mock_receipt");

async function runNativeFixture({ specFile, specSha256, signal }, mode) {
  let browserOwner;
  try {
    const root = process.env.BLUEY_TEST_RUN_ROOT;
    need(process.platform === "linux" && process.arch === "x64" && typeof process.getuid === "function"
      && process.getuid() > 0 && typeof root === "string" && (signal === undefined || signal instanceof AbortSignal));
    need(process.execArgv.length === 0 && ["NODE_OPTIONS", "NODE_PATH", "LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT"]
      .every(key => process.env[key] === undefined));
    requireLocalQaLinuxC2cEnvironment(process.env);
    signal?.throwIfAborted();
    await assertLocalQaLinuxPostgresRunRoot(root);
    const spec = await loadSpec(root, specFile, specSha256);
    const { unitCgroup, networkNamespace, buildBinding } = spec.binding;
    const assertOwnedUnit = await createLocalQaLinuxC2cUnitAssertion({
      ...spec.unit, root, unitCgroup, networkNamespace, buildBinding,
    });
    const observed = Object.freeze({ root, stateDirectory: join(root, "bluey-jobs-qa-draft"),
      unitCgroup, networkNamespace, buildBinding });
    const assertBrowserUnit = () => assertOwnedUnit(observed);
    need(await assertBrowserUnit() === true);
    const { closureRoot, inventoryFile, inventorySha256, port, webDirectory } = spec.browser;
    await requireRootOwnedPath(buildBinding.artifactPath);
    await requireRootOwnedPath(buildBinding.manifestFile);
    await requireRootOwnedPath(webDirectory, true);
    browserOwner = await createLocalQaLinuxC2cChromium({ root, closureRoot, inventoryFile, inventorySha256,
      assertOwnedUnit: assertBrowserUnit });
    const input = { binding: Object.freeze({ ...spec.binding, assertOwnedUnit }),
      browser: Object.freeze({ openContext: browserOwner.openContext, port, webDirectory }), signal };
    const evidence = mode === "saved_c2c" ? (await runLocalQaLinuxOwnedC2cJourney({ ...input, fixture: spec.fixture })).evidence
      : await runLocalQaLinuxOwnedBrowserSignup({ ...input,
        includeSampleResume: mode === "signup_sample_resume" || mode === "signup_sample_prepare"
          || mode === "signup_sample_mock_receipt",
        prepareFixture: mode === "signup_sample_prepare" ? spec.fixture : undefined,
        reviewFirstMockFixture: mode === "signup_sample_mock_receipt" ? spec.fixture : undefined });
    need(await assertBrowserUnit() === true);
    const event = mode === "signup_sample_mock_receipt" ? "local_qa_linux_browser_sample_mock_receipt_completed"
      : mode === "signup_sample_prepare" ? "local_qa_linux_browser_sample_prepare_completed"
      : mode === "signup_sample_resume" ? "local_qa_linux_browser_sample_resume_completed"
      : mode === "signup" ? "local_qa_linux_browser_signup_completed" : "local_qa_linux_c2c_native_completed";
    return Object.freeze({ event,
      sourceTree: spec.sourceTree,
      sourceInventorySha256: spec.sourceInventorySha256, evidence });
  } catch (error) { throw new Error(/unclosed$/i.test(error?.message ?? "") ? UNCLOSED : REFUSED); }
  finally {
    try { await browserOwner?.close(); } catch { throw new Error(UNCLOSED); }
  }
}

async function main(args) {
  const controller = new AbortController();
  const interrupt = () => controller.abort();
  process.once("SIGINT", interrupt);
  process.once("SIGTERM", interrupt);
  try {
    need(args.length === 4 && args[0] === "--spec" && closedPath(args[1])
      && args[2] === "--sha256" && whole(SHA, args[3]));
    const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(MAX_WALL_MS)]);
    const receipt = await runLocalQaLinuxC2cNative({ specFile: args[1], specSha256: args[3], signal });
    process.stdout.write(JSON.stringify(receipt) + "\n");
  } catch (error) {
    process.stderr.write((error?.message === UNCLOSED ? UNCLOSED : REFUSED) + "\n");
    process.exitCode = 1;
  } finally {
    process.removeListener("SIGINT", interrupt);
    process.removeListener("SIGTERM", interrupt);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main(process.argv.slice(2));
