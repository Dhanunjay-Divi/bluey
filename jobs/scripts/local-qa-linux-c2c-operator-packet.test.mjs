import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { parseLocalQaLinuxC2cNativeSpec } from "./local-qa-linux-c2c-native.mjs";
import { composeLocalQaLinuxC2cOperatorPacket } from "./local-qa-linux-c2c-operator-packet.mjs";
import { expectedLocalQaLinuxC2cPgCapsule } from "./local-qa-linux-c2c-pg-capsule.mjs";

const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const wire = value => Buffer.from(`${JSON.stringify(value)}\n`);
function ordered(value) {
  if (Array.isArray(value)) return value.map(ordered);
  if (value === null || typeof value !== "object") return value;
  return Object.fromEntries(Object.keys(value).sort().map(key => [key, ordered(value[key])]));
}
const portalWire = value => wire(ordered(value));
const pythonReceiptWire = value => Buffer.from(`${JSON.stringify(ordered(value), null, 2)}\n`);
const resolverWire = value => Buffer.from(`{${Object.entries(ordered(value))
  .map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(", ")}}\n`);
const root = "/tmp/bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360";
const hex = char => char.repeat(64);
const source = {
  sourceTree: "1".repeat(40), sourceInventorySha256: hex("2"), sourceReceiptSha256: hex("3"),
  sourceArchiveSha256: hex("4"), buildSourceMapSha256: hex("5"), fixtureSourceSha256: hex("6"), lockSha256: hex("7"),
  packageProjectionSha256: hex("b"),
};
const receipt = {
  schema: "bluey-jobs-c2c-native-crossbuild-qa-v1", sourceTree: source.sourceTree,
  sourceReceiptSha256: source.sourceReceiptSha256, sourceInventorySha256: source.sourceInventorySha256,
  sourceArchiveSha256: source.sourceArchiveSha256, buildSourceMapSha256: source.buildSourceMapSha256,
  fixtureSourceSha256: source.fixtureSourceSha256, toolchainAdmissionBase: "8".repeat(40), sourceRows: 938,
  target: "x86_64-unknown-linux-gnu.2.39", qaOnly: true, releaseArtifact: false, runtimeQualified: false,
  buildExecuted: true, artifactsExecuted: false, offlineResolver: undefined,
  artifacts: [
    { name: "bluey-server", spoolName: "bluey-server", bytes: 20, sha256: hex("8") },
    { name: "bluey-jobs-api", spoolName: "bluey-jobs-api", bytes: 21, sha256: hex("9") },
    { name: "examples/jobs-nonprod-browser-fixture", spoolName: "jobs-nonprod-browser-fixture", bytes: 22, sha256: hex("a") },
  ],
};
const resolver = { schema: "bluey-jobs-c2c-native-offline-resolver-v1", sourceTree: source.sourceTree,
  sourceReceiptSha256: source.sourceReceiptSha256, lockSha256: source.lockSha256,
  target: "x86_64-unknown-linux-gnu", feature: "integration-test-support", resolvedPackages: 353,
  packageProjectionSha256: hex("b"), offlineLocked: true };
receipt.offlineResolver = resolver;
const portal = { "/index.html": hex("c"), "/jobs/index.html": hex("d"),
  ...Object.fromEntries(Array.from({ length: 42 }, (_, n) => [`/jobs/assets/part-${n}.js`, hex("d")])) };
const portalReceipt = {
  schema: "bluey-jobs-c2c-exact-static-qa-v1", sourceTree: source.sourceTree,
  sourceInventorySha256: source.sourceInventorySha256, nativeSourceReceiptSha256: source.sourceReceiptSha256,
  inputRows: 123, inputMapSha256: hex("1"), portalFiles: 36, staticFiles: 44,
  testCounts: { numFailedTests: 0, numPendingTests: 1, skippedBodyExecuted: false },
  c2cHandoffTests: ["synthetic one", "synthetic two"],
  staticChecks: { inertGatewayRows: 44, inertGatewayRefusals: 2, authReturnTests: 7 },
  resourcesByHtml: { "/index.html": [] }, portalBundleSha256: hex("2"), archiveSha256: hex("3"),
  archiveBytes: 20, manifestSha256: "", browserExecuted: false, apiExecuted: false,
  postgresExecuted: false, releaseAdmitted: false,
};
const chromePaths = [
  ["receipt-journey-node/toolchain/bin/node", 0o555],
  ["receipt-prep-browser/chromium/chrome", 0o555],
  ["receipt-prep-browser/node_modules/playwright-core/browsers.json", 0o444],
  ["receipt-prep-browser/node_modules/playwright-core/index.mjs", 0o444],
  ["receipt-prep-browser/node_modules/playwright-core/package.json", 0o444],
].sort(([a], [b]) => a.localeCompare(b));
const chromium = { version: 1, files: chromePaths.map(([path, mode]) => ({ path, mode, size: 1, sha256: hex("e") })) };
const layout = { root, apiArtifactPath: "/run/admitted/bluey-server", apiManifestPath: "/run/admitted/portal-manifest.json",
  fixtureArtifactPath: `${root}/fixtures/jobs-nonprod-browser-fixture`, webDirectory: "/run/admitted/portal",
  browserClosureRoot: "/run/admitted/chromium", chromiumInventoryFile: "/run/admitted/chromium/inventory.json" };
