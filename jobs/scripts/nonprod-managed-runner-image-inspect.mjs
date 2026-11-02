import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, mkdir, open, realpath } from "node:fs/promises";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  canonicalJsonBytes,
  deriveManagedCloudRuntimeIdentitySha256,
  deriveManagedRunnerAutomationBundleSha256,
  inspectOciImageArtifact,
  validateTarRejectionDiagnostic,
} from "./managed-cloud-release-gate.mjs";

const EXPECTED = "/fixture/expected.json";
const ARCHIVE = "/fixture/runner.oci.tar";
const MEASUREMENT = "/app/.bluey/managed-cloud-runtime-measurement.json";
const OUT = "/out";
const NONZERO_SHA = /^(?!0{64}$)[a-f0-9]{64}$/;
const SHELL = /^ms-playwright\/chromium_headless_shell-([1-9][0-9]*)\/[a-z0-9._-]+\/chrome-headless-shell$/;
const CLAIMS = Object.freeze({ actualImageInspection: true, nativeAddon: true,
  browserOnly: true, normalRunnerStartup: false, managedAuthority: false,
  submission: false, production: false });
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const STAGES = new Set(["input", "archive", "runtime", "native", "browser", "output"]);
let inspectionStage = "input";
let archiveBytes = null;

const FAILURE_CLASSES = new Set(["tar_bounds_or_termination", "tar_header_magic_or_checksum",
  "tar_path_or_duplicate", "tar_entry_type_or_link", "tar_content_or_padding",
  "oci_layout_or_blob_digest", "oci_index_or_manifest_descriptor",
  "oci_config_descriptor_or_rootfs", "oci_layer_descriptor_or_application",
  "oci_runtime_contract", "runtime_content_contract", "io_file", "unknown"]);

export function validateImageInspectionFailure(value) {
  const keys = ["event", "version", "stage", "class", "messageSha256", "archiveBytes"];
  if (Object.hasOwn(value, "tarTypeByte")) keys.push("tarTypeByte");
  if (Object.hasOwn(value, "tarDiagnostic")) keys.push("tarDiagnostic");
  closed(value, keys);
  assert.equal(value.event, "managed_runner_image_inspection_failure");
  assert.equal(value.version, 1);
  assert.ok(STAGES.has(value.stage) && FAILURE_CLASSES.has(value.class));
  assert.match(value.messageSha256, NONZERO_SHA);
  assert.ok(value.archiveBytes === null || (Number.isSafeInteger(value.archiveBytes)
    && value.archiveBytes >= 0 && value.archiveBytes <= 8 * 1024 ** 3));
  if (Object.hasOwn(value, "tarTypeByte")) {
    assert.equal(value.class, "tar_entry_type_or_link");
    assert.match(value.tarTypeByte, /^[\x20-\x7e]$/);
  }
  if (Object.hasOwn(value, "tarDiagnostic")) {
    assert.equal(value.class, "tar_entry_type_or_link");
    assert.ok(Object.hasOwn(value, "tarTypeByte"));
    validateTarRejectionDiagnostic(value.tarDiagnostic);
  }
  assert.ok(canonicalJsonBytes(value).length <= 2048);
  return value;
}

