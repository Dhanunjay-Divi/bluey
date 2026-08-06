import assert from "node:assert/strict";
import { generateKeyPairSync, sign as signBytes } from "node:crypto";
import {
  appendFile,
  chmod,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { createPackage } from "@electron/asar";
import { Header } from "tar";
import {
  extractPreparedArtifactArchive,
  parseRequiredBuildEnvironment as parsePackageBuildEnvironment,
  sanitizedPackagingEnvironment,
  validateBuilderConfiguration,
  validatePackageExecutionContract,
  validatePreparedArchiveDescriptors,
  validatePreparedArtifactTree,
  validatePreparedPackagingTree,
  validatePreparedSourceAgainstTrusted,
} from "../browser/scripts/release-package-contract.mjs";
import {
  assembleCandidateSet,
  authorizeCandidateSet,
  BROWSER_PRODUCTION_CANARY_CHECK_IDS,
  canonicalAsarHeaderPaths,
  canonicalJsonBytes,
  collectCandidateTarget,
  createAppContentInventory,
  createPromotionSet,
  inspectPackagedAppAsar,
  locatePackagedResources,
  recordNativeVerification,
  requireReleaseTarget,
  requireArtifactBaseUrl,
  requireArtifactOrigin,
  requireArtifactPackageUrl,
  requireExactTargetArtifactContracts,
  requireImmutableArtifactUrl,
  requireImmutableReleaseUrl,
  requireWorkflowCredentials,
  sealCandidateTarget,
  sha256,
  validateCandidateSet,
  validateAuthorizedCandidateSet,
  validateManifestTargetContentBindings,
  validatePromotionSet,
  validateWorkflowContract,
} from "./browser-release-ci-gate.mjs";

const SOURCE_COMMIT = "a".repeat(40);
const RELEASE_ID = "browser-release-603-1";
const BUILD_ID = "browser-603.1";
const APP_VERSION = "0.1.0";
const CHROMIUM_REVISION = "1228";
const PUBLISHED_AT_MS = 1_785_970_000_000;
const VERIFICATION_TIME_MS = 1_785_970_200_000;
const ARTIFACT_BASE_URL =
  `https://downloads.bluey.sh/jobs/browser/releases/${RELEASE_ID}`;
const RELEASE_NOTES_URL = `${ARTIFACT_BASE_URL}/RELEASE.md`;
const WORKFLOW_PATH = resolve(".github/workflows/jobs-browser-release.yml");
const TARGETS = Object.freeze([
  Object.freeze({ name: "darwin-arm64", platform: "darwin", architecture: "arm64" }),
  Object.freeze({ name: "darwin-x64", platform: "darwin", architecture: "x64" }),
  Object.freeze({ name: "windows-x64", platform: "windows", architecture: "x64" }),
]);

test("the workflow exposes only exact native targets and fails closed on credentials", () => {
  for (const target of TARGETS) {
    assert.equal(requireReleaseTarget(target.name).target, target.name);
  }
  for (const unsupported of ["darwin-universal", "linux-x64", "windows-arm64"]) {
    assert.throws(() => requireReleaseTarget(unsupported), /exactly darwin-arm64/);
  }

  const common = {
    BLUEY_BROWSER_BUILD_PRIVATE_KEY_FILE: "/outside/build.pem",
    BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_FILE: "/outside/build-keys.json",
    BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256: "b".repeat(64),
  };
  assert.throws(
    () => requireWorkflowCredentials("darwin-arm64", common),
    /Missing CSC_LINK/,
  );
  assert.doesNotThrow(() =>
    requireWorkflowCredentials("darwin-arm64", {
      ...common,
      CSC_LINK: "certificate",
      CSC_KEY_PASSWORD: "password",
      APPLE_API_KEY: "/outside/AuthKey.p8",
      APPLE_API_KEY_ID: "KEY123",
      APPLE_API_ISSUER: "issuer",
      BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY:
        "Developer ID Application: Bluey Test (TEAM603)",
    }),
  );
  assert.throws(
    () =>
      requireWorkflowCredentials("darwin-arm64", {
        ...common,
        CSC_LINK: "certificate",
        CSC_KEY_PASSWORD: "password",
        APPLE_API_KEY: "/outside/AuthKey.p8",
        APPLE_API_KEY_ID: "KEY123",
        APPLE_API_ISSUER: "issuer",
      }),
    /Missing BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY/,
  );
  assert.throws(
    () =>
      requireWorkflowCredentials("windows-x64", {
        ...common,
        WIN_CSC_LINK: "certificate",
      }),
    /Missing WIN_CSC_KEY_PASSWORD/,
  );
  assert.doesNotThrow(() =>
    requireWorkflowCredentials("windows-x64", {
      ...common,
      WIN_CSC_LINK: "certificate",
      WIN_CSC_KEY_PASSWORD: "password",
      BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY:
        "CN=Bluey Test Authenticode",
    }),
  );
  assert.throws(
    () =>
      requireWorkflowCredentials("windows-x64", {
        ...common,
        WIN_CSC_LINK: "certificate",
        WIN_CSC_KEY_PASSWORD: "password",
      }),
    /Missing BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY/,
  );
});

test("release URLs are canonical, policy-origin bound, and never use mutable release IDs", () => {
  const origin = "https://downloads.bluey.sh";
  assert.equal(requireArtifactOrigin(origin), origin);
  for (const invalid of [
    "http://downloads.bluey.sh",
    "https://downloads.bluey.sh/",
    "https://downloads.bluey.sh:443",
    "https://user@downloads.bluey.sh",
    "https://downloads.bluey.sh/releases",
  ]) {
    assert.throws(() => requireArtifactOrigin(invalid), /artifact origin/i);
  }
  assert.equal(
    requireArtifactBaseUrl(ARTIFACT_BASE_URL, RELEASE_ID, origin),
    ARTIFACT_BASE_URL,
  );
  assert.equal(
    requireImmutableArtifactUrl(
      `${ARTIFACT_BASE_URL}/Bluey-Browser-0.1.0-win-x64.exe`,
      RELEASE_ID,
      origin,
    ),
    `${ARTIFACT_BASE_URL}/Bluey-Browser-0.1.0-win-x64.exe`,
  );
  assert.equal(
    requireImmutableReleaseUrl(
      `${ARTIFACT_BASE_URL}/notes/RELEASE.md`,
      RELEASE_ID,
      origin,
    ),
    `${ARTIFACT_BASE_URL}/notes/RELEASE.md`,
  );
  assert.throws(
    () =>
      requireArtifactBaseUrl(
        "https://downloads.bluey.sh/jobs/browser/releases/Latest",
        "Latest",
        origin,
      ),
    /release identity/i,
  );
  assert.throws(
    () =>
      requireImmutableArtifactUrl(
        `${ARTIFACT_BASE_URL}/nested/installer.exe`,
        RELEASE_ID,
        origin,
      ),
    /out of scope/i,
  );
  assert.throws(
    () =>
      requireArtifactPackageUrl(
        `${ARTIFACT_BASE_URL}/Bluey-Browser-0.1.0-win-x64.zip`,
        RELEASE_ID,
        "windows-nsis",
        origin,
      ),
    /package kind/i,
  );
  assert.throws(
    () =>
      validateManifestTargetContentBindings([
        {
          platform: "darwin",
          architecture: "arm64",
          appContentSha256: "1".repeat(64),
        },
        {
          platform: "darwin",
          architecture: "arm64",
          appContentSha256: "2".repeat(64),
        },
      ]),
    /conflicting packaged app content/i,
  );
  for (const field of [
    "automationBundleSha256",
    "chromiumExecutableSha256",
  ]) {
    assert.throws(
      () =>
        validateManifestTargetContentBindings([
          {
            platform: "darwin",
            architecture: "arm64",
            appContentSha256: "1".repeat(64),
            automationBundleSha256: "3".repeat(64),
            chromiumExecutableSha256: "4".repeat(64),
          },
          {
            platform: "darwin",
            architecture: "arm64",
            appContentSha256: "1".repeat(64),
            automationBundleSha256: "3".repeat(64),
            chromiumExecutableSha256: "4".repeat(64),
            [field]: "5".repeat(64),
          },
        ]),
      /runtime components/i,
    );
  }
  assert.throws(
    () =>
      requireExactTargetArtifactContracts(
        [
          {
            packageKind: "darwin-dmg",
            filename: `Bluey-Browser-${APP_VERSION}-mac-arm64.zip`,
          },
          {
            packageKind: "darwin-zip",
            filename: `Bluey-Browser-${APP_VERSION}-mac-arm64.dmg`,
          },
        ],
        requireReleaseTarget("darwin-arm64"),
        APP_VERSION,
      ),
    /exact target package kind/i,
  );
  assert.throws(
    () =>
      requireImmutableArtifactUrl(
        `https://evil.example/jobs/browser/releases/${RELEASE_ID}/installer.exe`,
        RELEASE_ID,
        origin,
      ),
    /out of scope/i,
  );
  assert.throws(
    () =>
      requireImmutableReleaseUrl(
        `https://downloads.bluey.sh/jobs/%62rowser/releases/${RELEASE_ID}/RELEASE.md`,
        RELEASE_ID,
        origin,
      ),
    /out of scope/i,
  );
});

test("trusted packaging rejects candidate hooks, rebuilds, and reserved release IDs", async () => {
  const configuration = await readFile(
    resolve("jobs/browser/electron-builder.yml"),
    "utf8",
  );
  assert.doesNotThrow(() => validateBuilderConfiguration(configuration));
  await assert.doesNotReject(
    validatePreparedSourceAgainstTrusted(
      resolve("jobs/browser"),
      resolve("jobs/browser"),
    ),
  );
  assert.throws(
    () => validateBuilderConfiguration(`${configuration}\nafterSign: ./candidate-hook.cjs\n`),
    /execute hooks/i,
  );
  assert.throws(
    () =>
      validateBuilderConfiguration(
        configuration.replace("npmRebuild: false", "npmRebuild: true"),
      ),
    /target contract/i,
  );
  assert.throws(
    () =>
      validatePackageExecutionContract({
        name: "@bluey/jobs-browser",
        build: { afterPack: "./candidate-hook.cjs" },
      }),
    /executable builder configuration/i,
  );
  const environment = {
    BLUEY_BROWSER_BUILD_ID: BUILD_ID,
    BLUEY_BROWSER_SOURCE_COMMIT: SOURCE_COMMIT,
    BLUEY_BROWSER_BUILD_SIGNING_KEY_ID: "build-key-603",
    BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256: "b".repeat(64),
    BLUEY_BROWSER_BUILD_PRIVATE_KEY_FILE: "/tmp/bluey-build-private.pem",
    BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_FILE: "/tmp/bluey-build-keys.json",
    BLUEY_BROWSER_PROTOCOL_VERSION: "1",
    BLUEY_BROWSER_BUILD_ISSUED_AT_MS: "1785970000000",
  };
  for (const releaseId of ["latest", "LATEST", "Current", "stable", "Beta"]) {
    assert.throws(
      () =>
        parsePackageBuildEnvironment({
          ...environment,
          BLUEY_BROWSER_RELEASE_ID: releaseId,
        }),
      /release_id/i,
    );
  }
  assert.deepEqual(
    sanitizedPackagingEnvironment({
      PATH: "/trusted/bin",
      APPLE_API_KEY: "/tmp/AuthKey.p8",
      APPLE_API_KEY_BASE64: "native-secret",
      BLUEY_BROWSER_BUILD_PRIVATE_KEY_PKCS8_BASE64: "build-secret",
      BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_BASE64: "public-authority",
    }),
    {
      PATH: "/trusted/bin",
      APPLE_API_KEY: "/tmp/AuthKey.p8",
    },
  );
});

test("trusted packaging rejects prepared-tree symlinks that escape candidate source", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bluey-browser-prepared-tree-"));
  t.after(async () => rm(root, { recursive: true, force: true }));
  const jobsRoot = join(root, "jobs");
  const browser = join(jobsRoot, "browser");
  for (const directory of [
    join(browser, "assets"),
    join(browser, "browser-bundle"),
    join(browser, "dist"),
    join(browser, "node_modules"),
    join(jobsRoot, "automation", "dist"),
    join(jobsRoot, "node_modules"),
  ]) {
    await mkdir(directory, { recursive: true });
    await writeFile(join(directory, "content.txt"), "prepared\n");
  }
  await assert.doesNotReject(validatePreparedPackagingTree(browser));
  const outside = join(root, "outside-secret.txt");
  await writeFile(outside, "must not be packaged\n");
  await symlink(outside, join(browser, "dist", "escaping-link"));
  await assert.rejects(
    validatePreparedPackagingTree(browser),
    /escapes its root/,
  );

  const artifactJobs = join(root, "artifact-jobs");
  for (const directory of [
    join(artifactJobs, "automation", "dist"),
    join(artifactJobs, "browser", "browser-bundle"),
    join(artifactJobs, "browser", "dist"),
  ]) {
    await mkdir(directory, { recursive: true });
    await writeFile(join(directory, "content.txt"), "prepared\n");
  }
  await assert.doesNotReject(validatePreparedArtifactTree(artifactJobs));
  await mkdir(join(artifactJobs, "unexpected"));
  await assert.rejects(
    validatePreparedArtifactTree(artifactJobs),
    /unexpected paths/,
  );
});

