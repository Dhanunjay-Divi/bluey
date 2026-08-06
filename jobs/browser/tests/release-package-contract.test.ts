import {
  generateKeyPairSync,
  type KeyObject,
} from "node:crypto";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import * as releaseAuthority from "../src/release-authority.js";
import {
  BLUEY_BROWSER_APP_ID,
  BLUEY_BROWSER_BUILD_KEYRING_AUDIENCE,
  browserRoot,
  generateSignedDescriptorResources,
  parseRequiredBuildEnvironment,
  readBrowserSourceContract,
  requireNativeSigningCredentials,
  requireReleaseTarget,
  sanitizedPackagingEnvironment,
  sanitizedSourceBuildEnvironment,
  sha256,
  validateBuilderConfiguration,
  validateExecutableArchitecture,
  validateHeadedChromiumBundle,
  validateReleaseArtifacts,
  validateSigningInputs,
  validateSourceRevision,
} from "../scripts/release-package-contract.mjs";

const temporaryDirectories: string[] = [];
const sourceCommit = "1".repeat(40);
const signingKeyId = "browser-build-key-2026-01";

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((path) =>
      rm(path, { recursive: true, force: true }),
    ),
  );
});

describe("Bluey Browser release package contract", () => {
  it("exposes only exact host-matched Darwin and Windows targets", () => {
    expect(requireReleaseTarget("darwin-arm64", "darwin", "arm64")).toMatchObject({
      platform: "darwin",
      architecture: "arm64",
      electronBuilderArguments: ["--mac", "--arm64"],
    });
    expect(requireReleaseTarget("darwin-x64", "darwin", "x64")).toMatchObject({
      platform: "darwin",
      architecture: "x64",
      electronBuilderArguments: ["--mac", "--x64"],
    });
    expect(requireReleaseTarget("windows-x64", "win32", "x64")).toMatchObject({
      platform: "windows",
      architecture: "x64",
      electronBuilderArguments: ["--win", "--x64"],
    });

    expect(() => requireReleaseTarget("darwin-universal", "darwin", "arm64")).toThrow(
      /exactly one/i,
    );
    expect(() => requireReleaseTarget("linux-x64", "linux", "x64")).toThrow(
      /exactly one/i,
    );
    expect(() => requireReleaseTarget("windows-x64", "darwin", "arm64")).toThrow(
      /exact.*host/i,
    );
  });

  it("keeps release entrypoints explicit and omits universal and Linux packaging", async () => {
    const appPackage = JSON.parse(
      await readFile(join(browserRoot, "package.json"), "utf8"),
    ) as { scripts: Record<string, string> };
    expect(appPackage.scripts.package).toBe("node scripts/package-release.mjs");
    expect(appPackage.scripts["package:darwin-arm64"]).toBe(
      "node scripts/package-release.mjs darwin-arm64",
    );
    expect(appPackage.scripts["package:darwin-x64"]).toBe(
      "node scripts/package-release.mjs darwin-x64",
    );
    expect(appPackage.scripts["package:windows-x64"]).toBe(
      "node scripts/package-release.mjs windows-x64",
    );
    expect(
      Object.keys(appPackage.scripts).filter((name) => name.startsWith("package:")),
    ).toEqual([
      "package:darwin-arm64",
      "package:darwin-x64",
      "package:windows-x64",
    ]);
  });

  it("validates the checked-in app, builder, lock, Electron, Playwright, and Chromium identity", async () => {
    await expect(readBrowserSourceContract()).resolves.toEqual({
      appId: BLUEY_BROWSER_APP_ID,
      appVersion: "0.1.0",
      electronVersion: "43.1.0",
      playwrightVersion: "1.61.1",
      chromiumRevision: "1228",
    });
    const configuration = await readFile(join(browserRoot, "electron-builder.yml"), "utf8");
    expect(configuration).not.toMatch(/^\s+arch:/m);
    expect(() => validateBuilderConfiguration(`${configuration}\nlinux:\n`)).toThrow(
      /target contract/i,
    );
    expect(() =>
      validateBuilderConfiguration(
        configuration.replace(
          "    - target: dmg",
          "    - target: dmg\n      arch:\n        - universal",
        ),
      ),
    ).toThrow(/target contract/i);
    expect(() =>
      validateBuilderConfiguration(
        configuration.replace("    - target: nsis", "    - target: msi"),
      ),
    ).toThrow(/architecture contract/i);
    expect(() =>
      validateBuilderConfiguration(
        configuration.replace(BLUEY_BROWSER_APP_ID, "sh.bluey.jobs.unapproved"),
      ),
    ).toThrow(/appId/i);
  });

  it("requires complete explicit release inputs and an exact clean source commit", () => {
    expect(() => parseRequiredBuildEnvironment({})).toThrow(/release_id/i);
    expect(() =>
      parseRequiredBuildEnvironment({
        ...validEnvironment("relative.pem", "/tmp/public.json", "a".repeat(64)),
      }),
    ).toThrow(/private_key_file/i);
    expect(validateSourceRevision(sourceCommit, `${sourceCommit}\n`, "")).toBe(
      sourceCommit,
    );
    expect(() =>
      validateSourceRevision(sourceCommit, `${"2".repeat(40)}\n`, ""),
    ).toThrow(/exact clean source/i);
    expect(() =>
      validateSourceRevision(sourceCommit, `${sourceCommit}\n`, " M package.json\n"),
    ).toThrow(/exact clean source/i);
  });

  it("fails closed without one exact native signing and notarization credential mode", () => {
    const mac = requireReleaseTarget("darwin-arm64", "darwin", "arm64");
    const windows = requireReleaseTarget("windows-x64", "win32", "x64");
    expect(() => requireNativeSigningCredentials(mac, {})).toThrow(/notarization/i);
    expect(() =>
      requireNativeSigningCredentials(mac, {
        APPLE_API_KEY: "/outside/AuthKey.p8",
        APPLE_API_KEY_ID: "KEY123",
        APPLE_API_ISSUER: "issuer",
      }),
    ).not.toThrow();
    expect(() =>
      requireNativeSigningCredentials(mac, {
        APPLE_API_KEY: "/outside/AuthKey.p8",
        APPLE_API_KEY_ID: "KEY123",
      }),
    ).toThrow(/notarization/i);
    expect(() => requireNativeSigningCredentials(windows, {})).toThrow(
      /Authenticode/i,
    );
    expect(() =>
      requireNativeSigningCredentials(windows, { WIN_CSC_LINK: "/outside/windows.p12" }),
    ).not.toThrow();
    expect(() =>
      requireNativeSigningCredentials(windows, {
        WIN_CSC_LINK: "/outside/windows.p12",
        CSC_LINK: "/outside/ambiguous.p12",
      }),
    ).toThrow(/Authenticode/i);
  });

  it("signs canonical resources from an external Ed25519 key and digest-pinned keyring", async () => {
    const fixture = await signingFixture();
    const inputs = parseRequiredBuildEnvironment(fixture.environment);
    const signingMaterial = await validateSigningInputs(inputs, fixture.repository);
    const target = requireReleaseTarget("darwin-arm64", "darwin", "arm64");
    const outputDirectory = join(fixture.repository, "release-authority");
    const sourceContract = await readBrowserSourceContract();

    const generated = await generateSignedDescriptorResources({
      outputDirectory,
      inputs,
      target,
      sourceContract,
      signingMaterial,
      authority: releaseAuthority,
    });
    expect(await readdir(outputDirectory)).toEqual([
      "build-descriptor.sig",
      "build-descriptor.txt",
      "build-public-keys.json",
    ]);
    const [descriptorBytes, signatureBytes, keyringBytes] = await Promise.all([
      readFile(join(outputDirectory, "build-descriptor.txt")),
      readFile(join(outputDirectory, "build-descriptor.sig")),
      readFile(join(outputDirectory, "build-public-keys.json")),
    ]);
    expect(descriptorBytes).toEqual(generated.descriptorBytes);
    expect(keyringBytes).toEqual(fixture.keyringBytes);
    expect(
      Buffer.concat([descriptorBytes, signatureBytes, keyringBytes]).includes(
        fixture.privateKeyBytes,
      ),
    ).toBe(false);
    expect(
      releaseAuthority.verifyBrowserBuildProof(
        {
          descriptor: descriptorBytes.toString("base64url"),
          signature: signatureBytes.toString("utf8").trimEnd(),
        },
        { [signingKeyId]: fixture.publicKey },
      ).descriptor,
    ).toMatchObject({
      releaseId: "browser-release-603-1",
      buildId: "browser-603.1",
      sourceCommit,
      platform: "darwin",
      architecture: "arm64",
      chromiumRevision: "1228",
    });
  });

  it("rejects an unapproved keyring, the wrong private key, and repository-local credentials", async () => {
    const fixture = await signingFixture();
    const inputs = parseRequiredBuildEnvironment(fixture.environment);
    await expect(
      validateSigningInputs(
        { ...inputs, publicKeyringSha256: "0".repeat(64) },
        fixture.repository,
      ),
    ).rejects.toThrow(/credentials are invalid/i);

    const wrongKey = generateKeyPairSync("ed25519").privateKey;
    await writePrivateKey(fixture.privateKeyFile, wrongKey);
    await expect(validateSigningInputs(inputs, fixture.repository)).rejects.toThrow(
      /credentials are invalid/i,
    );

    const localPrivateKey = join(fixture.repository, "private.pem");
    await writePrivateKey(localPrivateKey, fixture.privateKey);
    await expect(
      validateSigningInputs({ ...inputs, privateKeyFile: localPrivateKey }, fixture.repository),
    ).rejects.toThrow(/credentials are invalid/i);
  });

  it("removes authority inputs from all child-process environments", () => {
    const environment = {
      ...validEnvironment("/outside/private.pem", "/outside/public.json", "a".repeat(64)),
      PATH: "/bin",
      CSC_LINK: "/outside/apple-signing.p12",
      APPLE_APP_SPECIFIC_PASSWORD: "secret",
      BLUEY_BROWSER_EXPECTED_NATIVE_SIGNER_IDENTITY:
        "Developer ID Application: Bluey Test (TEAM603)",
    };
    expect(sanitizedPackagingEnvironment(environment)).toEqual({
      PATH: "/bin",
      CSC_LINK: "/outside/apple-signing.p12",
      APPLE_APP_SPECIFIC_PASSWORD: "secret",
    });
    expect(sanitizedSourceBuildEnvironment(environment)).toEqual({
      PATH: "/bin",
    });
  });

  it("accepts only a single exact headed Chromium revision and executable architecture", async () => {
    const armTarget = requireReleaseTarget("darwin-arm64", "darwin", "arm64");
    const bundle = await temporaryDirectory("bluey-browser-bundle-");
    const executable = join(
      bundle,
      "chromium-1228",
      ...armTarget.chromiumExecutable,
    );
    await mkdir(join(executable, ".."), { recursive: true });
    await writeFile(executable, machOHeader(0x0100000c), { mode: 0o755 });
    await expect(validateHeadedChromiumBundle(bundle, "1228", armTarget)).resolves.toMatchObject({
      directory: "chromium-1228",
    });

    await writeFile(executable, machOHeader(0x01000007), { mode: 0o755 });
    await expect(validateHeadedChromiumBundle(bundle, "1228", armTarget)).rejects.toThrow(
      /architecture/i,
    );
    await mkdir(join(bundle, "chromium-1227"));
    await expect(validateHeadedChromiumBundle(bundle, "1228", armTarget)).rejects.toThrow(
      /one exact headed Chromium/i,
    );
  });

  it("recognizes only thin Mach-O and x64 PE executable headers", () => {
    const armTarget = requireReleaseTarget("darwin-arm64", "darwin", "arm64");
    const windowsTarget = requireReleaseTarget("windows-x64", "win32", "x64");
    expect(() => validateExecutableArchitecture(machOHeader(0x0100000c), armTarget)).not.toThrow();
    expect(() => validateExecutableArchitecture(peHeader(0x8664), windowsTarget)).not.toThrow();
    expect(() => validateExecutableArchitecture(peHeader(0xaa64), windowsTarget)).toThrow(
      /architecture/i,
    );
    const universal = Buffer.alloc(512);
    universal.writeUInt32BE(0xcafebabe, 0);
    expect(() => validateExecutableArchitecture(universal, armTarget)).toThrow(/thin/i);
  });

  it("accepts only the complete clean artifact set for one exact target", async () => {
    const release = await temporaryDirectory("bluey-browser-release-");
    const target = requireReleaseTarget("darwin-arm64", "darwin", "arm64");
    await Promise.all([
      writeFile(join(release, "Bluey-Browser-0.1.0-mac-arm64.dmg"), "dmg"),
      writeFile(join(release, "Bluey-Browser-0.1.0-mac-arm64.zip"), "zip"),
      writeFile(join(release, "builder-effective-config.yaml"), "metadata"),
    ]);
    await expect(validateReleaseArtifacts(release, target, "0.1.0")).resolves.toEqual({
      artifacts: [
        "Bluey-Browser-0.1.0-mac-arm64.dmg",
        "Bluey-Browser-0.1.0-mac-arm64.zip",
      ],
    });

    await writeFile(join(release, "Bluey-Browser-0.1.0-mac-x64.zip"), "stale");
    await expect(validateReleaseArtifacts(release, target, "0.1.0")).rejects.toThrow(
      /missing or extra target/i,
    );
  });
});