export function imageInspectionFailureDetail(error, stage, size = null) {
  let message = "failure_message_unavailable";
  let io = false;
  try {
    if (typeof error?.message === "string") message = error.message;
    io = ["EACCES", "ENOENT", "EIO", "EBADF", "ELOOP", "ENOSPC", "EPERM"]
      .includes(error?.code);
  } catch { /* A hostile error getter must not prevent a closed diagnostic. */ }
  const rules = [
    ["tar_bounds_or_termination", /^Tar (?:archive bounds|header is truncated|archive has a nonterminal|archive must end|archive contains nonzero)/],
    ["tar_header_magic_or_checksum", /^Tar (?:entry must use canonical|header checksum|integer)/],
    ["tar_path_or_duplicate", /^(?:tar path |Tar archive contains a duplicate)/],
    ["tar_entry_type_or_link", /^(?:Tar archive contains unsupported entry type|Non-file tar entries|Tar link target)/],
    ["tar_content_or_padding", /^Tar (?:entry content|entry padding|archive contains too many)/],
    ["oci_layout_or_blob_digest", /^OCI (?:layout|blob)/],
    ["oci_index_or_manifest_descriptor", /^OCI (?:index|image descriptor|image manifest|candidate)/],
    ["oci_config_descriptor_or_rootfs", /^OCI (?:config|image config)/],
    ["oci_layer_descriptor_or_application", /^OCI (?:layer|verification|symbolic link|hard link|whiteout)/],
    ["oci_runtime_contract", /^jobs-runner OCI /],
    ["runtime_content_contract", /^(?:jobs-runner runtime |jobs-runner required runtime |Managed runner )/],
  ];
  const reason = io ? "io_file" : rules.find(([, pattern]) => pattern.test(message))?.[0] ?? "unknown";
  const value = { event: "managed_runner_image_inspection_failure", version: 1,
    stage: STAGES.has(stage) ? stage : "input", class: reason,
    messageSha256: sha256(Buffer.from(message)),
    archiveBytes: Number.isSafeInteger(size) && size >= 0 && size <= 8 * 1024 ** 3 ? size : null };
  const unsupported = /^Tar archive contains unsupported entry type ([\x20-\x7e])$/.exec(message);
  if (unsupported) value.tarTypeByte = unsupported[1];
  if (unsupported) {
    try {
      const attached = Object.getOwnPropertyDescriptor(error, "tarDiagnostic");
      if (attached && Object.hasOwn(attached, "value")) {
        const diagnostic = validateTarRejectionDiagnostic(attached.value);
        const withDiagnostic = { ...value, tarDiagnostic: diagnostic };
        if (canonicalJsonBytes(withDiagnostic).length <= 2048) value.tarDiagnostic = diagnostic;
      }
    } catch { /* Hostile or invalid attached properties are safely omitted. */ }
  }
  return validateImageInspectionFailure(value);
}

export function imageInspectionFailureCode(stage) {
  return `managed_runner_image_inspection_failed_${STAGES.has(stage) ? stage : "input"}`;
}

export function validateOutputReadback(expected, actual) {
  assert.ok(Buffer.isBuffer(expected) && Buffer.isBuffer(actual));
  assert.equal(actual.length, expected.length);
  assert.ok(actual.equals(expected));
}

function closed(value, keys) {
  assert.ok(value && Object.getPrototypeOf(value) === Object.prototype, "object");
  assert.deepEqual(Object.keys(value).sort(), [...keys].sort(), "keys");
}

export function validateExpected(value) {
  closed(value, ["version", "sourceCommit", "buildId", "configSchemaSha256",
    "migrationSetSha256", "protocolSetSha256"]);
  assert.equal(value.version, 1);
  assert.match(value.sourceCommit, /^[a-f0-9]{40}$/);
  assert.match(value.buildId, /^[a-z0-9][a-z0-9._:-]{2,127}$/);
  for (const key of ["configSchemaSha256", "migrationSetSha256", "protocolSetSha256"])
    assert.match(value[key], NONZERO_SHA);
  return value;
}

export const validateImageInspectionExpected = validateExpected;

export function validateMeasuredBinding(expected, measurement, local, bundle, identity) {
  validateExpected(expected);
  assert.equal(measurement.componentId, "jobs-runner");
  assert.deepEqual(measurement.roles, ["managed_runner"]);
  for (const key of ["sourceCommit", "buildId", "configSchemaSha256",
    "migrationSetSha256", "protocolSetSha256"]) assert.equal(measurement[key], expected[key]);
  closed(local, ["componentId", "configSchemaSha256", "migrationSetSha256",
    "protocolSetSha256", "runtimeIdentitySha256", "runtimeMeasurementSha256",
    "automationBundleSha256"]);
  assert.equal(local.componentId, "jobs-runner");
  for (const key of ["configSchemaSha256", "migrationSetSha256", "protocolSetSha256"])
    assert.equal(local[key], expected[key]);
  assert.equal(local.runtimeMeasurementSha256, sha256(canonicalJsonBytes(measurement)));
  assert.match(bundle, NONZERO_SHA);
  assert.match(identity, NONZERO_SHA);
  assert.equal(local.automationBundleSha256, bundle);
  assert.equal(local.runtimeIdentitySha256, identity);
}

