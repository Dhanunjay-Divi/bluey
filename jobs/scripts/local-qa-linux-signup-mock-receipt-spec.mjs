// Compose only data for the fixed, owned signup-to-mock receipt actor.
// The root manager must independently measure the installed bytes, waiting
// actor and reservation before it stages a spec or issues a release.
import { createHash } from "node:crypto";
import { parseLocalQaLinuxC2cChromiumInventory } from "./local-qa-linux-c2c-chromium.mjs";
import { parseLocalQaLinuxC2cNativeSpec } from "./local-qa-linux-c2c-native.mjs";
import { expectedLocalQaLinuxC2cPgCapsule } from "./local-qa-linux-c2c-pg-capsule.mjs";

const SOURCE_TREE = "6fe9eab0180160bc70079f8b9020fa21abf0db62";
const SOURCE_INVENTORY = "08459c30b037e839c798b4c5859dcbd2bb271905da36bcc3cdd7aaa4241eb7cc";
const APP_PINS = Object.freeze({
  nativeReceiptSha256: "8e923b0ec14fccc9ddb1ad13153fd4fd7831825dd73281155fe81f6123f361e3",
  resolverSha256: "21a0a4459f07381c5ee6ea65cc4399294d8d8a56c0667da8274955ea9d302e32",
  portalReceiptSha256: "cbcf82c5cd7468f21945cb042cf8b8298b32c82bb04a3bb92b0cc34acaa1a18a",
  portalManifestSha256: "ba874296d043ceb15ddae0893981b92ed8fd651c46abdb55a46f8b2637c43aa6",
});
const CODE_INVENTORY = "99035f76562ad58570f6f79e71907c02e53a0a14d69fae5b2d4a19e439930c56";
const BROWSER_INVENTORY = "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce";
const NODE_HASH = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90";
const PIDFD_HASH = "f15df1fa0777510d0f1d022dd35c64520cbbcb1c02d5fb1146c98fa413ae96a8";
const API_HASH = "29a50252ff22e740aea9e90a513a00eb82892849d0abfa52b76b2c27588f8884";
const FIXTURE_HASH = "8b10ffebe32a4488964533ac0af5d25911ddafbb389985d01d7c14629cb8fd0c";
const REFUSED = "local_qa_linux_signup_mock_receipt_spec_refused";
const HASH = /^[0-9a-f]{64}$/;
const RUN = /^[0-9a-f]{32}$/;
const whole = (re, value) => typeof value === "string" && re.exec(value)?.[0] === value;
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const wire = value => Buffer.from(JSON.stringify(value) + "\n");
const need = ok => { if (!ok) throw new Error(REFUSED); };
const exact = (a, b) => a !== null && typeof a === "object" && Object.getPrototypeOf(a) === Object.prototype
  && Reflect.ownKeys(a).length === Object.keys(b).length
  && Object.entries(b).every(([key, value]) => Object.hasOwn(a, key) && a[key] === value);

