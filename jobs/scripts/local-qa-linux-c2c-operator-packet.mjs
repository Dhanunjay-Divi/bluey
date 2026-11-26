// Pure operator-side composition for the sealed Linux C2C QA actor.  This
// module deliberately performs no filesystem, process, or network operation:
// physical ownership, systemd state, and root-owned staging are separate
// operator responsibilities.
import { createHash } from "node:crypto";
import { basename, dirname, resolve } from "node:path";
import { parseLocalQaBuildBinding } from "./local-qa-build-binding.mjs";
import { parseLocalQaLinuxC2cChromiumInventory } from "./local-qa-linux-c2c-chromium.mjs";
import { parseLocalQaLinuxC2cNativeSpec } from "./local-qa-linux-c2c-native.mjs";
import { expectedLocalQaLinuxC2cPgCapsule } from "./local-qa-linux-c2c-pg-capsule.mjs";

const REFUSED = "local_qa_linux_c2c_operator_packet_refused";
const SHA = /^[0-9a-f]{64}$/;
const TREE = /^[0-9a-f]{40}$/;
const BOOT = /^[0-9a-f]{32}$/;
const PATH = value => typeof value === "string" && value !== "/" && resolve(value) === value
  && !/[\0\r\n]/.test(value);
const own = value => value !== null && typeof value === "object"
  && Object.getPrototypeOf(value) === Object.prototype && !Array.isArray(value);
const keys = (value, expected) => own(value)
  && Object.keys(value).length === expected.length && expected.every(key => Object.hasOwn(value, key));
const whole = (re, value) => typeof value === "string" && re.exec(value)?.[0] === value;
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const canonical = value => Buffer.from(`${JSON.stringify(value)}\n`);
const fail = () => { throw new Error(REFUSED); };
const need = value => { if (!value) fail(); };

function sorted(value) {
  if (Array.isArray(value)) return value.map(sorted);
  if (!own(value)) return value;
  return Object.fromEntries(Object.keys(value).sort().map(key => [key, sorted(value[key])]));
}