test("trusted extraction rejects archive symlink traversal and hard links", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bluey-browser-prepared-archive-"));
  t.after(async () => rm(root, { recursive: true, force: true }));
  const validArchive = join(root, "valid.tar");
  await writeTarArchive(validArchive, [
    { path: "automation/dist/", type: "Directory" },
    { path: "automation/dist/index.js", contents: "automation\n" },
    { path: "browser/dist/", type: "Directory" },
    { path: "browser/dist/main.js", contents: "browser\n" },
    { path: "browser/browser-bundle/", type: "Directory" },
    { path: "browser/browser-bundle/chrome", contents: "chromium\n", mode: 0o755 },
    {
      path: "browser/browser-bundle/current",
      type: "SymbolicLink",
      linkpath: "chrome",
    },
  ]);
  const extracted = await extractPreparedArtifactArchive({
    archivePath: validArchive,
    outputParent: root,
  });
  assert.equal(await readFile(join(extracted, "browser", "dist", "main.js"), "utf8"), "browser\n");

  const traversalArchive = join(root, "traversal.tar");
  await writeTarArchive(traversalArchive, [
    { path: "automation/dist/", type: "Directory" },
    { path: "browser/dist/", type: "Directory" },
    { path: "browser/browser-bundle/", type: "Directory" },
    {
      path: "browser/dist/escape",
      type: "SymbolicLink",
      linkpath: "../../../trusted-release-tools",
    },
    { path: "browser/dist/escape/owned", contents: "must not escape\n" },
  ]);
  await assert.rejects(
    extractPreparedArtifactArchive({
      archivePath: traversalArchive,
      outputParent: root,
    }),
    /symbolic link|out of scope|writes through/i,
  );
  await assert.rejects(readFile(join(root, "trusted-release-tools", "owned")));

  const hardLinkArchive = join(root, "hard-link.tar");
  await writeTarArchive(hardLinkArchive, [
    { path: "automation/dist/", type: "Directory" },
    { path: "browser/dist/", type: "Directory" },
    { path: "browser/browser-bundle/", type: "Directory" },
    {
      path: "browser/dist/hard-link",
      type: "Link",
      linkpath: "../../outside",
    },
  ]);
  await assert.rejects(
    extractPreparedArtifactArchive({
      archivePath: hardLinkArchive,
      outputParent: root,
    }),
    /unsupported entry type/i,
  );

  const collisionCases = [
    ["case", "browser/dist/Main.js", "browser/dist/main.js"],
  ];
  for (const [name, left, right] of collisionCases) {
    const archive = join(root, `${name}-collision.tar`);
    await writeTarArchive(archive, [
      { path: "automation/dist/", type: "Directory" },
      { path: "browser/dist/", type: "Directory" },
      { path: "browser/browser-bundle/", type: "Directory" },
      { path: left, contents: "left\n" },
      { path: right, contents: "right\n" },
    ]);
    await assert.rejects(
      extractPreparedArtifactArchive({ archivePath: archive, outputParent: root }),
      /path collision/i,
    );
  }

  const ancestorArchive = join(root, "ancestor-collision.tar");
  await writeTarArchive(ancestorArchive, [
    { path: "automation/dist/", type: "Directory" },
    { path: "browser/dist/", type: "Directory" },
    { path: "browser/browser-bundle/", type: "Directory" },
    { path: "browser/dist/leaf", contents: "file\n" },
    { path: "browser/dist/leaf/child", contents: "child\n" },
  ]);
  await assert.rejects(
    extractPreparedArtifactArchive({
      archivePath: ancestorArchive,
      outputParent: root,
    }),
    /ancestor collision/i,
  );
  const descriptor = (path, type = "File", size = 1) => ({
    path,
    type,
    linkpath: "",
    mode: type === "Directory" ? 0o755 : 0o644,
    size,
  });
  const roots = [
    descriptor("automation/dist", "Directory", 0),
    descriptor("browser/browser-bundle", "Directory", 0),
    descriptor("browser/dist", "Directory", 0),
  ];
  assert.throws(
    () => validatePreparedArchiveDescriptors([
      ...roots,
      descriptor("browser/dist/caf\u00e9.js"),
      descriptor("browser/dist/cafe\u0301.js"),
    ]),
    /path collision/i,
  );
  assert.throws(
    () => validatePreparedArchiveDescriptors([
      ...roots,
      descriptor("browser/dist/oversized.js", "File", 3 * 1024 * 1024 * 1024 + 1),
    ]),
    /too large/i,
  );
  assert.throws(
    () => validatePreparedArchiveDescriptors(new Array(50_001).fill(roots[0])),
    /incomplete/i,
  );
});

