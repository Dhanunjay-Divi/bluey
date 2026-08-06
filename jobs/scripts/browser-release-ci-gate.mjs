import {
  createHash,
  createPublicKey,
  verify,
} from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  lstat,
  mkdir,
  open,
  readFile,
  readlink,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { createReadStream } from "node:fs";
import { basename, join, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";

export const BROWSER_RELEASE_TARGETS = Object.freeze({
  "darwin-arm64": Object.freeze({
    target: "darwin-arm64",
    platform: "darwin",
    architecture: "arm64",
    nativeSignatureKind: "apple-developer-id",
    packages: Object.freeze([
      Object.freeze({ extension: ".dmg", packageKind: "darwin-dmg" }),
      Object.freeze({ extension: ".zip", packageKind: "darwin-zip" }),
    ]),
  }),
  "darwin-x64": Object.freeze({
    target: "darwin-x64",
    platform: "darwin",
    architecture: "x64",
    nativeSignatureKind: "apple-developer-id",
    packages: Object.freeze([
      Object.freeze({ extension: ".dmg", packageKind: "darwin-dmg" }),
      Object.freeze({ extension: ".zip", packageKind: "darwin-zip" }),
    ]),
  }),
  "windows-x64": Object.freeze({
    target: "windows-x64",
    platform: "windows",
    architecture: "x64",
    nativeSignatureKind: "microsoft-authenticode",
    packages: Object.freeze([
      Object.freeze({ extension: ".exe", packageKind: "windows-nsis" }),
    ]),
  }),
});

export const BROWSER_PRODUCTION_CANARY_CHECK_IDS = Object.freeze([
  "darwin-arm64-clean-install",
  "darwin-arm64-protocol-claim",
  "darwin-arm64-upgrade-install",
  "darwin-arm64-rollback-install",
  "darwin-x64-clean-install",
  "darwin-x64-protocol-claim",
  "darwin-x64-upgrade-install",
  "darwin-x64-rollback-install",
  "immutable-artifact-readback",
  "portal-download-authority",
  "windows-x64-clean-install",
  "windows-x64-protocol-claim",
  "windows-x64-upgrade-install",
  "windows-x64-rollback-install",
].sort());

const TARGET_NAMES = Object.freeze(Object.keys(BROWSER_RELEASE_TARGETS));
const CONTENT_AUDIENCE = "bluey-jobs-browser-app-content-inventory-v1";
const NATIVE_AUDIENCE = "bluey-jobs-browser-native-verification-v1";
const PACKAGE_SEAL_AUDIENCE = "bluey-jobs-browser-package-seal-v1";
const EVIDENCE_AUDIENCE = "bluey-jobs-browser-verification-evidence-v1";
const PART_AUDIENCE = "bluey-jobs-browser-candidate-part-v1";
const CANDIDATE_AUDIENCE = "bluey-jobs-browser-candidate-set-v1";
const AUTHORIZATION_AUDIENCE =
  "bluey-jobs-browser-release-authorization-set-v1";
const PROMOTION_AUDIENCE = "bluey-jobs-browser-promotion-set-v1";
const CANARY_AUDIENCE = "bluey-jobs-browser-canary-evidence-v1";
const BUILD_AUDIENCE = "bluey-jobs-browser-build-v1";
const MANIFEST_AUDIENCE = "bluey-jobs-browser-release-manifest-v1";
const ACTIVATION_AUDIENCE = "bluey-jobs-browser-release-activation-v1";
const TRUST_POLICY_AUDIENCE =
  "bluey-jobs-browser-release-trust-policy-v1";
const SIGNATURE_SET_AUDIENCE =
  "bluey-jobs-browser-release-signature-set-v1";
const BUILD_KEYRING_AUDIENCE = "bluey-jobs-browser-build-keyring-v1";
const HEX_64 = /^[0-9a-f]{64}$/;
const SAFE_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const MUTABLE_RELEASE_IDS = new Set([
  "beta",
  "current",
  "download",
  "internal",
  "latest",
  "stable",
]);
const BUILD_ID = /^browser-(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;
const SEMVER = /^(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/;
const BASE64URL = /^[A-Za-z0-9_-]+$/;
const PRIMARY_PACKAGE = /\.(?:dmg|zip|exe|AppImage)$/;
const MAX_SAFE_GENERATION = 9_007_199_254_740_991;
const MAX_JSON_BYTES = 16 * 1024 * 1024;
const MAX_TRANSCRIPT_BYTES = 1024 * 1024;
const MAX_INVENTORY_ENTRIES = 40_000;
const MAX_ASAR_ENTRIES = 40_000;
const MAX_ASAR_REQUIRED_FILE_BYTES = 16 * 1024 * 1024;
const REQUIRED_ASAR_FILES = Object.freeze([
  "dist/app-lifecycle.js",
  "dist/main.js",
  "node_modules/@bluey/jobs-automation/dist/index.js",
  "node_modules/@bluey/jobs-automation/package.json",
  "package.json",
]);
const RUNTIME_BROWSER_ASSETS = new Set([
  "assets/icon-128.png",
  "assets/icon-512.png",
  "assets/trayTemplate.png",
  "assets/trayTemplate@2x.png",
]);
const AUTHORITY_FILES = Object.freeze([
  "build-descriptor.sig",
  "build-descriptor.txt",
  "build-public-keys.json",
]);
const PART_FILES = Object.freeze([
  "app-content-inventory.json",
  ...AUTHORITY_FILES,
  "native-verification-transcript.txt",
  "native-verification.json",
  "package-seal.json",
  "target-record.json",
  "verification-evidence.json",
  "artifacts",
].sort());
const CANDIDATE_FILES = Object.freeze([
  "candidate-set.json",
  "release-manifest.json",
  "release-trust-policy.json",
  "targets",
].sort());
const AUTHORIZATION_FILES = Object.freeze([
  "authorization-set.json",
  "candidate",
  "release-manifest-signatures.json",
].sort());
const PROMOTION_FILES = Object.freeze([
  "authorized-candidate",
  "canary-evidence.json",
  "promotion-set.json",
  "release-activation.json",
  "release-activation-signatures.json",
].sort());

export class BrowserReleaseGateError extends Error {
  constructor(message = "Bluey Browser release gate rejected the candidate") {
    super(message);
    this.name = "BrowserReleaseGateError";
  }
}

export function requireReleaseTarget(targetName) {
  const target = BROWSER_RELEASE_TARGETS[targetName];
  if (!target) {
    throw new BrowserReleaseGateError(
      "The release target must be exactly darwin-arm64, darwin-x64, or windows-x64",
    );
  }
  return target;
}

export function requireWorkflowCredentials(
  targetName,
  environment = process.env,
) {
  const target = requireReleaseTarget(targetName);
  for (const name of [
    "BLUEY_BROWSER_BUILD_PRIVATE_KEY_FILE",
    "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_FILE",
    "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256",
  ]) {
    requireNonempty(environment[name], `Missing ${name}`);
  }
  if (target.platform === "darwin") {
    for (const name of [
      "CSC_LINK",
      "CSC_KEY_PASSWORD",
      "APPLE_API_KEY",
      "APPLE_API_KEY_ID",
      "APPLE_API_ISSUER",
    ]) {
      requireNonempty(environment[name], `Missing ${name}`);
    }
    for (const forbidden of [
      "APPLE_ID",
      "APPLE_APP_SPECIFIC_PASSWORD",
      "APPLE_TEAM_ID",
      "APPLE_KEYCHAIN_PROFILE",
    ]) {
      if (nonempty(environment[forbidden])) {
        throw new BrowserReleaseGateError(
          "The Browser release workflow requires one unambiguous notarization mode",
        );
      }
    }
  } else {
    for (const name of ["WIN_CSC_LINK", "WIN_CSC_KEY_PASSWORD"]) {
      requireNonempty(environment[name], `Missing ${name}`);
    }
    if (nonempty(environment.CSC_LINK)) {
      throw new BrowserReleaseGateError(
        "The Browser Windows signing credential must be unambiguous",
      );
    }
  }
  requireNonempty(
    environment.BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY,
    "Missing BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY",
  );
  return target;
}

export async function locatePackagedResources(releaseDirectory, targetName) {
  requireReleaseTarget(targetName);
  const root = await requireDirectory(releaseDirectory);
  const matches = [];
  await visitDirectories(root, async (path) => {
    const names = await safeDirectoryNames(path);
    if (
      names.includes("app.asar") &&
      names.includes("playwright") &&
      names.includes("release")
    ) {
      matches.push(path);
    }
  });
  if (matches.length !== 1) {
    throw new BrowserReleaseGateError(
      "The packaged output must expose exactly one app resources directory",
    );
  }
  return matches[0];
}

export async function inspectPackagedAppAsar(appAsarPath) {
  const archivePath = resolve(appAsarPath);
  const archive = await requireFile(archivePath);
  if (archive.size < 1 || archive.size > MAX_JSON_BYTES * 64) {
    throw new BrowserReleaseGateError("Invalid packaged app.asar size");
  }
  const { extractFile, getRawHeader, statFile } = await import("@electron/asar");
  let listed;
  try {
    listed = canonicalAsarHeaderPaths(getRawHeader(archivePath).header);
  } catch {
    throw new BrowserReleaseGateError("The packaged app.asar cannot be inspected");
  }
  if (!Array.isArray(listed) || listed.length < 3 || listed.length > MAX_ASAR_ENTRIES) {
    throw new BrowserReleaseGateError("The packaged app.asar entry set is invalid");
  }
  const paths = [];
  const collisionKeys = new Set();
  const filePaths = new Set();
  const fileStats = new Map();
  for (const path of listed) {
    requireSafeRelativePath(path);
    if (path.normalize("NFC") !== path || /[\u0000-\u001f\u007f\ufffd]/.test(path)) {
      throw new BrowserReleaseGateError("The packaged app.asar path is not canonical");
    }
    const collisionKey = path.toLowerCase();
    if (collisionKeys.has(collisionKey)) {
      throw new BrowserReleaseGateError("The packaged app.asar contains a path collision");
    }
    collisionKeys.add(collisionKey);
    const stat = statFile(archivePath, path, false);
    if (stat.link) {
      throw new BrowserReleaseGateError("The packaged app.asar may not contain links");
    }
    const isDirectory = stat.files && typeof stat.files === "object";
    if (!isDirectory) {
      if (!Number.isSafeInteger(stat.size) || stat.size < 0) {
        throw new BrowserReleaseGateError("The packaged app.asar file size is invalid");
      }
      filePaths.add(path);
      fileStats.set(path, stat);
    }
    validatePackagedAsarPath(path, isDirectory);
    paths.push(path);
  }
  paths.sort();
  for (const required of REQUIRED_ASAR_FILES) {
    const stat = fileStats.get(required);
    if (
      !filePaths.has(required) ||
      !stat ||
      stat.unpacked === true ||
      stat.size < 1 ||
      stat.size > MAX_ASAR_REQUIRED_FILE_BYTES
    ) {
      throw new BrowserReleaseGateError(
        `The packaged app.asar is missing packed runtime file ${required}`,
      );
    }
  }
  let browserMain;
  let browserLifecycle;
  let automationMain;
  let automationPackage;
  let browserPackage;
  try {
    browserMain = extractFile(archivePath, "dist/main.js", false);
    browserLifecycle = extractFile(archivePath, "dist/app-lifecycle.js", false);
    automationMain = extractFile(
      archivePath,
      "node_modules/@bluey/jobs-automation/dist/index.js",
      false,
    );
    automationPackage = JSON.parse(
      extractFile(
        archivePath,
        "node_modules/@bluey/jobs-automation/package.json",
        false,
      ).toString("utf8"),
    );
    browserPackage = JSON.parse(
      extractFile(archivePath, "package.json", false).toString("utf8"),
    );
  } catch {
    throw new BrowserReleaseGateError("The packaged app.asar runtime cannot be opened");
  }
  if (
    browserMain.length < 1 ||
    browserMain.length > MAX_ASAR_REQUIRED_FILE_BYTES ||
    browserLifecycle.length < 1 ||
    browserLifecycle.length > MAX_ASAR_REQUIRED_FILE_BYTES ||
    automationMain.length < 1 ||
    automationMain.length > MAX_ASAR_REQUIRED_FILE_BYTES ||
    !browserPackage ||
    typeof browserPackage !== "object" ||
    Array.isArray(browserPackage) ||
    browserPackage.name !== "@bluey/jobs-browser" ||
    browserPackage.main !== "dist/main.js" ||
    Object.hasOwn(browserPackage, "build") ||
    !automationPackage ||
    typeof automationPackage !== "object" ||
    Array.isArray(automationPackage) ||
    automationPackage.name !== "@bluey/jobs-automation" ||
    automationPackage.type !== "module" ||
    automationPackage.main !== "dist/index.js" ||
    typeof automationPackage.version !== "string" ||
    !SEMVER.test(automationPackage.version) ||
    !browserPackage.dependencies ||
    typeof browserPackage.dependencies !== "object" ||
    Array.isArray(browserPackage.dependencies) ||
    browserPackage.dependencies?.["@bluey/jobs-automation"] !==
      automationPackage.version ||
    automationPackage.exports?.["."]?.import !== "./dist/index.js" ||
    !browserMain.toString("utf8").includes('from "./app-lifecycle.js"') ||
    !browserLifecycle
      .toString("utf8")
      .includes('app.setAsDefaultProtocolClient("bluey-jobs")')
  ) {
    throw new BrowserReleaseGateError("The packaged app.asar runtime contract is invalid");
  }
  return Object.freeze({
    entryCount: paths.length,
    entriesSha256: sha256(canonicalJsonBytes(paths)),
    browserMainSha256: sha256(browserMain),
    automationMainSha256: sha256(automationMain),
    protocolRegistrationSha256: sha256(browserLifecycle),
  });
}

export function canonicalAsarHeaderPaths(header) {
  if (!header || typeof header !== "object" || Array.isArray(header)) {
    throw new BrowserReleaseGateError("The packaged app.asar header is invalid");
  }
  const paths = [];
  function walk(files, prefix) {
    if (!files || typeof files !== "object" || Array.isArray(files)) {
      throw new BrowserReleaseGateError("The packaged app.asar directory is invalid");
    }
    for (const [name, entry] of Object.entries(files)) {
      if (
        !name ||
        name === "." ||
        name === ".." ||
        name.includes("/") ||
        name.includes("\\") ||
        name.normalize("NFC") !== name ||
        /[\u0000-\u001f\u007f\ufffd]/.test(name) ||
        !entry ||
        typeof entry !== "object" ||
        Array.isArray(entry)
      ) {
        throw new BrowserReleaseGateError("The packaged app.asar header path is invalid");
      }
      const path = prefix ? `${prefix}/${name}` : name;
      paths.push(path);
      if (paths.length > MAX_ASAR_ENTRIES) {
        throw new BrowserReleaseGateError("The packaged app.asar has too many entries");
      }
      if (entry.files !== undefined) walk(entry.files, path);
    }
  }
  walk(header.files, "");
  return paths;
}

function validatePackagedAsarPath(path, isDirectory) {
  const parts = path.split("/");
  const root = parts[0];
  const basename = parts.at(-1).toLowerCase();
  const lowerParts = parts.map((part) => part.toLowerCase());
  if (
    !["assets", "dist", "node_modules", "package.json"].includes(root) ||
    (!isDirectory && path.endsWith(".map")) ||
    basename === ".env" ||
    basename.startsWith(".env.") ||
    [
      "build-descriptor.sig",
      "build-descriptor.txt",
      "build-public-keys.json",
    ].includes(basename) ||
    lowerParts.includes("release-authority") ||
    lowerParts.some((part) => /^chromium-[0-9]+$/.test(part)) ||
    (!isDirectory && /\.(?:key|p12|pem|pfx)$/i.test(path)) ||
    lowerParts.some((part) => ["__tests__", "fixtures", "test", "tests"].includes(part)) ||
    (!isDirectory && /(?:^|\/)(?:fixtures|[^/]+\.(?:spec|test))\.(?:cjs|js|mjs)$/i.test(path))
  ) {
    throw new BrowserReleaseGateError(
      "The packaged app.asar contains excluded source, test, map, credential, or authority data",
    );
  }
  if (root === "package.json") {
    if (isDirectory || parts.length !== 1) {
      throw new BrowserReleaseGateError("The packaged app.asar has invalid package metadata");
    }
    return;
  }
  if (root === "dist") {
    if (!isDirectory && !/\.(?:cjs|css|html|js)$/.test(path)) {
      throw new BrowserReleaseGateError("The packaged app.asar has extra Browser sources");
    }
    return;
  }
  if (root === "assets") {
    if (!isDirectory && !RUNTIME_BROWSER_ASSETS.has(path)) {
      throw new BrowserReleaseGateError("The packaged app.asar has extra Browser assets");
    }
    return;
  }
  if (parts.length === 1 || parts[1] !== "@bluey") return;
  if (parts.length === 2) {
    if (!isDirectory) {
      throw new BrowserReleaseGateError("The packaged app.asar has invalid Bluey scope");
    }
    return;
  }
  if (parts[2] !== "jobs-automation") {
    throw new BrowserReleaseGateError("The packaged app.asar has an extra Bluey package");
  }
  const automationPath = parts.slice(3).join("/");
  const allowedDirectory =
    isDirectory &&
    (
      ["", "assets", "assets/fonts", "assets/licenses", "dist"].includes(
        automationPath,
      ) || automationPath.startsWith("dist/")
    );
  const allowedFile =
    !isDirectory &&
    (
      automationPath === "package.json" ||
      automationPath === "THIRD_PARTY_NOTICES.md" ||
      /^dist\/(?:[^/]+\/)*[^/]+\.js$/.test(automationPath) ||
      /^assets\/(?:fonts|licenses)\/[^/]+\.(?:ttf|txt)$/.test(automationPath)
    );
  if (!allowedDirectory && !allowedFile) {
    throw new BrowserReleaseGateError(
      "The packaged app.asar contains Bluey source, declarations, config, or extra files",
    );
  }
}

export async function createAppContentInventory({
  targetName,
  resourcesDirectory,
  authorityDirectory,
  packageSealSha256,
  outputPath,
}) {
  const target = requireReleaseTarget(targetName);
  const resourcesRoot = await requireDirectory(resourcesDirectory);
  const authorityRoot = await requireDirectory(authorityDirectory);
  const authority = await readBuildAuthority(authorityRoot, target);
  requirePattern(packageSealSha256, HEX_64, "Invalid package-seal digest");
  const entries = await inventoryDirectory(resourcesRoot);
  const byPath = new Map(entries.map((entry) => [entry.path, entry]));
  const appAsar = byPath.get("app.asar");
  if (!appAsar || appAsar.type !== "file" || appAsar.sizeBytes < 1) {
    throw new BrowserReleaseGateError("The packaged app.asar is missing");
  }
  const asarContract = await inspectPackagedAppAsar(
    join(resourcesRoot, "app.asar"),
  );
  const releaseEntries = entries
    .filter((entry) => entry.path === "release" || entry.path.startsWith("release/"))
    .map((entry) => `${entry.type}:${entry.path}`)
    .sort();
  const expectedReleaseEntries = [
    "directory:release",
    ...AUTHORITY_FILES.map((filename) => `file:release/${filename}`),
  ].sort();
  if (releaseEntries.join("\n") !== expectedReleaseEntries.join("\n")) {
    throw new BrowserReleaseGateError(
      "The packaged build authority contains a stale, missing, or extra resource",
    );
  }

  for (const filename of AUTHORITY_FILES) {
    const embeddedPath = `release/${filename}`;
    const embedded = byPath.get(embeddedPath);
    if (!embedded || embedded.type !== "file") {
      throw new BrowserReleaseGateError(
        "The packaged build authority resources are incomplete",
      );
    }
    const [left, right] = await Promise.all([
      readFile(join(resourcesRoot, ...embeddedPath.split("/"))),
      readFile(join(authorityRoot, filename)),
    ]);
    if (!left.equals(right)) {
      throw new BrowserReleaseGateError(
        "The packaged build authority differs from the verified source",
      );
    }
  }

  const chromiumCandidates = entries.filter(
    (entry) =>
      entry.type === "file" &&
      entry.path.startsWith("playwright/") &&
      (target.platform === "darwin"
        ? entry.path.endsWith("/Google Chrome for Testing")
        : entry.path.endsWith("/chrome.exe")),
  );
  if (chromiumCandidates.length !== 1) {
    throw new BrowserReleaseGateError(
      "The packaged app must contain exactly one headed Chromium executable",
    );
  }
  const chromium = chromiumCandidates[0];
  if (!chromium.path.includes(`chromium-${authority.descriptor.chromiumRevision}/`)) {
    throw new BrowserReleaseGateError(
      "The packaged Chromium revision does not match the descriptor",
    );
  }
  const chromiumBundleRoot =
    `playwright/chromium-${authority.descriptor.chromiumRevision}`;
  if (
    entries.some(
      (entry) =>
        entry.path.startsWith("playwright/") &&
        entry.path !== chromiumBundleRoot &&
        !entry.path.startsWith(`${chromiumBundleRoot}/`),
    )
  ) {
    throw new BrowserReleaseGateError(
      "The packaged app contains an extra Chromium revision or browser payload",
    );
  }
  await validateExecutableArchitecture(
    join(resourcesRoot, ...chromium.path.split("/")),
    target,
  );

  const inventory = Object.freeze({
    version: 1,
    audience: CONTENT_AUDIENCE,
    target: target.target,
    platform: target.platform,
    architecture: target.architecture,
    descriptorSha256: authority.descriptorSha256,
    packageSealSha256,
    appId: authority.descriptor.appId,
    protocolScheme: "bluey-jobs",
    protocolRegistrationSha256: asarContract.protocolRegistrationSha256,
    appAsarSha256: appAsar.sha256,
    appAsarEntryCount: asarContract.entryCount,
    appAsarEntriesSha256: asarContract.entriesSha256,
    browserMainSha256: asarContract.browserMainSha256,
    automationMainSha256: asarContract.automationMainSha256,
    chromiumRevision: authority.descriptor.chromiumRevision,
    chromiumExecutablePath: chromium.path,
    chromiumExecutableSha256: chromium.sha256,
    entries: Object.freeze(entries),
  });
  await writeCanonicalJsonExclusive(outputPath, inventory);
  return inventory;
}

export async function recordNativeVerification({
  targetName,
  signerIdentity,
  transcriptPath,
  packageSealSha256,
  toolVersions,
  outputPath,
}) {
  const target = requireReleaseTarget(targetName);
  requireBoundedString(signerIdentity, 3, 256, "Invalid native signer identity");
  requirePattern(packageSealSha256, HEX_64, "Invalid package-seal digest");
  const transcript = await readBoundedFile(
    transcriptPath,
    MAX_TRANSCRIPT_BYTES,
    "Invalid native verification transcript",
  );
  const versions = [...toolVersions]
    .map(parseToolVersion)
    .sort((left, right) => left.name.localeCompare(right.name));
  if (
    versions.length < 2 ||
    new Set(versions.map((entry) => entry.name)).size !== versions.length
  ) {
    throw new BrowserReleaseGateError(
      "Native verification must record an exact toolchain",
    );
  }
  const sealMarker = `package_seal_sha256=${packageSealSha256}`;
  const transcriptLines = transcript.toString("utf8").split(/\r?\n/);
  const requiredMarkers = target.platform === "darwin"
    ? [
      sealMarker,
      "app_asar_runtime_contract_verified=true",
      "dmg_app_content_matches_archive=true",
      "dmg_bundle_metadata_matches_archive=true",
      "dmg_mounted_application_codesign_verified=true",
      "dmg_mounted_application_gatekeeper_verified=true",
      "dmg_mounted_application_staple_verified=true",
      "macos_application_codesign_verified=true",
      "macos_application_gatekeeper_verified=true",
      "macos_application_staple_verified=true",
      "macos_bundle_identity_verified=true",
      "macos_dmg_codesign_verified=true",
      "macos_dmg_gatekeeper_verified=true",
      "macos_dmg_integrity_verified=true",
      "macos_dmg_staple_verified=true",
      "macos_notarization_verified=true",
      "macos_protocol_registration_verified=true",
    ]
    : [
      sealMarker,
      "app_asar_runtime_contract_verified=true",
      "installer_payload_inventory_verified=true",
      "outer_installer_authenticode_verified=true",
      "outer_installer_timestamp_verified=true",
      "payload_application_authenticode_verified=true",
      "payload_application_timestamp_verified=true",
      "windows_protocol_construction_verified=true",
    ];
  if (requiredMarkers.some((marker) => !transcriptLines.includes(marker))) {
    throw new BrowserReleaseGateError(
      "Native verification transcript does not bind the package seal and runtime contract",
    );
  }
  const native = Object.freeze({
    version: 1,
    audience: NATIVE_AUDIENCE,
    target: target.target,
    nativeSignatureKind: target.nativeSignatureKind,
    signerIdentity,
    packageSealSha256,
    codeSigningVerified: target.platform === "darwin",
    notarizationVerified: target.platform === "darwin",
    stapleVerified: target.platform === "darwin",
    authenticodeVerified: target.platform === "windows",
    timestampVerified: target.platform === "windows",
    transcriptSha256: sha256(transcript),
    toolVersions: Object.freeze(versions),
  });
  validateNativeVerification(native, target);
  await writeCanonicalJsonExclusive(outputPath, native);
  return native;
}

export async function sealCandidateTarget({
  targetName,
  releaseDirectory,
  authorityDirectory,
  sourceCommit,
  outputDirectory,
}) {
  const target = requireReleaseTarget(targetName);
  requirePattern(sourceCommit, SOURCE_COMMIT, "Invalid source commit");
  const releaseRoot = await requireDirectory(releaseDirectory);
  const authorityRoot = await requireDirectory(authorityDirectory);
  const authority = await readBuildAuthority(authorityRoot, target);
  if (authority.descriptor.sourceCommit !== sourceCommit) {
    throw new BrowserReleaseGateError(
      "The build descriptor source commit does not match the package seal",
    );
  }
  const artifacts = await validateReleaseOutputs(
    releaseRoot,
    target,
    authority.descriptor.appVersion,
  );
  const seal = Object.freeze({
    version: 1,
    audience: PACKAGE_SEAL_AUDIENCE,
    target: target.target,
    sourceCommit,
    releaseId: authority.descriptor.releaseId,
    buildId: authority.descriptor.buildId,
    appVersion: authority.descriptor.appVersion,
    descriptorSha256: authority.descriptorSha256,
    artifacts: Object.freeze(
      artifacts.map(({ filename, packageKind, sizeBytes, sha256: digest }) =>
        Object.freeze({ filename, packageKind, sizeBytes, sha256: digest }),
      ),
    ),
  });
  const outputRoot = resolve(outputDirectory);
  await mkdirExclusive(outputRoot);
  try {
    await mkdir(join(outputRoot, "artifacts"), { mode: 0o700 });
    await Promise.all([
      ...AUTHORITY_FILES.map((filename) =>
        copyFile(join(authorityRoot, filename), join(outputRoot, filename)),
      ),
      ...artifacts.map((artifact) =>
        copyFile(
          join(releaseRoot, artifact.filename),
          join(outputRoot, "artifacts", artifact.filename),
        ),
      ),
    ]);
    await writeCanonicalJsonExclusive(join(outputRoot, "package-seal.json"), seal);
    const validated = await validatePackageSeal(outputRoot, true);
    await Promise.all([
      chmod(join(outputRoot, "package-seal.json"), 0o444),
      ...AUTHORITY_FILES.map((filename) => chmod(join(outputRoot, filename), 0o444)),
      ...artifacts.map((artifact) =>
        chmod(join(outputRoot, "artifacts", artifact.filename), 0o444),
      ),
    ]);
    return validated;
  } catch (error) {
    await rm(outputRoot, { recursive: true, force: true });
    throw error;
  }
}

export async function validateSealedCandidateTarget({
  targetName,
  sealDirectory,
  sourceCommit,
}) {
  const target = requireReleaseTarget(targetName);
  requirePattern(sourceCommit, SOURCE_COMMIT, "Invalid source commit");
  const sealed = await validatePackageSeal(sealDirectory, true);
  if (
    sealed.authority.descriptor.platform !== target.platform ||
    sealed.authority.descriptor.architecture !== target.architecture ||
    sealed.authority.descriptor.sourceCommit !== sourceCommit
  ) {
    throw new BrowserReleaseGateError(
      "The package seal does not match the requested immutable target",
    );
  }
  return sealed;
}

export async function collectCandidateTarget({
  targetName,
  sealDirectory,
  inventoryPath,
  nativeVerificationPath,
  transcriptPath,
  sourceCommit,
  outputDirectory,
}) {
  const target = requireReleaseTarget(targetName);
  requirePattern(sourceCommit, SOURCE_COMMIT, "Invalid source commit");
  const sealed = await validatePackageSeal(sealDirectory, true);
  const { authority, authorityRoot, releaseRoot } = sealed;
  if (authority.descriptor.sourceCommit !== sourceCommit) {
    throw new BrowserReleaseGateError(
      "The build descriptor source commit does not match the candidate",
    );
  }
  const inventoryBytes = await readCanonicalJsonBytes(inventoryPath);
  const inventory = inventoryBytes.value;
  validateContentInventory(inventory, target, authority.descriptorSha256);
  if (inventory.packageSealSha256 !== sealed.packageSealSha256) {
    throw new BrowserReleaseGateError("The app inventory does not match the package seal");
  }
  const nativeBytes = await readCanonicalJsonBytes(nativeVerificationPath);
  const native = nativeBytes.value;
  validateNativeVerification(native, target);
  if (native.packageSealSha256 !== sealed.packageSealSha256) {
    throw new BrowserReleaseGateError("Native verification does not match the package seal");
  }
  const transcript = await readBoundedFile(
    transcriptPath,
    MAX_TRANSCRIPT_BYTES,
    "Invalid native verification transcript",
  );
  if (native.transcriptSha256 !== sha256(transcript)) {
    throw new BrowserReleaseGateError(
      "The native verification transcript does not match its evidence",
    );
  }

  const artifacts = sealed.artifacts;
  const evidence = Object.freeze({
    version: 1,
    audience: EVIDENCE_AUDIENCE,
    target: target.target,
    sourceCommit,
    descriptorSha256: authority.descriptorSha256,
    packageSealSha256: sealed.packageSealSha256,
    appContentSha256: sha256(inventoryBytes.bytes),
    appAsarSha256: inventory.appAsarSha256,
    chromiumRevision: inventory.chromiumRevision,
    chromiumExecutableSha256: inventory.chromiumExecutableSha256,
    artifacts: Object.freeze(
      artifacts.map(({ filename, packageKind, sizeBytes, sha256: digest }) =>
        Object.freeze({ filename, packageKind, sizeBytes, sha256: digest }),
      ),
    ),
    nativeVerification: native,
  });
  const evidenceBytes = canonicalJsonBytes(evidence);
  const verificationEvidenceSha256 = sha256(evidenceBytes);
  const record = Object.freeze({
    version: 1,
    audience: PART_AUDIENCE,
    target: target.target,
    platform: target.platform,
    architecture: target.architecture,
    releaseId: authority.descriptor.releaseId,
    buildId: authority.descriptor.buildId,
    appVersion: authority.descriptor.appVersion,
    protocolVersion: authority.descriptor.protocolVersion,
    sourceCommit: authority.descriptor.sourceCommit,
    electronVersion: authority.descriptor.electronVersion,
    playwrightVersion: authority.descriptor.playwrightVersion,
    chromiumRevision: authority.descriptor.chromiumRevision,
    descriptorSha256: authority.descriptorSha256,
    packageSealSha256: sealed.packageSealSha256,
    appContentSha256: evidence.appContentSha256,
    verificationEvidenceSha256,
    nativeSignatureKind: target.nativeSignatureKind,
    nativeSignerIdentity: native.signerIdentity,
    artifacts: Object.freeze(
      artifacts.map(({ filename, packageKind, sizeBytes, sha256: digest }) =>
        Object.freeze({
          filename,
          packageKind,
          sizeBytes,
          sha256: digest,
          relativePath: `artifacts/${filename}`,
        }),
      ),
    ),
  });

  const outputRoot = resolve(outputDirectory);
  await mkdirExclusive(outputRoot);
  try {
    await mkdir(join(outputRoot, "artifacts"), { mode: 0o755 });
    await Promise.all([
      ...AUTHORITY_FILES.map((filename) =>
        copyFile(join(authorityRoot, filename), join(outputRoot, filename)),
      ),
      copyFile(join(sealed.root, "package-seal.json"), join(outputRoot, "package-seal.json")),
      copyFile(inventoryPath, join(outputRoot, "app-content-inventory.json")),
      copyFile(
        nativeVerificationPath,
        join(outputRoot, "native-verification.json"),
      ),
      copyFile(
        transcriptPath,
        join(outputRoot, "native-verification-transcript.txt"),
      ),
      ...artifacts.map((artifact) =>
        copyFile(
          join(releaseRoot, artifact.filename),
          join(outputRoot, "artifacts", artifact.filename),
        ),
      ),
    ]);
    await writeCanonicalJsonExclusive(
      join(outputRoot, "verification-evidence.json"),
      evidence,
    );
    await writeCanonicalJsonExclusive(join(outputRoot, "target-record.json"), record);
    await validateCandidatePart(outputRoot);
    return record;
  } catch (error) {
    await rm(outputRoot, { recursive: true, force: true });
    throw error;
  }
}

export async function assembleCandidateSet({
  partsDirectory,
  outputDirectory,
  repository,
  candidateRunId,
  manifestId,
  manifestGeneration,
  releaseSequence,
  publishedAtMs,
  releaseNotesUrl,
  artifactBaseUrl,
  trustPolicyPath,
  trustPolicySha256,
}) {
  requireBoundedString(repository, 3, 256, "Invalid repository identity");
  requireSafeId(candidateRunId, "Invalid candidate run identity");
  requireSafeId(manifestId, "Invalid manifest identity");
  requirePositiveInteger(manifestGeneration, "Invalid manifest generation");
  requirePositiveInteger(releaseSequence, "Invalid release sequence");
  requireNonnegativeInteger(publishedAtMs, "Invalid publication time");
  requirePattern(trustPolicySha256, HEX_64, "Invalid trust-policy digest");

  const partsRoot = await requireDirectory(partsDirectory);
  const partDirectories = await discoverPartDirectories(partsRoot);
  const parts = await Promise.all(partDirectories.map(validateCandidatePart));
  parts.sort((left, right) => left.record.target.localeCompare(right.record.target));
  if (parts.map((part) => part.record.target).join("\n") !== [...TARGET_NAMES].sort().join("\n")) {
    throw new BrowserReleaseGateError(
      "A candidate set requires exactly all three supported release targets",
    );
  }
  const first = parts[0].record;
  for (const part of parts.slice(1)) {
    for (const field of [
      "releaseId",
      "buildId",
      "appVersion",
      "protocolVersion",
      "sourceCommit",
      "electronVersion",
      "playwrightVersion",
      "chromiumRevision",
    ]) {
      if (part.record[field] !== first[field]) {
        throw new BrowserReleaseGateError(
          "All target parts must come from one exact Browser build",
        );
      }
    }
  }
  const trustPolicyBytes = await readAuthorityJsonBytes(
    trustPolicyPath,
    parseTrustPolicy,
    64 * 1024,
    "Invalid canonical trust policy",
  );
  if (sha256(trustPolicyBytes.bytes) !== trustPolicySha256) {
    throw new BrowserReleaseGateError("The Browser trust policy is not approved");
  }
  const trustPolicy = trustPolicyBytes.value;
  const normalizedBaseUrl = requireArtifactBaseUrl(
    artifactBaseUrl,
    first.releaseId,
    trustPolicy.artifactOrigin,
  );
  const normalizedNotesUrl = requireImmutableReleaseUrl(
    releaseNotesUrl,
    first.releaseId,
    trustPolicy.artifactOrigin,
  );
  const manifestArtifacts = parts
    .flatMap((part) =>
      part.record.artifacts.map((artifact) =>
        Object.freeze({
          artifactId: `${first.releaseId}-${part.record.target}-${artifact.packageKind}`,
          platform: part.record.platform,
          architecture: part.record.architecture,
          packageKind: artifact.packageKind,
          buildDescriptorSha256: part.record.descriptorSha256,
          url: `${normalizedBaseUrl}/${artifact.filename}`,
          sizeBytes: artifact.sizeBytes,
          sha256: artifact.sha256,
          appContentSha256: part.record.appContentSha256,
          verificationEvidenceSha256: part.record.verificationEvidenceSha256,
          nativeSignatureKind: part.record.nativeSignatureKind,
          nativeSignerIdentity: part.record.nativeSignerIdentity,
        }),
      ),
    )
    .sort((left, right) => artifactIdentity(left).localeCompare(artifactIdentity(right)));

  const unsignedManifest = Object.freeze({
    version: 1,
    audience: MANIFEST_AUDIENCE,
    manifestId,
    manifestGeneration,
    releaseId: first.releaseId,
    releaseSequence,
    buildId: first.buildId,
    appVersion: first.appVersion,
    protocolVersion: first.protocolVersion,
    sourceCommit: first.sourceCommit,
    electronVersion: first.electronVersion,
    playwrightVersion: first.playwrightVersion,
    chromiumRevision: first.chromiumRevision,
    publishedAtMs,
    releaseNotesUrl: normalizedNotesUrl,
    artifacts: Object.freeze(manifestArtifacts),
  });
  const manifest = parseManifest(unsignedManifest);
  const manifestBytes = authorityJsonBytes(manifest);
  const manifestSha256 = sha256(manifestBytes);
  if (
    publishedAtMs < trustPolicy.validFromMs ||
    publishedAtMs >= trustPolicy.expiresAtMs
  ) {
    throw new BrowserReleaseGateError(
      "The manifest publication time is outside the approved trust policy",
    );
  }

  const outputRoot = resolve(outputDirectory);
  await mkdirExclusive(outputRoot);
  try {
    await mkdir(join(outputRoot, "targets"), { mode: 0o755 });
    for (const part of parts) {
      await cp(part.directory, join(outputRoot, "targets", part.record.target), {
        recursive: true,
        force: false,
        errorOnExist: true,
      });
    }
    await Promise.all([
      writeExclusive(join(outputRoot, "release-manifest.json"), manifestBytes),
      writeExclusive(
        join(outputRoot, "release-trust-policy.json"),
        trustPolicyBytes.bytes,
      ),
    ]);
    const targetRecords = [];
    for (const part of parts) {
      targetRecords.push(
        Object.freeze({
          target: part.record.target,
          recordSha256: await sha256File(
            join(outputRoot, "targets", part.record.target, "target-record.json"),
          ),
        }),
      );
    }
    const candidate = Object.freeze({
      version: 1,
      audience: CANDIDATE_AUDIENCE,
      repository,
      candidateRunId,
      sourceCommit: first.sourceCommit,
      releaseId: first.releaseId,
      buildId: first.buildId,
      manifestSha256,
      trustPolicySha256,
      trustGeneration: trustPolicy.trustGeneration,
      artifactBaseUrl: normalizedBaseUrl,
      readinessLabel: "native-verified-candidate",
      artifactCount: manifest.artifacts.length,
      targetRecords: Object.freeze(targetRecords),
    });
    await writeCanonicalJsonExclusive(join(outputRoot, "candidate-set.json"), candidate);
    await validateCandidateSet({
      candidateDirectory: outputRoot,
      expectedSourceCommit: first.sourceCommit,
      expectedReleaseId: first.releaseId,
      expectedManifestSha256: manifestSha256,
      expectedTrustPolicySha256: trustPolicySha256,
    });
    return Object.freeze({ candidate, manifest });
  } catch (error) {
    await rm(outputRoot, { recursive: true, force: true });
    throw error;
  }
}

export async function validateCandidateSet({
  candidateDirectory,
  expectedSourceCommit,
  expectedReleaseId,
  expectedManifestSha256,
  expectedTrustPolicySha256,
}) {
  const root = await requireDirectory(candidateDirectory);
  await requireExactDirectoryEntries(root, CANDIDATE_FILES);
  const candidateBytes = await readCanonicalJsonBytes(join(root, "candidate-set.json"));
  const candidate = candidateBytes.value;
  requireExactKeys(candidate, [
    "artifactBaseUrl",
    "artifactCount",
    "audience",
    "buildId",
    "candidateRunId",
    "manifestSha256",
    "readinessLabel",
    "releaseId",
    "repository",
    "sourceCommit",
    "targetRecords",
    "trustGeneration",
    "trustPolicySha256",
    "version",
  ]);
  if (candidate.version !== 1 || candidate.audience !== CANDIDATE_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid candidate set authority");
  }
  requireBoundedString(candidate.repository, 3, 256, "Invalid repository identity");
  requireSafeId(candidate.candidateRunId, "Invalid candidate run identity");
  requirePattern(candidate.sourceCommit, SOURCE_COMMIT, "Invalid candidate source commit");
  requireReleaseId(candidate.releaseId, "Invalid candidate release identity");
  requirePattern(candidate.buildId, BUILD_ID, "Invalid candidate build identity");
  requirePattern(candidate.manifestSha256, HEX_64, "Invalid candidate manifest digest");
  requirePattern(candidate.trustPolicySha256, HEX_64, "Invalid trust-policy digest");
  requirePositiveInteger(candidate.trustGeneration, "Invalid trust generation");
  requirePositiveInteger(candidate.artifactCount, "Invalid artifact count");
  if (
    candidate.readinessLabel !== "native-verified-candidate" ||
    candidate.artifactCount !== 5
  ) {
    throw new BrowserReleaseGateError(
      "An unsigned candidate may claim only exact native verification",
    );
  }
  if (
    expectedSourceCommit && candidate.sourceCommit !== expectedSourceCommit ||
    expectedReleaseId && candidate.releaseId !== expectedReleaseId ||
    expectedManifestSha256 && candidate.manifestSha256 !== expectedManifestSha256 ||
    expectedTrustPolicySha256 &&
      candidate.trustPolicySha256 !== expectedTrustPolicySha256
  ) {
    throw new BrowserReleaseGateError(
      "The downloaded candidate does not match the requested immutable authority",
    );
  }
  const policyBytes = await readAuthorityJsonBytes(
    join(root, "release-trust-policy.json"),
    parseTrustPolicy,
    64 * 1024,
    "Invalid canonical trust policy",
  );
  if (
    sha256(policyBytes.bytes) !== candidate.trustPolicySha256 ||
    policyBytes.value.trustGeneration !== candidate.trustGeneration
  ) {
    throw new BrowserReleaseGateError("The downloaded trust policy changed");
  }
  const manifestJsonPath = join(root, "release-manifest.json");
  const manifestBytes = await readAuthorityJsonBytes(
    manifestJsonPath,
    parseManifest,
    32 * 1024,
    "Invalid canonical release manifest",
  );
  const manifest = manifestBytes.value;
  if (sha256(manifestBytes.bytes) !== candidate.manifestSha256) {
    throw new BrowserReleaseGateError("The release manifest bytes changed");
  }
  if (
    manifest.sourceCommit !== candidate.sourceCommit ||
    manifest.releaseId !== candidate.releaseId ||
    manifest.buildId !== candidate.buildId ||
    requireArtifactBaseUrl(
      candidate.artifactBaseUrl,
      candidate.releaseId,
      policyBytes.value.artifactOrigin,
    ) !==
      candidate.artifactBaseUrl
  ) {
    throw new BrowserReleaseGateError("The candidate index and manifest disagree");
  }
  requireImmutableReleaseUrl(
    manifest.releaseNotesUrl,
    candidate.releaseId,
    policyBytes.value.artifactOrigin,
  );
  for (const artifact of manifest.artifacts) {
    requireArtifactPackageUrl(
      artifact.url,
      candidate.releaseId,
      artifact.packageKind,
      policyBytes.value.artifactOrigin,
    );
  }

  const targetsRoot = await requireDirectory(join(root, "targets"));
  await requireExactDirectoryEntries(targetsRoot, [...TARGET_NAMES].sort());
  const parts = [];
  for (const targetName of TARGET_NAMES) {
    parts.push(await validateCandidatePart(join(targetsRoot, targetName)));
  }
  const recordTargets = requireArray(candidate.targetRecords, 3, 3, "Invalid target records")
    .map((entry) => {
      requireExactKeys(entry, ["recordSha256", "target"]);
      requireReleaseTarget(entry.target);
      requirePattern(entry.recordSha256, HEX_64, "Invalid target record digest");
      return entry;
    })
    .sort((left, right) => left.target.localeCompare(right.target));
  if (recordTargets.map((entry) => entry.target).join("\n") !== [...TARGET_NAMES].sort().join("\n")) {
    throw new BrowserReleaseGateError("Candidate target records are incomplete");
  }
  for (const entry of recordTargets) {
    if (
      await sha256File(join(targetsRoot, entry.target, "target-record.json")) !==
      entry.recordSha256
    ) {
      throw new BrowserReleaseGateError("A target record changed after assembly");
    }
  }

  const expectedArtifacts = parts
    .flatMap((part) =>
      part.record.artifacts.map((artifact) => ({
        artifactId: `${candidate.releaseId}-${part.record.target}-${artifact.packageKind}`,
        platform: part.record.platform,
        architecture: part.record.architecture,
        packageKind: artifact.packageKind,
        buildDescriptorSha256: part.record.descriptorSha256,
        url: `${candidate.artifactBaseUrl}/${artifact.filename}`,
        sizeBytes: artifact.sizeBytes,
        sha256: artifact.sha256,
        appContentSha256: part.record.appContentSha256,
        verificationEvidenceSha256: part.record.verificationEvidenceSha256,
        nativeSignatureKind: part.record.nativeSignatureKind,
        nativeSignerIdentity: part.record.nativeSignerIdentity,
      })),
    )
    .sort((left, right) => artifactIdentity(left).localeCompare(artifactIdentity(right)));
  if (canonicalString(expectedArtifacts) !== canonicalString(manifest.artifacts)) {
    throw new BrowserReleaseGateError(
      "The signed manifest does not describe the exact stored target artifacts",
    );
  }
  if (manifest.artifacts.length !== 5) {
    throw new BrowserReleaseGateError(
      "The release manifest must contain exactly five native packages",
    );
  }
  return Object.freeze({
    candidate,
    candidateBytes: candidateBytes.bytes,
    directory: root,
    manifest,
    manifestBytes: manifestBytes.bytes,
    parts: Object.freeze(parts),
    policy: policyBytes.value,
    policyBytes: policyBytes.bytes,
  });
}

export async function authorizeCandidateSet({
  candidateDirectory,
  manifestSignatureSetPath,
  outputDirectory,
  authorizationRunId,
  expectedSourceCommit,
  expectedReleaseId,
  expectedManifestSha256,
  expectedTrustPolicySha256,
  expectedManifestSignatureSetSha256,
  verificationTimeMs,
}) {
  requireSafeId(authorizationRunId, "Invalid authorization run identity");
  const candidate = await validateCandidateSet({
    candidateDirectory,
    expectedSourceCommit,
    expectedReleaseId,
    expectedManifestSha256,
    expectedTrustPolicySha256,
  });
  const signatureSet = await readAuthorityJsonBytes(
    manifestSignatureSetPath,
    parseSignatureSet,
    32 * 1024,
    "Invalid canonical manifest signature set",
  );
  const signatureSetSha256 = sha256(signatureSet.bytes);
  if (
    expectedManifestSignatureSetSha256 &&
    signatureSetSha256 !== expectedManifestSignatureSetSha256
  ) {
    throw new BrowserReleaseGateError(
      "The manifest signature set does not match its immutable digest",
    );
  }
  verifyAuthoritySignatureSet({
    targetBytes: candidate.manifestBytes,
    signatureSet: signatureSet.value,
    policy: candidate.policy,
    requiredRole: "release",
    targetAudience: MANIFEST_AUDIENCE,
    targetIssuedAtMs: candidate.manifest.publishedAtMs,
    verificationTimeMs,
    allowRetiredHistoricalKeys: false,
  });

  const outputRoot = resolve(outputDirectory);
  await mkdirExclusive(outputRoot);
  try {
    await cp(candidate.directory, join(outputRoot, "candidate"), {
      recursive: true,
      force: false,
      errorOnExist: true,
    });
    await writeExclusive(
      join(outputRoot, "release-manifest-signatures.json"),
      signatureSet.bytes,
    );
    const authorization = Object.freeze({
      version: 1,
      audience: AUTHORIZATION_AUDIENCE,
      repository: candidate.candidate.repository,
      authorizationRunId,
      candidateRunId: candidate.candidate.candidateRunId,
      sourceCommit: candidate.candidate.sourceCommit,
      releaseId: candidate.candidate.releaseId,
      buildId: candidate.candidate.buildId,
      manifestSha256: candidate.candidate.manifestSha256,
      trustPolicySha256: candidate.candidate.trustPolicySha256,
      trustGeneration: candidate.policy.trustGeneration,
      manifestSignatureSetId: signatureSet.value.signatureSetId,
      manifestSignatureSetSha256: signatureSetSha256,
      signatureCount: signatureSet.value.signatures.length,
      readinessLabel: "threshold-release-authorized",
    });
    await writeCanonicalJsonExclusive(
      join(outputRoot, "authorization-set.json"),
      authorization,
    );
    return await validateAuthorizedCandidateSet({
      authorizedDirectory: outputRoot,
      expectedSourceCommit,
      expectedReleaseId,
      expectedManifestSha256,
      expectedTrustPolicySha256,
      expectedManifestSignatureSetSha256: signatureSetSha256,
      verificationTimeMs,
    });
  } catch (error) {
    await rm(outputRoot, { recursive: true, force: true });
    throw error;
  }
}

export async function validateAuthorizedCandidateSet({
  authorizedDirectory,
  expectedSourceCommit,
  expectedReleaseId,
  expectedManifestSha256,
  expectedTrustPolicySha256,
  expectedManifestSignatureSetSha256,
  verificationTimeMs,
}) {
  const root = await requireDirectory(authorizedDirectory);
  await requireExactDirectoryEntries(root, AUTHORIZATION_FILES);
  const indexBytes = await readCanonicalJsonBytes(join(root, "authorization-set.json"));
  const authorization = indexBytes.value;
  requireExactKeys(authorization, [
    "audience",
    "authorizationRunId",
    "buildId",
    "candidateRunId",
    "manifestSha256",
    "manifestSignatureSetId",
    "manifestSignatureSetSha256",
    "readinessLabel",
    "releaseId",
    "repository",
    "signatureCount",
    "sourceCommit",
    "trustGeneration",
    "trustPolicySha256",
    "version",
  ]);
  if (
    authorization.version !== 1 ||
    authorization.audience !== AUTHORIZATION_AUDIENCE ||
    authorization.readinessLabel !== "threshold-release-authorized"
  ) {
    throw new BrowserReleaseGateError("Invalid release authorization set");
  }
  requireSafeId(authorization.authorizationRunId, "Invalid authorization run identity");
  requireSafeId(authorization.manifestSignatureSetId, "Invalid signature-set identity");
  requirePositiveInteger(authorization.signatureCount, "Invalid signature count");
  requirePositiveInteger(authorization.trustGeneration, "Invalid trust generation");
  requirePattern(
    authorization.manifestSignatureSetSha256,
    HEX_64,
    "Invalid manifest signature-set digest",
  );
  const candidate = await validateCandidateSet({
    candidateDirectory: join(root, "candidate"),
    expectedSourceCommit,
    expectedReleaseId,
    expectedManifestSha256,
    expectedTrustPolicySha256,
  });
  const signatureSet = await readAuthorityJsonBytes(
    join(root, "release-manifest-signatures.json"),
    parseSignatureSet,
    32 * 1024,
    "Invalid canonical manifest signature set",
  );
  const signatureSetSha256 = sha256(signatureSet.bytes);
  if (
    signatureSetSha256 !== authorization.manifestSignatureSetSha256 ||
    expectedManifestSignatureSetSha256 &&
      signatureSetSha256 !== expectedManifestSignatureSetSha256 ||
    authorization.manifestSignatureSetId !== signatureSet.value.signatureSetId ||
    authorization.signatureCount !== signatureSet.value.signatures.length ||
    authorization.repository !== candidate.candidate.repository ||
    authorization.candidateRunId !== candidate.candidate.candidateRunId ||
    authorization.sourceCommit !== candidate.candidate.sourceCommit ||
    authorization.releaseId !== candidate.candidate.releaseId ||
    authorization.buildId !== candidate.candidate.buildId ||
    authorization.manifestSha256 !== candidate.candidate.manifestSha256 ||
    authorization.trustPolicySha256 !== candidate.candidate.trustPolicySha256 ||
    authorization.trustGeneration !== candidate.policy.trustGeneration
  ) {
    throw new BrowserReleaseGateError(
      "The authorization set does not match the stored candidate authority",
    );
  }
  verifyAuthoritySignatureSet({
    targetBytes: candidate.manifestBytes,
    signatureSet: signatureSet.value,
    policy: candidate.policy,
    requiredRole: "release",
    targetAudience: MANIFEST_AUDIENCE,
    targetIssuedAtMs: candidate.manifest.publishedAtMs,
    verificationTimeMs,
    allowRetiredHistoricalKeys: true,
  });
  return Object.freeze({
    authorization,
    candidate,
    directory: root,
    signatureSet: signatureSet.value,
    signatureSetBytes: signatureSet.bytes,
  });
}

export async function createPromotionSet({
  authorizedDirectory,
  activationPath,
  activationSignatureSetPath,
  canaryEvidencePath,
  outputDirectory,
  promotionRunId,
  expectedSourceCommit,
  expectedReleaseId,
  expectedManifestSha256,
  expectedTrustPolicySha256,
  expectedManifestSignatureSetSha256,
  expectedActivationSha256,
  expectedActivationSignatureSetSha256,
  verificationTimeMs,
  requireProductionReady = false,
}) {
  requireSafeId(promotionRunId, "Invalid promotion run identity");
  const authorized = await validateAuthorizedCandidateSet({
    authorizedDirectory,
    expectedSourceCommit,
    expectedReleaseId,
    expectedManifestSha256,
    expectedTrustPolicySha256,
    expectedManifestSignatureSetSha256,
    verificationTimeMs,
  });
  const activation = await readAuthorityJsonBytes(
    activationPath,
    parseActivation,
    16 * 1024,
    "Invalid canonical release activation",
  );
  const activationSha256 = sha256(activation.bytes);
  const activationSignatureSet = await readAuthorityJsonBytes(
    activationSignatureSetPath,
    parseSignatureSet,
    32 * 1024,
    "Invalid canonical activation signature set",
  );
  const activationSignatureSetSha256 = sha256(activationSignatureSet.bytes);
  const canary = await readAuthorityJsonBytes(
    canaryEvidencePath,
    parseCanaryEvidence,
    MAX_JSON_BYTES,
    "Invalid canonical canary evidence",
  );
  const canaryEvidenceSha256 = sha256(canary.bytes);
  if (
    expectedActivationSha256 && activationSha256 !== expectedActivationSha256 ||
    expectedActivationSignatureSetSha256 &&
      activationSignatureSetSha256 !== expectedActivationSignatureSetSha256 ||
    activation.value.manifestSha256 !== authorized.authorization.manifestSha256 ||
    activation.value.signatureSetSha256 !==
      authorized.authorization.manifestSignatureSetSha256 ||
    activation.value.canaryEvidenceSha256 !== canaryEvidenceSha256 ||
    activation.value.trustGeneration !== authorized.candidate.policy.trustGeneration ||
    canary.value.observedAtMs > activation.value.issuedAtMs ||
    verificationTimeMs < activation.value.issuedAtMs ||
    verificationTimeMs >= activation.value.expiresAtMs
  ) {
    throw new BrowserReleaseGateError(
      "The activation does not bind the exact authorized manifest and canary evidence",
    );
  }
  validateCanaryBindings(
    canary.value,
    authorized.candidate.manifest,
    activation.value.channel === "stable",
  );
  verifyAuthoritySignatureSet({
    targetBytes: activation.bytes,
    signatureSet: activationSignatureSet.value,
    policy: authorized.candidate.policy,
    requiredRole: "promotion",
    targetAudience: ACTIVATION_AUDIENCE,
    targetIssuedAtMs: activation.value.issuedAtMs,
    targetTrustGeneration: activation.value.trustGeneration,
    verificationTimeMs,
    allowRetiredHistoricalKeys: false,
  });
  const readinessLabel = activation.value.channel === "stable"
    ? "production-ready"
    : "canary-authorized";
  if (requireProductionReady && readinessLabel !== "production-ready") {
    throw new BrowserReleaseGateError(
      "Production-ready promotion requires an exact stable activation",
    );
  }

  const outputRoot = resolve(outputDirectory);
  await mkdirExclusive(outputRoot);
  try {
    await cp(authorized.directory, join(outputRoot, "authorized-candidate"), {
      recursive: true,
      force: false,
      errorOnExist: true,
    });
    await Promise.all([
      writeExclusive(join(outputRoot, "release-activation.json"), activation.bytes),
      writeExclusive(
        join(outputRoot, "release-activation-signatures.json"),
        activationSignatureSet.bytes,
      ),
      writeExclusive(join(outputRoot, "canary-evidence.json"), canary.bytes),
    ]);
    const promotion = Object.freeze({
      version: 1,
      audience: PROMOTION_AUDIENCE,
      repository: authorized.authorization.repository,
      promotionRunId,
      authorizationRunId: authorized.authorization.authorizationRunId,
      sourceCommit: authorized.authorization.sourceCommit,
      releaseId: authorized.authorization.releaseId,
      buildId: authorized.authorization.buildId,
      manifestSha256: authorized.authorization.manifestSha256,
      manifestSignatureSetSha256:
        authorized.authorization.manifestSignatureSetSha256,
      trustPolicySha256: authorized.authorization.trustPolicySha256,
      activationId: activation.value.activationId,
      activationSha256,
      activationSignatureSetId: activationSignatureSet.value.signatureSetId,
      activationSignatureSetSha256,
      canaryEvidenceSha256,
      channel: activation.value.channel,
      readinessLabel,
    });
    await writeCanonicalJsonExclusive(join(outputRoot, "promotion-set.json"), promotion);
    return await validatePromotionSet({
      promotionDirectory: outputRoot,
      expectedSourceCommit,
      expectedReleaseId,
      expectedManifestSha256,
      expectedTrustPolicySha256,
      expectedManifestSignatureSetSha256,
      expectedActivationSha256: activationSha256,
      expectedActivationSignatureSetSha256: activationSignatureSetSha256,
      verificationTimeMs,
      requireProductionReady,
    });
  } catch (error) {
    await rm(outputRoot, { recursive: true, force: true });
    throw error;
  }
}

export async function validatePromotionSet({
  promotionDirectory,
  expectedSourceCommit,
  expectedReleaseId,
  expectedManifestSha256,
  expectedTrustPolicySha256,
  expectedManifestSignatureSetSha256,
  expectedActivationSha256,
  expectedActivationSignatureSetSha256,
  verificationTimeMs,
  requireProductionReady = false,
}) {
  const root = await requireDirectory(promotionDirectory);
  await requireExactDirectoryEntries(root, PROMOTION_FILES);
  const indexBytes = await readCanonicalJsonBytes(join(root, "promotion-set.json"));
  const promotion = indexBytes.value;
  requireExactKeys(promotion, [
    "activationId",
    "activationSha256",
    "activationSignatureSetId",
    "activationSignatureSetSha256",
    "audience",
    "authorizationRunId",
    "buildId",
    "canaryEvidenceSha256",
    "channel",
    "manifestSha256",
    "manifestSignatureSetSha256",
    "promotionRunId",
    "readinessLabel",
    "releaseId",
    "repository",
    "sourceCommit",
    "trustPolicySha256",
    "version",
  ]);
  if (promotion.version !== 1 || promotion.audience !== PROMOTION_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid promotion authority set");
  }
  const authorized = await validateAuthorizedCandidateSet({
    authorizedDirectory: join(root, "authorized-candidate"),
    expectedSourceCommit,
    expectedReleaseId,
    expectedManifestSha256,
    expectedTrustPolicySha256,
    expectedManifestSignatureSetSha256,
    verificationTimeMs,
  });
  const activation = await readAuthorityJsonBytes(
    join(root, "release-activation.json"),
    parseActivation,
    16 * 1024,
    "Invalid canonical release activation",
  );
  const activationSignatureSet = await readAuthorityJsonBytes(
    join(root, "release-activation-signatures.json"),
    parseSignatureSet,
    32 * 1024,
    "Invalid canonical activation signature set",
  );
  const canary = await readAuthorityJsonBytes(
    join(root, "canary-evidence.json"),
    parseCanaryEvidence,
    MAX_JSON_BYTES,
    "Invalid canonical canary evidence",
  );
  const activationSha256 = sha256(activation.bytes);
  const activationSignatureSetSha256 = sha256(activationSignatureSet.bytes);
  const canaryEvidenceSha256 = sha256(canary.bytes);
  const readinessLabel = activation.value.channel === "stable"
    ? "production-ready"
    : "canary-authorized";
  if (
    expectedActivationSha256 && activationSha256 !== expectedActivationSha256 ||
    expectedActivationSignatureSetSha256 &&
      activationSignatureSetSha256 !== expectedActivationSignatureSetSha256 ||
    promotion.repository !== authorized.authorization.repository ||
    promotion.authorizationRunId !== authorized.authorization.authorizationRunId ||
    promotion.sourceCommit !== authorized.authorization.sourceCommit ||
    promotion.releaseId !== authorized.authorization.releaseId ||
    promotion.buildId !== authorized.authorization.buildId ||
    promotion.manifestSha256 !== authorized.authorization.manifestSha256 ||
    promotion.manifestSignatureSetSha256 !==
      authorized.authorization.manifestSignatureSetSha256 ||
    promotion.trustPolicySha256 !== authorized.authorization.trustPolicySha256 ||
    promotion.activationId !== activation.value.activationId ||
    promotion.activationSha256 !== activationSha256 ||
    promotion.activationSignatureSetId !== activationSignatureSet.value.signatureSetId ||
    promotion.activationSignatureSetSha256 !== activationSignatureSetSha256 ||
    promotion.canaryEvidenceSha256 !== canaryEvidenceSha256 ||
    promotion.channel !== activation.value.channel ||
    promotion.readinessLabel !== readinessLabel ||
    activation.value.manifestSha256 !== authorized.authorization.manifestSha256 ||
    activation.value.signatureSetSha256 !==
      authorized.authorization.manifestSignatureSetSha256 ||
    activation.value.canaryEvidenceSha256 !== canaryEvidenceSha256 ||
    activation.value.trustGeneration !== authorized.candidate.policy.trustGeneration ||
    canary.value.observedAtMs > activation.value.issuedAtMs ||
    verificationTimeMs < activation.value.issuedAtMs ||
    verificationTimeMs >= activation.value.expiresAtMs
  ) {
    throw new BrowserReleaseGateError(
      "The promotion set does not bind the exact stored authorities",
    );
  }
  if (requireProductionReady && readinessLabel !== "production-ready") {
    throw new BrowserReleaseGateError(
      "Production-ready promotion requires an exact stable activation",
    );
  }
  validateCanaryBindings(
    canary.value,
    authorized.candidate.manifest,
    activation.value.channel === "stable",
  );
  verifyAuthoritySignatureSet({
    targetBytes: activation.bytes,
    signatureSet: activationSignatureSet.value,
    policy: authorized.candidate.policy,
    requiredRole: "promotion",
    targetAudience: ACTIVATION_AUDIENCE,
    targetIssuedAtMs: activation.value.issuedAtMs,
    targetTrustGeneration: activation.value.trustGeneration,
    verificationTimeMs,
    allowRetiredHistoricalKeys: false,
  });
  return Object.freeze({
    activation: activation.value,
    activationSignatureSet: activationSignatureSet.value,
    authorized,
    canary: canary.value,
    directory: root,
    promotion,
  });
}

export function validateWorkflowContract(workflowSource) {
  if (
    typeof workflowSource !== "string" ||
    Buffer.byteLength(workflowSource, "utf8") < 100 ||
    Buffer.byteLength(workflowSource, "utf8") > 256_000 ||
    workflowSource.includes("\0")
  ) {
    throw new BrowserReleaseGateError("Invalid Browser release workflow");
  }
  for (const fragment of [
    "name: Bluey Browser Release Authority Gate",
    "operation:",
    "target: darwin-arm64\n            runner: macos-14",
    "target: darwin-x64\n            runner: macos-15-intel",
    "target: windows-x64\n            runner: windows-latest",
    "trusted-release-tools/jobs/scripts/browser-release-ci-gate.mjs credentials",
    'node "$trusted_gate" inventory',
    "trusted-release-tools/jobs/scripts/browser-release-ci-gate.mjs collect",
    "node jobs/scripts/browser-release-ci-gate.mjs assemble",
    "node jobs/scripts/browser-release-ci-gate.mjs validate",
    "node jobs/scripts/browser-release-ci-gate.mjs authorize",
    "node jobs/scripts/browser-release-ci-gate.mjs promote",
    "node jobs/scripts/browser-release-ci-gate.mjs validate-promotion",
    "environment: bluey-browser-release-signing",
    "path: candidate-source",
    "path: trusted-release-tools",
    "npm ci --ignore-scripts --prefix jobs",
    "npm ci --prefix candidate-source/jobs",
    "npm run --prefix candidate-source/jobs/browser build",
    "npm run --prefix candidate-source/jobs/browser prepare:chromium",
    "npm ci --prefix trusted-release-tools/jobs",
    "npm run --prefix trusted-release-tools/jobs/browser build",
    "trusted-release-tools/jobs/browser/scripts/package-release.mjs",
    "validatePreparedPackagingTree('./candidate-source/jobs/browser')",
    "validatePreparedSourceAgainstTrusted('./candidate-source/jobs/browser'",
    "browser-release-ci-gate.mjs extract-prepared",
    '--out-parent "$RUNNER_TEMP"',
    '--prepared-source-root "$GITHUB_WORKSPACE/candidate-source/jobs/browser"',
    "browser-release-ci-gate.mjs seal",
    " validate-seal ",
    "package_seal_sha256=$PACKAGE_SEAL_SHA256",
    '--package-seal-sha256 "$PACKAGE_SEAL_SHA256"',
    '--seal-dir "$SEAL_DIRECTORY"',
    "codesign --verify --deep --strict",
    "spctl --assess --type execute",
    "xcrun stapler validate",
    'xcrun stapler validate "$images[1]"',
    "hdiutil verify",
    "hdiutil attach -readonly -nobrowse -noautoopen",
    "dmg-app-content-inventory.json",
    "cmp -s",
    "plutil -convert json",
    "CFBundleIdentifier",
    "CFBundleDisplayName",
    "CFBundleURLSchemes",
    "macos_bundle_identity_verified=true",
    "macos_protocol_registration_verified=true",
    "dmg_bundle_metadata_matches_archive=true",
    "app_asar_runtime_contract_verified=true",
    "windows_protocol_construction_verified=true",
    "dmg_app_content_matches_archive=true",
    "$signature = Get-AuthenticodeSignature -LiteralPath $installers[0].FullName",
    "SignatureStatus]::Valid",
    "TimeStamperCertificate",
    "outer_installer_authenticode_verified=true",
    "outer_installer_timestamp_verified=true",
    "Get-Command 7z.exe",
    "seven_zip_version=",
    "nsis_installer_extracted=true",
    'Filter "app-64.7z"',
    '& $sevenZip x "-o$payloadRoot" -y -- $installerPath',
    '& $sevenZip x "-o$unpackedRoot" -y -- $appArchivePath',
    "nsis_app_archive_extracted=true",
    'Where-Object { $_.Name -ceq "Bluey Browser.exe" }',
    "$appSignature = Get-AuthenticodeSignature",
    "$appSignature.TimeStamperCertificate",
    "$appSigner -cne $env:BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY",
    "payload_application_authenticode_verified=true",
    "payload_application_timestamp_verified=true",
    "installer_payload_inventory_verified=true",
    "BLUEY_BROWSER_MACOS_CSC_LINK",
    "BLUEY_BROWSER_MACOS_SIGNER_IDENTITY",
    "BLUEY_BROWSER_APPLE_API_KEY_BASE64",
    "BLUEY_BROWSER_WINDOWS_CSC_LINK",
    "BLUEY_BROWSER_WINDOWS_SIGNER_IDENTITY",
    '[[ "$signer" == "$BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY" ]]',
    '[[ "$dmg_signer" == "$BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY" ]]',
    "$signer -cne $env:BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY",
    "BLUEY_BROWSER_BUILD_PRIVATE_KEY_PKCS8_BASE64",
    "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_BASE64",
    "BLUEY_BROWSER_RELEASE_TRUST_POLICY_BASE64",
    "BLUEY_BROWSER_RELEASE_TRUST_POLICY_SHA256",
    "MANIFEST_SIGNATURE_SET_BASE64: ${{ inputs.manifest_signature_set_base64 }}",
    "ACTIVATION_SIGNATURE_SET_BASE64: ${{ inputs.activation_signature_set_base64 }}",
    "CANARY_EVIDENCE_BASE64: ${{ inputs.canary_evidence_base64 }}",
    "process.stdout.write(String(Date.now()))",
    "github.ref_name == github.event.repository.default_branch",
    "ref: ${{ github.workflow_sha }}",
    "git merge-base --is-ancestor",
    "fetch-depth: 0",
    "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02",
    "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093",
    "run-id: ${{ inputs.candidate_run_id }}",
    "run-id: ${{ inputs.authorized_run_id }}",
    "merge-multiple: false",
    "overwrite: false",
    "if-no-files-found: error",
    "--require-production-ready",
    "# AUTHORIZATION-NO-REBUILD-BEGIN",
    "# AUTHORIZATION-NO-REBUILD-END",
    "# PROMOTION-NO-REBUILD-BEGIN",
    "# PROMOTION-NO-REBUILD-END",
  ]) {
    if (!workflowSource.includes(fragment)) {
      throw new BrowserReleaseGateError(
        `The Browser release workflow is missing ${JSON.stringify(fragment)}`,
      );
    }
  }
  const inventoryStep = workflowSource.indexOf(
    "- name: Inventory the exact signed application content",
  );
  const recordNativeStep = workflowSource.indexOf(
    "- name: Record native verification evidence",
  );
  const collectStep = workflowSource.indexOf(
    "- name: Collect the immutable target evidence",
  );
  const dmgInventoryMarker = workflowSource.indexOf(
    "dmg_app_content_matches_archive=true",
  );
  const windowsInventoryMarker = workflowSource.indexOf(
    "installer_payload_inventory_verified=true",
  );
  const asarContractMarker = workflowSource.indexOf(
    "app_asar_runtime_contract_verified=true",
  );
  const macBundleMarker = workflowSource.indexOf(
    "dmg_bundle_metadata_matches_archive=true",
  );
  const windowsProtocolMarker = workflowSource.indexOf(
    "windows_protocol_construction_verified=true",
  );
  if (
    inventoryStep < 0 ||
    recordNativeStep <= inventoryStep ||
    collectStep <= recordNativeStep ||
    dmgInventoryMarker <= inventoryStep ||
    dmgInventoryMarker >= recordNativeStep ||
    windowsInventoryMarker <= inventoryStep ||
    windowsInventoryMarker >= recordNativeStep ||
    asarContractMarker <= inventoryStep ||
    asarContractMarker >= recordNativeStep ||
    macBundleMarker <= inventoryStep ||
    macBundleMarker >= recordNativeStep ||
    windowsProtocolMarker <= inventoryStep ||
    windowsProtocolMarker >= recordNativeStep
  ) {
    throw new BrowserReleaseGateError(
      "Native package inventory must be bound before native evidence is recorded",
    );
  }
  const targetRows = [...workflowSource.matchAll(/^\s+- target:\s+([^\s#]+)\s*$/gm)]
    .map((match) => match[1])
    .sort();
  const expectedWorkflowTargets = [...TARGET_NAMES, ...TARGET_NAMES].sort();
  if (targetRows.join("\n") !== expectedWorkflowTargets.join("\n")) {
    throw new BrowserReleaseGateError(
      "Preparation and signing must each contain exactly the three supported targets",
    );
  }
  const trustedValidatorCheckouts = workflowSource.match(
    /ref:\s+\$\{\{ github\.workflow_sha \}\}/g,
  )?.length ?? 0;
  if (trustedValidatorCheckouts !== 4) {
    throw new BrowserReleaseGateError(
      "Packaging, assembly, authorization, and promotion require trusted workflow tooling",
    );
  }
  const candidateJob = workflowSource.match(
    /^  candidate-targets:\n([\s\S]*?)(?=^  assemble-candidate:)/m,
  )?.[1];
  if (!candidateJob) {
    throw new BrowserReleaseGateError("The protected candidate job is missing");
  }
  const preparationJob = workflowSource.match(
    /^  prepare-candidate:\n([\s\S]*?)(?=^  candidate-targets:)/m,
  )?.[1];
  if (
    !preparationJob ||
    preparationJob.includes("${{ secrets.") ||
    preparationJob.includes("bluey-browser-release-signing") ||
    !preparationJob.includes("npm ci --prefix candidate-source/jobs") ||
    !preparationJob.includes(
      "npm run --prefix candidate-source/jobs/browser build",
    ) ||
    !preparationJob.includes("prepare:chromium") ||
    !preparationJob.includes("bluey-browser-prepared-${{ matrix.target }}")
  ) {
    throw new BrowserReleaseGateError(
      "Candidate code must run only in a separate credential-free preparation job",
    );
  }
  const expectedTargetRows = [...TARGET_NAMES].sort().join("\n");
  for (const job of [preparationJob, candidateJob]) {
    const rows = [...job.matchAll(/^\s+- target:\s+([^\s#]+)\s*$/gm)]
      .map((match) => match[1])
      .sort()
      .join("\n");
    if (rows !== expectedTargetRows) {
      throw new BrowserReleaseGateError(
        "Credential-free preparation and protected signing require matching targets",
      );
    }
  }
  const contractJob = workflowSource.match(
    /^  contract:\n([\s\S]*?)(?=^  prepare-candidate:)/m,
  )?.[1];
  if (!contractJob || contractJob.includes("${{ secrets.")) {
    throw new BrowserReleaseGateError(
      "Pull-request contract validation must remain secret-free",
    );
  }
  const trustedCheckout = candidateJob.indexOf(
    "- name: Check out trusted release tooling on the isolated signing runner",
  );
  const preparedDownload = candidateJob.indexOf(
    "- name: Download the prepared bytes from the credential-free job",
  );
  const trustedInstall = candidateJob.indexOf(
    "- name: Materialize prepared bytes and install dependencies without scripts or secrets",
  );
  const trustedDependencies = candidateJob.indexOf(
    "npm ci --prefix trusted-release-tools/jobs",
  );
  const safeExtraction = candidateJob.indexOf(
    "browser-release-ci-gate.mjs extract-prepared",
  );
  const candidateDependencies = candidateJob.indexOf(
    "npm ci --ignore-scripts --prefix candidate-source/jobs",
  );
  const packageStep = candidateJob.indexOf(
    "- name: Package with trusted tooling and step-scoped signing authority",
  );
  const sealStep = candidateJob.indexOf(
    "- name: Seal and hash the exact package bytes immediately",
  );
  const nativeMacStep = candidateJob.indexOf(
    "- name: Verify macOS signing, notarization, and staples",
  );
  if (
    trustedCheckout < 0 ||
    preparedDownload <= trustedCheckout ||
    trustedInstall <= preparedDownload ||
    trustedDependencies <= trustedInstall ||
    safeExtraction <= trustedDependencies ||
    candidateDependencies <= safeExtraction ||
    packageStep <= trustedInstall ||
    sealStep <= packageStep ||
    nativeMacStep <= sealStep ||
    candidateJob.slice(0, packageStep).includes("${{ secrets.") ||
    candidateJob.includes("$GITHUB_ENV") ||
    !candidateJob.includes("npm ci --ignore-scripts --prefix candidate-source/jobs") ||
    /npm\s+run\s+--prefix\s+candidate-source/.test(candidateJob) ||
    /\btar\s+[^\n]*(?:-x|--extract)/i.test(candidateJob)
  ) {
    throw new BrowserReleaseGateError(
      "Isolated prepared bytes and trusted tooling must finish before step-scoped secrets",
    );
  }
  const packageBoundary = candidateJob.slice(packageStep, sealStep);
  if (
    !packageBoundary.includes("${{ secrets.") ||
    !packageBoundary.includes(
      "trusted-release-tools/jobs/browser/scripts/package-release.mjs",
    ) ||
    /npm\s+(?:run\s+)?(?:--prefix\s+)?candidate-source[^\n]*package/i.test(
      packageBoundary,
    ) ||
    /candidate-source\/jobs\/browser\/scripts\//.test(packageBoundary)
  ) {
    throw new BrowserReleaseGateError(
      "Only trusted packaging code may receive native signing authority",
    );
  }
  const sealedEvidenceBoundary = candidateJob.slice(nativeMacStep);
  if (
    sealedEvidenceBoundary.includes("candidate-source/jobs/browser/release") ||
    (sealedEvidenceBoundary.match(/\bvalidate-seal\b/g)
      ?.length ?? 0) < 4 ||
    !sealedEvidenceBoundary.includes(
      "archives=($SEAL_DIRECTORY/artifacts/Bluey-Browser-*-mac-",
    ) ||
    !sealedEvidenceBoundary.includes(
      '$artifacts = Join-Path $env:SEAL_DIRECTORY "artifacts"',
    )
  ) {
    throw new BrowserReleaseGateError(
      "Native verification and collection must consume only sealed package bytes",
    );
  }
  for (const forbidden of [
    /target:\s+(?:linux|darwin-universal|macos-universal)/i,
    /^\s*contents:\s*write\s*$/im,
    /softprops\/action-gh-release/i,
    /\bgh\s+release\b/i,
    /\bnpm\s+publish\b/i,
    /\bdocker\s+push\b/i,
    /BLUEY_BROWSER_(?:MANIFEST|RELEASE|PROMOTION)_[A-Z0-9_]*PRIVATE/i,
    /\bcreatePrivateKey\b/,
    /\bopenssl\s+(?:dgst|pkeyutl)\b[^\n]*\b(?:sign|signraw)\b/i,
    /inputs\.(?:authorization|promotion)_verification_time_ms/,
  ]) {
    if (forbidden.test(workflowSource)) {
      throw new BrowserReleaseGateError(
        "The Browser workflow violates target, offline signing, or publishing authority",
      );
    }
  }
  const promotion = workflowSource.match(
    /# PROMOTION-NO-REBUILD-BEGIN([\s\S]*?)# PROMOTION-NO-REBUILD-END/,
  )?.[1];
  if (!promotion) {
    throw new BrowserReleaseGateError("The no-rebuild promotion boundary is missing");
  }
  const authorization = workflowSource.match(
    /# AUTHORIZATION-NO-REBUILD-BEGIN([\s\S]*?)# AUTHORIZATION-NO-REBUILD-END/,
  )?.[1];
  if (!authorization) {
    throw new BrowserReleaseGateError("The no-rebuild authorization boundary is missing");
  }
  for (const forbidden of [
    /npm\s+(?:run\s+)?package/i,
    /electron-builder/i,
    /\btsc\b/i,
    /cargo\s+build/i,
    /docker\s+build/i,
    /\bmake\b/i,
    /browser-release-ci-gate\.mjs\s+(?:assemble|collect|inventory|record-native)/i,
  ]) {
    if (forbidden.test(promotion) || forbidden.test(authorization)) {
      throw new BrowserReleaseGateError(
        "Promotion must consume stored bytes without rebuilding",
      );
    }
  }
  return true;
}

async function validateCandidatePart(directory) {
  const root = await requireDirectory(directory);
  await requireExactDirectoryEntries(root, PART_FILES);
  const recordBytes = await readCanonicalJsonBytes(join(root, "target-record.json"));
  const record = recordBytes.value;
  validateTargetRecord(record);
  const target = requireReleaseTarget(record.target);
  const sealed = await validatePackageSeal(root, false);
  const authority = await readBuildAuthority(root, target);
  if (
    sealed.packageSealSha256 !== record.packageSealSha256 ||
    canonicalString(sealed.artifacts) !== canonicalString(
      record.artifacts.map(({ filename, packageKind, sizeBytes, sha256: digest }) => ({
        filename,
        packageKind,
        sizeBytes,
        sha256: digest,
      })),
    ) ||
    authority.descriptorSha256 !== record.descriptorSha256 ||
    authority.descriptor.releaseId !== record.releaseId ||
    authority.descriptor.buildId !== record.buildId ||
    authority.descriptor.appVersion !== record.appVersion ||
    authority.descriptor.sourceCommit !== record.sourceCommit ||
    authority.descriptor.protocolVersion !== record.protocolVersion
  ) {
    throw new BrowserReleaseGateError("The candidate part descriptor changed");
  }
  const inventoryBytes = await readCanonicalJsonBytes(
    join(root, "app-content-inventory.json"),
  );
  validateContentInventory(inventoryBytes.value, target, record.descriptorSha256);
  if (
    inventoryBytes.value.packageSealSha256 !== record.packageSealSha256 ||
    sha256(inventoryBytes.bytes) !== record.appContentSha256
  ) {
    throw new BrowserReleaseGateError("The candidate app-content inventory changed");
  }
  const nativeBytes = await readCanonicalJsonBytes(
    join(root, "native-verification.json"),
  );
  validateNativeVerification(nativeBytes.value, target);
  const transcript = await readBoundedFile(
    join(root, "native-verification-transcript.txt"),
    MAX_TRANSCRIPT_BYTES,
    "Invalid native verification transcript",
  );
  if (
    nativeBytes.value.packageSealSha256 !== record.packageSealSha256 ||
    !transcript.toString("utf8").split(/\r?\n/)
      .includes(`package_seal_sha256=${record.packageSealSha256}`) ||
    sha256(transcript) !== nativeBytes.value.transcriptSha256
  ) {
    throw new BrowserReleaseGateError("The candidate native transcript changed");
  }
  const evidenceBytes = await readCanonicalJsonBytes(
    join(root, "verification-evidence.json"),
  );
  if (sha256(evidenceBytes.bytes) !== record.verificationEvidenceSha256) {
    throw new BrowserReleaseGateError("The candidate verification evidence changed");
  }
  const evidence = evidenceBytes.value;
  validateVerificationEvidence(evidence, record, nativeBytes.value);
  const artifactRoot = await requireDirectory(join(root, "artifacts"));
  const expectedNames = record.artifacts.map((artifact) => artifact.filename).sort();
  await requireExactDirectoryEntries(artifactRoot, expectedNames);
  for (const artifact of record.artifacts) {
    const artifactPath = join(artifactRoot, artifact.filename);
    const info = await requireFile(artifactPath);
    if (info.size !== artifact.sizeBytes || await sha256File(artifactPath) !== artifact.sha256) {
      throw new BrowserReleaseGateError("A candidate target artifact changed");
    }
  }
  return Object.freeze({ directory: root, record });
}

function validateTargetRecord(record) {
  requireExactKeys(record, [
    "appContentSha256",
    "appVersion",
    "architecture",
    "artifacts",
    "audience",
    "buildId",
    "chromiumRevision",
    "descriptorSha256",
    "electronVersion",
    "nativeSignatureKind",
    "nativeSignerIdentity",
    "packageSealSha256",
    "platform",
    "playwrightVersion",
    "protocolVersion",
    "releaseId",
    "sourceCommit",
    "target",
    "verificationEvidenceSha256",
    "version",
  ]);
  const target = requireReleaseTarget(record.target);
  if (
    record.version !== 1 ||
    record.audience !== PART_AUDIENCE ||
    record.platform !== target.platform ||
    record.architecture !== target.architecture ||
    record.nativeSignatureKind !== target.nativeSignatureKind
  ) {
    throw new BrowserReleaseGateError("Invalid candidate target record");
  }
  requireReleaseId(record.releaseId, "Invalid release identity");
  requirePattern(record.buildId, BUILD_ID, "Invalid build identity");
  requirePattern(record.appVersion, SEMVER, "Invalid app version");
  requirePositiveInteger(record.protocolVersion, "Invalid protocol version");
  requirePattern(record.sourceCommit, SOURCE_COMMIT, "Invalid source commit");
  requirePattern(record.electronVersion, SEMVER, "Invalid Electron version");
  requirePattern(record.playwrightVersion, SEMVER, "Invalid Playwright version");
  requireDecimal(record.chromiumRevision, "Invalid Chromium revision");
  for (const field of [
    "descriptorSha256",
    "packageSealSha256",
    "appContentSha256",
    "verificationEvidenceSha256",
  ]) {
    requirePattern(record[field], HEX_64, `Invalid ${field}`);
  }
  requireBoundedString(record.nativeSignerIdentity, 3, 256, "Invalid signer identity");
  const artifacts = requireArray(
    record.artifacts,
    target.packages.length,
    target.packages.length,
    "Invalid candidate artifact list",
  );
  const expectedKinds = target.packages.map((entry) => entry.packageKind).sort();
  const actualKinds = artifacts.map((artifact) => {
    requireExactKeys(artifact, [
      "filename",
      "packageKind",
      "relativePath",
      "sha256",
      "sizeBytes",
    ]);
    requireSafeFilename(artifact.filename);
    requirePositiveInteger(artifact.sizeBytes, "Invalid artifact size");
    requirePattern(artifact.sha256, HEX_64, "Invalid artifact digest");
    if (artifact.relativePath !== `artifacts/${artifact.filename}`) {
      throw new BrowserReleaseGateError("Invalid candidate artifact path");
    }
    return artifact.packageKind;
  }).sort();
  if (actualKinds.join("\n") !== expectedKinds.join("\n")) {
    throw new BrowserReleaseGateError("Invalid target package kinds");
  }
  requireExactTargetArtifactContracts(artifacts, target, record.appVersion);
}

function validateContentInventory(inventory, target, descriptorSha256) {
  requireExactKeys(inventory, [
    "appAsarEntriesSha256",
    "appAsarEntryCount",
    "appAsarSha256",
    "appId",
    "architecture",
    "automationMainSha256",
    "audience",
    "browserMainSha256",
    "chromiumExecutablePath",
    "chromiumExecutableSha256",
    "chromiumRevision",
    "descriptorSha256",
    "entries",
    "packageSealSha256",
    "platform",
    "protocolRegistrationSha256",
    "protocolScheme",
    "target",
    "version",
  ]);
  if (
    inventory.version !== 1 ||
    inventory.audience !== CONTENT_AUDIENCE ||
    inventory.target !== target.target ||
    inventory.platform !== target.platform ||
    inventory.architecture !== target.architecture ||
    inventory.descriptorSha256 !== descriptorSha256 ||
    inventory.appId !== "sh.bluey.jobs.browser" ||
    inventory.protocolScheme !== "bluey-jobs"
  ) {
    throw new BrowserReleaseGateError("Invalid app-content inventory authority");
  }
  requirePattern(inventory.appAsarSha256, HEX_64, "Invalid app.asar digest");
  requirePattern(
    inventory.appAsarEntriesSha256,
    HEX_64,
    "Invalid app.asar entry digest",
  );
  requirePositiveInteger(inventory.appAsarEntryCount, "Invalid app.asar entry count");
  if (inventory.appAsarEntryCount > MAX_ASAR_ENTRIES) {
    throw new BrowserReleaseGateError("Invalid app.asar entry count");
  }
  requirePattern(
    inventory.browserMainSha256,
    HEX_64,
    "Invalid Browser main digest",
  );
  requirePattern(
    inventory.automationMainSha256,
    HEX_64,
    "Invalid automation main digest",
  );
  requirePattern(
    inventory.protocolRegistrationSha256,
    HEX_64,
    "Invalid protocol registration digest",
  );
  requirePattern(inventory.packageSealSha256, HEX_64, "Invalid package-seal digest");
  requirePattern(
    inventory.chromiumExecutableSha256,
    HEX_64,
    "Invalid Chromium executable digest",
  );
  requireDecimal(inventory.chromiumRevision, "Invalid Chromium revision");
  requireSafeRelativePath(inventory.chromiumExecutablePath);
  const entries = requireArray(
    inventory.entries,
    4,
    MAX_INVENTORY_ENTRIES,
    "Invalid app-content entries",
  );
  let previous = "";
  for (const entry of entries) {
    requireExactKeys(entry, [
      "linkTarget",
      "mode",
      "path",
      "sha256",
      "sizeBytes",
      "type",
    ]);
    requireSafeRelativePath(entry.path);
    if (entry.path <= previous) {
      throw new BrowserReleaseGateError("App-content inventory is not canonical");
    }
    previous = entry.path;
    if (!Number.isInteger(entry.mode) || entry.mode < 0 || entry.mode > 0o777) {
      throw new BrowserReleaseGateError("Invalid app-content mode");
    }
    requireNonnegativeInteger(entry.sizeBytes, "Invalid app-content size");
    if (entry.type === "file") {
      requirePattern(entry.sha256, HEX_64, "Invalid app-content digest");
      if (entry.linkTarget !== null) {
        throw new BrowserReleaseGateError("Invalid app-content file entry");
      }
    } else if (entry.type === "symlink") {
      requirePattern(entry.sha256, HEX_64, "Invalid app-content link digest");
      requireBoundedString(entry.linkTarget, 1, 4_096, "Invalid app-content link");
      if (entry.sizeBytes !== Buffer.byteLength(entry.linkTarget, "utf8")) {
        throw new BrowserReleaseGateError("Invalid app-content link size");
      }
    } else if (
      entry.type !== "directory" ||
      entry.sha256 !== null ||
      entry.sizeBytes !== 0 ||
      entry.linkTarget !== null
    ) {
      throw new BrowserReleaseGateError("Invalid app-content entry type");
    }
  }
}

function validateNativeVerification(native, target) {
  requireExactKeys(native, [
    "audience",
    "authenticodeVerified",
    "codeSigningVerified",
    "nativeSignatureKind",
    "notarizationVerified",
    "packageSealSha256",
    "signerIdentity",
    "stapleVerified",
    "target",
    "timestampVerified",
    "toolVersions",
    "transcriptSha256",
    "version",
  ]);
  if (
    native.version !== 1 ||
    native.audience !== NATIVE_AUDIENCE ||
    native.target !== target.target ||
    native.nativeSignatureKind !== target.nativeSignatureKind
  ) {
    throw new BrowserReleaseGateError("Invalid native verification authority");
  }
  requireBoundedString(native.signerIdentity, 3, 256, "Invalid native signer identity");
  requirePattern(native.transcriptSha256, HEX_64, "Invalid native transcript digest");
  requirePattern(native.packageSealSha256, HEX_64, "Invalid package-seal digest");
  const tools = requireArray(native.toolVersions, 2, 16, "Invalid native toolchain");
  let prior = "";
  for (const tool of tools) {
    requireExactKeys(tool, ["name", "version"]);
    requireSafeId(tool.name, "Invalid native tool name");
    requireBoundedString(tool.version, 1, 256, "Invalid native tool version");
    if (tool.name <= prior) throw new BrowserReleaseGateError("Native toolchain is not sorted");
    prior = tool.name;
  }
  const expected = target.platform === "darwin"
    ? [true, true, true, false, false]
    : [false, false, false, true, true];
  const actual = [
    native.codeSigningVerified,
    native.notarizationVerified,
    native.stapleVerified,
    native.authenticodeVerified,
    native.timestampVerified,
  ];
  if (actual.some((value, index) => value !== expected[index])) {
    throw new BrowserReleaseGateError(
      "Native signing, notarization, Authenticode, or timestamp evidence is incomplete",
    );
  }
}

function validateVerificationEvidence(evidence, record, native) {
  requireExactKeys(evidence, [
    "appAsarSha256",
    "appContentSha256",
    "artifacts",
    "audience",
    "chromiumExecutableSha256",
    "chromiumRevision",
    "descriptorSha256",
    "nativeVerification",
    "packageSealSha256",
    "sourceCommit",
    "target",
    "version",
  ]);
  if (
    evidence.version !== 1 ||
    evidence.audience !== EVIDENCE_AUDIENCE ||
    evidence.target !== record.target ||
    evidence.sourceCommit !== record.sourceCommit ||
    evidence.descriptorSha256 !== record.descriptorSha256 ||
    evidence.packageSealSha256 !== record.packageSealSha256 ||
    evidence.appContentSha256 !== record.appContentSha256 ||
    canonicalString(evidence.nativeVerification) !== canonicalString(native)
  ) {
    throw new BrowserReleaseGateError("Invalid target verification evidence");
  }
  requirePattern(evidence.appAsarSha256, HEX_64, "Invalid evidence app.asar digest");
  requirePattern(
    evidence.chromiumExecutableSha256,
    HEX_64,
    "Invalid evidence Chromium digest",
  );
  if (
    canonicalString(evidence.artifacts) !==
    canonicalString(
      record.artifacts.map(({ filename, packageKind, sizeBytes, sha256: digest }) => ({
        filename,
        packageKind,
        sizeBytes,
        sha256: digest,
      })),
    )
  ) {
    throw new BrowserReleaseGateError("Verification evidence artifacts changed");
  }
}

async function validatePackageSeal(directory, requireExactEntries) {
  const root = await requireDirectory(directory);
  if (requireExactEntries) {
    await requireExactDirectoryEntries(
      root,
      [...AUTHORITY_FILES, "artifacts", "package-seal.json"].sort(),
    );
  }
  const sealBytes = await readCanonicalJsonBytes(join(root, "package-seal.json"));
  const seal = sealBytes.value;
  requireExactKeys(seal, [
    "appVersion",
    "artifacts",
    "audience",
    "buildId",
    "descriptorSha256",
    "releaseId",
    "sourceCommit",
    "target",
    "version",
  ]);
  const target = requireReleaseTarget(seal.target);
  if (seal.version !== 1 || seal.audience !== PACKAGE_SEAL_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid package-seal authority");
  }
  requirePattern(seal.sourceCommit, SOURCE_COMMIT, "Invalid sealed source commit");
  requireReleaseId(seal.releaseId, "Invalid sealed release identity");
  requirePattern(seal.buildId, BUILD_ID, "Invalid sealed build identity");
  requirePattern(seal.appVersion, SEMVER, "Invalid sealed app version");
  requirePattern(seal.descriptorSha256, HEX_64, "Invalid sealed descriptor digest");
  const artifacts = requireArray(
    seal.artifacts,
    target.packages.length,
    target.packages.length,
    "Invalid sealed artifact list",
  ).map((artifact) => {
    requireExactKeys(artifact, ["filename", "packageKind", "sha256", "sizeBytes"]);
    requireSafeFilename(artifact.filename);
    requirePattern(artifact.sha256, HEX_64, "Invalid sealed artifact digest");
    requirePositiveInteger(artifact.sizeBytes, "Invalid sealed artifact size");
    return Object.freeze({
      filename: artifact.filename,
      packageKind: artifact.packageKind,
      sizeBytes: artifact.sizeBytes,
      sha256: artifact.sha256,
    });
  });
  const expectedKinds = target.packages.map((entry) => entry.packageKind).sort();
  if (
    artifacts.map((artifact) => artifact.packageKind).sort().join("\n") !==
      expectedKinds.join("\n") ||
    new Set(artifacts.map((artifact) => artifact.filename)).size !== artifacts.length ||
    new Set(artifacts.map((artifact) => artifact.sha256)).size !== artifacts.length
  ) {
    throw new BrowserReleaseGateError("Invalid sealed package contract");
  }
  requireExactTargetArtifactContracts(artifacts, target, seal.appVersion);
  const authority = await readBuildAuthority(root, target);
  if (
    authority.descriptorSha256 !== seal.descriptorSha256 ||
    authority.descriptor.sourceCommit !== seal.sourceCommit ||
    authority.descriptor.releaseId !== seal.releaseId ||
    authority.descriptor.buildId !== seal.buildId ||
    authority.descriptor.appVersion !== seal.appVersion
  ) {
    throw new BrowserReleaseGateError("The package seal and build descriptor disagree");
  }
  const releaseRoot = await requireDirectory(join(root, "artifacts"));
  await requireExactDirectoryEntries(
    releaseRoot,
    artifacts.map((artifact) => artifact.filename).sort(),
  );
  for (const artifact of artifacts) {
    const path = join(releaseRoot, artifact.filename);
    const info = await requireFile(path);
    if (info.size !== artifact.sizeBytes || await sha256File(path) !== artifact.sha256) {
      throw new BrowserReleaseGateError("A sealed release artifact changed");
    }
  }
  return Object.freeze({
    root,
    authorityRoot: root,
    releaseRoot,
    authority,
    artifacts: Object.freeze(artifacts),
    packageSealSha256: sha256(sealBytes.bytes),
  });
}

export function requireExactTargetArtifactContracts(artifacts, target, appVersion) {
  const osName = target.platform === "darwin" ? "mac" : "win";
  const stem = `Bluey-Browser-${appVersion}-${osName}-${target.architecture}`;
  const expected = target.packages.map(
    (entry) => `${entry.packageKind}:${stem}${entry.extension}`,
  );
  const actual = artifacts.map(
    (artifact) => `${artifact.packageKind}:${artifact.filename}`,
  );
  if (actual.join("\n") !== expected.join("\n")) {
    throw new BrowserReleaseGateError(
      "Artifact filename does not match its exact target package kind",
    );
  }
}

async function readBuildAuthority(authorityDirectory, target) {
  const root = await requireDirectory(authorityDirectory);
  await requireFilesExist(root, AUTHORITY_FILES);
  const [descriptorBytes, signatureText, keyringBytes] = await Promise.all([
    readBoundedFile(
      join(root, "build-descriptor.txt"),
      4096,
      "Invalid build descriptor",
    ),
    readFile(join(root, "build-descriptor.sig"), "utf8"),
    readBoundedFile(
      join(root, "build-public-keys.json"),
      MAX_JSON_BYTES,
      "Invalid build keyring",
    ),
  ]);
  if (!signatureText.endsWith("\n") || signatureText.trimEnd() !== signatureText.slice(0, -1)) {
    throw new BrowserReleaseGateError("Invalid build descriptor signature file");
  }
  const signature = signatureText.slice(0, -1);
  requireBase64Url(signature, 64, "Invalid build descriptor signature");
  const descriptor = parseBuildDescriptor(descriptorBytes);
  if (
    descriptor.platform !== target.platform ||
    descriptor.architecture !== target.architecture
  ) {
    throw new BrowserReleaseGateError("Build descriptor target mismatch");
  }
  const keyring = parseKeyring(keyringBytes, BUILD_KEYRING_AUDIENCE);
  verifyEd25519(
    descriptorBytes,
    signature,
    keyring.keys[descriptor.signingKeyId],
    "The build descriptor signature is invalid",
  );
  return Object.freeze({
    descriptor,
    descriptorSha256: sha256(
      Buffer.concat([
        descriptorBytes,
        Buffer.from(`signature=${signature}\n`, "utf8"),
      ]),
    ),
  });
}

function parseBuildDescriptor(bytes) {
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const prefixes = [
    "version=",
    "audience=",
    "release_id=",
    "build_id=",
    "app_version=",
    "app_id=",
    "protocol_version=",
    "source_commit=",
    "platform=",
    "architecture=",
    "electron_version=",
    "playwright_version=",
    "chromium_revision=",
    "issued_at_ms=",
    "signing_key_id=",
  ];
  const lines = text.split("\n");
  if (lines.length !== prefixes.length + 1 || lines.at(-1) !== "") {
    throw new BrowserReleaseGateError("Invalid canonical build descriptor");
  }
  const values = prefixes.map((prefix, index) => {
    if (!lines[index].startsWith(prefix) || lines[index].length === prefix.length) {
      throw new BrowserReleaseGateError("Invalid canonical build descriptor");
    }
    return lines[index].slice(prefix.length);
  });
  if (values[0] !== "1" || values[1] !== BUILD_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid build descriptor audience");
  }
  const descriptor = Object.freeze({
    releaseId: values[2],
    buildId: values[3],
    appVersion: values[4],
    appId: values[5],
    protocolVersion: parsePositiveInteger(values[6], "Invalid protocol version"),
    sourceCommit: values[7],
    platform: values[8],
    architecture: values[9],
    electronVersion: values[10],
    playwrightVersion: values[11],
    chromiumRevision: values[12],
    issuedAtMs: parseNonnegativeInteger(values[13], "Invalid descriptor time"),
    signingKeyId: values[14],
  });
  requireReleaseId(descriptor.releaseId, "Invalid descriptor release identity");
  requirePattern(descriptor.buildId, BUILD_ID, "Invalid descriptor build identity");
  requirePattern(descriptor.appVersion, SEMVER, "Invalid descriptor app version");
  requirePattern(descriptor.sourceCommit, SOURCE_COMMIT, "Invalid descriptor source commit");
  requirePattern(descriptor.electronVersion, SEMVER, "Invalid Electron version");
  requirePattern(descriptor.playwrightVersion, SEMVER, "Invalid Playwright version");
  requireDecimal(descriptor.chromiumRevision, "Invalid Chromium revision");
  requireSafeId(descriptor.signingKeyId, "Invalid descriptor signing key");
  if (
    descriptor.appId !== "sh.bluey.jobs.browser" ||
    !["darwin", "windows"].includes(descriptor.platform) ||
    !["arm64", "x64"].includes(descriptor.architecture) ||
    (descriptor.platform === "windows" && descriptor.architecture !== "x64")
  ) {
    throw new BrowserReleaseGateError("Invalid descriptor runtime target");
  }
  return descriptor;
}

async function validateReleaseOutputs(root, target, appVersion) {
  const osName = target.platform === "darwin" ? "mac" : "win";
  const stem = `Bluey-Browser-${appVersion}-${osName}-${target.architecture}`;
  const expected = target.packages.map((entry) => ({
    filename: `${stem}${entry.extension}`,
    packageKind: entry.packageKind,
  }));
  const entries = await readdir(root, { withFileTypes: true });
  const primary = entries
    .filter((entry) => entry.isFile() && PRIMARY_PACKAGE.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  if (primary.join("\n") !== expected.map((entry) => entry.filename).sort().join("\n")) {
    throw new BrowserReleaseGateError(
      "Release output contains a stale, missing, or extra package",
    );
  }
  const artifacts = [];
  for (const entry of expected) {
    const path = join(root, entry.filename);
    const info = await requireFile(path);
    if (info.size < 1) throw new BrowserReleaseGateError("Release package is empty");
    artifacts.push(
      Object.freeze({
        ...entry,
        sizeBytes: info.size,
        sha256: await sha256File(path),
      }),
    );
  }
  return artifacts;
}

async function inventoryDirectory(root) {
  const entries = [];
  async function walk(directory, prefix) {
    const children = await readdir(directory, { withFileTypes: true });
    children.sort((left, right) => left.name.localeCompare(right.name));
    for (const child of children) {
      if (child.name.includes("\0") || child.name === "." || child.name === "..") {
        throw new BrowserReleaseGateError("Invalid packaged app path");
      }
      const path = join(directory, child.name);
      const info = await lstat(path);
      const logical = prefix ? `${prefix}/${child.name}` : child.name;
      requireSafeRelativePath(logical);
      if (info.isSymbolicLink()) {
        const linkTarget = await readlink(path);
        const contained = relative(root, resolve(directory, linkTarget));
        if (
          !linkTarget ||
          linkTarget.includes("\0") ||
          linkTarget.includes("\\") ||
          resolve(linkTarget) === linkTarget ||
          contained === ".." ||
          contained.startsWith(`..${sep}`)
        ) {
          throw new BrowserReleaseGateError(
            "Packaged app contains an escaping or invalid symlink",
          );
        }
        const targetBytes = Buffer.from(linkTarget, "utf8");
        entries.push(
          Object.freeze({
            path: logical,
            type: "symlink",
            mode: info.mode & 0o777,
            sizeBytes: targetBytes.length,
            sha256: sha256(targetBytes),
            linkTarget,
          }),
        );
      } else if (info.isDirectory()) {
        entries.push(
          Object.freeze({
            path: logical,
            type: "directory",
            mode: info.mode & 0o777,
            sizeBytes: 0,
            sha256: null,
            linkTarget: null,
          }),
        );
        await walk(path, logical);
      } else if (info.isFile()) {
        entries.push(
          Object.freeze({
            path: logical,
            type: "file",
            mode: info.mode & 0o777,
            sizeBytes: info.size,
            sha256: await sha256File(path),
            linkTarget: null,
          }),
        );
      } else {
        throw new BrowserReleaseGateError("Packaged app contains an unsupported file type");
      }
      if (entries.length > MAX_INVENTORY_ENTRIES) {
        throw new BrowserReleaseGateError("Packaged app inventory is too large");
      }
    }
  }
  await walk(root, "");
  entries.sort((left, right) => left.path.localeCompare(right.path));
  return Object.freeze(entries);
}

async function validateExecutableArchitecture(path, target) {
  const handle = await open(path, "r");
  const bytes = Buffer.alloc(4_096);
  let bytesRead;
  try {
    ({ bytesRead } = await handle.read(bytes, 0, bytes.length, 0));
  } finally {
    await handle.close();
  }
  const header = bytes.subarray(0, bytesRead);
  if (target.platform === "darwin") {
    if (header.length < 12 || header.readUInt32LE(0) !== 0xfeedfacf) {
      throw new BrowserReleaseGateError("Packaged Chromium is not a thin 64-bit Mach-O");
    }
    const expected = target.architecture === "arm64" ? 0x0100000c : 0x01000007;
    if (header.readUInt32LE(4) !== expected) {
      throw new BrowserReleaseGateError("Packaged Chromium architecture mismatch");
    }
    return;
  }
  if (header.length < 128 || header[0] !== 0x4d || header[1] !== 0x5a) {
    throw new BrowserReleaseGateError("Packaged Chromium is not a Windows PE executable");
  }
  const peOffset = header.readUInt32LE(0x3c);
  if (
    peOffset > header.length - 6 ||
    header.subarray(peOffset, peOffset + 4).toString("hex") !== "50450000" ||
    header.readUInt16LE(peOffset + 4) !== 0x8664
  ) {
    throw new BrowserReleaseGateError("Packaged Chromium architecture mismatch");
  }
}

function parseManifest(input) {
  requireExactKeys(input, [
    "appVersion",
    "artifacts",
    "audience",
    "buildId",
    "chromiumRevision",
    "electronVersion",
    "manifestGeneration",
    "manifestId",
    "playwrightVersion",
    "protocolVersion",
    "publishedAtMs",
    "releaseId",
    "releaseNotesUrl",
    "releaseSequence",
    "sourceCommit",
    "version",
  ]);
  if (input.version !== 1 || input.audience !== MANIFEST_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid release manifest audience");
  }
  const artifacts = requireArray(input.artifacts, 5, 5, "Invalid manifest artifacts")
    .map(parseManifestArtifact);
  let prior = "";
  const descriptorByTarget = new Map();
  for (const artifact of artifacts) {
    const identity = artifactIdentity(artifact);
    if (identity <= prior) throw new BrowserReleaseGateError("Manifest artifacts are not sorted");
    prior = identity;
    const targetIdentity = `${artifact.platform}:${artifact.architecture}`;
    const priorDescriptor = descriptorByTarget.get(targetIdentity);
    if (priorDescriptor && priorDescriptor !== artifact.buildDescriptorSha256) {
      throw new BrowserReleaseGateError("One target has conflicting build descriptors");
    }
    descriptorByTarget.set(targetIdentity, artifact.buildDescriptorSha256);
  }
  const expectedTargets = [
    "darwin:arm64:darwin-dmg",
    "darwin:arm64:darwin-zip",
    "darwin:x64:darwin-dmg",
    "darwin:x64:darwin-zip",
    "windows:x64:windows-nsis",
  ];
  if (
    artifacts.map(artifactTargetIdentity).join("\n") !== expectedTargets.join("\n") ||
    new Set(artifacts.map((artifact) => artifact.artifactId)).size !== 5 ||
    new Set(artifacts.map((artifact) => artifact.sha256)).size !== 5 ||
    descriptorByTarget.size !== 3 ||
    new Set(descriptorByTarget.values()).size !== 3
  ) {
    throw new BrowserReleaseGateError(
      "The manifest must bind exactly five distinct native target artifacts",
    );
  }
  validateManifestTargetContentBindings(artifacts);
  const manifest = Object.freeze({
    version: 1,
    audience: MANIFEST_AUDIENCE,
    manifestId: requireSafeId(input.manifestId, "Invalid manifest identity"),
    manifestGeneration: requirePositiveInteger(
      input.manifestGeneration,
      "Invalid manifest generation",
    ),
    releaseId: requireReleaseId(input.releaseId, "Invalid release identity"),
    releaseSequence: requirePositiveInteger(
      input.releaseSequence,
      "Invalid release sequence",
    ),
    buildId: requirePattern(input.buildId, BUILD_ID, "Invalid build identity"),
    appVersion: requirePattern(input.appVersion, SEMVER, "Invalid app version"),
    protocolVersion: requirePositiveInteger(
      input.protocolVersion,
      "Invalid protocol version",
    ),
    sourceCommit: requirePattern(input.sourceCommit, SOURCE_COMMIT, "Invalid source commit"),
    electronVersion: requirePattern(
      input.electronVersion,
      SEMVER,
      "Invalid Electron version",
    ),
    playwrightVersion: requirePattern(
      input.playwrightVersion,
      SEMVER,
      "Invalid Playwright version",
    ),
    chromiumRevision: requireDecimal(input.chromiumRevision, "Invalid Chromium revision"),
    publishedAtMs: requireNonnegativeInteger(
      input.publishedAtMs,
      "Invalid publication time",
    ),
    releaseNotesUrl: requireImmutableReleaseUrl(
      input.releaseNotesUrl,
      input.releaseId,
    ),
    artifacts: Object.freeze(artifacts),
  });
  for (const artifact of artifacts) {
    requireArtifactPackageUrl(
      artifact.url,
      manifest.releaseId,
      artifact.packageKind,
    );
  }
  return manifest;
}

function parseManifestArtifact(input) {
  requireExactKeys(input, [
    "appContentSha256",
    "architecture",
    "artifactId",
    "buildDescriptorSha256",
    "nativeSignatureKind",
    "nativeSignerIdentity",
    "packageKind",
    "platform",
    "sha256",
    "sizeBytes",
    "url",
    "verificationEvidenceSha256",
  ]);
  if (!["darwin", "windows"].includes(input.platform)) {
    throw new BrowserReleaseGateError("Invalid artifact platform");
  }
  if (!["arm64", "x64"].includes(input.architecture)) {
    throw new BrowserReleaseGateError("Invalid artifact architecture");
  }
  const target = requireReleaseTarget(`${input.platform}-${input.architecture}`);
  if (
    !target.packages.some((entry) => entry.packageKind === input.packageKind) ||
    target.nativeSignatureKind !== input.nativeSignatureKind
  ) {
    throw new BrowserReleaseGateError("Invalid artifact package authority");
  }
  return Object.freeze({
    artifactId: requireSafeId(input.artifactId, "Invalid artifact identity"),
    platform: input.platform,
    architecture: input.architecture,
    packageKind: input.packageKind,
    buildDescriptorSha256: requirePattern(
      input.buildDescriptorSha256,
      HEX_64,
      "Invalid build-descriptor digest",
    ),
    url: requireBoundedString(input.url, 1, 2_048, "Invalid artifact URL"),
    sizeBytes: requirePositiveInteger(input.sizeBytes, "Invalid artifact size"),
    sha256: requirePattern(input.sha256, HEX_64, "Invalid artifact digest"),
    appContentSha256: requirePattern(
      input.appContentSha256,
      HEX_64,
      "Invalid app-content digest",
    ),
    verificationEvidenceSha256: requirePattern(
      input.verificationEvidenceSha256,
      HEX_64,
      "Invalid verification-evidence digest",
    ),
    nativeSignatureKind: input.nativeSignatureKind,
    nativeSignerIdentity: requireBoundedString(
      input.nativeSignerIdentity,
      3,
      256,
      "Invalid native signer identity",
    ),
  });
}

function parseTrustPolicy(input) {
  requireExactKeys(input, [
    "artifactOrigin",
    "audience",
    "expiresAtMs",
    "issuedAtMs",
    "keys",
    "policyId",
    "predecessorPolicySha256",
    "roles",
    "trustGeneration",
    "validFromMs",
    "version",
  ]);
  if (input.version !== 1 || input.audience !== TRUST_POLICY_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid Browser trust-policy audience");
  }
  const trustGeneration = requirePositiveInteger(
    input.trustGeneration,
    "Invalid trust generation",
  );
  const issuedAtMs = requireNonnegativeInteger(input.issuedAtMs, "Invalid policy issuance");
  const validFromMs = requireNonnegativeInteger(input.validFromMs, "Invalid policy validity");
  const expiresAtMs = requireNonnegativeInteger(input.expiresAtMs, "Invalid policy expiry");
  const roles = requireArray(input.roles, 4, 4, "Invalid Browser trust roles")
    .map((role) => {
      requireExactKeys(role, ["role", "threshold"]);
      return Object.freeze({
        role: requireAuthorityRole(role.role),
        threshold: requirePositiveInteger(role.threshold, "Invalid role threshold"),
      });
    });
  if (roles.map((role) => role.role).join(",") !== "incident,promotion,release,root") {
    throw new BrowserReleaseGateError("Browser trust roles are not canonical");
  }
  const keys = requireArray(input.keys, 4, 64, "Invalid Browser trust keys")
    .map((key) => parseTrustKey(key));
  requireStrictlySorted(keys.map((key) => key.keyId), "Browser trust keys are not canonical");
  const policy = Object.freeze({
    version: 1,
    audience: TRUST_POLICY_AUDIENCE,
    policyId: requireSafeId(input.policyId, "Invalid trust-policy identity"),
    trustGeneration,
    predecessorPolicySha256: input.predecessorPolicySha256 === null
      ? null
      : requirePattern(
        input.predecessorPolicySha256,
        HEX_64,
        "Invalid predecessor policy digest",
      ),
    artifactOrigin: requireArtifactOrigin(input.artifactOrigin),
    issuedAtMs,
    validFromMs,
    expiresAtMs,
    roles: Object.freeze(roles),
    keys: Object.freeze(keys),
  });
  if (
    validFromMs > issuedAtMs ||
    issuedAtMs >= expiresAtMs ||
    (trustGeneration === 1) !== (policy.predecessorPolicySha256 === null)
  ) {
    throw new BrowserReleaseGateError("Invalid Browser trust-policy lifetime");
  }
  for (const role of roles) {
    const active = keys.filter(
      (key) =>
        key.role === role.role &&
        key.state === "active" &&
        keyAuthorizes(key, trustGeneration, issuedAtMs),
    ).length;
    if (active < role.threshold) {
      throw new BrowserReleaseGateError("The trust policy cannot meet every role threshold");
    }
  }
  return policy;
}

function parseTrustKey(input) {
  requireExactKeys(input, [
    "keyId",
    "maximumTrustGeneration",
    "minimumTrustGeneration",
    "publicKey",
    "role",
    "state",
    "validFromMs",
    "validUntilMs",
  ]);
  const validFromMs = requireNonnegativeInteger(input.validFromMs, "Invalid key validity");
  const validUntilMs = requireNonnegativeInteger(input.validUntilMs, "Invalid key validity");
  const minimumTrustGeneration = requirePositiveInteger(
    input.minimumTrustGeneration,
    "Invalid key generation",
  );
  const maximumTrustGeneration = requirePositiveInteger(
    input.maximumTrustGeneration,
    "Invalid key generation",
  );
  if (
    validUntilMs < validFromMs ||
    maximumTrustGeneration < minimumTrustGeneration
  ) {
    throw new BrowserReleaseGateError("Invalid Browser trust-key authority window");
  }
  if (!["active", "retired", "revoked"].includes(input.state)) {
    throw new BrowserReleaseGateError("Invalid Browser trust-key state");
  }
  return Object.freeze({
    keyId: requireSafeId(input.keyId, "Invalid trust-key identity"),
    role: requireAuthorityRole(input.role),
    publicKey: requireBase64Url(input.publicKey, 32, "Invalid trust public key"),
    state: input.state,
    validFromMs,
    validUntilMs,
    minimumTrustGeneration,
    maximumTrustGeneration,
  });
}

function parseSignatureSet(input) {
  requireExactKeys(input, [
    "audience",
    "role",
    "signatureSetId",
    "signatures",
    "signedAtMs",
    "targetAudience",
    "targetSha256",
    "trustGeneration",
    "version",
  ]);
  if (input.version !== 1 || input.audience !== SIGNATURE_SET_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid Browser signature-set audience");
  }
  const signatures = requireArray(input.signatures, 1, 32, "Invalid detached signatures")
    .map((signature) => {
      requireExactKeys(signature, ["keyId", "signature"]);
      return Object.freeze({
        keyId: requireSafeId(signature.keyId, "Invalid detached signer identity"),
        signature: requireBase64Url(
          signature.signature,
          64,
          "Invalid detached signature",
        ),
      });
    });
  requireStrictlySorted(
    signatures.map((signature) => signature.keyId),
    "Detached signatures are not canonical",
  );
  return Object.freeze({
    version: 1,
    audience: SIGNATURE_SET_AUDIENCE,
    signatureSetId: requireSafeId(input.signatureSetId, "Invalid signature-set identity"),
    trustGeneration: requirePositiveInteger(
      input.trustGeneration,
      "Invalid signature-set generation",
    ),
    role: requireAuthorityRole(input.role),
    targetAudience: requireSignedAudience(input.targetAudience),
    targetSha256: requirePattern(
      input.targetSha256,
      HEX_64,
      "Invalid signature-set target digest",
    ),
    signedAtMs: requireNonnegativeInteger(input.signedAtMs, "Invalid signature time"),
    signatures: Object.freeze(signatures),
  });
}

function signatureSetPayload(signatureSet) {
  return Object.freeze({
    version: signatureSet.version,
    audience: signatureSet.audience,
    signatureSetId: signatureSet.signatureSetId,
    trustGeneration: signatureSet.trustGeneration,
    role: signatureSet.role,
    targetAudience: signatureSet.targetAudience,
    targetSha256: signatureSet.targetSha256,
    signedAtMs: signatureSet.signedAtMs,
  });
}

function parseActivation(input) {
  requireExactKeys(input, [
    "acceptedServerReleaseIds",
    "activationGeneration",
    "activationId",
    "audience",
    "canaryEvidenceSha256",
    "channel",
    "channelSequence",
    "expiresAtMs",
    "issuedAtMs",
    "manifestSha256",
    "signatureSetSha256",
    "trustGeneration",
    "version",
  ]);
  if (input.version !== 1 || input.audience !== ACTIVATION_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid Browser activation audience");
  }
  const acceptedServerReleaseIds = requireArray(
    input.acceptedServerReleaseIds,
    1,
    32,
    "Invalid accepted server releases",
  ).map((value) => requireSafeId(value, "Invalid accepted server release"));
  requireStrictlySorted(
    acceptedServerReleaseIds,
    "Accepted server releases are not canonical",
  );
  if (!["internal", "beta", "stable"].includes(input.channel)) {
    throw new BrowserReleaseGateError("Invalid Browser activation channel");
  }
  const issuedAtMs = requireNonnegativeInteger(input.issuedAtMs, "Invalid activation time");
  const expiresAtMs = requireNonnegativeInteger(input.expiresAtMs, "Invalid activation expiry");
  if (expiresAtMs <= issuedAtMs) {
    throw new BrowserReleaseGateError("Invalid Browser activation lifetime");
  }
  return Object.freeze({
    version: 1,
    audience: ACTIVATION_AUDIENCE,
    activationId: requireSafeId(input.activationId, "Invalid activation identity"),
    activationGeneration: requirePositiveInteger(
      input.activationGeneration,
      "Invalid activation generation",
    ),
    trustGeneration: requirePositiveInteger(
      input.trustGeneration,
      "Invalid activation trust generation",
    ),
    channel: input.channel,
    channelSequence: requirePositiveInteger(
      input.channelSequence,
      "Invalid activation channel sequence",
    ),
    manifestSha256: requirePattern(
      input.manifestSha256,
      HEX_64,
      "Invalid activation manifest digest",
    ),
    signatureSetSha256: requirePattern(
      input.signatureSetSha256,
      HEX_64,
      "Invalid activation manifest signature-set digest",
    ),
    acceptedServerReleaseIds: Object.freeze(acceptedServerReleaseIds),
    canaryEvidenceSha256: requirePattern(
      input.canaryEvidenceSha256,
      HEX_64,
      "Invalid activation canary digest",
    ),
    issuedAtMs,
    expiresAtMs,
  });
}

function parseCanaryEvidence(input) {
  requireExactKeys(input, [
    "artifactSha256s",
    "audience",
    "canaryEvidenceId",
    "checks",
    "manifestSha256",
    "nativeTargets",
    "observedAtMs",
    "sourceCommit",
    "version",
  ]);
  if (input.version !== 1 || input.audience !== CANARY_AUDIENCE) {
    throw new BrowserReleaseGateError("Invalid Browser canary evidence audience");
  }
  const nativeTargets = requireArray(input.nativeTargets, 3, 3, "Invalid canary targets")
    .map((target) => requireReleaseTarget(target).target);
  if (nativeTargets.join("\n") !== [...TARGET_NAMES].sort().join("\n")) {
    throw new BrowserReleaseGateError("Canary evidence does not cover all native targets");
  }
  const artifactSha256s = requireArray(
    input.artifactSha256s,
    5,
    5,
    "Invalid canary artifacts",
  ).map((digest) => requirePattern(digest, HEX_64, "Invalid canary artifact digest"));
  requireStrictlySorted(artifactSha256s, "Canary artifact digests are not canonical");
  const checks = requireArray(input.checks, 1, 64, "Invalid canary checks")
    .map((check) => {
      requireExactKeys(check, ["checkId", "evidenceSha256", "status"]);
      if (check.status !== "passed") {
        throw new BrowserReleaseGateError("Every production canary check must pass");
      }
      return Object.freeze({
        checkId: requireSafeId(check.checkId, "Invalid canary check identity"),
        status: "passed",
        evidenceSha256: requirePattern(
          check.evidenceSha256,
          HEX_64,
          "Invalid canary check evidence digest",
        ),
      });
    });
  requireStrictlySorted(
    checks.map((check) => check.checkId),
    "Canary checks are not canonical",
  );
  return Object.freeze({
    version: 1,
    audience: CANARY_AUDIENCE,
    canaryEvidenceId: requireSafeId(input.canaryEvidenceId, "Invalid canary identity"),
    manifestSha256: requirePattern(
      input.manifestSha256,
      HEX_64,
      "Invalid canary manifest digest",
    ),
    sourceCommit: requirePattern(input.sourceCommit, SOURCE_COMMIT, "Invalid canary source"),
    observedAtMs: requireNonnegativeInteger(input.observedAtMs, "Invalid canary time"),
    nativeTargets: Object.freeze(nativeTargets),
    artifactSha256s: Object.freeze(artifactSha256s),
    checks: Object.freeze(checks),
  });
}

function validateCanaryBindings(canary, manifest, requireProductionCanaries) {
  const expectedArtifacts = manifest.artifacts.map((artifact) => artifact.sha256).sort();
  if (
    canary.manifestSha256 !== sha256(authorityJsonBytes(manifest)) ||
    canary.sourceCommit !== manifest.sourceCommit ||
    canary.observedAtMs < manifest.publishedAtMs ||
    canary.artifactSha256s.join("\n") !== expectedArtifacts.join("\n")
  ) {
    throw new BrowserReleaseGateError(
      "Canary evidence does not bind the exact five-artifact manifest",
    );
  }
  if (
    requireProductionCanaries &&
    canary.checks.map((check) => check.checkId).join("\n") !==
      BROWSER_PRODUCTION_CANARY_CHECK_IDS.join("\n")
  ) {
    throw new BrowserReleaseGateError(
      "Production-ready promotion requires the exact physical canary matrix",
    );
  }
}

function verifyAuthoritySignatureSet({
  targetBytes,
  signatureSet,
  policy,
  requiredRole,
  targetAudience,
  targetIssuedAtMs,
  targetTrustGeneration,
  verificationTimeMs,
  allowRetiredHistoricalKeys,
}) {
  requireNonnegativeInteger(verificationTimeMs, "Invalid authority verification time");
  if (verificationTimeMs < policy.validFromMs || verificationTimeMs >= policy.expiresAtMs) {
    throw new BrowserReleaseGateError("The Browser trust policy is not current");
  }
  if (
    targetIssuedAtMs > verificationTimeMs ||
    signatureSet.signedAtMs > verificationTimeMs
  ) {
    throw new BrowserReleaseGateError("Release authority is future-dated");
  }
  if (
    targetTrustGeneration !== undefined &&
      (targetTrustGeneration !== policy.trustGeneration ||
        signatureSet.trustGeneration !== targetTrustGeneration) ||
    targetTrustGeneration === undefined &&
      signatureSet.trustGeneration > policy.trustGeneration ||
    signatureSet.role !== requiredRole ||
    signatureSet.targetAudience !== targetAudience ||
    signatureSet.targetSha256 !== sha256(targetBytes) ||
    signatureSet.signedAtMs !== targetIssuedAtMs
  ) {
    throw new BrowserReleaseGateError("Detached signature-set binding mismatch");
  }
  const threshold = policy.roles.find((role) => role.role === requiredRole)?.threshold;
  if (!threshold) throw new BrowserReleaseGateError("Missing trust-policy role");
  const keyById = new Map(policy.keys.map((key) => [key.keyId, key]));
  const payload = authorityJsonBytes(signatureSetPayload(signatureSet));
  let authorized = 0;
  for (const detached of signatureSet.signatures) {
    const key = keyById.get(detached.keyId);
    if (!key || key.role !== requiredRole) {
      throw new BrowserReleaseGateError("A detached signer has the wrong authority role");
    }
    if (
      key.state === "revoked" ||
      !keyAuthorizes(key, signatureSet.trustGeneration, signatureSet.signedAtMs) ||
      key.state === "retired" &&
        (!allowRetiredHistoricalKeys ||
          targetIssuedAtMs >= policy.issuedAtMs ||
          targetIssuedAtMs > key.validUntilMs)
    ) {
      throw new BrowserReleaseGateError("A detached signer is not authorized");
    }
    verifyEd25519(
      payload,
      detached.signature,
      key.publicKey,
      "A detached threshold signature is invalid",
    );
    authorized += 1;
  }
  if (authorized < threshold) {
    throw new BrowserReleaseGateError("The detached signature threshold was not met");
  }
}

function keyAuthorizes(key, trustGeneration, signedAtMs) {
  return trustGeneration >= key.minimumTrustGeneration &&
    trustGeneration <= key.maximumTrustGeneration &&
    signedAtMs >= key.validFromMs &&
    signedAtMs <= key.validUntilMs;
}

function requireAuthorityRole(value) {
  if (!["incident", "promotion", "release", "root"].includes(value)) {
    throw new BrowserReleaseGateError("Invalid Browser authority role");
  }
  return value;
}

function requireSignedAudience(value) {
  if (![
    ACTIVATION_AUDIENCE,
    MANIFEST_AUDIENCE,
    "bluey-jobs-browser-release-revocation-v1",
    "bluey-jobs-browser-release-rollback-v1",
    TRUST_POLICY_AUDIENCE,
  ].includes(value)) {
    throw new BrowserReleaseGateError("Invalid Browser signed audience");
  }
  return value;
}

function requireStrictlySorted(values, message) {
  let prior = "";
  for (const value of values) {
    if (value <= prior) throw new BrowserReleaseGateError(message);
    prior = value;
  }
}

function artifactTargetIdentity(artifact) {
  return `${artifact.platform}:${artifact.architecture}:${artifact.packageKind}`;
}

function authorityJsonBytes(value) {
  return Buffer.from(`${JSON.stringify(value)}\n`, "utf8");
}

async function readAuthorityJsonBytes(path, parser, maximum, message) {
  const bytes = await readBoundedFile(path, maximum, message);
  let input;
  try {
    input = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new BrowserReleaseGateError(message);
  }
  const value = parser(input);
  if (!authorityJsonBytes(value).equals(bytes)) {
    throw new BrowserReleaseGateError(message);
  }
  return Object.freeze({ bytes, value });
}

function parseKeyring(bytes, audience) {
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new BrowserReleaseGateError("Invalid release keyring JSON");
  }
  requireExactKeys(value, ["audience", "keys", "version"]);
  if (value.version !== 1 || value.audience !== audience) {
    throw new BrowserReleaseGateError("Invalid release keyring audience");
  }
  if (!value.keys || typeof value.keys !== "object" || Array.isArray(value.keys)) {
    throw new BrowserReleaseGateError("Invalid release keyring keys");
  }
  const entries = Object.entries(value.keys).sort(([left], [right]) =>
    left.localeCompare(right),
  );
  if (entries.length < 1 || entries.length > 16) {
    throw new BrowserReleaseGateError("Invalid release keyring size");
  }
  const keys = {};
  for (const [keyId, publicKey] of entries) {
    requireSafeId(keyId, "Invalid release key identity");
    requireBase64Url(publicKey, 32, "Invalid release public key");
    keys[keyId] = publicKey;
  }
  const canonical = authorityJsonBytes({ version: 1, audience, keys });
  if (!canonical.equals(bytes)) {
    throw new BrowserReleaseGateError("Release keyring is not canonical");
  }
  return Object.freeze({ version: 1, audience, keys: Object.freeze(keys) });
}

function verifyEd25519(message, signature, encodedKey, errorMessage) {
  if (typeof encodedKey !== "string") {
    throw new BrowserReleaseGateError(errorMessage);
  }
  requireBase64Url(encodedKey, 32, errorMessage);
  requireBase64Url(signature, 64, errorMessage);
  const publicKey = createPublicKey({
    format: "jwk",
    key: { crv: "Ed25519", kty: "OKP", x: encodedKey },
  });
  if (!verify(null, message, publicKey, Buffer.from(signature, "base64url"))) {
    throw new BrowserReleaseGateError(errorMessage);
  }
}

async function discoverPartDirectories(root) {
  const children = await readdir(root, { withFileTypes: true });
  if (children.length !== 3 || children.some((entry) => !entry.isDirectory())) {
    throw new BrowserReleaseGateError(
      "Candidate assembly requires exactly three immutable target parts",
    );
  }
  return children.map((entry) => join(root, entry.name));
}

async function visitDirectories(root, visitor) {
  await visitor(root);
  const entries = await readdir(root, { withFileTypes: true });
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    if (!entry.isDirectory()) continue;
    const child = join(root, entry.name);
    const info = await lstat(child);
    if (info.isSymbolicLink()) {
      throw new BrowserReleaseGateError("Release output cannot contain symlink directories");
    }
    await visitDirectories(child, visitor);
  }
}

async function safeDirectoryNames(path) {
  return (await readdir(path, { withFileTypes: true })).map((entry) => entry.name);
}

async function requireExactDirectoryEntries(path, expected) {
  const entries = await readdir(path, { withFileTypes: true });
  if (entries.some((entry) => entry.isSymbolicLink())) {
    throw new BrowserReleaseGateError("Release evidence cannot contain symlinks");
  }
  const actual = entries.map((entry) => entry.name).sort();
  if (actual.join("\n") !== [...expected].sort().join("\n")) {
    throw new BrowserReleaseGateError(
      "Release evidence contains a stale, missing, or extra output",
    );
  }
}

async function requireFilesExist(root, names) {
  for (const name of names) await requireFile(join(root, name));
}

async function requireDirectory(path) {
  const resolved = resolve(path);
  const info = await lstat(resolved).catch(() => null);
  if (!info || !info.isDirectory() || info.isSymbolicLink()) {
    throw new BrowserReleaseGateError("Expected a real release directory");
  }
  return resolved;
}

async function requireFile(path) {
  const info = await lstat(path).catch(() => null);
  if (!info || !info.isFile() || info.isSymbolicLink()) {
    throw new BrowserReleaseGateError("Expected a real release evidence file");
  }
  return info;
}

async function mkdirExclusive(path) {
  await mkdir(path, { recursive: false, mode: 0o700 }).catch(() => {
    throw new BrowserReleaseGateError("Release evidence output already exists");
  });
}

async function writeExclusive(path, bytes) {
  await writeFile(path, bytes, { flag: "wx", mode: 0o444 });
}

async function writeCanonicalJsonExclusive(path, value) {
  await writeExclusive(path, canonicalJsonBytes(value));
}

async function readCanonicalJsonBytes(path) {
  const bytes = await readBoundedFile(path, MAX_JSON_BYTES, "Invalid canonical JSON evidence");
  let value;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new BrowserReleaseGateError("Invalid canonical JSON evidence");
  }
  if (!canonicalJsonBytes(value).equals(bytes)) {
    throw new BrowserReleaseGateError("Release evidence JSON is not canonical");
  }
  return Object.freeze({ bytes, value });
}

async function readBoundedFile(path, maximum, message) {
  const info = await requireFile(path);
  if (info.size < 1 || info.size > maximum) throw new BrowserReleaseGateError(message);
  const bytes = await readFile(path);
  if (bytes.length !== info.size) throw new BrowserReleaseGateError(message);
  return bytes;
}

export function canonicalJsonBytes(value) {
  return Buffer.from(`${canonicalString(value)}\n`, "utf8");
}

function canonicalString(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) {
      throw new BrowserReleaseGateError("Canonical evidence requires safe integers");
    }
    return String(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalString).join(",")}]`;
  }
  if (!value || typeof value !== "object" || Object.getPrototypeOf(value) !== Object.prototype) {
    throw new BrowserReleaseGateError("Canonical evidence contains an unsupported value");
  }
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalString(value[key])}`)
    .join(",")}}`;
}

function parseToolVersion(value) {
  const index = value.indexOf("=");
  if (index < 1) throw new BrowserReleaseGateError("Invalid native tool version");
  const name = value.slice(0, index);
  const version = value.slice(index + 1);
  requireSafeId(name, "Invalid native tool name");
  requireBoundedString(version, 1, 256, "Invalid native tool version");
  return Object.freeze({ name, version });
}

export function requireArtifactOrigin(value) {
  requireBoundedString(value, 1, 2_048, "Invalid artifact origin");
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new BrowserReleaseGateError("Invalid artifact origin");
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.username ||
    url.password ||
    url.port ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    url.origin !== value
  ) {
    throw new BrowserReleaseGateError("Invalid artifact origin");
  }
  return value;
}

export function requireArtifactBaseUrl(value, releaseId, expectedOrigin) {
  const url = parseCanonicalReleaseUrl(value, releaseId, expectedOrigin);
  const segments = url.pathname.split("/");
  if (
    segments.length !== 5 ||
    segments[0] !== "" ||
    segments[1] !== "jobs" ||
    segments[2] !== "browser" ||
    segments[3] !== "releases" ||
    segments[4] !== releaseId
  ) {
    throw new BrowserReleaseGateError("Artifact base URL is not immutable");
  }
  return value;
}

export function requireImmutableArtifactUrl(value, releaseId, expectedOrigin) {
  const url = parseCanonicalReleaseUrl(value, releaseId, expectedOrigin);
  const segments = url.pathname.split("/");
  if (
    segments.length !== 6 ||
    segments[0] !== "" ||
    segments[1] !== "jobs" ||
    segments[2] !== "browser" ||
    segments[3] !== "releases" ||
    segments[4] !== releaseId ||
    !segments[5]
  ) {
    throw new BrowserReleaseGateError("Release URL is mutable or out of scope");
  }
  requireSafeFilename(segments[5]);
  return value;
}

export function requireArtifactPackageUrl(
  value,
  releaseId,
  packageKind,
  expectedOrigin,
) {
  requireImmutableArtifactUrl(value, releaseId, expectedOrigin);
  const packageContract = Object.values(BROWSER_RELEASE_TARGETS)
    .flatMap((target) => target.packages)
    .find((entry) => entry.packageKind === packageKind);
  const filename = new URL(value).pathname.split("/").at(-1);
  if (!packageContract || !filename?.endsWith(packageContract.extension)) {
    throw new BrowserReleaseGateError(
      "Artifact URL extension does not match its package kind",
    );
  }
  return value;
}

export function validateManifestTargetContentBindings(artifacts) {
  const contentByTarget = new Map();
  for (const artifact of artifacts) {
    const target = `${artifact.platform}:${artifact.architecture}`;
    const prior = contentByTarget.get(target);
    if (prior !== undefined && prior !== artifact.appContentSha256) {
      throw new BrowserReleaseGateError(
        "One native target has conflicting packaged app content",
      );
    }
    contentByTarget.set(target, artifact.appContentSha256);
  }
  return true;
}

export function requireImmutableReleaseUrl(value, releaseId, expectedOrigin) {
  const url = parseCanonicalReleaseUrl(value, releaseId, expectedOrigin);
  const segments = url.pathname.split("/");
  if (
    segments.length < 6 ||
    segments[0] !== "" ||
    segments[1] !== "jobs" ||
    segments[2] !== "browser" ||
    segments[3] !== "releases" ||
    segments[4] !== releaseId ||
    segments.slice(5).some((segment) => !segment || segment === "." || segment === "..")
  ) {
    throw new BrowserReleaseGateError("Release URL is mutable or out of scope");
  }
  return value;
}

function parseCanonicalReleaseUrl(value, releaseId, expectedOrigin) {
  requireBoundedString(value, 1, 2048, "Invalid immutable release URL");
  requireReleaseId(releaseId, "Invalid release identity");
  if (expectedOrigin !== undefined) requireArtifactOrigin(expectedOrigin);
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new BrowserReleaseGateError("Invalid immutable release URL");
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.username ||
    url.password ||
    url.port ||
    url.search ||
    url.hash ||
    url.toString() !== value ||
    url.pathname.includes("%") ||
    expectedOrigin !== undefined && url.origin !== expectedOrigin
  ) {
    throw new BrowserReleaseGateError("Release URL is mutable or out of scope");
  }
  return url;
}

function artifactIdentity(artifact) {
  return `${artifact.platform}:${artifact.architecture}:${artifact.packageKind}:${artifact.artifactId}`;
}

function requireExactKeys(value, keys) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new BrowserReleaseGateError("Release evidence must be an object");
  }
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.join("\n") !== expected.join("\n")) {
    throw new BrowserReleaseGateError("Release evidence has unknown or missing fields");
  }
}

function requireArray(value, minimum, maximum, message) {
  if (!Array.isArray(value) || value.length < minimum || value.length > maximum) {
    throw new BrowserReleaseGateError(message);
  }
  return value;
}

function requirePattern(value, pattern, message) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new BrowserReleaseGateError(message);
  }
  return value;
}

function requireSafeId(value, message) {
  return requirePattern(value, SAFE_ID, message);
}

function requireReleaseId(value, message) {
  const releaseId = requireSafeId(value, message);
  if (MUTABLE_RELEASE_IDS.has(releaseId.toLowerCase())) {
    throw new BrowserReleaseGateError(message);
  }
  return releaseId;
}

function requireSafeFilename(value) {
  requireBoundedString(value, 1, 256, "Invalid artifact filename");
  if (basename(value) !== value || value === "." || value === "..") {
    throw new BrowserReleaseGateError("Invalid artifact filename");
  }
}

function requireSafeRelativePath(value) {
  requireBoundedString(value, 1, 4096, "Invalid relative release path");
  if (
    value.startsWith("/") ||
    value.includes("\\") ||
    value.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new BrowserReleaseGateError("Invalid relative release path");
  }
}

function requireBoundedString(value, minimum, maximum, message) {
  if (
    typeof value !== "string" ||
    Buffer.byteLength(value, "utf8") < minimum ||
    Buffer.byteLength(value, "utf8") > maximum ||
    value.trim() !== value ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    throw new BrowserReleaseGateError(message);
  }
  return value;
}

function requireNonempty(value, message) {
  if (!nonempty(value)) throw new BrowserReleaseGateError(message);
  return value;
}

function nonempty(value) {
  return typeof value === "string" && value.trim() !== "";
}

function requireDecimal(value, message) {
  return requirePattern(value, /^(0|[1-9][0-9]{0,12})$/, message);
}

function requireNonnegativeInteger(value, message) {
  if (
    !Number.isSafeInteger(value) ||
    value < 0 ||
    value > MAX_SAFE_GENERATION
  ) {
    throw new BrowserReleaseGateError(message);
  }
  return value;
}

function requirePositiveInteger(value, message) {
  requireNonnegativeInteger(value, message);
  if (value < 1) throw new BrowserReleaseGateError(message);
  return value;
}

function parseNonnegativeInteger(value, message) {
  if (!/^(0|[1-9][0-9]{0,15})$/.test(value)) {
    throw new BrowserReleaseGateError(message);
  }
  return requireNonnegativeInteger(Number(value), message);
}

function parsePositiveInteger(value, message) {
  const parsed = parseNonnegativeInteger(value, message);
  if (parsed < 1) throw new BrowserReleaseGateError(message);
  return parsed;
}

function requireBase64Url(value, expectedBytes, message) {
  requirePattern(value, BASE64URL, message);
  const bytes = Buffer.from(value, "base64url");
  if (bytes.length !== expectedBytes || bytes.toString("base64url") !== value) {
    throw new BrowserReleaseGateError(message);
  }
  return value;
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function sha256File(path) {
  await requireFile(path);
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return hash.digest("hex");
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    if (!name.startsWith("--") || name.length < 3) {
      throw new BrowserReleaseGateError("Invalid release gate argument");
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new BrowserReleaseGateError(`Missing value for ${name}`);
    }
    index += 1;
    const key = name.slice(2);
    if (key === "tool-version") {
      const current = values.get(key) ?? [];
      current.push(value);
      values.set(key, current);
    } else if (values.has(key)) {
      throw new BrowserReleaseGateError(`Duplicate release gate argument ${name}`);
    } else {
      values.set(key, value);
    }
  }
  return values;
}

function requiredOption(options, name) {
  const value = options.get(name);
  if (typeof value !== "string" || !value) {
    throw new BrowserReleaseGateError(`Missing --${name}`);
  }
  return value;
}

function optionalOption(options, name) {
  const value = options.get(name);
  return typeof value === "string" ? value : undefined;
}

function assertOnlyOptions(options, names) {
  const allowed = new Set(names);
  for (const name of options.keys()) {
    if (!allowed.has(name)) {
      throw new BrowserReleaseGateError(`Unknown release gate option --${name}`);
    }
  }
}

function promotionExpectedOptions(options) {
  return {
    expectedSourceCommit: requiredOption(options, "expected-source-commit"),
    expectedReleaseId: requiredOption(options, "expected-release-id"),
    expectedManifestSha256: requiredOption(options, "expected-manifest-sha256"),
    expectedTrustPolicySha256: requiredOption(
      options,
      "expected-trust-policy-sha256",
    ),
    expectedManifestSignatureSetSha256: requiredOption(
      options,
      "expected-manifest-signature-set-sha256",
    ),
    expectedActivationSha256: requiredOption(options, "expected-activation-sha256"),
    expectedActivationSignatureSetSha256: requiredOption(
      options,
      "expected-activation-signature-set-sha256",
    ),
    verificationTimeMs: parseNonnegativeInteger(
      requiredOption(options, "verification-time-ms"),
      "Invalid authority verification time",
    ),
    requireProductionReady:
      optionalOption(options, "require-production-ready") === "true",
  };
}

async function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  const options = parseArguments(rest);
  if (command === "credentials") {
    assertOnlyOptions(options, ["target"]);
    requireWorkflowCredentials(requiredOption(options, "target"));
    console.log("Bluey Browser release credentials are complete for the selected target");
    return;
  }
  if (command === "extract-prepared") {
    assertOnlyOptions(options, ["archive", "out-parent"]);
    const { extractPreparedArtifactArchive } = await import(
      "../browser/scripts/release-package-contract.mjs"
    );
    console.log(
      await extractPreparedArtifactArchive({
        archivePath: requiredOption(options, "archive"),
        outputParent: requiredOption(options, "out-parent"),
      }),
    );
    return;
  }
  if (command === "locate-resources") {
    assertOnlyOptions(options, ["release-dir", "target"]);
    console.log(
      await locatePackagedResources(
        requiredOption(options, "release-dir"),
        requiredOption(options, "target"),
      ),
    );
    return;
  }
  if (command === "inventory") {
    assertOnlyOptions(options, [
      "authority-dir",
      "out",
      "package-seal-sha256",
      "resources-dir",
      "target",
    ]);
    await createAppContentInventory({
      targetName: requiredOption(options, "target"),
      resourcesDirectory: requiredOption(options, "resources-dir"),
      authorityDirectory: requiredOption(options, "authority-dir"),
      packageSealSha256: requiredOption(options, "package-seal-sha256"),
      outputPath: requiredOption(options, "out"),
    });
    return;
  }
  if (command === "record-native") {
    assertOnlyOptions(options, [
      "out",
      "package-seal-sha256",
      "signer-identity",
      "target",
      "tool-version",
      "transcript",
    ]);
    await recordNativeVerification({
      targetName: requiredOption(options, "target"),
      signerIdentity: requiredOption(options, "signer-identity"),
      transcriptPath: requiredOption(options, "transcript"),
      packageSealSha256: requiredOption(options, "package-seal-sha256"),
      toolVersions: options.get("tool-version") ?? [],
      outputPath: requiredOption(options, "out"),
    });
    return;
  }
  if (command === "seal") {
    assertOnlyOptions(options, [
      "authority-dir",
      "out",
      "release-dir",
      "source-commit",
      "target",
    ]);
    const sealed = await sealCandidateTarget({
      targetName: requiredOption(options, "target"),
      releaseDirectory: requiredOption(options, "release-dir"),
      authorityDirectory: requiredOption(options, "authority-dir"),
      sourceCommit: requiredOption(options, "source-commit"),
      outputDirectory: requiredOption(options, "out"),
    });
    console.log(sealed.packageSealSha256);
    return;
  }
  if (command === "validate-seal") {
    assertOnlyOptions(options, ["seal-dir", "source-commit", "target"]);
    const sealed = await validateSealedCandidateTarget({
      targetName: requiredOption(options, "target"),
      sealDirectory: requiredOption(options, "seal-dir"),
      sourceCommit: requiredOption(options, "source-commit"),
    });
    console.log(sealed.packageSealSha256);
    return;
  }
  if (command === "collect") {
    assertOnlyOptions(options, [
      "inventory",
      "native-verification",
      "out",
      "seal-dir",
      "source-commit",
      "target",
      "transcript",
    ]);
    await collectCandidateTarget({
      targetName: requiredOption(options, "target"),
      sealDirectory: requiredOption(options, "seal-dir"),
      inventoryPath: requiredOption(options, "inventory"),
      nativeVerificationPath: requiredOption(options, "native-verification"),
      transcriptPath: requiredOption(options, "transcript"),
      sourceCommit: requiredOption(options, "source-commit"),
      outputDirectory: requiredOption(options, "out"),
    });
    return;
  }
  if (command === "assemble") {
    assertOnlyOptions(options, [
      "artifact-base-url",
      "candidate-run-id",
      "manifest-generation",
      "manifest-id",
      "out",
      "parts-dir",
      "published-at-ms",
      "release-notes-url",
      "release-sequence",
      "repository",
      "trust-policy",
      "trust-policy-sha256",
    ]);
    await assembleCandidateSet({
      partsDirectory: requiredOption(options, "parts-dir"),
      outputDirectory: requiredOption(options, "out"),
      repository: requiredOption(options, "repository"),
      candidateRunId: requiredOption(options, "candidate-run-id"),
      manifestId: requiredOption(options, "manifest-id"),
      manifestGeneration: parsePositiveInteger(
        requiredOption(options, "manifest-generation"),
        "Invalid manifest generation",
      ),
      releaseSequence: parsePositiveInteger(
        requiredOption(options, "release-sequence"),
        "Invalid release sequence",
      ),
      publishedAtMs: parseNonnegativeInteger(
        requiredOption(options, "published-at-ms"),
        "Invalid publication time",
      ),
      releaseNotesUrl: requiredOption(options, "release-notes-url"),
      artifactBaseUrl: requiredOption(options, "artifact-base-url"),
      trustPolicyPath: requiredOption(options, "trust-policy"),
      trustPolicySha256: requiredOption(options, "trust-policy-sha256"),
    });
    return;
  }
  if (command === "validate") {
    assertOnlyOptions(options, [
      "candidate-dir",
      "expected-manifest-sha256",
      "expected-release-id",
      "expected-source-commit",
      "expected-trust-policy-sha256",
    ]);
    await validateCandidateSet({
      candidateDirectory: requiredOption(options, "candidate-dir"),
      expectedSourceCommit: requiredOption(options, "expected-source-commit"),
      expectedReleaseId: requiredOption(options, "expected-release-id"),
      expectedManifestSha256: requiredOption(options, "expected-manifest-sha256"),
      expectedTrustPolicySha256: requiredOption(
        options,
        "expected-trust-policy-sha256",
      ),
    });
    console.log("Bluey Browser immutable candidate set verified");
    return;
  }
  if (command === "authorize") {
    assertOnlyOptions(options, [
      "authorization-run-id",
      "candidate-dir",
      "expected-manifest-sha256",
      "expected-release-id",
      "expected-signature-set-sha256",
      "expected-source-commit",
      "expected-trust-policy-sha256",
      "manifest-signature-set",
      "out",
      "verification-time-ms",
    ]);
    await authorizeCandidateSet({
      candidateDirectory: requiredOption(options, "candidate-dir"),
      manifestSignatureSetPath: requiredOption(options, "manifest-signature-set"),
      outputDirectory: requiredOption(options, "out"),
      authorizationRunId: requiredOption(options, "authorization-run-id"),
      expectedSourceCommit: requiredOption(options, "expected-source-commit"),
      expectedReleaseId: requiredOption(options, "expected-release-id"),
      expectedManifestSha256: requiredOption(options, "expected-manifest-sha256"),
      expectedTrustPolicySha256: requiredOption(
        options,
        "expected-trust-policy-sha256",
      ),
      expectedManifestSignatureSetSha256: requiredOption(
        options,
        "expected-signature-set-sha256",
      ),
      verificationTimeMs: parseNonnegativeInteger(
        requiredOption(options, "verification-time-ms"),
        "Invalid authority verification time",
      ),
    });
    console.log("Bluey Browser detached release threshold verified");
    return;
  }
  if (command === "validate-authorized") {
    assertOnlyOptions(options, [
      "authorized-dir",
      "expected-manifest-sha256",
      "expected-release-id",
      "expected-signature-set-sha256",
      "expected-source-commit",
      "expected-trust-policy-sha256",
      "verification-time-ms",
    ]);
    await validateAuthorizedCandidateSet({
      authorizedDirectory: requiredOption(options, "authorized-dir"),
      expectedSourceCommit: requiredOption(options, "expected-source-commit"),
      expectedReleaseId: requiredOption(options, "expected-release-id"),
      expectedManifestSha256: requiredOption(options, "expected-manifest-sha256"),
      expectedTrustPolicySha256: requiredOption(
        options,
        "expected-trust-policy-sha256",
      ),
      expectedManifestSignatureSetSha256: requiredOption(
        options,
        "expected-signature-set-sha256",
      ),
      verificationTimeMs: parseNonnegativeInteger(
        requiredOption(options, "verification-time-ms"),
        "Invalid authority verification time",
      ),
    });
    console.log("Bluey Browser stored release authorization verified");
    return;
  }
  if (command === "promote" || command === "validate-promotion") {
    const shared = [
      "expected-activation-sha256",
      "expected-activation-signature-set-sha256",
      "expected-manifest-sha256",
      "expected-manifest-signature-set-sha256",
      "expected-release-id",
      "expected-source-commit",
      "expected-trust-policy-sha256",
      "require-production-ready",
      "verification-time-ms",
    ];
    if (command === "promote") {
      assertOnlyOptions(options, [
        ...shared,
        "activation",
        "activation-signature-set",
        "authorized-dir",
        "canary-evidence",
        "out",
        "promotion-run-id",
      ]);
      await createPromotionSet({
        authorizedDirectory: requiredOption(options, "authorized-dir"),
        activationPath: requiredOption(options, "activation"),
        activationSignatureSetPath: requiredOption(options, "activation-signature-set"),
        canaryEvidencePath: requiredOption(options, "canary-evidence"),
        outputDirectory: requiredOption(options, "out"),
        promotionRunId: requiredOption(options, "promotion-run-id"),
        ...promotionExpectedOptions(options),
      });
    } else {
      assertOnlyOptions(options, [...shared, "promotion-dir"]);
      await validatePromotionSet({
        promotionDirectory: requiredOption(options, "promotion-dir"),
        ...promotionExpectedOptions(options),
      });
    }
    console.log("Bluey Browser stored promotion authority verified");
    return;
  }
  if (command === "workflow") {
    assertOnlyOptions(options, ["file"]);
    validateWorkflowContract(await readFile(requiredOption(options, "file"), "utf8"));
    console.log("Bluey Browser release workflow contract verified");
    return;
  }
  throw new BrowserReleaseGateError("Unknown Bluey Browser release gate command");
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  main().catch((error) => {
    const message = error instanceof Error ? error.message : "Unknown release gate failure";
    console.error(`Bluey Browser release gate failed: ${message}`);
    process.exitCode = 1;
  });
}