function parsePinnedJson(bytes, expected, wire = "compact") {
  need(Buffer.isBuffer(bytes) && bytes.length > 0 && bytes.length <= 65_536 && whole(SHA, expected)
    && sha(bytes) === expected);
  let value;
  try { value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
  catch { fail(); }
  const ordered = sorted(value);
  // Match the exact producer wires; round-trip equality refuses duplicate
  // fields and alternate encodings even if an incorrect pin is supplied.
  if (wire === "resolver") {
    need(own(ordered) && Object.values(ordered).every(item => !own(item) && !Array.isArray(item)));
    const entries = Object.entries(ordered).map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`);
    need(Buffer.from(`{${entries.join(", ")}}\n`).equals(bytes));
  } else if (wire === "native") {
    need(Buffer.from(`${JSON.stringify(ordered, null, 2)}\n`).equals(bytes));
  } else {
    need(canonical(ordered).equals(bytes));
  }
  return value;
}

function parseNativeReceipt(bytes, expected, source) {
  const value = parsePinnedJson(bytes, expected, "native");
  need(keys(value, ["artifacts", "buildExecuted", "buildSourceMapSha256", "fixtureSourceSha256",
    "offlineResolver", "qaOnly", "releaseArtifact", "runtimeQualified", "artifactsExecuted",
    "schema", "sourceArchiveSha256", "sourceInventorySha256", "sourceReceiptSha256", "sourceRows",
    "sourceTree", "target", "toolchainAdmissionBase"])
    && value.schema === "bluey-jobs-c2c-native-crossbuild-qa-v1"
    && value.qaOnly === true && value.buildExecuted === true && value.releaseArtifact === false
    && value.runtimeQualified === false && value.artifactsExecuted === false && value.sourceRows === 938
    && value.target === "x86_64-unknown-linux-gnu.2.39"
    && value.sourceTree === source.sourceTree
    && value.sourceInventorySha256 === source.sourceInventorySha256
    && value.sourceReceiptSha256 === source.sourceReceiptSha256
    && value.sourceArchiveSha256 === source.sourceArchiveSha256
    && value.buildSourceMapSha256 === source.buildSourceMapSha256
    && value.fixtureSourceSha256 === source.fixtureSourceSha256
    && whole(TREE, value.toolchainAdmissionBase));
  need(Array.isArray(value.artifacts) && value.artifacts.length === 3
    && value.artifacts.map(row => row?.name).join(",")
      === "bluey-server,bluey-jobs-api,examples/jobs-nonprod-browser-fixture");
  const result = new Map();
  for (const row of value.artifacts) {
    need(keys(row, ["bytes", "name", "sha256", "spoolName"])
      && typeof row.bytes === "number" && Number.isSafeInteger(row.bytes) && row.bytes >= 20
      && row.bytes <= 192 * 1024 * 1024 && whole(SHA, row.sha256)
      && row.spoolName === basename(row.name));
    result.set(row.name, Object.freeze({ ...row }));
  }
  return Object.freeze({ receipt: Object.freeze(value), artifacts: result });
}

function parseResolver(bytes, expected, source) {
  const value = parsePinnedJson(bytes, expected, "resolver");
  need(keys(value, ["feature", "lockSha256", "offlineLocked", "packageProjectionSha256",
    "resolvedPackages", "schema", "sourceReceiptSha256", "sourceTree", "target"])
    && value.schema === "bluey-jobs-c2c-native-offline-resolver-v1"
    && value.sourceTree === source.sourceTree && value.sourceReceiptSha256 === source.sourceReceiptSha256
    && value.lockSha256 === source.lockSha256 && value.target === "x86_64-unknown-linux-gnu"
    && value.feature === "integration-test-support" && value.resolvedPackages === 353
    && value.offlineLocked === true && value.packageProjectionSha256 === source.packageProjectionSha256);
  return Object.freeze(value);
}

function parsePortalManifest(bytes, expected) {
  const value = parsePinnedJson(bytes, expected);
  need(own(value));
  const entries = Object.entries(value);
  need(entries.length === 44);
  for (const [url, digest] of entries) {
    need(whole(/^\/(?:index\.html|assets\/[A-Za-z0-9._/-]+|jobs\/[A-Za-z0-9._/-]+)$/, url)
      && !url.includes("..") && !url.includes("//") && whole(SHA, digest));
  }
  need(Object.hasOwn(value, "/index.html") && Object.hasOwn(value, "/jobs/index.html"));
  return Object.freeze({ ...value });
}

function parsePortalReceipt(bytes, expected, source, manifest) {
  const value = parsePinnedJson(bytes, expected);
  need(keys(value, ["apiExecuted", "archiveBytes", "archiveSha256", "browserExecuted",
    "c2cHandoffTests", "inputMapSha256", "inputRows", "manifestSha256", "nativeSourceReceiptSha256",
    "portalBundleSha256", "portalFiles", "postgresExecuted", "releaseAdmitted", "resourcesByHtml",
    "schema", "sourceInventorySha256", "sourceTree", "staticChecks", "staticFiles", "testCounts"])
    && value.schema === "bluey-jobs-c2c-exact-static-qa-v1"
    && value.sourceTree === source.sourceTree && value.sourceInventorySha256 === source.sourceInventorySha256
    && value.nativeSourceReceiptSha256 === source.sourceReceiptSha256
    && value.manifestSha256 === source.portalManifestSha256
    && value.browserExecuted === false && value.apiExecuted === false && value.postgresExecuted === false
    && value.releaseAdmitted === false && value.staticFiles === 44 && value.inputRows === 123
    && value.portalFiles === 36 && own(value.staticChecks) && own(value.resourcesByHtml)
    && Array.isArray(value.c2cHandoffTests) && value.c2cHandoffTests.length === 2 && own(value.testCounts));
  need(Object.keys(manifest).length === value.staticFiles
    && value.staticChecks.inertGatewayRows === value.staticFiles && value.staticChecks.inertGatewayRefusals === 2
    && value.staticChecks.authReturnTests === 7 && value.testCounts.numFailedTests === 0
    && value.testCounts.numPendingTests === 1 && value.testCounts.skippedBodyExecuted === false);
  return Object.freeze(value);
}

function parseExpected(value) {
  need(keys(value, ["buildSourceMapSha256", "chromiumInventorySha256", "fixtureSourceSha256",
    "lockSha256", "nativeReceiptSha256", "packageProjectionSha256", "portalManifestSha256", "portalReceiptSha256",
    "resolverSha256",
    "sourceArchiveSha256", "sourceInventorySha256", "sourceReceiptSha256", "sourceTree"])
    && whole(TREE, value.sourceTree)
    && Object.entries(value).every(([key, item]) => key === "sourceTree" || whole(SHA, item)));
  return Object.freeze({ ...value });
}

function parseLayout(value) {
  need(keys(value, ["apiArtifactPath", "apiManifestPath", "browserClosureRoot", "chromiumInventoryFile",
    "fixtureArtifactPath", "root", "webDirectory"])
    && Object.values(value).every(PATH));
  const writable = dirname(value.root);
  const external = path => path !== writable && !path.startsWith(`${writable}/`) && !writable.startsWith(`${path}/`);
  need(external(value.apiArtifactPath) && external(value.apiManifestPath) && external(value.webDirectory)
    && external(value.browserClosureRoot) && external(value.chromiumInventoryFile)
    && value.chromiumInventoryFile === `${value.browserClosureRoot}/inventory.json`
    && value.fixtureArtifactPath.startsWith(`${value.root}/`) && value.fixtureArtifactPath !== value.apiArtifactPath
    && basename(value.apiArtifactPath) === "bluey-server"
    && basename(value.fixtureArtifactPath) === "jobs-nonprod-browser-fixture"
    && value.apiManifestPath !== value.chromiumInventoryFile);
  return Object.freeze({ ...value });
}

function parseLive(value, root) {
  need(keys(value, ["apiPort", "bootId", "browserPort", "invocationId", "mainPid", "networkNamespace", "objectPort",
    "pidfdToolchain", "postgresPort", "smtpPort", "systemctl", "unitCgroup", "unitName"])
    && whole(/^bluey-jobs-qa-[0-9a-f]{32}\.service$/, value.unitName)
    && typeof value.bootId === "string" && BOOT.test(value.bootId)
    && typeof value.invocationId === "string" && BOOT.test(value.invocationId)
    && Number.isSafeInteger(value.mainPid) && value.mainPid > 1 && value.mainPid <= 2_147_483_647
    && value.unitCgroup === `/system.slice/${value.unitName}`
    && typeof value.networkNamespace === "string" && /^net:\[[1-9][0-9]{0,19}\]$/.test(value.networkNamespace)
    && keys(value.systemctl, ["path", "sha256"]) && value.systemctl.path === "/usr/bin/systemctl"
    && SHA.test(value.systemctl.sha256));
  const ports = [value.apiPort, value.postgresPort, value.smtpPort, value.objectPort, value.browserPort];
  need(ports.every(port => Number.isSafeInteger(port) && port > 1024 && port <= 65535 && ![5187, 5432].includes(port))
    && new Set(ports).size === ports.length);
  const pgCapsule = expectedLocalQaLinuxC2cPgCapsule(value.unitName.slice("bluey-jobs-qa-".length, -".service".length));
  // The downstream native parser binds the capsule and pidfd proofs again.
  return Object.freeze({ ...value, root, pgCapsule });
}

/**
 * Canonically compose the actor spec from already measured bytes and observed
 * values.  It does not assert that paths are root-owned or that `live` came
 * from systemd; the root-only launcher must establish those facts separately.
 */
export function composeLocalQaLinuxC2cOperatorPacket({ expected, nativeReceiptBytes, resolverBytes,
  portalManifestBytes, portalReceiptBytes, chromiumInventoryBytes, layout, live }) {
  try {
    const source = parseExpected(expected);
    const native = parseNativeReceipt(nativeReceiptBytes, source.nativeReceiptSha256, source);
    const resolver = parseResolver(resolverBytes, source.resolverSha256, source);
    need(JSON.stringify(native.receipt.offlineResolver) === JSON.stringify(resolver));
    const portal = parsePortalManifest(portalManifestBytes, source.portalManifestSha256);
    parsePortalReceipt(portalReceiptBytes, source.portalReceiptSha256, source, portal);
    const inventory = parseLocalQaLinuxC2cChromiumInventory(chromiumInventoryBytes);
    need(sha(chromiumInventoryBytes) === source.chromiumInventorySha256);
    // C2C QA performs the full synthetic signup/login choreography.  That
    // lives in bluey-server's merged public-auth + protected Jobs router; the
    // dedicated Jobs API binary intentionally does not expose public auth.
    const api = native.artifacts.get("bluey-server");
    const fixture = native.artifacts.get("examples/jobs-nonprod-browser-fixture");
    const paths = parseLayout(layout);
    const manager = parseLive(live, paths.root);
    const buildBinding = parseLocalQaBuildBinding({ version: 1, qualificationOnly: true,
      artifactPath: paths.apiArtifactPath, artifactSha256: api.sha256,
      manifestFile: paths.apiManifestPath, manifestSha256: source.portalManifestSha256,
      sourceSha256: source.sourceInventorySha256 });
    const spec = { version: 1, sourceTree: source.sourceTree, sourceInventorySha256: source.sourceInventorySha256,
      binding: { root: paths.root, apiPort: manager.apiPort, postgresPort: manager.postgresPort,
        smtpPort: manager.smtpPort, objectPort: manager.objectPort, unitCgroup: manager.unitCgroup,
        networkNamespace: manager.networkNamespace, buildBinding, pgCapsule: manager.pgCapsule,
        pidfdToolchain: manager.pidfdToolchain },
      fixture: { artifactPath: paths.fixtureArtifactPath, artifactSha256: fixture.sha256 },
      unit: { unitName: manager.unitName, bootId: manager.bootId, invocationId: manager.invocationId,
        mainPid: manager.mainPid, systemctl: manager.systemctl },
      browser: { port: manager.browserPort, webDirectory: paths.webDirectory, closureRoot: paths.browserClosureRoot,
        inventoryFile: paths.chromiumInventoryFile, inventorySha256: source.chromiumInventorySha256 } };
    const bytes = canonical(spec);
    const digest = sha(bytes);
    const parsed = parseLocalQaLinuxC2cNativeSpec(bytes, digest);
    need(parsed.sourceTree === source.sourceTree
      && parsed.sourceInventorySha256 === source.sourceInventorySha256
      && parsed.binding.buildBinding.artifactSha256 === api.sha256
      && parsed.fixture.artifactSha256 === fixture.sha256
      && parsed.browser.inventorySha256 === source.chromiumInventorySha256);
    return Object.freeze({ specBytes: bytes, specSha256: digest, portalManifest: portal,
      chromiumFiles: inventory.files.length, sourceTree: source.sourceTree,
      physicalVerificationRequired: true });
  } catch { fail(); }
}