test("trusted app.asar inspection opens exact runtime files and rejects mutations", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "bluey-browser-asar-contract-"));
  t.after(async () => rm(root, { recursive: true, force: true }));
  const valid = join(root, "valid.asar");
  await writeTestAppAsar(join(root, "valid-source"), valid);
  const contract = await inspectPackagedAppAsar(valid);
  assert.ok(contract.entryCount >= 10);
  assert.match(contract.entriesSha256, /^[0-9a-f]{64}$/);
  assert.match(contract.automationBundleSha256, /^[0-9a-f]{64}$/);
  assert.notEqual(contract.automationBundleSha256, contract.automationMainSha256);
  const changedAutomation = join(root, "changed-automation.asar");
  await writeTestAppAsar(
    join(root, "changed-automation-source"),
    changedAutomation,
    async (source) => {
      await writeFile(
        join(source, "node_modules", "@bluey", "jobs-automation", "dist", "index.js"),
        "export const changedRuntime = true;\n",
      );
    },
  );
  const changedContract = await inspectPackagedAppAsar(changedAutomation);
  assert.notEqual(
    changedContract.automationBundleSha256,
    contract.automationBundleSha256,
  );
  assert.deepEqual(
    canonicalAsarHeaderPaths({
      files: { dist: { files: { "main.js": { size: 1, offset: "0" } } } },
    }),
    ["dist", "dist/main.js"],
  );
  assert.throws(
    () => canonicalAsarHeaderPaths({ files: { "dist\\main.js": { size: 1 } } }),
    /header path/i,
  );

  const mutations = [
    ["bluey declaration", async (source) => {
      await writeFile(
        join(source, "node_modules", "@bluey", "jobs-automation", "dist", "index.d.ts"),
        "export {};\n",
      );
    }],
    ["test-shaped output", async (source) => {
      await writeFile(join(source, "dist", "runtime.test.js"), "test\n");
    }],
    ["stale authority", async (source) => {
      await writeFile(join(source, "dist", "build-descriptor.txt"), "stale\n");
    }],
    ["invalid automation package", async (source) => {
      await writeFile(
        join(source, "node_modules", "@bluey", "jobs-automation", "package.json"),
        JSON.stringify({
          name: "@bluey/jobs-automation",
          version: "0.1.0",
          type: "module",
          main: "src/index.ts",
          exports: { ".": { import: "./src/index.ts" } },
        }),
      );
    }],
    ["asar link", async (source) => {
      await symlink("main.js", join(source, "dist", "linked.js"));
    }],
  ];
  for (const [name, mutate] of mutations) {
    const source = join(root, `${name.replaceAll(" ", "-")}-source`);
    const archive = join(root, `${name.replaceAll(" ", "-")}.asar`);
    await writeTestAppAsar(source, archive, mutate);
    await assert.rejects(inspectPackagedAppAsar(archive), /packaged app\.asar/i);
  }
});

