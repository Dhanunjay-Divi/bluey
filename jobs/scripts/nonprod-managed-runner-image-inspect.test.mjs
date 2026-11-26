import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { validateExpected, validateMeasuredBinding, validateReport, selectChromium,
  validateImageInspectionExpected, validateImageInspectionReport,
  imageInspectionFailureCode, imageInspectionFailureDetail,
  validateImageInspectionFailure, validateOutputReadback }
  from "./nonprod-managed-runner-image-inspect.mjs";
import { canonicalJsonBytes, deriveManagedCloudRuntimeIdentitySha256, sha256, validateTarRejectionDiagnostic }
  from "./managed-cloud-release-gate.mjs";

const expected = () => ({ version: 1, sourceCommit: "a".repeat(40), buildId: "runner-p639-test",
  configSchemaSha256: "b".repeat(64), migrationSetSha256: "c".repeat(64), protocolSetSha256: "d".repeat(64) });
const executable = () => ({ executablePath: "/ms-playwright/chromium_headless_shell-1228/chrome-linux/chrome-headless-shell",
  executableSha256: "e".repeat(64), chromiumRevision: "1228" });

test("inspection failure details classify known archive failures without raw text", () => {
  const cases = [
    ["Tar archive bounds are invalid", "tar_bounds_or_termination"],
    ["Tar entry must use canonical POSIX ustar magic and version", "tar_header_magic_or_checksum"],
    ["Tar archive contains a duplicate path: private/example", "tar_path_or_duplicate"],
    ["Tar archive contains unsupported entry type x", "tar_entry_type_or_link"],
    ["Tar entry padding must be all zero", "tar_content_or_padding"],
    ["OCI blob does not match its content digest: private/example", "oci_layout_or_blob_digest"],
    ["OCI image manifest descriptor is not size-bound", "oci_index_or_manifest_descriptor"],
    ["OCI config rootfs does not bind the exact uncompressed layers", "oci_config_descriptor_or_rootfs"],
    ["OCI symbolic link target is missing: private/example", "oci_layer_descriptor_or_application"],
    ["jobs-runner OCI environment is not the exact closed set", "oci_runtime_contract"],
    ["jobs-runner runtime contains development content: private/example", "runtime_content_contract"],
    ["unrecognized private/example failure", "unknown"],
  ];
  for (const [message, reason] of cases) {
    const detail = imageInspectionFailureDetail(new Error(message), "archive", 1234);
    assert.equal(detail.class, reason);
    assert.equal(detail.messageSha256, sha256(Buffer.from(message)));
    assert.equal(detail.archiveBytes, 1234);
    assert.equal(JSON.stringify(detail).includes("private/example"), false);
    assert.equal(Object.hasOwn(detail, "message"), false);
    assert.equal(Object.hasOwn(detail, "stack"), false);
    assert.equal(validateImageInspectionFailure(detail), detail);
  }
  assert.equal(imageInspectionFailureDetail(new Error(cases[3][0]), "archive").tarTypeByte, "x");
  const io = Object.assign(new Error("private/example"), { code: "EACCES" });
  assert.equal(imageInspectionFailureDetail(io, "archive").class, "io_file");
});

test("failure detail rejects extra fields, unsafe types and malformed bounds", () => {
  const original = imageInspectionFailureDetail(new Error("unknown"), "archive", 1234);
  for (const patch of [{ message: "private/example" }, { class: "pass" },
    { archiveBytes: -1 }, { archiveBytes: 8 * 1024 ** 3 + 1 },
    { stage: "arbitrary" }, { messageSha256: "0".repeat(64) },
    { tarTypeByte: "x" }, { class: "tar_entry_type_or_link", tarTypeByte: "\n" },
    { class: "tar_entry_type_or_link", tarTypeByte: "xx" }]) {
    assert.throws(() => validateImageInspectionFailure({ ...original, ...patch }));
  }
  const hostile = { get message() { throw new Error("private/example"); } };
  const fallback = imageInspectionFailureDetail(hostile, "invalid", NaN);
  assert.equal(fallback.class, "unknown");
  assert.equal(fallback.stage, "input");
  assert.equal(fallback.archiveBytes, null);
  assert.equal(JSON.stringify(fallback).includes("private/example"), false);
});