/** Validate installed-role reports against the fixed candidate and emit the actor's native wire. */
export function composeLocalQaLinuxSignupMockReceiptSpec({
  reservationSha256, waiting, application, browser, postgres, codePlanBytes, chromiumInventoryBytes,
  ports, systemctlSha256, python,
}) {
  try {
    const { run, root, bootId, invocationId, mainPid, unitName, unitCgroup, networkNamespace } = waiting;
    need(whole(RUN, run) && run !== "0".repeat(32)
      && whole(HASH, reservationSha256) && reservationSha256 !== "0".repeat(64)
      && whole(HASH, systemctlSha256) && whole(HASH, python.sha256)
      && whole(/^\/usr\/bin\/python3\.[0-9]{1,2}$/, python.path)
      && exact(python, { path: python.path, sha256: python.sha256 }));
    const appRoot = `/run/bluey-jobs-c2c-application/${run}`;
    const browserRoot = `/run/bluey-jobs-c2c-browser/${run}`;
    const apiPath = `${appRoot}/api/bluey-server`;
    const webDirectory = `${appRoot}/portal/web`;
    const portalManifestPath = `${appRoot}/portal/static-manifest.json`;
    const inventoryFile = `${browserRoot}/inventory.json`;
    const pending = { reservationSha256, runtimeQualified: false };
    need(exact(application, { ...pending, ...APP_PINS, sourceTree: SOURCE_TREE,
      root: appRoot, apiPath, webDirectory, portalManifestPath,
      fixturePath: `${appRoot}/fixture/jobs-nonprod-browser-fixture`, releasePublished: false }));
    need(exact(browser, { ...pending, run, browserRoot, inventoryFile, browserInventorySha256: BROWSER_INVENTORY,
      files: 410, payloadBytes: 533_939_096, installed: true }));
    need(Buffer.isBuffer(chromiumInventoryBytes) && chromiumInventoryBytes.length === 70_130
      && sha(chromiumInventoryBytes) === BROWSER_INVENTORY
      && parseLocalQaLinuxC2cChromiumInventory(chromiumInventoryBytes).files.length === browser.files);
    const pgCapsule = expectedLocalQaLinuxC2cPgCapsule(run);
    const { capsuleRoot, archiveSha256, manifestSha256 } = pgCapsule;
    need(exact(postgres, { ...pending, run, capsuleRoot, archiveSha256, manifestSha256,
      manifestPath: `${capsuleRoot}/files.json`, files: 1640, directories: 150,
      payloadBytes: 47_718_773, installed: true }));
    const nodePath = `${browserRoot}/receipt-journey-node/toolchain/bin/node`;
    const codePlan = {
      codeInventorySha256: CODE_INVENTORY, node: { path: nodePath, sha256: NODE_HASH }, run,
      schema: "bluey-jobs-c2c-actor-plan-v1", sourceInventorySha256: SOURCE_INVENTORY, sourceTree: SOURCE_TREE,
    };
    // Match the existing Python plan writer's sorted compact wire exactly.
    need(Buffer.isBuffer(codePlanBytes) && wire(codePlan).equals(codePlanBytes));
    const { apiPort, postgresPort, smtpPort, objectPort, browserPort } = ports;
    need(exact(ports, { apiPort, postgresPort, smtpPort, objectPort, browserPort }));
    const value = {
      version: 1, sourceTree: SOURCE_TREE, sourceInventorySha256: SOURCE_INVENTORY,
      binding: { root, apiPort, postgresPort, smtpPort, objectPort, unitCgroup, networkNamespace, pgCapsule,
        buildBinding: { version: 1, qualificationOnly: true, artifactPath: apiPath, artifactSha256: API_HASH,
          manifestFile: portalManifestPath, manifestSha256: APP_PINS.portalManifestSha256, sourceSha256: SOURCE_INVENTORY },
        pidfdToolchain: { python,
          helper: { path: `/run/bluey-jobs-c2c-code/${run}/local-qa-linux-postmaster-pidfd.py`, sha256: PIDFD_HASH } } },
      fixture: { artifactPath: `${root}/fixtures/jobs-nonprod-browser-fixture`, artifactSha256: FIXTURE_HASH },
      unit: { bootId, invocationId, mainPid, unitName, systemctl: { path: "/usr/bin/systemctl", sha256: systemctlSha256 } },
      browser: { port: browserPort, webDirectory, closureRoot: browserRoot, inventoryFile,
        inventorySha256: BROWSER_INVENTORY },
    };
    const specBytes = wire(value), specSha256 = sha(specBytes);
    parseLocalQaLinuxC2cNativeSpec(specBytes, specSha256);
    // The protected manager copies the measured application input into the
    // actor's private run root before it issues a release. This is a plan,
    // not an assertion that that copy already exists.
    const fixtureStaging = Object.freeze({ sourcePath: application.fixturePath,
      actorPath: value.fixture.artifactPath, sha256: FIXTURE_HASH });
    return Object.freeze({ specBytes, specSha256, fixtureStaging,
      codePlanSha256: sha(codePlanBytes), runtimeQualified: false });
  } catch {
    throw new Error(REFUSED);
  }
}