test("the checked-in workflow preserves offline threshold and no-rebuild boundaries", async () => {
  const source = await readFile(WORKFLOW_PATH, "utf8");
  assert.equal(validateWorkflowContract(source), true);
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace("          - target: windows-x64", "          - target: linux-x64"),
      ),
    /missing|three supported targets|target, offline signing/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "# PROMOTION-NO-REBUILD-END",
          "npm run package:darwin-arm64\n          # PROMOTION-NO-REBUILD-END",
        ),
      ),
    /without rebuilding/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "$signature = Get-AuthenticodeSignature -LiteralPath $installers[0].FullName",
          "$signature = Skipped",
        ),
      ),
    /Get-AuthenticodeSignature/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace("hdiutil verify", "hdiutil imageinfo"),
      ),
    /hdiutil verify/,
  );
  assert.throws(
    () => validateWorkflowContract(source.replace("cmp -s", "cmp -l")),
    /cmp -s/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replaceAll(
          "secrets.BLUEY_BROWSER_MACOS_SIGNER_IDENTITY",
          "secrets.BLUEY_BROWSER_MACOS_CSC_LINK",
        ),
      ),
    /BLUEY_BROWSER_MACOS_SIGNER_IDENTITY/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          '[[ "$signer" == "$BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY" ]]',
          '[[ -n "$signer" ]]',
        ),
      ),
    /BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "installer_payload_inventory_verified=true",
          "installer_payload_inventory_verified=false",
        ),
      ),
    /installer_payload_inventory_verified=true/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "payload_application_authenticode_verified=true",
          "payload_application_authenticode_verified=false",
        ),
      ),
    /payload_application_authenticode_verified=true/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "- name: Record native verification evidence",
          "- name: Record native verification too early",
        ),
      ),
    /inventory must be bound before native evidence/,
  );
  assert.throws(
    () => validateWorkflowContract(`${source}\n          - target: darwin-arm64\n`),
    /exactly the three supported targets/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        `${source}\n# BLUEY_BROWSER_RELEASE_PRIVATE_KEY_PKCS8_BASE64\n`,
      ),
    /offline signing/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "BLUEY_BROWSER_BUILD_ID: ${{ inputs.build_id }}",
          "BLUEY_BROWSER_BUILD_ID: ${{ secrets.BLUEY_BROWSER_BUILD_ID }}",
        ),
      ),
    /before step-scoped secrets/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "trusted-release-tools/jobs/browser/scripts/package-release.mjs",
          "candidate-source/jobs/browser/scripts/package-release.mjs",
        ),
      ),
    /trusted-release-tools|Only trusted packaging code/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "archives=($SEAL_DIRECTORY/artifacts/Bluey-Browser-*-mac-",
          "archives=(candidate-source/jobs/browser/release/Bluey-Browser-*-mac-",
        ),
      ),
    /sealed package bytes|archives=/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "environment: bluey-browser-release-signing",
          "environment: unprotected-release-signing",
        ),
      ),
    /bluey-browser-release-signing/,
  );
  assert.throws(
    () =>
      validateWorkflowContract(
        source.replace(
          "node-version: 22",
          "node-version: 22\n          token: ${{ secrets.GITHUB_TOKEN }}",
        ),
      ),
    /secret-free/,
  );
});