export function selectChromium(measurement, inventory) {
  const candidates = measurement.measuredFiles.filter((entry) => SHELL.test(entry.path));
  assert.equal(candidates.length, 1, "single measured chromium headless shell");
  const measured = candidates[0];
  assert.match(measured.sha256, NONZERO_SHA);
  const entry = inventory.entries.find((item) => item.path === measured.path);
  assert.ok(entry && entry.type === "file" && entry.sizeBytes > 0);
  assert.equal(entry.sha256, measured.sha256);
  assert.ok((Number.parseInt(entry.mode, 8) & 0o111) !== 0);
  return { executablePath: `/${measured.path}`, executableSha256: measured.sha256,
    chromiumRevision: SHELL.exec(measured.path)[1] };
}

export function validateReport(value) {
  closed(value, ["version", "expected", "oci", "runtime", "native", "browser",
    "claims", "inspectorSha256"]);
  assert.equal(value.version, 1);
  validateExpected(value.expected);
  assert.match(value.inspectorSha256, NONZERO_SHA);
  closed(value.oci, ["manifestSha256", "imageConfigSha256", "inventorySha256"]);
  for (const digest of Object.values(value.oci)) assert.match(digest, NONZERO_SHA);
  closed(value.runtime, ["runtimeMeasurementSha256", "runtimeIdentitySha256", "automationBundleSha256"]);
  for (const digest of Object.values(value.runtime)) assert.match(digest, NONZERO_SHA);
  closed(value.native, ["writeReadback", "inventory", "removed"]);
  assert.deepEqual(value.native, { writeReadback: true, inventory: true, removed: true });
  closed(value.browser, ["playwrightVersion", "chromiumVersion", "chromiumRevision",
    "executablePath", "executableSha256", "titleReadback", "requestsAborted", "launchProfile"]);
  assert.equal(value.browser.launchProfile, "production_default_options_synthetic_only");
  assert.match(value.browser.playwrightVersion, /^[0-9]+\.[0-9]+\.[0-9]+$/);
  assert.match(value.browser.chromiumVersion, /^[0-9]+(?:\.[0-9]+){1,3}$/);
  assert.match(value.browser.chromiumRevision, /^[1-9][0-9]{0,9}$/);
  assert.match(value.browser.executablePath.slice(1), SHELL);
  assert.ok(value.browser.executablePath.startsWith("/"));
  assert.equal(SHELL.exec(value.browser.executablePath.slice(1))[1], value.browser.chromiumRevision);
  assert.match(value.browser.executableSha256, NONZERO_SHA);
  assert.equal(value.browser.titleReadback, true);
  assert.ok(Number.isSafeInteger(value.browser.requestsAborted) && value.browser.requestsAborted >= 0);
  assert.deepEqual(value.claims, CLAIMS);
  return value;
}

export function validateImageInspectionReport(value, expected) {
  validateReport(value);
  assert.deepEqual(value.expected, validateImageInspectionExpected(expected));
  return value;
}

async function boundedRegularBytes(path, maximum) {
  const before = await lstat(path);
  assert.ok(before.isFile() && !before.isSymbolicLink() && before.size > 0 && before.size <= maximum);
  const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const opened = await handle.stat();
    assert.ok(opened.dev === before.dev && opened.ino === before.ino && opened.size === before.size);
    const bytes = await handle.readFile();
    const after = await handle.stat();
    const named = await lstat(path);
    assert.ok(bytes.length === before.size && after.size === before.size
      && after.mtimeMs === before.mtimeMs && after.ctimeMs === before.ctimeMs
      && named.dev === before.dev && named.ino === before.ino && !named.isSymbolicLink());
    return bytes;
  } finally { await handle.close(); }
}

