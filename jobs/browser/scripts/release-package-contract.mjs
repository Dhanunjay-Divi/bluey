import { createHash, createPrivateKey, createPublicKey } from "node:crypto";
import {
  lstat,
  mkdir,
  mkdtemp,
  open,
  readFile,
  readlink,
  readdir,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import { createRequire } from "node:module";
import {
  dirname,
  isAbsolute,
  join,
  posix,
  relative,
  resolve,
  sep,
} from "node:path";
import { fileURLToPath } from "node:url";

export const BLUEY_BROWSER_APP_ID = "sh.bluey.jobs.browser";
export const BLUEY_BROWSER_PRODUCT_NAME = "Bluey Browser";
export const BLUEY_BROWSER_BUILD_KEYRING_AUDIENCE =
  "bluey-jobs-browser-build-keyring-v1";

const BROWSER_PACKAGE_NAME = "@bluey/jobs-browser";
const RELEASE_AUTHORITY_FILES = Object.freeze([
  "build-descriptor.txt",
  "build-descriptor.sig",
  "build-public-keys.json",
]);
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
const HEX_64 = /^[0-9a-f]{64}$/;
const DECIMAL_REVISION = /^(0|[1-9][0-9]{0,12})$/;
const BASE64URL = /^[A-Za-z0-9_-]+$/;
const MAX_SAFE_INTEGER = 9_007_199_254_740_991;
const MAX_PRIVATE_KEY_BYTES = 16_384;
const MAX_KEYRING_BYTES = 16_384;
const MAX_PREPARED_TREE_ENTRIES = 250_000;
const MAX_PREPARED_ARCHIVE_BYTES = 4 * 1024 * 1024 * 1024;
const MAX_PREPARED_ARCHIVE_CONTENT_BYTES = 3 * 1024 * 1024 * 1024;
const PREPARED_ARTIFACT_PREFIXES = Object.freeze([
  "automation/dist",
  "browser/browser-bundle",
  "browser/dist",
]);
const EXECUTABLE_BUILDER_HOOK = new RegExp(
  `^\\s*(?:${[
    "afterAllArtifactBuild",
    "afterExtract",
    "afterPack",
    "afterPrune",
    "afterSign",
    "appxManifestCreated",
    "artifactBuildCompleted",
    "artifactBuildStarted",
    "beforeBuild",
    "beforePack",
    "effectiveOptionComputed",
    "msiProjectCreated",
  ].join("|")}):`,
  "m",
);
const PACKAGING_AUTHORITY_INPUT_NAMES = Object.freeze([
  "BLUEY_BROWSER_RELEASE_ID",
  "BLUEY_BROWSER_BUILD_ID",
  "BLUEY_BROWSER_SOURCE_COMMIT",
  "BLUEY_BROWSER_BUILD_SIGNING_KEY_ID",
  "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256",
  "BLUEY_BROWSER_BUILD_PRIVATE_KEY_PKCS8_BASE64",
  "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_BASE64",
  "BLUEY_BROWSER_BUILD_PRIVATE_KEY_FILE",
  "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_FILE",
  "BLUEY_BROWSER_PROTOCOL_VERSION",
  "BLUEY_BROWSER_BUILD_ISSUED_AT_MS",
  "BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY",
  "APPLE_API_KEY_BASE64",
]);
const NATIVE_SIGNING_INPUT_NAMES = Object.freeze([
  "CSC_LINK",
  "CSC_NAME",
  "CSC_KEY_PASSWORD",
  "WIN_CSC_LINK",
  "WIN_CSC_KEY_PASSWORD",
  "APPLE_API_KEY",
  "APPLE_API_KEY_ID",
  "APPLE_API_ISSUER",
  "APPLE_ID",
  "APPLE_APP_SPECIFIC_PASSWORD",
  "APPLE_TEAM_ID",
  "APPLE_KEYCHAIN",
  "APPLE_KEYCHAIN_PROFILE",
]);

const scriptRoot = dirname(fileURLToPath(import.meta.url));
export const browserRoot = resolve(scriptRoot, "..");
export const repositoryRoot = resolve(browserRoot, "../..");

const RELEASE_TARGETS = Object.freeze({
  "darwin-arm64": Object.freeze({
    name: "darwin-arm64",
    hostPlatform: "darwin",
    hostArchitecture: "arm64",
    platform: "darwin",
    architecture: "arm64",
    electronBuilderArguments: Object.freeze(["--mac", "--arm64"]),
    chromiumExecutable: Object.freeze([
      "chrome-mac-arm64",
      "Google Chrome for Testing.app",
      "Contents",
      "MacOS",
      "Google Chrome for Testing",
    ]),
  }),
  "darwin-x64": Object.freeze({
    name: "darwin-x64",
    hostPlatform: "darwin",
    hostArchitecture: "x64",
    platform: "darwin",
    architecture: "x64",
    electronBuilderArguments: Object.freeze(["--mac", "--x64"]),
    chromiumExecutable: Object.freeze([
      "chrome-mac-x64",
      "Google Chrome for Testing.app",
      "Contents",
      "MacOS",
      "Google Chrome for Testing",
    ]),
  }),
  "windows-x64": Object.freeze({
    name: "windows-x64",
    hostPlatform: "win32",
    hostArchitecture: "x64",
    platform: "windows",
    architecture: "x64",
    electronBuilderArguments: Object.freeze(["--win", "--x64"]),
    chromiumExecutable: Object.freeze(["chrome-win64", "chrome.exe"]),
  }),
});

export function requireReleaseTarget(
  targetName,
  hostPlatform = process.platform,
  hostArchitecture = process.arch,
) {
  const target = RELEASE_TARGETS[targetName];
  if (!target) {
    throw new Error(
      "Choose exactly one Bluey Browser release target: darwin-arm64, darwin-x64, or windows-x64",
    );
  }
  if (
    hostPlatform !== target.hostPlatform ||
    hostArchitecture !== target.hostArchitecture
  ) {
    throw new Error(
      `Bluey Browser ${target.name} packaging requires an exact ${target.hostPlatform}/${target.hostArchitecture} host`,
    );
  }
  return target;
}

export function parseRequiredBuildEnvironment(environment) {
  const releaseId = requiredEnvironmentValue(
    environment,
    "BLUEY_BROWSER_RELEASE_ID",
    SAFE_ID,
  );
  if (MUTABLE_RELEASE_IDS.has(releaseId.toLowerCase())) {
    throw new Error("Missing or invalid BLUEY_BROWSER_RELEASE_ID");
  }
  const buildId = requiredEnvironmentValue(
    environment,
    "BLUEY_BROWSER_BUILD_ID",
    BUILD_ID,
  );
  const sourceCommit = requiredEnvironmentValue(
    environment,
    "BLUEY_BROWSER_SOURCE_COMMIT",
    SOURCE_COMMIT,
  );
  const signingKeyId = requiredEnvironmentValue(
    environment,
    "BLUEY_BROWSER_BUILD_SIGNING_KEY_ID",
    SAFE_ID,
  );
  const publicKeyringSha256 = requiredEnvironmentValue(
    environment,
    "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256",
    HEX_64,
  );
  const privateKeyFile = requiredAbsolutePath(
    environment,
    "BLUEY_BROWSER_BUILD_PRIVATE_KEY_FILE",
  );
  const publicKeyringFile = requiredAbsolutePath(
    environment,
    "BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_FILE",
  );
  const protocolVersion = requiredPositiveInteger(
    environment.BLUEY_BROWSER_PROTOCOL_VERSION,
    "BLUEY_BROWSER_PROTOCOL_VERSION",
  );
  const issuedAtMs = requiredPositiveInteger(
    environment.BLUEY_BROWSER_BUILD_ISSUED_AT_MS,
    "BLUEY_BROWSER_BUILD_ISSUED_AT_MS",
  );
  if (privateKeyFile === publicKeyringFile) {
    throw new Error("Bluey Browser build signing inputs must be separate files");
  }
  return Object.freeze({
    releaseId,
    buildId,
    sourceCommit,
    signingKeyId,
    publicKeyringSha256,
    privateKeyFile,
    publicKeyringFile,
    protocolVersion,
    issuedAtMs,
  });
}

export function requireNativeSigningCredentials(target, environment) {
  if (target.platform === "darwin") {
    const groups = [
      ["APPLE_API_KEY", "APPLE_API_KEY_ID", "APPLE_API_ISSUER"],
      ["APPLE_ID", "APPLE_APP_SPECIFIC_PASSWORD", "APPLE_TEAM_ID"],
      ["APPLE_KEYCHAIN_PROFILE"],
    ];
    const states = groups.map((names) =>
      names.map((name) => nonemptyEnvironmentValue(environment[name])),
    );
    const complete = states.filter((state) => state.every(Boolean)).length;
    const partial = states.some(
      (state) => state.some(Boolean) && !state.every(Boolean),
    );
    if (complete !== 1 || partial) {
      throw new Error(
        "Bluey Browser macOS packaging requires one complete notarization credential mode",
      );
    }
    return;
  }
  if (target.platform === "windows") {
    const windowsLink = nonemptyEnvironmentValue(environment.WIN_CSC_LINK);
    const genericLink = nonemptyEnvironmentValue(environment.CSC_LINK);
    if (windowsLink === genericLink) {
      throw new Error(
        "Bluey Browser Windows packaging requires one explicit Authenticode credential",
      );
    }
    return;
  }
  throw new Error("Unsupported Bluey Browser native signing target");
}

export function validateSourceRevision(expectedCommit, headCommit, statusOutput) {
  if (
    !SOURCE_COMMIT.test(expectedCommit) ||
    headCommit.trim() !== expectedCommit ||
    statusOutput.trim() !== ""
  ) {
    throw new Error(
      "Bluey Browser release packaging requires the exact clean source commit",
    );
  }
  return expectedCommit;
}

export async function readBrowserSourceContract(root = browserRoot) {
  const require = createRequire(join(root, "package.json"));
  const electronPackagePath = require.resolve("electron/package.json");
  const playwrightPackagePath = require.resolve("playwright/package.json");
  const playwrightCorePackagePath = require.resolve("playwright-core/package.json");
  const lockfilePath = resolve(root, "..", "package-lock.json");
  const [
    appPackage,
    builderConfiguration,
    lockfile,
    electronPackage,
    playwrightPackage,
    playwrightCorePackage,
    browserManifest,
  ] = await Promise.all([
    readJsonFile(join(root, "package.json")),
    readFile(join(root, "electron-builder.yml"), "utf8"),
    readJsonFile(lockfilePath),
    readJsonFile(electronPackagePath),
    readJsonFile(playwrightPackagePath),
    readJsonFile(playwrightCorePackagePath),
    readJsonFile(join(dirname(playwrightCorePackagePath), "browsers.json")),
  ]);
  return validateBrowserSourceContract({
    appPackage,
    builderConfiguration,
    lockfile,
    electronPackage,
    playwrightPackage,
    playwrightCorePackage,
    browserManifest,
  });
}

export async function validatePreparedSourceAgainstTrusted(
  candidateRoot,
  trustedRoot = browserRoot,
) {
  const [candidate, trusted, candidateConfiguration, trustedConfiguration] =
    await Promise.all([
      readBrowserSourceContract(candidateRoot),
      readBrowserSourceContract(trustedRoot),
      readFile(join(candidateRoot, "electron-builder.yml")),
      readFile(join(trustedRoot, "electron-builder.yml")),
    ]);
  if (!candidateConfiguration.equals(trustedConfiguration)) {
    throw new Error(
      "Prepared Browser source does not use the exact trusted non-executable builder contract",
    );
  }
  if (
    candidate.electronVersion !== trusted.electronVersion ||
    candidate.playwrightVersion !== trusted.playwrightVersion ||
    candidate.chromiumRevision !== trusted.chromiumRevision
  ) {
    throw new Error("Prepared Browser runtime dependencies differ from trusted release tooling");
  }
  return candidate;
}

export function validateBrowserSourceContract(input) {
  const appPackage = requireObject(input.appPackage, "Browser package");
  validatePackageExecutionContract(appPackage);
  const electronPackage = requireObject(
    input.electronPackage,
    "Electron package",
  );
  const playwrightPackage = requireObject(
    input.playwrightPackage,
    "Playwright package",
  );
  const playwrightCorePackage = requireObject(
    input.playwrightCorePackage,
    "Playwright Core package",
  );
  const lockfile = requireObject(input.lockfile, "Jobs lockfile");
  const appVersion = requirePattern(
    appPackage.version,
    SEMVER,
    "Bluey Browser app version",
  );
  if (
    appPackage.name !== BROWSER_PACKAGE_NAME ||
    appPackage.main !== "dist/main.js"
  ) {
    throw new Error("Invalid or executable Bluey Browser package identity");
  }
  const appDependencies = requireObject(
    appPackage.dependencies,
    "Browser dependencies",
  );
  const appDevDependencies = requireObject(
    appPackage.devDependencies,
    "Browser development dependencies",
  );
  const electronVersion = requirePattern(
    electronPackage.version,
    SEMVER,
    "Electron version",
  );
  const playwrightVersion = requirePattern(
    playwrightPackage.version,
    SEMVER,
    "Playwright version",
  );
  if (
    electronPackage.name !== "electron" ||
    appDevDependencies.electron !== electronVersion ||
    playwrightPackage.name !== "playwright" ||
    playwrightCorePackage.name !== "playwright-core" ||
    playwrightCorePackage.version !== playwrightVersion ||
    !dependencySpecAcceptsVersion(appDependencies.playwright, playwrightVersion)
  ) {
    throw new Error("Bluey Browser runtime dependencies do not match source");
  }
  validateLockedDependency(
    lockfile,
    "electron",
    appDevDependencies.electron,
    electronVersion,
  );
  validateLockedDependency(
    lockfile,
    "playwright",
    appDependencies.playwright,
    playwrightVersion,
  );
  validateLockedDependency(
    lockfile,
    "playwright-core",
    playwrightVersion,
    playwrightVersion,
    false,
  );
  validateBuilderConfiguration(input.builderConfiguration);

  const browserManifest = requireObject(
    input.browserManifest,
    "Playwright browser manifest",
  );
  if (!Array.isArray(browserManifest.browsers)) {
    throw new Error("Invalid Playwright browser manifest");
  }
  const chromiumEntries = browserManifest.browsers.filter(
    (entry) =>
      entry && typeof entry === "object" && !Array.isArray(entry) && entry.name === "chromium",
  );
  if (chromiumEntries.length !== 1) {
    throw new Error("Playwright must declare exactly one headed Chromium runtime");
  }
  const chromium = chromiumEntries[0];
  const chromiumRevision = requirePattern(
    chromium.revision,
    DECIMAL_REVISION,
    "Chromium revision",
  );
  if (chromium.installByDefault !== true) {
    throw new Error("Playwright headed Chromium is not an installed runtime");
  }
  return Object.freeze({
    appId: BLUEY_BROWSER_APP_ID,
    appVersion,
    electronVersion,
    playwrightVersion,
    chromiumRevision,
  });
}

export function validatePackageExecutionContract(appPackage) {
  const value = requireObject(appPackage, "Browser package");
  if (Object.hasOwn(value, "build")) {
    throw new Error("Bluey Browser package may not define executable builder configuration");
  }
  return true;
}

export function validateBuilderConfiguration(configuration) {
  if (typeof configuration !== "string") {
    throw new Error("Invalid Bluey Browser builder configuration");
  }
  requireSingleTopLevelScalar(
    configuration,
    "appId",
    BLUEY_BROWSER_APP_ID,
  );
  requireSingleTopLevelScalar(
    configuration,
    "productName",
    BLUEY_BROWSER_PRODUCT_NAME,
  );
  requireSingleTopLevelScalar(
    configuration,
    "artifactName",
    "Bluey-Browser-${version}-${os}-${arch}.${ext}",
  );
  if (
    /^linux:/m.test(configuration) ||
    /\buniversal\b/i.test(configuration) ||
    /^\s+arch:/m.test(configuration) ||
    !/^asar: true$/m.test(configuration) ||
    countExactLine(configuration, "npmRebuild: false") !== 1 ||
    countExactLine(configuration, "nodeGypRebuild: false") !== 1 ||
    countExactLine(configuration, "buildDependenciesFromSource: false") !== 1 ||
    !/^\s{2}output: release$/m.test(configuration) ||
    countExactLine(configuration, "  forceCodeSigning: true") !== 2 ||
    countExactLine(configuration, "  notarize: true") !== 1 ||
    countExactLine(configuration, "      - bluey-jobs") !== 1
  ) {
    throw new Error("Bluey Browser builder target contract is invalid");
  }
  if (EXECUTABLE_BUILDER_HOOK.test(configuration)) {
    throw new Error("Bluey Browser builder configuration may not execute hooks");
  }
  for (const line of [
    "  - assets/icon-128.png",
    "  - assets/icon-512.png",
    "  - assets/trayTemplate.png",
    "  - assets/trayTemplate@2x.png",
    '  - "!**/*.map"',
    '  - "!**/.env{,.*}"',
    '  - "!**/{__tests__,fixtures,test,tests}{,/**}"',
    '  - "!**/*.{spec,test}.{cjs,js,mjs}"',
    '  - "!**/fixtures.{cjs,js,mjs}"',
    '  - "!node_modules/@bluey/jobs-automation/dist/**/*.d.ts"',
  ]) {
    if (countExactLine(configuration, line) !== 1) {
      throw new Error("Bluey Browser builder source exclusions are incomplete");
    }
  }
  for (const file of RELEASE_AUTHORITY_FILES) {
    if (countExactLine(configuration, `      - "${file}"`) !== 1) {
      throw new Error("Bluey Browser release resources are incomplete");
    }
  }
  const macSection = requireTopLevelSection(configuration, "mac");
  const winSection = requireTopLevelSection(configuration, "win");
  if (
    countExactLine(configuration, "mac:") !== 1 ||
    countExactLine(configuration, "win:") !== 1 ||
    countExactLine(macSection, "    - target: dmg") !== 1 ||
    countExactLine(macSection, "    - target: zip") !== 1 ||
    countIndentedValues(macSection, "target", 4) !== 2 ||
    countIndentedListValues(macSection, 8) !== 0 ||
    countExactLine(winSection, "    - target: nsis") !== 1 ||
    countIndentedValues(winSection, "target", 4) !== 1 ||
    countIndentedListValues(winSection, 8) !== 0
  ) {
    throw new Error("Bluey Browser builder architecture contract is invalid");
  }
}

export async function validateHeadedChromiumBundle(
  bundleRoot,
  chromiumRevision,
  target,
) {
  if (!DECIMAL_REVISION.test(chromiumRevision)) {
    throw new Error("Invalid Playwright Chromium revision");
  }
  const rootEntry = await lstat(bundleRoot);
  if (!rootEntry.isDirectory() || rootEntry.isSymbolicLink()) {
    throw new Error("Invalid Playwright browser bundle directory");
  }
  const entries = await readdir(bundleRoot, { withFileTypes: true });
  const expectedDirectory = `chromium-${chromiumRevision}`;
  if (
    entries.length !== 1 ||
    entries[0].name !== expectedDirectory ||
    !entries[0].isDirectory() ||
    entries[0].isSymbolicLink()
  ) {
    throw new Error(
      "Bluey Browser package must contain one exact headed Chromium revision",
    );
  }
  const executablePath = join(
    bundleRoot,
    expectedDirectory,
    ...target.chromiumExecutable,
  );
  const executableEntry = await lstat(executablePath);
  if (!executableEntry.isFile() || executableEntry.isSymbolicLink()) {
    throw new Error("Invalid packaged Chromium executable");
  }
  if (target.platform === "darwin" && (executableEntry.mode & 0o111) === 0) {
    throw new Error("Packaged Chromium is not executable");
  }
  const [canonicalRoot, canonicalExecutable] = await Promise.all([
    realpath(bundleRoot),
    realpath(executablePath),
  ]);
  requireContainedPath(canonicalRoot, canonicalExecutable);
  const handle = await open(executablePath, "r");
  try {
    const header = Buffer.alloc(512);
    const { bytesRead } = await handle.read(header, 0, header.length, 0);
    validateExecutableArchitecture(header.subarray(0, bytesRead), target);
  } finally {
    await handle.close();
  }
  return Object.freeze({
    directory: expectedDirectory,
    executablePath,
  });
}

export async function validatePreparedPackagingTree(root) {
  const browserDirectory = await realpath(root);
  const jobsDirectory = await realpath(resolve(browserDirectory, ".."));
  const inputs = [
    resolve(browserDirectory, "assets"),
    resolve(browserDirectory, "browser-bundle"),
    resolve(browserDirectory, "dist"),
    resolve(browserDirectory, "node_modules"),
    resolve(jobsDirectory, "automation", "dist"),
    resolve(jobsDirectory, "node_modules"),
  ];
  await validatePreparedPaths(jobsDirectory, inputs, MAX_PREPARED_TREE_ENTRIES);
  return true;
}

export async function validatePreparedArtifactTree(root) {
  const jobsDirectory = await realpath(root);
  await requireExactDirectoryNames(jobsDirectory, ["automation", "browser"]);
  await requireExactDirectoryNames(resolve(jobsDirectory, "automation"), ["dist"]);
  await requireExactDirectoryNames(resolve(jobsDirectory, "browser"), [
    "browser-bundle",
    "dist",
  ]);
  await validatePreparedPaths(
    jobsDirectory,
    [
      resolve(jobsDirectory, "automation", "dist"),
      resolve(jobsDirectory, "browser", "browser-bundle"),
      resolve(jobsDirectory, "browser", "dist"),
    ],
    50_000,
  );
  return true;
}

export async function extractPreparedArtifactArchive({
  archivePath,
  outputParent,
} = {}) {
  const archive = requireAbsoluteInputPath(
    archivePath,
    "prepared Browser archive",
  );
  const parent = requireAbsoluteInputPath(
    outputParent,
    "prepared Browser extraction parent",
  );
  const [archiveEntry, parentEntry, canonicalArchive, canonicalParent] =
    await Promise.all([
      lstat(archive),
      lstat(parent),
      realpath(archive),
      realpath(parent),
    ]);
  if (
    !archiveEntry.isFile() ||
    archiveEntry.isSymbolicLink() ||
    archiveEntry.size < 1_024 ||
    archiveEntry.size > MAX_PREPARED_ARCHIVE_BYTES ||
    !parentEntry.isDirectory() ||
    parentEntry.isSymbolicLink()
  ) {
    throw new Error("Invalid prepared Browser archive extraction boundary");
  }

  const { t: listArchive, x: extractArchive } = await import("tar");
  const descriptors = [];
  let listingError;
  await listArchive({
    file: canonicalArchive,
    strict: true,
    onReadEntry: (entry) => {
      if (listingError) return;
      try {
        descriptors.push(preparedArchiveEntryDescriptor(entry));
        if (descriptors.length > 50_000) {
          throw new Error("Prepared Browser archive contains too many entries");
        }
      } catch (error) {
        listingError = error;
      }
    },
  });
  if (listingError) throw listingError;
  validatePreparedArchiveDescriptors(descriptors);
  await requireUnchangedFile(canonicalArchive, archiveEntry);

  const temporaryRoot = await mkdtemp(
    join(canonicalParent, "bluey-browser-prepared-"),
  );
  const extractionRoot = join(temporaryRoot, "jobs");
  try {
    await mkdir(extractionRoot, { recursive: false, mode: 0o700 });
    let extractedEntries = 0;
    let extractionError;
    await extractArchive({
      cwd: extractionRoot,
      file: canonicalArchive,
      strict: true,
      preservePaths: false,
      preserveOwner: false,
      unlink: true,
      chmod: true,
      noMtime: true,
      maxDepth: 100,
      filter: (_path, entry) => {
        if (extractionError) return false;
        try {
          const actual = preparedArchiveEntryDescriptor(entry);
          const expected = descriptors[extractedEntries];
          if (
            expected === undefined ||
            JSON.stringify(actual) !== JSON.stringify(expected)
          ) {
            throw new Error("Prepared Browser archive changed during extraction");
          }
          extractedEntries += 1;
          entry.mode = actual.mode;
          return true;
        } catch (error) {
          extractionError = error;
          return false;
        }
      },
    });
    if (extractionError) throw extractionError;
    if (extractedEntries !== descriptors.length) {
      throw new Error("Prepared Browser archive changed during extraction");
    }
    await requireUnchangedFile(canonicalArchive, archiveEntry);
    await validatePreparedArtifactTree(extractionRoot);
    return extractionRoot;
  } catch (error) {
    await rm(temporaryRoot, { recursive: true, force: true });
    throw error;
  }
}

function preparedArchiveEntryDescriptor(entry) {
  const path = requirePreparedArchivePath(entry.path);
  if (!["Directory", "File", "SymbolicLink"].includes(entry.type)) {
    throw new Error("Prepared Browser archive contains an unsupported entry type");
  }
  const size = entry.size ?? 0;
  if (
    !Number.isSafeInteger(size) ||
    size < 0 ||
    size > MAX_PREPARED_ARCHIVE_CONTENT_BYTES ||
    (entry.type !== "File" && size !== 0)
  ) {
    throw new Error("Prepared Browser archive contains an invalid entry size");
  }
  const rawMode = entry.mode ?? (entry.type === "Directory" ? 0o755 : 0o644);
  if (!Number.isSafeInteger(rawMode) || rawMode < 0 || (rawMode & ~0o777) !== 0) {
    throw new Error("Prepared Browser archive contains an unsafe entry mode");
  }
  const mode = rawMode & 0o777;
  let linkpath = "";
  if (entry.type === "SymbolicLink") {
    linkpath = requirePreparedArchiveLink(entry.linkpath, path);
  } else if (entry.linkpath) {
    throw new Error("Prepared Browser archive contains an unexpected link target");
  }
  return Object.freeze({ path, type: entry.type, linkpath, mode, size });
}

export function validatePreparedArchiveDescriptors(descriptors) {
  if (
    !Array.isArray(descriptors) ||
    descriptors.length < 3 ||
    descriptors.length > 50_000
  ) {
    throw new Error("Prepared Browser archive is incomplete");
  }
  const paths = new Map();
  const collisionKeys = new Set();
  let contentBytes = 0;
  for (const descriptor of descriptors) {
    const collisionKey = descriptor.path.normalize("NFC").toLowerCase();
    if (paths.has(descriptor.path) || collisionKeys.has(collisionKey)) {
      throw new Error("Prepared Browser archive contains a path collision");
    }
    paths.set(descriptor.path, descriptor);
    collisionKeys.add(collisionKey);
    contentBytes += descriptor.size;
    if (contentBytes > MAX_PREPARED_ARCHIVE_CONTENT_BYTES) {
      throw new Error("Prepared Browser archive content is too large");
    }
  }
  for (const prefix of PREPARED_ARTIFACT_PREFIXES) {
    if (paths.get(prefix)?.type !== "Directory") {
      throw new Error("Prepared Browser archive is missing an exact root directory");
    }
  }
  const symbolicLinks = descriptors
    .filter((descriptor) => descriptor.type === "SymbolicLink")
    .map((descriptor) => descriptor.path);
  for (const descriptor of descriptors) {
    for (const link of symbolicLinks) {
      if (descriptor.path.startsWith(`${link}/`)) {
        throw new Error("Prepared Browser archive writes through a symbolic link");
      }
    }
    for (const ancestor of preparedArchiveAncestors(descriptor.path)) {
      const existing = paths.get(ancestor);
      if (existing && existing.type !== "Directory") {
        throw new Error("Prepared Browser archive contains an ancestor collision");
      }
    }
  }
}

function requirePreparedArchivePath(value) {
  if (
    typeof value !== "string" ||
    value.length < 1 ||
    value.length > 4_096 ||
    value.includes("\\") ||
    /[\u0000-\u001f\u007f\ufffd]/.test(value)
  ) {
    throw new Error("Prepared Browser archive contains an invalid path");
  }
  const path = value.endsWith("/") ? value.slice(0, -1) : value;
  if (
    !path ||
    posix.isAbsolute(path) ||
    posix.normalize(path) !== path ||
    path.split("/").some((part) => !part || part === "." || part === "..") ||
    !isPreparedArtifactPath(path)
  ) {
    throw new Error("Prepared Browser archive path is out of scope");
  }
  return path;
}

function requirePreparedArchiveLink(value, entryPath) {
  if (
    typeof value !== "string" ||
    value.length < 1 ||
    value.length > 4_096 ||
    value.includes("\\") ||
    posix.isAbsolute(value) ||
    /[\u0000-\u001f\u007f\ufffd]/.test(value)
  ) {
    throw new Error("Prepared Browser archive contains an invalid symbolic link");
  }
  const target = posix.normalize(posix.join(posix.dirname(entryPath), value));
  if (!isPreparedArtifactPath(target)) {
    throw new Error(
      `Prepared Browser archive symbolic link escapes prepared bytes: ${entryPath}`,
    );
  }
  return value;
}

function isPreparedArtifactPath(path) {
  return PREPARED_ARTIFACT_PREFIXES.some(
    (prefix) => path === prefix || path.startsWith(`${prefix}/`),
  );
}

function preparedArchiveAncestors(path) {
  const parts = path.split("/");
  const ancestors = [];
  for (let index = 1; index < parts.length; index += 1) {
    ancestors.push(parts.slice(0, index).join("/"));
  }
  return ancestors;
}

function requireAbsoluteInputPath(value, label) {
  if (
    typeof value !== "string" ||
    !value ||
    value.length > 4_096 ||
    value.includes("\0") ||
    !isAbsolute(value)
  ) {
    throw new Error(`Invalid ${label}`);
  }
  return resolve(value);
}

async function requireUnchangedFile(path, expected) {
  const actual = await lstat(path);
  if (
    !actual.isFile() ||
    actual.isSymbolicLink() ||
    actual.size !== expected.size ||
    actual.mtimeMs !== expected.mtimeMs ||
    (expected.dev !== undefined && actual.dev !== expected.dev) ||
    (expected.ino !== undefined && actual.ino !== expected.ino)
  ) {
    throw new Error("Prepared Browser archive changed during validation");
  }
}

async function validatePreparedPaths(containmentRoot, inputs, maximumEntries) {
  let entries = 0;
  async function walk(path) {
    const children = await readdir(path, { withFileTypes: true });
    for (const child of children) {
      if (
        !child.name ||
        child.name === "." ||
        child.name === ".." ||
        /[\u0000-\u001f\u007f]/.test(child.name)
      ) {
        throw new Error("Prepared Browser package contains an invalid path");
      }
      entries += 1;
      if (entries > maximumEntries) {
        throw new Error("Prepared Browser package tree is too large");
      }
      const childPath = join(path, child.name);
      const info = await lstat(childPath);
      if (info.isSymbolicLink()) {
        const target = await readlink(childPath);
        if (!target || target.includes("\0")) {
          throw new Error("Prepared Browser package contains an invalid symlink");
        }
        const canonicalTarget = await realpath(childPath);
        requireContainedPath(containmentRoot, canonicalTarget);
      } else if (info.isDirectory()) {
        await walk(childPath);
      } else if (!info.isFile()) {
        throw new Error("Prepared Browser package contains an unsupported file");
      }
    }
  }
  for (const path of inputs) {
    const info = await lstat(path);
    if (!info.isDirectory() || info.isSymbolicLink()) {
      throw new Error("Prepared Browser package input is invalid");
    }
    requireContainedPath(containmentRoot, await realpath(path));
    await walk(path);
  }
}

export function validateExecutableArchitecture(bytes, target) {
  const header = Buffer.from(bytes);
  if (target.platform === "darwin") {
    if (header.length < 12 || header.readUInt32LE(0) !== 0xfeedfacf) {
      throw new Error("Packaged Chromium is not a thin 64-bit Mach-O executable");
    }
    const expectedCpu = target.architecture === "arm64" ? 0x0100000c : 0x01000007;
    if (header.readUInt32LE(4) !== expectedCpu) {
      throw new Error("Packaged Chromium architecture does not match the release target");
    }
    return;
  }
  if (target.platform === "windows") {
    if (header.length < 64 || header[0] !== 0x4d || header[1] !== 0x5a) {
      throw new Error("Packaged Chromium is not a Windows PE executable");
    }
    const peOffset = header.readUInt32LE(0x3c);
    if (
      peOffset > header.length - 6 ||
      header.subarray(peOffset, peOffset + 4).toString("hex") !== "50450000" ||
      header.readUInt16LE(peOffset + 4) !== 0x8664
    ) {
      throw new Error("Packaged Chromium architecture does not match the release target");
    }
    return;
  }
  throw new Error("Unsupported Bluey Browser executable target");
}

export async function validateReleaseArtifacts(
  releaseDirectory,
  target,
  appVersion,
) {
  if (!SEMVER.test(appVersion)) {
    throw new Error("Invalid Bluey Browser release app version");
  }
  const rootEntry = await lstat(releaseDirectory);
  if (!rootEntry.isDirectory() || rootEntry.isSymbolicLink()) {
    throw new Error("Invalid Bluey Browser release output directory");
  }
  const osName = target.platform === "darwin" ? "mac" : "win";
  const stem = `Bluey-Browser-${appVersion}-${osName}-${target.architecture}`;
  const expected = target.platform === "darwin"
    ? [`${stem}.dmg`, `${stem}.zip`]
    : [`${stem}.exe`];
  const entries = await readdir(releaseDirectory, { withFileTypes: true });
  const primaryArtifacts = entries
    .filter((entry) => /\.(?:dmg|zip|exe|AppImage)$/.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  if (primaryArtifacts.join("\n") !== [...expected].sort().join("\n")) {
    throw new Error("Bluey Browser release output has a missing or extra target artifact");
  }
  for (const filename of expected) {
    const entry = await lstat(join(releaseDirectory, filename));
    if (!entry.isFile() || entry.isSymbolicLink() || entry.size < 1) {
      throw new Error("Invalid Bluey Browser release artifact");
    }
  }
  return Object.freeze({ artifacts: Object.freeze(expected) });
}

export async function validateSigningInputs(inputs, root = repositoryRoot) {
  try {
    const canonicalRoot = await realpath(root);
    const [privateKeyBytes, keyringBytes, privatePath, keyringPath] =
      await Promise.all([
        readBoundedCredentialFile(
          inputs.privateKeyFile,
          MAX_PRIVATE_KEY_BYTES,
          true,
        ),
        readBoundedCredentialFile(
          inputs.publicKeyringFile,
          MAX_KEYRING_BYTES,
          false,
        ),
        realpath(inputs.privateKeyFile),
        realpath(inputs.publicKeyringFile),
      ]);
    requireExternalPath(canonicalRoot, privatePath);
    requireExternalPath(canonicalRoot, keyringPath);
    if (privatePath === keyringPath) throw new Error("same signing input");
    if (sha256(keyringBytes) !== inputs.publicKeyringSha256) {
      throw new Error("unapproved keyring");
    }
    const keyring = parseCanonicalBuildKeyring(keyringBytes);
    const privateKey = createPrivateKey(privateKeyBytes);
    if (privateKey.asymmetricKeyType !== "ed25519") {
      throw new Error("wrong key type");
    }
    const publicKey = createPublicKey(privateKey).export({ format: "jwk" });
    if (
      typeof publicKey.x !== "string" ||
      keyring.keys[inputs.signingKeyId] !== publicKey.x
    ) {
      throw new Error("key mismatch");
    }
    return Object.freeze({ privateKey, keyring, keyringBytes });
  } catch {
    throw new Error("Bluey Browser build signing credentials are invalid");
  }
}

export async function generateSignedDescriptorResources({
  outputDirectory,
  inputs,
  target,
  sourceContract,
  signingMaterial,
  authority,
}) {
  const descriptor = authority.createBrowserBuildDescriptor(
    {
      version: 1,
      audience: authority.BLUEY_BROWSER_BUILD_AUDIENCE,
      releaseId: inputs.releaseId,
      buildId: inputs.buildId,
      appVersion: sourceContract.appVersion,
      appId: sourceContract.appId,
      protocolVersion: inputs.protocolVersion,
      sourceCommit: inputs.sourceCommit,
      platform: target.platform,
      architecture: target.architecture,
      electronVersion: sourceContract.electronVersion,
      playwrightVersion: sourceContract.playwrightVersion,
      chromiumRevision: sourceContract.chromiumRevision,
      issuedAtMs: inputs.issuedAtMs,
      signingKeyId: inputs.signingKeyId,
    },
    signingMaterial.privateKey,
  );
  const descriptorBytes = authority.canonicalBrowserBuildDescriptorBytes(descriptor);
  const proof = {
    descriptor: descriptorBytes.toString("base64url"),
    signature: descriptor.signature,
  };
  const verified = authority.verifyBrowserBuildProof(
    proof,
    signingMaterial.keyring.keys,
  );
  if (
    verified.descriptor.sourceCommit !== inputs.sourceCommit ||
    verified.descriptor.platform !== target.platform ||
    verified.descriptor.architecture !== target.architecture
  ) {
    throw new Error("Generated Bluey Browser descriptor did not verify exactly");
  }

  await rm(outputDirectory, { recursive: true, force: true });
  await mkdir(outputDirectory, { recursive: false, mode: 0o700 });
  const signatureBytes = Buffer.from(`${descriptor.signature}\n`, "utf8");
  await Promise.all([
    writeExclusiveFile(
      join(outputDirectory, RELEASE_AUTHORITY_FILES[0]),
      descriptorBytes,
    ),
    writeExclusiveFile(
      join(outputDirectory, RELEASE_AUTHORITY_FILES[1]),
      signatureBytes,
    ),
    writeExclusiveFile(
      join(outputDirectory, RELEASE_AUTHORITY_FILES[2]),
      signingMaterial.keyringBytes,
    ),
  ]);
  const readback = await Promise.all(
    RELEASE_AUTHORITY_FILES.map((name) => readFile(join(outputDirectory, name))),
  );
  if (
    !readback[0].equals(descriptorBytes) ||
    !readback[1].equals(signatureBytes) ||
    !readback[2].equals(signingMaterial.keyringBytes)
  ) {
    throw new Error("Bluey Browser release resource read-back failed");
  }
  return Object.freeze({ descriptor, descriptorBytes, proof });
}

export function sanitizedPackagingEnvironment(environment) {
  const sanitized = { ...environment };
  for (const name of PACKAGING_AUTHORITY_INPUT_NAMES) delete sanitized[name];
  return sanitized;
}

export function sanitizedSourceBuildEnvironment(environment) {
  const sanitized = sanitizedPackagingEnvironment(environment);
  for (const name of NATIVE_SIGNING_INPUT_NAMES) delete sanitized[name];
  return sanitized;
}

export async function cleanReleaseOutputs(
  root = browserRoot,
  { preservePreparedBuild = false } = {},
) {
  const names = preservePreparedBuild
    ? ["release", "release-authority"]
    : ["release", "release-authority", "browser-bundle"];
  const paths = names.map((name) =>
    resolve(root, name),
  );
  for (const path of paths) requireContainedPath(resolve(root), path);
  await Promise.all(paths.map((path) => rm(path, { recursive: true, force: true })));
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function requiredEnvironmentValue(environment, name, pattern) {
  const value = environment[name];
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`Missing or invalid ${name}`);
  }
  return value;
}

function requiredAbsolutePath(environment, name) {
  const value = environment[name];
  if (
    typeof value !== "string" ||
    value.length < 1 ||
    value.length > 4_096 ||
    !isAbsolute(value)
  ) {
    throw new Error(`Missing or invalid ${name}`);
  }
  return resolve(value);
}

function requiredPositiveInteger(value, name) {
  if (typeof value !== "string" || !/^[1-9][0-9]{0,15}$/.test(value)) {
    throw new Error(`Missing or invalid ${name}`);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed > MAX_SAFE_INTEGER) {
    throw new Error(`Missing or invalid ${name}`);
  }
  return parsed;
}

function nonemptyEnvironmentValue(value) {
  return typeof value === "string" && value.trim() !== "";
}

async function readJsonFile(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Invalid ${label}`);
  }
  return value;
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`Invalid ${label}`);
  }
  return value;
}

function dependencySpecAcceptsVersion(specification, version) {
  if (typeof specification !== "string") return false;
  if (specification === version) return true;
  if (!specification.startsWith("^") || !SEMVER.test(specification.slice(1))) {
    return false;
  }
  const minimum = specification.slice(1).split(".").map(Number);
  const actual = version.split(".").map(Number);
  if (compareVersions(actual, minimum) < 0) return false;
  if (minimum[0] > 0) return actual[0] === minimum[0];
  if (minimum[1] > 0) return actual[0] === 0 && actual[1] === minimum[1];
  return actual[0] === 0 && actual[1] === 0 && actual[2] === minimum[2];
}

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function validateLockedDependency(
  lockfile,
  name,
  sourceSpecification,
  installedVersion,
  requireBrowserDeclaration = true,
) {
  const packages = requireObject(lockfile.packages, "Jobs lockfile packages");
  const browserLock = requireObject(packages.browser, "Browser lock entry");
  if (requireBrowserDeclaration) {
    const section = name === "electron" ? browserLock.devDependencies : browserLock.dependencies;
    const lockedDeclarations = requireObject(section, "Browser lock dependencies");
    if (lockedDeclarations[name] !== sourceSpecification) {
      throw new Error(`Browser ${name} dependency is not locked to source`);
    }
  }
  const installed = requireObject(
    packages[`node_modules/${name}`],
    `Locked ${name} package`,
  );
  if (installed.version !== installedVersion) {
    throw new Error(`Installed ${name} version does not match the lockfile`);
  }
}

function requireSingleTopLevelScalar(configuration, key, expected) {
  const matches = [...configuration.matchAll(new RegExp(`^${key}:\\s*(.+?)\\s*$`, "gm"))];
  if (matches.length !== 1 || matches[0][1] !== expected) {
    throw new Error(`Bluey Browser ${key} is invalid`);
  }
}

function countExactLine(configuration, line) {
  return configuration.split("\n").filter((candidate) => candidate === line).length;
}

function requireTopLevelSection(configuration, name) {
  const heading = `${name}:\n`;
  const start = configuration.indexOf(heading);
  if (start < 0 || (start > 0 && configuration[start - 1] !== "\n")) {
    throw new Error(`Missing Bluey Browser ${name} builder section`);
  }
  const remainder = configuration.slice(start + heading.length);
  const nextHeading = remainder.search(/^[A-Za-z][A-Za-z0-9]*:/m);
  return nextHeading < 0 ? remainder : remainder.slice(0, nextHeading);
}

async function requireExactDirectoryNames(path, expectedNames) {
  const entries = await readdir(path, { withFileTypes: true });
  const names = entries.map((entry) => entry.name).sort();
  if (
    names.join("\n") !== [...expectedNames].sort().join("\n") ||
    entries.some((entry) => !entry.isDirectory() || entry.isSymbolicLink())
  ) {
    throw new Error("Prepared Browser artifact contains unexpected paths");
  }
}

function countIndentedValues(section, key, spaces) {
  const prefix = " ".repeat(spaces);
  return section
    .split("\n")
    .filter((line) => line.startsWith(`${prefix}- ${key}: `)).length;
}

function countIndentedListValues(section, spaces) {
  const prefix = " ".repeat(spaces);
  return section
    .split("\n")
    .filter((line) => line.startsWith(`${prefix}- `)).length;
}

async function readBoundedCredentialFile(path, maximumBytes, privateInput) {
  if (!isAbsolute(path)) throw new Error("credential path must be absolute");
  const before = await lstat(path);
  if (
    !before.isFile() ||
    before.isSymbolicLink() ||
    before.size < 1 ||
    before.size > maximumBytes ||
    (privateInput && process.platform !== "win32" && (before.mode & 0o077) !== 0)
  ) {
    throw new Error("invalid credential file");
  }
  const flags = fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0);
  const handle = await open(path, flags);
  try {
    const after = await handle.stat();
    if (
      !after.isFile() ||
      after.size !== before.size ||
      (before.dev !== undefined && after.dev !== before.dev) ||
      (before.ino !== undefined && after.ino !== before.ino)
    ) {
      throw new Error("credential changed while reading");
    }
    const bytes = await handle.readFile();
    if (bytes.length !== after.size) throw new Error("short credential read");
    return bytes;
  } finally {
    await handle.close();
  }
}

function parseCanonicalBuildKeyring(bytes) {
  const input = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  const record = requireObject(input, "Browser build keyring");
  if (
    Object.keys(record).sort().join(",") !== "audience,keys,version" ||
    record.version !== 1 ||
    record.audience !== BLUEY_BROWSER_BUILD_KEYRING_AUDIENCE
  ) {
    throw new Error("invalid keyring authority");
  }
  const rawKeys = requireObject(record.keys, "Browser build keyring keys");
  const entries = Object.entries(rawKeys).sort(([left], [right]) =>
    left < right ? -1 : left > right ? 1 : 0,
  );
  if (entries.length < 1 || entries.length > 16) {
    throw new Error("invalid keyring size");
  }
  const keys = {};
  for (const [keyId, publicKey] of entries) {
    if (
      !SAFE_ID.test(keyId) ||
      typeof publicKey !== "string" ||
      !BASE64URL.test(publicKey) ||
      Buffer.from(publicKey, "base64url").length !== 32 ||
      Buffer.from(publicKey, "base64url").toString("base64url") !== publicKey
    ) {
      throw new Error("invalid keyring key");
    }
    keys[keyId] = publicKey;
  }
  const keyring = Object.freeze({
    version: 1,
    audience: BLUEY_BROWSER_BUILD_KEYRING_AUDIENCE,
    keys: Object.freeze(keys),
  });
  const canonical = Buffer.from(`${JSON.stringify(keyring)}\n`, "utf8");
  if (!canonical.equals(bytes)) throw new Error("noncanonical keyring");
  return keyring;
}

function requireExternalPath(root, path) {
  const child = relative(root, path);
  if (!child || (child !== ".." && !child.startsWith(`..${sep}`))) {
    throw new Error("signing input is inside the repository");
  }
}

function requireContainedPath(root, path) {
  const child = relative(root, path);
  if (!child || child === ".." || child.startsWith(`..${sep}`)) {
    throw new Error("Bluey Browser release path escapes its root");
  }
}

async function writeExclusiveFile(path, bytes) {
  await writeFile(path, bytes, { flag: "wx", mode: 0o444 });
}