test("candidate, threshold authorization, and stable activation bind stored bytes", async (t) => {
  const fixture = await createCandidateFixture();
  t.after(async () => rm(fixture.root, { recursive: true, force: true }));

  const incompleteParts = join(fixture.root, "incomplete-parts");
  await mkdir(incompleteParts);
  for (const target of ["darwin-arm64", "windows-x64"]) {
    await cp(join(fixture.partsRoot, target), join(incompleteParts, target), {
      recursive: true,
    });
  }
  await assert.rejects(
    assembleCandidateSet({
      ...fixture.assembly,
      partsDirectory: incompleteParts,
      outputDirectory: join(fixture.root, "incomplete-candidate"),
    }),
    /exactly three immutable target parts/,
  );

  const assembled = await assembleCandidateSet({
    ...fixture.assembly,
    outputDirectory: fixture.candidateRoot,
  });
  assert.equal(assembled.candidate.readinessLabel, "native-verified-candidate");
  assert.equal(assembled.manifest.artifacts.length, 5);
  const candidateExpected = {
    candidateDirectory: fixture.candidateRoot,
    expectedSourceCommit: SOURCE_COMMIT,
    expectedReleaseId: RELEASE_ID,
    expectedManifestSha256: assembled.candidate.manifestSha256,
    expectedTrustPolicySha256: fixture.trustPolicySha256,
  };
  const candidate = await validateCandidateSet(candidateExpected);
  assert.equal(candidate.parts.length, 3);
  assert.ok(
    candidate.manifest.artifacts.every(
      (artifact) =>
        artifact.appContentSha256 !== artifact.sha256 &&
        artifact.automationBundleSha256 !== artifact.sha256 &&
        artifact.chromiumExecutableSha256 !== artifact.sha256 &&
        artifact.verificationEvidenceSha256 !== artifact.sha256,
    ),
  );
  for (const artifact of candidate.manifest.artifacts) {
    const target = `${artifact.platform}-${artifact.architecture}`;
    const part = candidate.parts.find((entry) => entry.record.target === target);
    assert.ok(part);
    assert.equal(
      artifact.automationBundleSha256,
      part.record.automationBundleSha256,
    );
    assert.equal(
      artifact.chromiumExecutableSha256,
      part.record.chromiumExecutableSha256,
    );
  }

  const manifestSignatures = createSignatureSet({
    signatureSetId: "manifest-signatures-603-1",
    role: "release",
    targetAudience: "bluey-jobs-browser-release-manifest-v1",
    targetBytes: candidate.manifestBytes,
    signedAtMs: candidate.manifest.publishedAtMs,
    signers: fixture.releaseSigners,
  });
  const manifestSignaturePath = join(fixture.root, "manifest-signatures.json");
  await writeFile(manifestSignaturePath, manifestSignatures.bytes);
  await assert.rejects(
    authorizeCandidateSet({
      ...authorizationArguments(
        fixture,
        assembled,
        manifestSignaturePath,
        manifestSignatures.sha256,
      ),
      outputDirectory: join(fixture.root, "future-dated-authorization"),
      verificationTimeMs: candidate.manifest.publishedAtMs - 1,
    }),
    /future-dated/,
  );
  const oneSignature = authorityJsonBytes({
    ...manifestSignatures.value,
    signatures: manifestSignatures.value.signatures.slice(0, 1),
  });
  const oneSignaturePath = join(fixture.root, "one-manifest-signature.json");
  await writeFile(oneSignaturePath, oneSignature);
  await assert.rejects(
    authorizeCandidateSet({
      ...authorizationArguments(fixture, assembled, oneSignaturePath, sha256(oneSignature)),
      outputDirectory: join(fixture.root, "under-threshold-authorization"),
    }),
    /threshold was not met/,
  );

  const authorizedRoot = join(fixture.root, "authorized");
  const authorized = await authorizeCandidateSet({
    ...authorizationArguments(
      fixture,
      assembled,
      manifestSignaturePath,
      manifestSignatures.sha256,
    ),
    outputDirectory: authorizedRoot,
  });
  assert.equal(authorized.authorization.signatureCount, 2);
  assert.equal(authorized.authorization.readinessLabel, "threshold-release-authorized");

  const canary = createCanaryEvidence(candidate.manifest);
  const canaryPath = join(fixture.root, "canary-evidence.json");
  await writeFile(canaryPath, canary.bytes);
  const activation = createActivation({
    manifestSha256: assembled.candidate.manifestSha256,
    manifestSignatureSetSha256: manifestSignatures.sha256,
    canaryEvidenceSha256: canary.sha256,
    channel: "stable",
  });
  const activationPath = join(fixture.root, "stable-activation.json");
  await writeFile(activationPath, activation.bytes);
  const activationSignatures = createSignatureSet({
    signatureSetId: "stable-activation-signatures-603-1",
    role: "promotion",
    targetAudience: "bluey-jobs-browser-release-activation-v1",
    targetBytes: activation.bytes,
    signedAtMs: activation.value.issuedAtMs,
    signers: fixture.promotionSigners,
  });
  const activationSignaturePath = join(fixture.root, "activation-signatures.json");
  await writeFile(activationSignaturePath, activationSignatures.bytes);
  const promotionRoot = join(fixture.root, "promotion");
  const promoted = await createPromotionSet({
    authorizedDirectory: authorizedRoot,
    activationPath,
    activationSignatureSetPath: activationSignaturePath,
    canaryEvidencePath: canaryPath,
    outputDirectory: promotionRoot,
    promotionRunId: "promotion-run-603-1",
    ...promotionExpected(
      fixture,
      assembled,
      manifestSignatures.sha256,
      activation.sha256,
      activationSignatures.sha256,
    ),
    requireProductionReady: true,
  });
  assert.equal(promoted.promotion.readinessLabel, "production-ready");
  assert.equal(promoted.promotion.channel, "stable");
  await validatePromotionSet({
    promotionDirectory: promotionRoot,
    ...promotionExpected(
      fixture,
      assembled,
      manifestSignatures.sha256,
      activation.sha256,
      activationSignatures.sha256,
    ),
    requireProductionReady: true,
  });

  const incompleteCanaryBytes = authorityJsonBytes({
    ...canary.value,
    checks: canary.value.checks.slice(1),
  });
  const incompleteCanaryPath = join(fixture.root, "incomplete-canary-evidence.json");
  await writeFile(incompleteCanaryPath, incompleteCanaryBytes);
  const incompleteActivation = createActivation({
    manifestSha256: assembled.candidate.manifestSha256,
    manifestSignatureSetSha256: manifestSignatures.sha256,
    canaryEvidenceSha256: sha256(incompleteCanaryBytes),
    channel: "stable",
  });
  const incompleteActivationPath = join(fixture.root, "incomplete-stable-activation.json");
  await writeFile(incompleteActivationPath, incompleteActivation.bytes);
  const incompleteActivationSignatures = createSignatureSet({
    signatureSetId: "incomplete-stable-activation-signatures-603-1",
    role: "promotion",
    targetAudience: "bluey-jobs-browser-release-activation-v1",
    targetBytes: incompleteActivation.bytes,
    signedAtMs: incompleteActivation.value.issuedAtMs,
    signers: fixture.promotionSigners,
  });
  const incompleteActivationSignaturePath = join(
    fixture.root,
    "incomplete-stable-activation-signatures.json",
  );
  await writeFile(
    incompleteActivationSignaturePath,
    incompleteActivationSignatures.bytes,
  );
  await assert.rejects(
    createPromotionSet({
      authorizedDirectory: authorizedRoot,
      activationPath: incompleteActivationPath,
      activationSignatureSetPath: incompleteActivationSignaturePath,
      canaryEvidencePath: incompleteCanaryPath,
      outputDirectory: join(fixture.root, "incomplete-stable-promotion"),
      promotionRunId: "promotion-run-incomplete-stable-603-1",
      ...promotionExpected(
        fixture,
        assembled,
        manifestSignatures.sha256,
        incompleteActivation.sha256,
        incompleteActivationSignatures.sha256,
      ),
      requireProductionReady: false,
    }),
    /exact physical canary matrix/,
  );

  const betaActivation = createActivation({
    manifestSha256: assembled.candidate.manifestSha256,
    manifestSignatureSetSha256: manifestSignatures.sha256,
    canaryEvidenceSha256: canary.sha256,
    channel: "beta",
  });
  const betaActivationPath = join(fixture.root, "beta-activation.json");
  await writeFile(betaActivationPath, betaActivation.bytes);
  const betaSignatures = createSignatureSet({
    signatureSetId: "beta-activation-signatures-603-1",
    role: "promotion",
    targetAudience: "bluey-jobs-browser-release-activation-v1",
    targetBytes: betaActivation.bytes,
    signedAtMs: betaActivation.value.issuedAtMs,
    signers: fixture.promotionSigners,
  });
  const betaSignaturePath = join(fixture.root, "beta-activation-signatures.json");
  await writeFile(betaSignaturePath, betaSignatures.bytes);
  await assert.rejects(
    createPromotionSet({
      authorizedDirectory: authorizedRoot,
      activationPath: betaActivationPath,
      activationSignatureSetPath: betaSignaturePath,
      canaryEvidencePath: canaryPath,
      outputDirectory: join(fixture.root, "beta-production-claim"),
      promotionRunId: "promotion-run-beta-603-1",
      ...promotionExpected(
        fixture,
        assembled,
        manifestSignatures.sha256,
        betaActivation.sha256,
        betaSignatures.sha256,
      ),
      requireProductionReady: true,
    }),
    /requires an exact stable activation/,
  );

  const extra = join(promotionRoot, "stale-output.txt");
  await writeFile(extra, "stale\n");
  await assert.rejects(
    validatePromotionSet({
      promotionDirectory: promotionRoot,
      ...promotionExpected(
        fixture,
        assembled,
        manifestSignatures.sha256,
        activation.sha256,
        activationSignatures.sha256,
      ),
    }),
    /stale, missing, or extra output/,
  );
  await unlink(extra);

  const artifact = join(
    fixture.candidateRoot,
    "targets",
    "windows-x64",
    "artifacts",
    `Bluey-Browser-${APP_VERSION}-win-x64.exe`,
  );
  await chmod(artifact, 0o644);
  await appendFile(artifact, "tampered");
  await assert.rejects(validateCandidateSet(candidateExpected), /artifact changed/);
});