async function writeOutput(name, value, maximum) {
  const bytes = canonicalJsonBytes(value);
  assert.ok(bytes.length > 0 && bytes.length <= maximum);
  const handle = await open(`${OUT}/${name}`, "wx", 0o600);
  try {
    await handle.writeFile(bytes);
    await handle.sync();
    const stored = await handle.stat();
    assert.ok(stored.isFile() && stored.uid === process.getuid()
      && (stored.mode & 0o777) === 0o600 && stored.size === bytes.length);
  } finally { await handle.close(); }
  validateOutputReadback(bytes, await boundedRegularBytes(`${OUT}/${name}`, maximum));
  return sha256(bytes);
}

async function nativeSmoke() {
  const { openNativeRunnerStorageRoot } = await import("/app/runner/dist/native-runner-storage.js");
  await mkdir("/var/lib/bluey-jobs-runner/native-smoke", { mode: 0o700 });
  const root = openNativeRunnerStorageRoot("/var/lib/bluey-jobs-runner/native-smoke");
  root.assertUnchanged();
  const directory = await root.ensureDirectory(["probe"]);
  let ownsFile = false;
  let removed = false;
  try {
    ownsFile = await directory.writeFileExclusive("record", Buffer.from("bluey-image-smoke\n"));
    assert.equal(ownsFile, true);
    assert.deepEqual(await directory.readFileBounded("record", 64), Buffer.from("bluey-image-smoke\n"));
    const inventory = await directory.inventory();
    assert.equal(inventory.count, 1);
    assert.equal(inventory.bytes, Buffer.byteLength("bluey-image-smoke\n"));
    assert.equal(inventory.entries[0].sha256, sha256(Buffer.from("bluey-image-smoke\n")));
    await directory.removeEntry("record");
    ownsFile = false;
    assert.equal((await directory.inventory()).count, 0);
    await (await root.openDirectory([])).removeEntry("probe");
    removed = true;
    root.assertUnchanged();
    return { writeReadback: true, inventory: true, removed: true };
  } finally {
    if (ownsFile) await directory.removeEntry("record");
    if (!removed && (await directory.inventory()).count === 0)
      await (await root.openDirectory([])).removeEntry("probe");
  }
}

async function browserSmoke(executable) {
  const require = createRequire("/app/runner/package.json");
  const { chromium } = require("playwright");
  const { version: playwrightVersion } = require("playwright/package.json");
  const { CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS } = await import("/app/automation/dist/network.js");
  // Match the production runner's default launch options; this trusted synthetic smoke
  // does not qualify browser sandbox isolation or justify privileges/launch fallbacks.
  const browser = await chromium.launch({ executablePath: executable.executablePath,
    headless: true, timeout: 30000,
    args: [...CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS] });
  let context;
  let requestsAborted = 0;
  try {
    context = await browser.newContext({ acceptDownloads: true, offline: true,
      serviceWorkers: "block", viewport: { width: 1440, height: 1000 } });
    context.setDefaultTimeout(10000);
    await context.route("**/*", async (route) => { requestsAborted += 1; await route.abort(); });
    const page = await context.newPage();
    await page.setContent("<!doctype html><title>Bluey synthetic image smoke</title><h1>synthetic-only</h1>",
      { timeout: 15000, waitUntil: "domcontentloaded" });
    assert.equal(await page.title(), "Bluey synthetic image smoke");
    assert.equal(await page.locator("h1").textContent(), "synthetic-only");
    return { ...executable, playwrightVersion, chromiumVersion: browser.version(),
      titleReadback: true, requestsAborted,
      launchProfile: "production_default_options_synthetic_only" };
  } finally {
    try { if (context) await context.close(); } finally { await browser.close(); }
  }
}