function tarDiagnostic() {
  return { version: 1, scope: "layer", layerDigest: "sha256:" + "a".repeat(64),
    layerSizeBytes: 2048, layerOrdinal: 2, headerOrdinal: 3,
    headerNameSha256: "b".repeat(64), headerNameBytes: 32,
    paxState: "parsed", paxBodyBytes: 64, paxBodySha256: "c".repeat(64),
    paxRecordCount: 1, paxKnownKeys: ["path"], paxUnknownKeyCount: 0,
    paxKeySetSha256: sha256(canonicalJsonBytes(["path"])),
    pathOverride: true, linkOverride: false, sizeOverride: false, ownershipOverride: false,
    timeOverride: false, xattrOverride: false, sparseOverride: false };
}

test("image failure output includes only a cloned closed tar diagnostic within the total bound", () => {
  for (const scope of ["layout", "layer", "runtime-bundle"]) {
    const attached = tarDiagnostic(); attached.scope = scope;
    if (scope !== "layer") {
      attached.layerDigest = null; attached.layerSizeBytes = null; attached.layerOrdinal = null;
    }
    const error = Object.assign(new Error("Tar archive contains unsupported entry type x"),
      { tarDiagnostic: attached });
    const detail = imageInspectionFailureDetail(error, "archive", 4096);
    assert.deepEqual(detail.tarDiagnostic, attached);
    assert.notEqual(detail.tarDiagnostic, attached);
    assert.notEqual(detail.tarDiagnostic.paxKnownKeys, attached.paxKnownKeys);
    assert.equal(detail.tarTypeByte, "x");
    assert.equal(validateImageInspectionFailure(detail), detail);
    assert.ok(canonicalJsonBytes(detail).length <= 2048);
    attached.paxKnownKeys.push("private-secret-key");
    assert.deepEqual(detail.tarDiagnostic.paxKnownKeys, ["path"]);
  }
});

test("tar diagnostic validator rejects open schemas, invalid enums, counts and hostile accessors", () => {
  const mutations = [
    (v) => { v.path = "private-secret-path"; },
    (v) => { v.paxValues = ["private-secret-value"]; },
    (v) => { v.paxValueSha256 = "f".repeat(64); },
    (v) => { v.scope = "private-secret-scope"; },
    (v) => { v.layerDigest = "private-secret-digest"; },
    (v) => { v.layerOrdinal = 0; }, (v) => { v.headerOrdinal = -1; },
    (v) => { v.headerNameBytes = 257; }, (v) => { v.headerNameSha256 = "bad"; },
    (v) => { v.paxState = "admitted"; }, (v) => { v.paxRecordCount = 129; },
    (v) => { v.paxBodyBytes = 65537; }, (v) => { v.paxBodySha256 = null; },
    (v) => { v.paxKnownKeys = ["private-secret-key"]; },
    (v) => { v.paxKnownKeys = ["path", "path"]; },
    (v) => { v.paxUnknownKeyCount = 2; }, (v) => { v.pathOverride = "private-secret-value"; },
    (v) => { v.pathOverride = false; }, (v) => { v.xattrOverride = true; },
    (v) => { v.paxKeySetSha256 = "f".repeat(64); },
    (v) => { v.paxState = "oversize"; },
    (v) => { Object.defineProperty(v, "scope", { get() { throw Error("private-secret-getter"); } }); },
    (v) => { Object.defineProperty(v.paxKnownKeys, "0", { get() { throw Error("private-secret-getter"); } }); },
    (v) => { v.toJSON = () => "private-secret-value"; },
    (v) => { v.paxKnownKeys.toJSON = () => "private-secret-value"; },
  ];
  for (const mutate of mutations) {
    const attached = tarDiagnostic(); mutate(attached);
    assert.throws(() => validateTarRejectionDiagnostic(attached));
    const detail = imageInspectionFailureDetail(Object.assign(
      new Error("Tar archive contains unsupported entry type x"), { tarDiagnostic: attached }), "archive");
    assert.equal(Object.hasOwn(detail, "tarDiagnostic"), false);
    assert.equal(JSON.stringify(detail).includes("private-secret"), false);
    assert.ok(canonicalJsonBytes(detail).length <= 2048);
  }
});

test("hostile attached diagnostic getters are omitted without invocation or changing the original class", () => {
  let invoked = false;
  const error = new Error("Tar archive contains unsupported entry type x");
  Object.defineProperty(error, "tarDiagnostic", { get() { invoked = true; throw Error("private-secret"); } });
  const detail = imageInspectionFailureDetail(error, "archive");
  assert.equal(invoked, false);
  assert.equal(Object.hasOwn(detail, "tarDiagnostic"), false);
  assert.equal(detail.class, "tar_entry_type_or_link");
  const unrelated = Object.assign(new Error("private-secret"), { tarDiagnostic: tarDiagnostic() });
  assert.equal(Object.hasOwn(imageInspectionFailureDetail(unrelated, "archive"), "tarDiagnostic"), false);
});