test("new authorization rejects retired signers while stored historical authority revalidates", async (t) => {
  const fixture = await createCandidateFixture({ retiredReleaseSigners: true });
  t.after(async () => rm(fixture.root, { recursive: true, force: true }));
  const assembled = await assembleCandidateSet({
    ...fixture.assembly,
    outputDirectory: fixture.candidateRoot,
  });
  const candidate = await validateCandidateSet({
    candidateDirectory: fixture.candidateRoot,
    expectedSourceCommit: SOURCE_COMMIT,
    expectedReleaseId: RELEASE_ID,
    expectedManifestSha256: assembled.candidate.manifestSha256,
    expectedTrustPolicySha256: fixture.trustPolicySha256,
  });
  const signatures = createSignatureSet({
    signatureSetId: "historical-manifest-signatures-603-1",
    role: "release",
    targetAudience: "bluey-jobs-browser-release-manifest-v1",
    targetBytes: candidate.manifestBytes,
    signedAtMs: candidate.manifest.publishedAtMs,
    signers: fixture.releaseSigners,
  });
  const signaturePath = join(fixture.root, "historical-manifest-signatures.json");
  await writeFile(signaturePath, signatures.bytes);
  await assert.rejects(
    authorizeCandidateSet({
      ...authorizationArguments(fixture, assembled, signaturePath, signatures.sha256),
      outputDirectory: join(fixture.root, "retired-new-authorization"),
    }),
    /not authorized/,
  );

  const storedRoot = join(fixture.root, "stored-historical-authorization");
  await mkdir(storedRoot);
  await cp(fixture.candidateRoot, join(storedRoot, "candidate"), { recursive: true });
  await writeFile(join(storedRoot, "release-manifest-signatures.json"), signatures.bytes);
  await writeFile(
    join(storedRoot, "authorization-set.json"),
    canonicalJsonBytes({
      version: 1,
      audience: "bluey-jobs-browser-release-authorization-set-v1",
      repository: assembled.candidate.repository,
      authorizationRunId: "historical-authorization-run-603-1",
      candidateRunId: assembled.candidate.candidateRunId,
      sourceCommit: SOURCE_COMMIT,
      releaseId: RELEASE_ID,
      buildId: BUILD_ID,
      manifestSha256: assembled.candidate.manifestSha256,
      trustPolicySha256: fixture.trustPolicySha256,
      trustGeneration: 1,
      manifestSignatureSetId: signatures.value.signatureSetId,
      manifestSignatureSetSha256: signatures.sha256,
      signatureCount: signatures.value.signatures.length,
      readinessLabel: "threshold-release-authorized",
    }),
  );
  await assert.doesNotReject(
    validateAuthorizedCandidateSet({
      authorizedDirectory: storedRoot,
      expectedSourceCommit: SOURCE_COMMIT,
      expectedReleaseId: RELEASE_ID,
      expectedManifestSha256: assembled.candidate.manifestSha256,
      expectedTrustPolicySha256: fixture.trustPolicySha256,
      expectedManifestSignatureSetSha256: signatures.sha256,
      verificationTimeMs: VERIFICATION_TIME_MS,
    }),
  );
});

