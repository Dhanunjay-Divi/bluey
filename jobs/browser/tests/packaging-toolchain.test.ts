import { lstat, mkdir, mkdtemp, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { packageBrowserRelease } from "../scripts/package-release.mjs";
import {
  auditPackagingMetadata,
  packagingAuditArguments,
  packagingAuditEnvironment,
  packagingNpmCandidates,
  verifyPackagingToolchain,
} from "../scripts/verify-packaging-toolchain.mjs";
import { browserRoot } from "../scripts/release-package-contract.mjs";

const temporaryDirectories: string[] = [];
const metadataOnly = {
  auditClean: true, nativePublicationAllowed: false,
  installedGraphQualified: false, installedSdkExecuted: false,
};

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) =>
    rm(path, { recursive: true, force: true })));
});

describe("quarantined native packaging SDK", () => {
  it("separates the private pinned release SDK from the web workspace", async () => {
    const browser = JSON.parse(await readFile(join(browserRoot, "package.json"), "utf8"));
    const sdk = JSON.parse(await readFile(join(browserRoot, "packaging/package.json"), "utf8"));
    const root = JSON.parse(await readFile(join(browserRoot, "../package.json"), "utf8"));
    expect(browser.devDependencies["electron-builder"]).toBeUndefined();
    expect(browser.devDependencies["@electron/asar"]).toBe("4.3.1");
    expect(sdk.private).toBe(true);
    expect(sdk.devDependencies).toEqual({ "electron-builder": "26.15.3" });
    expect(sdk.scripts.audit).toBe("node ../scripts/verify-packaging-toolchain.mjs");
    expect(root.workspaces).not.toContain("browser/packaging");
    expect(root.workspaces).not.toContain("browser/*");
  });

  it("uses only strict public metadata auditing and independently derived npm paths", () => {
    expect(packagingAuditArguments("/unit/user", "/unit/global")).toEqual([
      "audit", "--audit-level=moderate", "--include=dev", "--package-lock-only", "--workspaces=false",
      "--ignore-scripts", "--registry=https://registry.npmjs.org/",
      "--userconfig=/unit/user", "--globalconfig=/unit/global",
      "--fetch-retries=0", "--fetch-timeout=30000",
    ]);
    const node = resolve("unit-node/bin/node");
    expect(packagingNpmCandidates(node)).toEqual([
      join(dirname(node), "node_modules/npm/bin/npm-cli.js"),
      resolve(dirname(node), "../lib/node_modules/npm/bin/npm-cli.js"),
    ]);
    const environment = packagingAuditEnvironment("/unit/private", {
      PATH: "/unit/untrusted", npm_execpath: "/unit/untrusted/npm-cli.js", NPM_TOKEN: "unit-secret",
      NODE_OPTIONS: "--require=unit", CSC_LINK: "unit-signing-secret", APPLE_API_KEY: "unit-key",
      HTTPS_PROXY: "unit-proxy", NPM_CONFIG_USERCONFIG: "/owner/config",
      NPM_CONFIG_REGISTRY: "https://not-public.invalid",
    });
    expect(environment.HOME).toBe("/unit/private");
    for (const name of ["PATH", "npm_execpath", "NPM_TOKEN", "NODE_OPTIONS", "CSC_LINK", "APPLE_API_KEY",
      "HTTPS_PROXY", "NPM_CONFIG_USERCONFIG", "NPM_CONFIG_REGISTRY"]) {
      expect(environment).not.toHaveProperty(name);
    }
  });

  it("audits exact two copied inputs without installed SDK files or execution authority", async () => {
    const root = await syntheticMetadata();
    await writeFile(join(root, ".npmrc"), "//registry.npmjs.org/:_authToken=unit-owner-secret\n");
    let scratch = "";
    const auditRunner = vi.fn(async (args: string[], options: { cwd: string; env: Record<string, string> }) => {
      scratch = options.cwd;
      expect(scratch).not.toBe(root);
      expect(options.env.HOME).toBe(scratch);
      for (const name of ["package.json", "package-lock.json"]) {
        expect(await readFile(join(scratch, name))).toEqual(await readFile(join(root, name)));
      }
      expect(await readFile(join(scratch, "user.npmrc"), "utf8")).toBe("");
      expect(await readFile(join(scratch, "global.npmrc"), "utf8")).toBe("");
      await expect(lstat(join(scratch, ".npmrc"))).rejects.toMatchObject({ code: "ENOENT" });
      await expect(lstat(join(root, "node_modules"))).rejects.toMatchObject({ code: "ENOENT" });
      expect(args).toContain("--audit-level=moderate");
      return { status: 0, signal: null };
    });
    // Synthetic unit-only audit response; not an actual registry/native qualification.
    const result = await auditPackagingMetadata({ root, auditRunner });
    expect(result).toEqual(metadataOnly);
    expect(Object.isFrozen(result)).toBe(true);
    expect(auditRunner).toHaveBeenCalledOnce();
    await expect(lstat(scratch)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it.each(["missing SDK", "missing lock", "wrong identity", "linked dependency", "symlink lock"])(
    "rejects %s before metadata auditing", async (mode) => {
      const root = await syntheticMetadata();
      const auditRunner = vi.fn();
      if (mode === "missing SDK") await rm(root, { recursive: true });
      if (mode === "missing lock") await rm(join(root, "package-lock.json"));
      if (mode === "wrong identity") {
        const path = join(root, "package.json");
        const manifest = JSON.parse(await readFile(path, "utf8"));
        await writeFile(path, JSON.stringify({ ...manifest, name: "unit-wrong-sdk" }));
      }
      if (mode === "linked dependency") {
        const path = join(root, "package-lock.json");
        const lock = JSON.parse(await readFile(path, "utf8"));
        lock.packages["node_modules/electron-builder"].link = true;
        await writeFile(path, JSON.stringify(lock));
      }
      if (mode === "symlink lock") {
        const path = join(root, "package-lock.json");
        await rm(path);
        await symlink(join(root, "package.json"), path);
      }
      await expect(auditPackagingMetadata({ root, auditRunner })).rejects.toThrow(/publication remains quarantined/);
      expect(auditRunner).not.toHaveBeenCalled();
    },
  );

  it.each([
    { status: 1, signal: null }, { status: 0, signal: "SIGTERM" },
    { status: null, error: new Error("unit-secret-child-error") },
  ])("refuses failed, interrupted or unavailable metadata audits without child output", async (result) => {
    const root = await syntheticMetadata();
    await expect(auditPackagingMetadata({ root, auditRunner: async () => result })).rejects.toThrow(
      "Bluey Browser native packaging metadata is unavailable or failed its strict audit; " +
      "native publication remains quarantined",
    );
  });

  it("refuses metadata changed during audit", async () => {
    const root = await syntheticMetadata();
    await expect(auditPackagingMetadata({ root, auditRunner: async () => {
      await writeFile(join(root, "package-lock.json"), "changed unit fixture\n");
      return { status: 0, signal: null };
    } })).rejects.toThrow(/publication remains quarantined/);
  });

  it.each(["without SDK inputs", "after clean metadata audit"])(
    "default packaging refuses %s before source/env/key/authority/commands/cleanup", async (mode) => {
      const root = await temporaryDirectory();
      if (mode === "after clean metadata audit") {
        expect(await auditPackagingMetadata({ root: await syntheticMetadata(),
          auditRunner: async () => ({ status: 0, signal: null }) })).toEqual(metadataOnly);
      }
      await mkdir(join(root, "release"));
      await writeFile(join(root, "release/preserve.txt"), "unit-existing-output");
      const commandRunner = vi.fn();
      const gitReader = vi.fn();
      const authorityLoader = vi.fn();
      const environment = new Proxy({}, { get: () => { throw new Error("environment read before quarantine"); } });
      await expect(packageBrowserRelease({
        targetName: "darwin-arm64", hostPlatform: "darwin", hostArchitecture: "arm64",
        preparedSourceRoot: root, environment, commandRunner, gitReader, authorityLoader,
      })).rejects.toThrow(/native publication is quarantined/);
      await expect(verifyPackagingToolchain()).rejects.toThrow(/native publication is quarantined/);
      expect(commandRunner).not.toHaveBeenCalled();
      expect(gitReader).not.toHaveBeenCalled();
      expect(authorityLoader).not.toHaveBeenCalled();
      expect(await readFile(join(root, "release/preserve.txt"), "utf8")).toBe("unit-existing-output");
    },
  );

  it("allows only an explicit pure unit verifier fixture to reach normal source validation", async () => {
    const root = await temporaryDirectory();
    const toolchainVerifier = vi.fn(async () => join(root, "synthetic-builder-cli.js"));
    await expect(packageBrowserRelease({
      targetName: "darwin-arm64", hostPlatform: "darwin", hostArchitecture: "arm64",
      preparedSourceRoot: "", toolchainVerifier,
    })).rejects.toThrow(/requires prepared source/);
    expect(toolchainVerifier).toHaveBeenCalledOnce();
  });

  it("keeps actual CLIs quarantined/audit-only with no waiver or SDK module read", async () => {
    const source = await readFile(join(browserRoot, "scripts/package-release.mjs"), "utf8");
    expect(source).toContain("toolchainVerifier = verifyPackagingToolchain");
    expect(source.indexOf("await toolchainVerifier()")).toBeLessThan(source.indexOf("await requirePreparedSourceRoot("));
    expect(source.slice(source.indexOf("async function main()"))).not.toContain("toolchainVerifier:");
    expect(source).not.toContain('require.resolve("electron-builder/package.json")');
    const verifier = await readFile(join(browserRoot, "scripts/verify-packaging-toolchain.mjs"), "utf8");
    const main = verifier.slice(verifier.indexOf("if (process.argv[1]"));
    expect(main).toContain("auditPackagingMetadata()");
    expect(main).not.toContain("verifyPackagingToolchain()");
    expect(verifier).toContain("validateNativePackagingLock(manifest, lock)");
    expect(verifier).not.toContain("process.env.npm_execpath");
    expect(verifier).not.toContain('"electron-builder", "cli.js"');
    expect(verifier).not.toMatch(/process\.env\.[A-Z_]*(SKIP|BYPASS|WAIVER)/);
  });
});

async function temporaryDirectory() {
  const root = await realpath(await mkdtemp(join(tmpdir(), "bluey-packaging-sdk-unit-")));
  temporaryDirectories.push(root);
  return root;
}

async function syntheticMetadata() {
  const root = await temporaryDirectory();
  const manifest = {
    name: "@bluey/jobs-browser-packaging", private: true, version: "0.1.0",
    devDependencies: { "electron-builder": "26.15.3" },
  };
  await Promise.all([
    writeFile(join(root, "package.json"), JSON.stringify(manifest)),
    writeFile(join(root, "package-lock.json"), JSON.stringify({
      name: manifest.name, version: manifest.version, lockfileVersion: 3,
      packages: {
        "": manifest,
        "node_modules/electron-builder": {
          version: "26.15.3", license: "MIT",
          resolved: "https://registry.npmjs.org/electron-builder/-/electron-builder-26.15.3.tgz",
          integrity: `sha512-${Buffer.alloc(64).toString("base64")}`,
        },
      },
    })),
  ]);
  return root;
}