test("image failure validator rejects a tar diagnostic without the unsupported-type envelope", () => {
  const detail = imageInspectionFailureDetail(new Error("unknown"), "archive");
  assert.throws(() => validateImageInspectionFailure({ ...detail, tarDiagnostic: tarDiagnostic() }));
  const type = imageInspectionFailureDetail(new Error("Tar archive contains unsupported entry type x"), "archive");
  assert.throws(() => validateImageInspectionFailure({ ...type,
    tarDiagnostic: { ...tarDiagnostic(), rawHeaderName: "private-secret" } }));
});

function binding() {
  const input = expected();
  const measurement = { ...input, audience: "bluey-jobs-managed-cloud-runtime-measurement-v1",
    componentId: "jobs-runner", roles: ["managed_runner"], measuredFiles: [] };
  const digest = sha256(canonicalJsonBytes(measurement));
  const identity = deriveManagedCloudRuntimeIdentitySha256(digest, "jobs-runner", "managed_runner");
  const bundle = "f".repeat(64);
  return { input, measurement, bundle, identity, local: { componentId: "jobs-runner",
    configSchemaSha256: input.configSchemaSha256, migrationSetSha256: input.migrationSetSha256,
    protocolSetSha256: input.protocolSetSha256, runtimeMeasurementSha256: digest,
    runtimeIdentitySha256: identity, automationBundleSha256: bundle } };
}
function report() {
  return { version: 1, expected: expected(), oci: { manifestSha256: "1".repeat(64),
    imageConfigSha256: "2".repeat(64), inventorySha256: "3".repeat(64) },
    runtime: { runtimeMeasurementSha256: "4".repeat(64), runtimeIdentitySha256: "5".repeat(64),
      automationBundleSha256: "6".repeat(64) },
    native: { writeReadback: true, inventory: true, removed: true },
    browser: { ...executable(), playwrightVersion: "1.61.1", chromiumVersion: "148.0.1.2",
      titleReadback: true, requestsAborted: 0,
      launchProfile: "production_default_options_synthetic_only" },
    claims: { actualImageInspection: true, nativeAddon: true, browserOnly: true,
      normalRunnerStartup: false, managedAuthority: false, submission: false, production: false },
    inspectorSha256: "7".repeat(64) };
}

test("expected contract admits exactly the six named nonzero authority fields", () => {
  assert.equal(validateExpected(expected()).version, 1);
  for (const mutate of [
    (v) => { v.extra = "private"; }, (v) => { delete v.buildId; }, (v) => { v.version = 2; },
    (v) => { v.sourceCommit = "A".repeat(40); }, (v) => { v.buildId = "../escape"; },
    ...["configSchemaSha256", "migrationSetSha256", "protocolSetSha256"].map((key) =>
      (v) => { v[key] = "0".repeat(64); }),
  ]) { const value = expected(); mutate(value); assert.throws(() => validateExpected(value)); }
});

test("measured binding requires the exact runner role and all local derived identities", () => {
  const f = binding();
  assert.doesNotThrow(() => validateMeasuredBinding(f.input, f.measurement, f.local, f.bundle, f.identity));
  for (const mutate of [
    (v) => { v.measurement.componentId = "jobs-workflows"; },
    (v) => { v.measurement.roles.push("workflow_worker"); },
    (v) => { v.measurement.sourceCommit = "9".repeat(40); },
    (v) => { v.measurement.buildId = "different-build"; },
    (v) => { v.local.extra = "private"; },
    ...["componentId", "configSchemaSha256", "migrationSetSha256", "protocolSetSha256",
      "runtimeMeasurementSha256", "runtimeIdentitySha256", "automationBundleSha256"].map((key) =>
      (v) => { v.local[key] = "9".repeat(64); }),
  ]) {
    const value = binding(); mutate(value);
    assert.throws(() => validateMeasuredBinding(value.input, value.measurement, value.local,
      value.bundle, value.identity));
  }
});