async function createCandidateFixture({ retiredReleaseSigners = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "bluey-browser-release-gate-"));
  const partsRoot = join(root, "parts");
  await mkdir(partsRoot);
  const buildKeys = generateKeyPairSync("ed25519");
  const buildKeyId = "build-key-603";
  const buildKeyringBytes = authorityJsonBytes({
    version: 1,
    audience: "bluey-jobs-browser-build-keyring-v1",
    keys: { [buildKeyId]: buildKeys.publicKey.export({ format: "jwk" }).x },
  });

  for (const [index, target] of TARGETS.entries()) {
    const targetRoot = join(root, `raw-${target.name}`);
    const authorityDirectory = join(targetRoot, "authority");
    const releaseDirectory = join(targetRoot, "release");
    const resourcesDirectory = join(releaseDirectory, "unpacked", "resources");
    await Promise.all([
      mkdir(authorityDirectory, { recursive: true }),
      mkdir(resourcesDirectory, { recursive: true }),
    ]);
    const descriptorBytes = buildDescriptorBytes(target, buildKeyId);
    const descriptorSignature = signBytes(
      null,
      descriptorBytes,
      buildKeys.privateKey,
    ).toString("base64url");
    await Promise.all([
      writeFile(join(authorityDirectory, "build-descriptor.txt"), descriptorBytes),
      writeFile(join(authorityDirectory, "build-descriptor.sig"), `${descriptorSignature}\n`),
      writeFile(join(authorityDirectory, "build-public-keys.json"), buildKeyringBytes),
    ]);
    await mkdir(join(resourcesDirectory, "release"), { recursive: true });
    for (const filename of [
      "build-descriptor.txt",
      "build-descriptor.sig",
      "build-public-keys.json",
    ]) {
      await cp(
        join(authorityDirectory, filename),
        join(resourcesDirectory, "release", filename),
      );
    }
    await writeTestAppAsar(
      join(targetRoot, "app-asar-source"),
      join(resourcesDirectory, "app.asar"),
    );
    const executablePath = target.platform === "darwin"
      ? join(
        resourcesDirectory,
        "playwright",
        `chromium-${CHROMIUM_REVISION}`,
        `chrome-mac-${target.architecture}`,
        "Google Chrome for Testing",
      )
      : join(
        resourcesDirectory,
        "playwright",
        `chromium-${CHROMIUM_REVISION}`,
        "chrome-win64",
        "chrome.exe",
      );
    await mkdir(dirname(executablePath), { recursive: true });
    await writeFile(executablePath, executableBytes(target), { mode: 0o755 });
    if (target.name === "darwin-arm64") {
      await symlink(
        "Google Chrome for Testing",
        join(dirname(executablePath), "Current Chromium"),
      );
    }
    await writeReleaseArtifacts(releaseDirectory, target);
    assert.equal(await locatePackagedResources(releaseDirectory, target.name), resourcesDirectory);

    if (index === 0) {
      const staleOutput = join(releaseDirectory, "stale.AppImage");
      await writeFile(staleOutput, "unsupported\n");
      await assert.rejects(
        sealCandidateTarget({
          targetName: target.name,
          releaseDirectory,
          authorityDirectory,
          sourceCommit: SOURCE_COMMIT,
          outputDirectory: join(targetRoot, "rejected-seal"),
        }),
        /stale, missing, or extra package/,
      );
      await unlink(staleOutput);
    }
    const sealDirectory = join(targetRoot, "package-seal");
    const sealed = await sealCandidateTarget({
      targetName: target.name,
      releaseDirectory,
      authorityDirectory,
      sourceCommit: SOURCE_COMMIT,
      outputDirectory: sealDirectory,
    });

    if (index === 0) {
      const staleAuthority = join(resourcesDirectory, "release", "stale.json");
      await writeFile(staleAuthority, "{}\n");
      await assert.rejects(
        createAppContentInventory({
          targetName: target.name,
          resourcesDirectory,
          authorityDirectory: sealDirectory,
          packageSealSha256: sealed.packageSealSha256,
          outputPath: join(targetRoot, "rejected-app-content-inventory.json"),
        }),
        /stale, missing, or extra resource/i,
      );
      await unlink(staleAuthority);

      const extraChromium = join(
        resourcesDirectory,
        "playwright",
        "chromium-9999",
      );
      await mkdir(extraChromium, { recursive: true });
      await writeFile(join(extraChromium, "extra-browser"), "stale\n");
      await assert.rejects(
        createAppContentInventory({
          targetName: target.name,
          resourcesDirectory,
          authorityDirectory: sealDirectory,
          packageSealSha256: sealed.packageSealSha256,
          outputPath: join(targetRoot, "rejected-extra-chromium-inventory.json"),
        }),
        /extra Chromium revision or browser payload/i,
      );
      await rm(extraChromium, { recursive: true, force: true });
    }

    const inventoryPath = join(targetRoot, "app-content-inventory.json");
    await createAppContentInventory({
      targetName: target.name,
      resourcesDirectory,
      authorityDirectory: sealDirectory,
      packageSealSha256: sealed.packageSealSha256,
      outputPath: inventoryPath,
    });
    const transcriptPath = join(targetRoot, "native-verification-transcript.txt");
    await writeFile(
      transcriptPath,
      [
        `package_seal_sha256=${sealed.packageSealSha256}`,
        "app_asar_runtime_contract_verified=true",
        ...(target.platform === "darwin"
          ? [
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
            "installer_payload_inventory_verified=true",
            "outer_installer_authenticode_verified=true",
            "outer_installer_timestamp_verified=true",
            "payload_application_authenticode_verified=true",
            "payload_application_timestamp_verified=true",
            "windows_protocol_construction_verified=true",
          ]),
        `${target.name} native verification passed`,
        "",
      ].join("\n"),
    );
    const nativeVerificationPath = join(targetRoot, "native-verification.json");
    await recordNativeVerification({
      targetName: target.name,
      signerIdentity: target.platform === "darwin"
        ? "Developer ID Application: Bluey Test (TEAM603)"
        : "CN=Bluey Test Authenticode",
      transcriptPath,
      packageSealSha256: sealed.packageSealSha256,
      toolVersions: target.platform === "darwin"
        ? ["codesign=Xcode-16", "stapler=Xcode-16"]
        : ["powershell=7.4.0", "signtool=10.0"],
      outputPath: nativeVerificationPath,
    });
    await collectCandidateTarget({
      targetName: target.name,
      sealDirectory,
      inventoryPath,
      nativeVerificationPath,
      transcriptPath,
      sourceCommit: SOURCE_COMMIT,
      outputDirectory: join(partsRoot, target.name),
    });
  }

  const authorityKeys = new Map();
  const authorityKeyIds = [
    "incident-key-1",
    "promotion-key-1",
    "promotion-key-2",
    "release-key-1",
    "release-key-2",
    "root-key-1",
  ];
  if (retiredReleaseSigners) {
    authorityKeyIds.push("release-key-3", "release-key-4");
  }
  authorityKeyIds.sort();
  for (const keyId of authorityKeyIds) {
    authorityKeys.set(keyId, generateKeyPairSync("ed25519"));
  }
  const roles = new Map([
    ["incident-key-1", "incident"],
    ["promotion-key-1", "promotion"],
    ["promotion-key-2", "promotion"],
    ["release-key-1", "release"],
    ["release-key-2", "release"],
    ["release-key-3", "release"],
    ["release-key-4", "release"],
    ["root-key-1", "root"],
  ]);
  const trustPolicy = {
    version: 1,
    audience: "bluey-jobs-browser-release-trust-policy-v1",
    policyId: "browser-trust-policy-603-1",
    trustGeneration: 1,
    predecessorPolicySha256: null,
    artifactOrigin: "https://downloads.bluey.sh",
    issuedAtMs: 1_785_960_000_000,
    validFromMs: 1_785_950_000_000,
    expiresAtMs: 1_789_000_000_000,
    roles: [
      { role: "incident", threshold: 1 },
      { role: "promotion", threshold: 2 },
      { role: "release", threshold: 2 },
      { role: "root", threshold: 1 },
    ],
    keys: [...authorityKeys.entries()].map(([keyId, pair]) => ({
      keyId,
      role: roles.get(keyId),
      publicKey: pair.publicKey.export({ format: "jwk" }).x,
      state: retiredReleaseSigners && ["release-key-1", "release-key-2"].includes(keyId)
        ? "retired"
        : "active",
      validFromMs: 1_785_950_000_000,
      validUntilMs: 1_789_000_000_000,
      minimumTrustGeneration: 1,
      maximumTrustGeneration: 2,
    })),
  };
  const trustPolicyBytes = authorityJsonBytes(trustPolicy);
  const trustPolicyPath = join(root, "trust-policy.json");
  await writeFile(trustPolicyPath, trustPolicyBytes);
  const trustPolicySha256 = sha256(trustPolicyBytes);
  return {
    root,
    partsRoot,
    candidateRoot: join(root, "candidate"),
    trustPolicySha256,
    releaseSigners: [
      ["release-key-1", authorityKeys.get("release-key-1").privateKey],
      ["release-key-2", authorityKeys.get("release-key-2").privateKey],
    ],
    promotionSigners: [
      ["promotion-key-1", authorityKeys.get("promotion-key-1").privateKey],
      ["promotion-key-2", authorityKeys.get("promotion-key-2").privateKey],
    ],
    assembly: {
      partsDirectory: partsRoot,
      repository: "bluey-sh/cue",
      candidateRunId: "run-603-1",
      manifestId: "browser-manifest-603-1",
      manifestGeneration: 603,
      releaseSequence: 603,
      publishedAtMs: retiredReleaseSigners
        ? 1_785_955_000_000
        : PUBLISHED_AT_MS,
      releaseNotesUrl: RELEASE_NOTES_URL,
      artifactBaseUrl: ARTIFACT_BASE_URL,
      trustPolicyPath,
      trustPolicySha256,
    },
  };
}

function authorizationArguments(fixture, assembled, signaturePath, signatureSha256) {
  return {
    candidateDirectory: fixture.candidateRoot,
    manifestSignatureSetPath: signaturePath,
    authorizationRunId: "authorization-run-603-1",
    expectedSourceCommit: SOURCE_COMMIT,
    expectedReleaseId: RELEASE_ID,
    expectedManifestSha256: assembled.candidate.manifestSha256,
    expectedTrustPolicySha256: fixture.trustPolicySha256,
    expectedManifestSignatureSetSha256: signatureSha256,
    verificationTimeMs: VERIFICATION_TIME_MS,
  };
}

function promotionExpected(
  fixture,
  assembled,
  manifestSignatureSetSha256,
  activationSha256,
  activationSignatureSetSha256,
) {
  return {
    expectedSourceCommit: SOURCE_COMMIT,
    expectedReleaseId: RELEASE_ID,
    expectedManifestSha256: assembled.candidate.manifestSha256,
    expectedTrustPolicySha256: fixture.trustPolicySha256,
    expectedManifestSignatureSetSha256: manifestSignatureSetSha256,
    expectedActivationSha256: activationSha256,
    expectedActivationSignatureSetSha256: activationSignatureSetSha256,
    verificationTimeMs: VERIFICATION_TIME_MS,
  };
}

function createSignatureSet({
  signatureSetId,
  role,
  targetAudience,
  targetBytes,
  signedAtMs,
  signers,
}) {
  const payload = {
    version: 1,
    audience: "bluey-jobs-browser-release-signature-set-v1",
    signatureSetId,
    trustGeneration: 1,
    role,
    targetAudience,
    targetSha256: sha256(targetBytes),
    signedAtMs,
  };
  const payloadBytes = authorityJsonBytes(payload);
  const value = {
    ...payload,
    signatures: signers.map(([keyId, privateKey]) => ({
      keyId,
      signature: signBytes(null, payloadBytes, privateKey).toString("base64url"),
    })),
  };
  const bytes = authorityJsonBytes(value);
  return { value, bytes, sha256: sha256(bytes) };
}