function validEnvironment(
  privateKeyFile: string,
  publicKeyringFile: string,
  publicKeyringSha256: string,
) {
  return {
    BLUEY_BROWSER_RELEASE_ID: "browser-release-603-1",
    BLUEY_BROWSER_BUILD_ID: "browser-603.1",
    BLUEY_BROWSER_SOURCE_COMMIT: sourceCommit,
    BLUEY_BROWSER_BUILD_SIGNING_KEY_ID: signingKeyId,
    BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256: publicKeyringSha256,
    BLUEY_BROWSER_BUILD_PRIVATE_KEY_FILE: privateKeyFile,
    BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_FILE: publicKeyringFile,
    BLUEY_BROWSER_PROTOCOL_VERSION: "1",
    BLUEY_BROWSER_BUILD_ISSUED_AT_MS: "1785970000000",
  };
}

async function signingFixture() {
  const outer = await temporaryDirectory("bluey-browser-signing-");
  const repository = join(outer, "repository");
  const credentials = join(outer, "credentials");
  await Promise.all([mkdir(repository), mkdir(credentials)]);
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const privateKeyFile = join(credentials, "private.pem");
  const publicKeyringFile = join(credentials, "public-keyring.json");
  const publicJwk = publicKey.export({ format: "jwk" });
  if (!publicJwk.x) throw new Error("Test Ed25519 public key is missing x");
  const keyringBytes = Buffer.from(
    `${JSON.stringify({
      version: 1,
      audience: BLUEY_BROWSER_BUILD_KEYRING_AUDIENCE,
      keys: { [signingKeyId]: publicJwk.x },
    })}\n`,
    "utf8",
  );
  const privateKeyBytes = await writePrivateKey(privateKeyFile, privateKey);
  await writeFile(publicKeyringFile, keyringBytes, { mode: 0o644 });
  return {
    repository,
    privateKey,
    privateKeyFile,
    privateKeyBytes,
    publicKey: publicJwk.x,
    keyringBytes,
    environment: validEnvironment(
      privateKeyFile,
      publicKeyringFile,
      sha256(keyringBytes),
    ),
  };
}

async function writePrivateKey(path: string, key: KeyObject): Promise<Buffer> {
  const bytes = Buffer.from(
    key.export({ format: "pem", type: "pkcs8" }).toString(),
    "utf8",
  );
  await writeFile(path, bytes, { mode: 0o600 });
  if (process.platform !== "win32") await chmod(path, 0o600);
  return bytes;
}

function machOHeader(cpuType: number): Buffer {
  const header = Buffer.alloc(512);
  header.writeUInt32LE(0xfeedfacf, 0);
  header.writeUInt32LE(cpuType, 4);
  return header;
}

function peHeader(machine: number): Buffer {
  const header = Buffer.alloc(512);
  header.write("MZ", 0, "ascii");
  header.writeUInt32LE(0x80, 0x3c);
  header.write("PE\0\0", 0x80, "binary");
  header.writeUInt16LE(machine, 0x84);
  return header;
}

async function temporaryDirectory(prefix: string): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), prefix));
  temporaryDirectories.push(path);
  return path;
}