test("chromium selection requires one measured executable with exact inventory bytes", () => {
  const path = executable().executablePath.slice(1);
  const measurement = { measuredFiles: [{ path, sha256: "e".repeat(64) }] };
  const inventory = { entries: [{ path, type: "file", mode: "0555", sizeBytes: 5, sha256: "e".repeat(64) }] };
  assert.deepEqual(selectChromium(measurement, inventory), executable());
  assert.throws(() => selectChromium({ measuredFiles: [] }, inventory));
  assert.throws(() => selectChromium({ measuredFiles: [...measurement.measuredFiles,
    ...measurement.measuredFiles] }, inventory));
  for (const altered of [{ mode: "0444" }, { type: "symlink" }, { sizeBytes: 0 },
    { sha256: "f".repeat(64) }]) {
    assert.throws(() => selectChromium(measurement, { entries: [{ ...inventory.entries[0], ...altered }] }));
  }
});

test("report keeps OCI manifest and Docker config identities distinct and claims closed", () => {
  const value = report();
  assert.equal(validateReport(value), value);
  assert.notEqual(value.oci.manifestSha256, value.oci.imageConfigSha256);
  for (const mutate of [
    (v) => { v.privateToken = "forbidden"; }, (v) => { delete v.oci.imageConfigSha256; },
    (v) => { v.native.removed = false; }, (v) => { v.browser.chromiumRevision = "999"; },
    (v) => { v.browser.requestsAborted = -1; }, (v) => { v.browser.titleReadback = false; },
    (v) => { v.browser.launchProfile = "sandbox_certified"; },
    ...["normalRunnerStartup", "managedAuthority", "submission", "production"].map((key) =>
      (v) => { v.claims[key] = true; }),
    (v) => { v.claims.networkIsolation = true; },
    (v) => { v.runtime.automationBundleSha256 = "0".repeat(64); },
  ]) { const altered = report(); mutate(altered); assert.throws(() => validateReport(altered)); }
});

test("public readback validator binds independently supplied expectations and rejects omission", () => {
  assert.equal(validateImageInspectionExpected(expected()).version, 1);
  assert.equal(validateImageInspectionReport(report(), expected()).version, 1);
  assert.throws(() => validateImageInspectionReport(report()));
  for (const key of ["sourceCommit", "buildId", "configSchemaSha256",
    "migrationSetSha256", "protocolSetSha256"]) {
    const different = expected();
    different[key] = key === "sourceCommit" ? "9".repeat(40)
      : key === "buildId" ? "different-build" : "9".repeat(64);
    assert.throws(() => validateImageInspectionReport(report(), different));
  }
});

test("closed stage diagnostics never incorporate errors, paths or private values", () => {
  for (const stage of ["input", "archive", "runtime", "native", "browser", "output"])
    assert.equal(imageInspectionFailureCode(stage), `managed_runner_image_inspection_failed_${stage}`);
  for (const unknown of [undefined, "PRIVATE /path token", {}, "native\nsecret"])
    assert.equal(imageInspectionFailureCode(unknown), "managed_runner_image_inspection_failed_input");
});

test("output readback requires exact bytes and length", () => {
  const bytes = Buffer.from('{"synthetic":true}\n');
  assert.doesNotThrow(() => validateOutputReadback(bytes, Buffer.from(bytes)));
  for (const value of [bytes.subarray(1), Buffer.concat([bytes, Buffer.from("\n")]),
    Buffer.alloc(bytes.length), bytes.toString("utf8")])
    assert.throws(() => validateOutputReadback(bytes, value));
});

test("inspection source has only fixed paths, lazy actual imports and bounded synthetic smoke", async () => {
  const source = await readFile(new URL("./nonprod-managed-runner-image-inspect.mjs", import.meta.url), "utf8");
  assert.ok(source.includes('process.argv.slice(2), ["--inspect-image"]'));
  assert.ok(source.includes('inspectOciImageArtifact(ARCHIVE, "jobs-runner")'));
  assert.equal(source.match(/await inspectOciImageArtifact\(/g)?.length, 1);
  assert.ok(source.includes('await import("/app/automation/dist/managed-cloud-runtime.js")'));
  assert.ok(source.includes('await import("/app/runner/dist/native-runner-storage.js")'));
  assert.ok(source.includes('serviceWorkers: "block"'));
  assert.ok(source.includes('offline: true'));
  assert.ok(!source.includes('chromiumSandbox:'));
  assert.ok(source.includes('args: [...CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS]'));
  assert.ok(source.includes('await route.abort()'));
  assert.ok(!source.includes("page.goto("));
  assert.ok(!source.includes("--no-sandbox"));
  assert.ok(!source.includes("process.env"));
  assert.ok(source.includes('validateOutputReadback(bytes, await boundedRegularBytes('));
  for (const stage of ["input", "archive", "runtime", "native", "browser", "output"])
    assert.ok(source.includes(`inspectionStage = "${stage}"`));
});