export async function inspectImage() {
  inspectionStage = "input";
  archiveBytes = null;
  assert.equal(process.platform, "linux");
  assert.equal(process.arch, "x64");
  assert.ok(process.getuid() !== 0 && process.getuid() === process.geteuid());
  const out = await lstat(OUT);
  assert.ok(out.isDirectory() && !out.isSymbolicLink() && out.uid === process.getuid()
    && (out.mode & 0o077) === 0 && await realpath(OUT) === OUT);
  const expected = validateExpected(JSON.parse((await boundedRegularBytes(EXPECTED, 4096)).toString("utf8")));
  inspectionStage = "archive";
  const archiveBefore = await lstat(ARCHIVE);
  archiveBytes = archiveBefore.size;
  assert.ok(archiveBefore.isFile() && !archiveBefore.isSymbolicLink()
    && archiveBefore.size > 0 && archiveBefore.size <= 4 * 1024 * 1024 * 1024);
  const { inventory, imageConfigSha256 } = await inspectOciImageArtifact(ARCHIVE, "jobs-runner");
  const archiveAfter = await lstat(ARCHIVE);
  assert.ok(archiveBefore.dev === archiveAfter.dev && archiveBefore.ino === archiveAfter.ino
    && archiveBefore.size === archiveAfter.size && archiveBefore.mtimeMs === archiveAfter.mtimeMs
    && archiveBefore.ctimeMs === archiveAfter.ctimeMs);
  inspectionStage = "runtime";
  const bytes = await boundedRegularBytes(MEASUREMENT, 128 * 1024);
  const measurement = JSON.parse(bytes.toString("utf8"));
  assert.deepEqual(bytes, canonicalJsonBytes(measurement));
  const measurementSha256 = sha256(bytes);
  const identity = deriveManagedCloudRuntimeIdentitySha256(measurementSha256, "jobs-runner", "managed_runner");
  const context = { ...expected, componentId: "jobs-runner", roles: ["managed_runner"], inventory };
  const bundle = deriveManagedRunnerAutomationBundleSha256(measurement, measurementSha256,
    [{ role: "managed_runner", runtimeIdentitySha256: identity }], context);
  const { measureManagedCloudRuntimeIdentity } = await import("/app/automation/dist/managed-cloud-runtime.js");
  const local = measureManagedCloudRuntimeIdentity("managed_runner");
  validateMeasuredBinding(expected, measurement, local, bundle, identity);
  const executable = selectChromium(measurement, inventory);
  inspectionStage = "output";
  const inventorySha256 = await writeOutput("inventory.json", inventory, 32 * 1024 * 1024);
  await writeOutput("measurement.json", measurement, 128 * 1024);
  inspectionStage = "native";
  const native = await nativeSmoke();
  inspectionStage = "browser";
  const browser = await browserSmoke(executable);
  inspectionStage = "output";
  const report = validateImageInspectionReport({ version: 1, expected,
    oci: { manifestSha256: inventory.artifactSha256, imageConfigSha256, inventorySha256 },
    runtime: { runtimeMeasurementSha256: measurementSha256, runtimeIdentitySha256: identity, automationBundleSha256: bundle },
    native, browser, claims: { ...CLAIMS },
    inspectorSha256: sha256(await boundedRegularBytes(fileURLToPath(import.meta.url), 128 * 1024)) }, expected);
  const reportSha256 = await writeOutput("report.json", report, 16384);
  return { event: "managed_runner_image_inspected", version: 1, reportSha256,
    manifestSha256: report.oci.manifestSha256, imageConfigSha256, inspectorSha256: report.inspectorSha256 };
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  try {
    assert.deepEqual(process.argv.slice(2), ["--inspect-image"]);
    const result = await inspectImage();
    process.stdout.write(canonicalJsonBytes(result));
  } catch (error) {
    process.stderr.write(canonicalJsonBytes(imageInspectionFailureDetail(error,
      inspectionStage, archiveBytes)));
    process.stderr.write(imageInspectionFailureCode(inspectionStage) + "\n");
    process.exitCode = 1;
  }
}