const run = "1".repeat(32);
const unitName = `bluey-jobs-qa-${run}.service`;
const live = { unitName, bootId: "1".repeat(32), invocationId: "2".repeat(32),
  mainPid: 12345, unitCgroup: `/system.slice/${unitName}`, networkNamespace: "net:[4026532555]",
  systemctl: { path: "/usr/bin/systemctl", sha256: hex("0") }, apiPort: 43107, postgresPort: 43108,
  smtpPort: 43109, objectPort: 43110, browserPort: 43111,
  pidfdToolchain: { python: { path: "/usr/bin/python3.12", sha256: hex("f") }, helper: { path: "/run/admitted/local-qa-linux-postmaster-pidfd.py", sha256: hex("e") } } };

function input(change = {}) {
  const nativeReceiptBytes = pythonReceiptWire(receipt), resolverBytes = resolverWire(resolver);
  const portalManifestBytes = portalWire(portal), chromiumInventoryBytes = wire(chromium);
  const boundPortalReceipt = { ...portalReceipt, manifestSha256: sha(portalManifestBytes) };
  const portalReceiptBytes = portalWire(boundPortalReceipt);
  return { expected: { ...source, nativeReceiptSha256: sha(nativeReceiptBytes), resolverSha256: sha(resolverBytes),
      portalManifestSha256: sha(portalManifestBytes), portalReceiptSha256: sha(portalReceiptBytes),
      chromiumInventorySha256: sha(chromiumInventoryBytes) },
    nativeReceiptBytes, resolverBytes, portalManifestBytes, portalReceiptBytes, chromiumInventoryBytes, layout, live, ...change };
}

test("operator packet canonically binds independently pinned C2C inputs into an actor spec", () => {
  const packet = composeLocalQaLinuxC2cOperatorPacket(input());
  assert.equal(packet.physicalVerificationRequired, true);
  const parsed = parseLocalQaLinuxC2cNativeSpec(packet.specBytes, packet.specSha256);
  assert.equal(parsed.sourceTree, source.sourceTree);
  assert.equal(parsed.binding.buildBinding.artifactSha256, hex("8"));
  assert.deepEqual(parsed.binding.pgCapsule, expectedLocalQaLinuxC2cPgCapsule(run));
  assert.equal(parsed.fixture.artifactSha256, hex("a"));
  assert.equal(packet.chromiumFiles, 5);
});

test("operator packet refuses mixed source, forged output, portal mutation, and invalid live fields", () => {
  const mixed = input({ expected: { ...input().expected, sourceTree: "0".repeat(40) } });
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(mixed), /operator_packet_refused/);
  const forgedReceipt = structuredClone(receipt); forgedReceipt.qaOnly = false;
  const forgedBytes = pythonReceiptWire(forgedReceipt);
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ nativeReceiptBytes: forgedBytes,
    expected: { ...input().expected, nativeReceiptSha256: sha(forgedBytes) } })), /operator_packet_refused/);
  const mutatedPortal = wire({ ...portal, "/assets/app.js": "x".repeat(64) });
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ portalManifestBytes: mutatedPortal })), /operator_packet_refused/);
  const oldReceipt = structuredClone(receipt); oldReceipt.schema = "bluey-jobs-current-source-linux-qa-v1";
  const oldReceiptBytes = pythonReceiptWire(oldReceipt);
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ nativeReceiptBytes: oldReceiptBytes,
    expected: { ...input().expected, nativeReceiptSha256: sha(oldReceiptBytes) } })), /operator_packet_refused/);
  const malformedManifest = wire({ "/index.html": hex("c") });
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ portalManifestBytes: malformedManifest,
    expected: { ...input().expected, portalManifestSha256: sha(malformedManifest) } })), /operator_packet_refused/);
  const stalePortalReceipt = portalWire({ ...portalReceipt, manifestSha256: sha(portalWire(portal)),
    sourceTree: "0".repeat(40) });
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ portalReceiptBytes: stalePortalReceipt,
    expected: { ...input().expected, portalReceiptSha256: sha(stalePortalReceipt) } })), /operator_packet_refused/);
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ live: { ...live, browserPort: live.apiPort } })), /operator_packet_refused/);
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ live: {
    ...live, toolchain: expectedLocalQaLinuxC2cPgCapsule(run).tools,
  } })), /operator_packet_refused/);
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ layout: { ...layout, browserClosureRoot: `${root}/mutable` } })), /operator_packet_refused/);
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ layout: {
    ...layout, apiArtifactPath: "/run/admitted/bluey-jobs-api",
  } })), /operator_packet_refused/);
});

test("operator packet rejects duplicate-key wires, resolver projection drift, and incomplete static rows", () => {
  const original = input();
  const duplicate = Buffer.from(original.nativeReceiptBytes.toString().replace("{\n", '{\n  "qaOnly": false,\n'));
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ nativeReceiptBytes: duplicate,
    expected: { ...original.expected, nativeReceiptSha256: sha(duplicate) } })), /operator_packet_refused/);
  const different = { ...resolver, packageProjectionSha256: hex("c") };
  const resolverBytes = resolverWire(different), nativeReceiptBytes = pythonReceiptWire({ ...receipt, offlineResolver: different });
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ resolverBytes, nativeReceiptBytes,
    expected: { ...original.expected, resolverSha256: sha(resolverBytes), nativeReceiptSha256: sha(nativeReceiptBytes) } })),
  /operator_packet_refused/);
  const portalReceiptBytes = portalWire({ ...portalReceipt, staticFiles: 43, manifestSha256: sha(portalWire(portal)) });
  assert.throws(() => composeLocalQaLinuxC2cOperatorPacket(input({ portalReceiptBytes,
    expected: { ...original.expected, portalReceiptSha256: sha(portalReceiptBytes) } })), /operator_packet_refused/);
});
