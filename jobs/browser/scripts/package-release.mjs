import { spawnSync } from "node:child_process";
import { lstat, realpath } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  browserRoot,
  cleanReleaseOutputs,
  generateSignedDescriptorResources,
  parseRequiredBuildEnvironment,
  requireNativeSigningCredentials,
  requireReleaseTarget,
  sanitizedPackagingEnvironment,
  validateHeadedChromiumBundle,
  validatePreparedPackagingTree,
  validatePreparedSourceAgainstTrusted,
  validateReleaseArtifacts,
  validateSigningInputs,
  validateSourceRevision,
} from "./release-package-contract.mjs";

const require = createRequire(import.meta.url);

export async function packageBrowserRelease({
  targetName,
  preparedSourceRoot,
  environment = process.env,
  hostPlatform = process.platform,
  hostArchitecture = process.arch,
  commandRunner = runCommand,
  gitReader = readGitState,
  authorityLoader = loadCompiledAuthority,
} = {}) {
  const target = requireReleaseTarget(targetName, hostPlatform, hostArchitecture);
  const candidateBrowserRoot = await requirePreparedSourceRoot(preparedSourceRoot);
  await validatePreparedPackagingTree(candidateBrowserRoot);
  const candidateRepositoryRoot = resolve(candidateBrowserRoot, "../..");
  const sourceContract = await validatePreparedSourceAgainstTrusted(
    candidateBrowserRoot,
    browserRoot,
  );
  const inputs = parseRequiredBuildEnvironment(environment);
  const gitState = gitReader(candidateRepositoryRoot);
  validateSourceRevision(inputs.sourceCommit, gitState.headCommit, gitState.statusOutput);
  requireNativeSigningCredentials(target, environment);
  const signingMaterial = await validateSigningInputs(inputs, candidateRepositoryRoot);
  const builderEnvironment = sanitizedPackagingEnvironment(environment);
  const releaseDirectory = join(candidateBrowserRoot, "release");
  const authorityDirectory = join(candidateBrowserRoot, "release-authority");
  const bundleDirectory = join(candidateBrowserRoot, "browser-bundle");

  await cleanReleaseOutputs(candidateBrowserRoot, { preservePreparedBuild: true });
  try {
    await requirePreparedApplication(candidateBrowserRoot);
    await validateHeadedChromiumBundle(
      bundleDirectory,
      sourceContract.chromiumRevision,
      target,
    );
    const authority = await authorityLoader();
    await generateSignedDescriptorResources({
      outputDirectory: authorityDirectory,
      inputs,
      target,
      sourceContract,
      signingMaterial,
      authority,
    });
    const builderCli = join(dirname(require.resolve("electron-builder/package.json")), "cli.js");
    commandRunner(
      process.execPath,
      [
        builderCli,
        "--projectDir",
        candidateBrowserRoot,
        "--config",
        join(browserRoot, "electron-builder.yml"),
        ...target.electronBuilderArguments,
      ],
      browserRoot,
      builderEnvironment,
    );
    await validateReleaseArtifacts(
      releaseDirectory,
      target,
      sourceContract.appVersion,
    );
  } catch (error) {
    await cleanReleaseOutputs(candidateBrowserRoot, { preservePreparedBuild: true });
    throw error;
  }
  return Object.freeze({ target: target.name, releaseDirectory });
}

function readGitState(root) {
  return {
    headCommit: captureCommand(
      "git",
      ["rev-parse", "HEAD"],
      root,
    ),
    statusOutput: captureCommand(
      "git",
      ["status", "--porcelain=v1", "--untracked-files=all"],
      root,
    ),
  };
}

function captureCommand(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: sanitizedPackagingEnvironment(process.env),
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error("Unable to verify the Bluey Browser source revision");
  }
  return result.stdout;
}

function runCommand(command, args, cwd, environment) {
  const result = spawnSync(command, args, {
    cwd,
    env: environment,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error("Bluey Browser release packaging command failed");
  }
}

async function loadCompiledAuthority() {
  return import(pathToFileURL(join(browserRoot, "dist", "release-authority.js")).href);
}

async function main() {
  const hasExplicitPreparedRoot =
    process.argv.length === 5 && process.argv[3] === "--prepared-source-root";
  if (process.argv.length !== 3 && !hasExplicitPreparedRoot) {
    throw new Error(
      "Bluey Browser release packaging requires a target and --prepared-source-root",
    );
  }
  const result = await packageBrowserRelease({
    targetName: process.argv[2],
    preparedSourceRoot: hasExplicitPreparedRoot ? process.argv[4] : ".",
  });
  console.log(`Bluey Browser ${result.target} package output is in the clean release directory`);
}

async function requirePreparedSourceRoot(value) {
  if (typeof value !== "string" || !value || value.includes("\0")) {
    throw new Error("Bluey Browser release packaging requires prepared source");
  }
  const root = resolve(value);
  const entry = await lstat(root);
  if (!entry.isDirectory() || entry.isSymbolicLink() || await realpath(root) !== root) {
    throw new Error("Bluey Browser prepared source root is invalid");
  }
  return root;
}

async function requirePreparedApplication(root) {
  const mainPath = join(root, "dist", "main.js");
  const entry = await lstat(mainPath);
  if (!entry.isFile() || entry.isSymbolicLink() || entry.size < 1) {
    throw new Error("Bluey Browser prepared application build is missing");
  }
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  main().catch((error) => {
    const message = error instanceof Error ? error.message : "Unknown packaging failure";
    console.error(`Bluey Browser release packaging failed: ${message}`);
    process.exitCode = 1;
  });
}