function createCanaryEvidence(manifest) {
  const value = {
    version: 1,
    audience: "bluey-jobs-browser-canary-evidence-v1",
    canaryEvidenceId: "canary-evidence-603-1",
    manifestSha256: sha256(authorityJsonBytes(manifest)),
    sourceCommit: manifest.sourceCommit,
    observedAtMs: 1_785_970_100_000,
    nativeTargets: TARGETS.map((target) => target.name).sort(),
    artifactSha256s: manifest.artifacts.map((artifact) => artifact.sha256).sort(),
    checks: BROWSER_PRODUCTION_CANARY_CHECK_IDS.map((checkId, index) => ({
      checkId,
      status: "passed",
      evidenceSha256: (index + 1).toString(16).padStart(2, "0").repeat(32),
    })),
  };
  const bytes = authorityJsonBytes(value);
  return { value, bytes, sha256: sha256(bytes) };
}

function createActivation({
  manifestSha256,
  manifestSignatureSetSha256,
  canaryEvidenceSha256,
  channel,
}) {
  const value = {
    version: 1,
    audience: "bluey-jobs-browser-release-activation-v1",
    activationId: `browser-activation-${channel}-603-1`,
    activationGeneration: 603,
    trustGeneration: 1,
    channel,
    channelSequence: 603,
    manifestSha256,
    signatureSetSha256: manifestSignatureSetSha256,
    acceptedServerReleaseIds: ["server-603.1", "server-603.2"],
    canaryEvidenceSha256,
    issuedAtMs: VERIFICATION_TIME_MS,
    expiresAtMs: VERIFICATION_TIME_MS + 86_400_000,
  };
  const bytes = authorityJsonBytes(value);
  return { value, bytes, sha256: sha256(bytes) };
}

function buildDescriptorBytes(target, signingKeyId) {
  return Buffer.from(
    [
      "version=1",
      "audience=bluey-jobs-browser-build-v1",
      `release_id=${RELEASE_ID}`,
      `build_id=${BUILD_ID}`,
      `app_version=${APP_VERSION}`,
      "app_id=sh.bluey.jobs.browser",
      "protocol_version=1",
      `source_commit=${SOURCE_COMMIT}`,
      `platform=${target.platform}`,
      `architecture=${target.architecture}`,
      "electron_version=37.2.3",
      "playwright_version=1.55.0",
      `chromium_revision=${CHROMIUM_REVISION}`,
      "issued_at_ms=1785970000000",
      `signing_key_id=${signingKeyId}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

function executableBytes(target) {
  if (target.platform === "darwin") {
    const bytes = Buffer.alloc(64);
    bytes.writeUInt32LE(0xfeedfacf, 0);
    bytes.writeUInt32LE(
      target.architecture === "arm64" ? 0x0100000c : 0x01000007,
      4,
    );
    return bytes;
  }
  const bytes = Buffer.alloc(512);
  bytes.write("MZ", 0, "ascii");
  bytes.writeUInt32LE(0x80, 0x3c);
  bytes.write("PE\0\0", 0x80, "binary");
  bytes.writeUInt16LE(0x8664, 0x84);
  return bytes;
}

async function writeReleaseArtifacts(releaseDirectory, target) {
  const osName = target.platform === "darwin" ? "mac" : "win";
  const extensions = target.platform === "darwin" ? ["dmg", "zip"] : ["exe"];
  await Promise.all(
    extensions.map((extension) =>
      writeFile(
        join(
          releaseDirectory,
          `Bluey-Browser-${APP_VERSION}-${osName}-${target.architecture}.${extension}`,
        ),
        `${target.name} ${extension} signed package\n`,
      ),
    ),
  );
}

async function writeTarArchive(path, entries) {
  const blocks = [];
  for (const entry of entries) {
    const contents = Buffer.from(entry.contents ?? "", "utf8");
    const header = new Header({
      path: entry.path,
      type: entry.type ?? "File",
      linkpath: entry.linkpath,
      mode: entry.mode ?? (entry.type === "Directory" ? 0o755 : 0o644),
      uid: 0,
      gid: 0,
      size: contents.length,
      mtime: new Date(1_785_970_000_000),
    });
    const headerBlock = Buffer.alloc(512);
    assert.equal(header.encode(headerBlock), false);
    blocks.push(headerBlock);
    if (contents.length > 0) {
      const contentBlock = Buffer.alloc(Math.ceil(contents.length / 512) * 512);
      contents.copy(contentBlock);
      blocks.push(contentBlock);
    }
  }
  blocks.push(Buffer.alloc(1_024));
  await writeFile(path, Buffer.concat(blocks));
}

async function writeTestAppAsar(source, archive, mutate = async () => {}) {
  const automation = join(source, "node_modules", "@bluey", "jobs-automation");
  await Promise.all([
    mkdir(join(source, "assets"), { recursive: true }),
    mkdir(join(source, "dist"), { recursive: true }),
    mkdir(join(automation, "assets", "fonts"), { recursive: true }),
    mkdir(join(automation, "assets", "licenses"), { recursive: true }),
    mkdir(join(automation, "dist", "providers"), { recursive: true }),
  ]);
  await Promise.all([
    writeFile(
      join(source, "dist", "main.js"),
      'import { start } from "./app-lifecycle.js";\nvoid start;\n',
    ),
    writeFile(
      join(source, "dist", "app-lifecycle.js"),
      'app.setAsDefaultProtocolClient("bluey-jobs");\n',
    ),
    writeFile(
      join(source, "package.json"),
      JSON.stringify({
        name: "@bluey/jobs-browser",
        version: APP_VERSION,
        main: "dist/main.js",
        dependencies: { "@bluey/jobs-automation": "0.1.0" },
      }),
    ),
    ...[
      "icon-128.png",
      "icon-512.png",
      "trayTemplate.png",
      "trayTemplate@2x.png",
    ].map((name) => writeFile(join(source, "assets", name), "image\n")),
    writeFile(join(automation, "dist", "index.js"), "export const ready = true;\n"),
    writeFile(join(automation, "dist", "providers", "index.js"), "export {};\n"),
    writeFile(join(automation, "assets", "fonts", "font.ttf"), "font\n"),
    writeFile(join(automation, "assets", "licenses", "license.txt"), "license\n"),
    writeFile(join(automation, "THIRD_PARTY_NOTICES.md"), "notices\n"),
    writeFile(
      join(automation, "package.json"),
      JSON.stringify({
        name: "@bluey/jobs-automation",
        version: "0.1.0",
        type: "module",
        main: "dist/index.js",
        exports: { ".": { import: "./dist/index.js" } },
      }),
    ),
  ]);
  await mutate(source);
  await createPackage(source, archive);
}

function authorityJsonBytes(value) {
  return Buffer.from(`${JSON.stringify(value)}\n`, "utf8");
}
