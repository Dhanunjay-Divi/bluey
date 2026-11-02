import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { validateNativePackagingLock } from "../../scripts/check-provenance-licenses.mjs";

const sdkRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../packaging");
const quarantine = "Bluey Browser native publication is quarantined; installed SDK qualification requires separate review";
const auditRefusal = "Bluey Browser native packaging metadata is unavailable or failed its strict audit; " +
  "native publication remains quarantined";

// Metadata, including a clean audit, can never authorize native SDK execution.
export async function verifyPackagingToolchain() {
  throw new Error(quarantine);
}

export function packagingAuditArguments(userConfig, globalConfig) {
  return [
    "audit", "--audit-level=moderate", "--include=dev", "--package-lock-only", "--workspaces=false",
    "--ignore-scripts", "--registry=https://registry.npmjs.org/",
    `--userconfig=${userConfig}`, `--globalconfig=${globalConfig}`,
    "--fetch-retries=0", "--fetch-timeout=30000",
  ];
}

export function packagingAuditEnvironment(root, environment = process.env) {
  // No signing keys, npm tokens, proxies, NODE_OPTIONS or owner configuration.
  return {
    ...(process.platform === "win32" && environment.SystemRoot
      ? { SystemRoot: environment.SystemRoot } : {}),
    HOME: root, USERPROFILE: root, TMPDIR: root, TMP: root, TEMP: root,
    NPM_CONFIG_CACHE: join(root, "cache"),
    NPM_CONFIG_UPDATE_NOTIFIER: "false",
    NPM_CONFIG_FUND: "false",
  };
}

export function packagingNpmCandidates(nodeExecutable) {
  const directory = dirname(nodeExecutable);
  return [
    join(directory, "node_modules/npm/bin/npm-cli.js"),
    resolve(directory, "../lib/node_modules/npm/bin/npm-cli.js"),
  ];
}

// Explicit dependency injection is for synthetic unit fixtures only. The CLI
// accepts no SDK override, audit waiver or package-execution authority.
export async function auditPackagingMetadata({ root = sdkRoot, auditRunner = runAudit } = {}) {
  let scratch;
  try {
    if (!isAbsolute(root) || resolve(root) !== root || await realpath(root) !== root) {
      throw new Error("invalid SDK root");
    }
    const rootStat = await lstat(root);
    if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) throw new Error("invalid SDK root");
    const paths = [join(root, "package.json"), join(root, "package-lock.json")];
    const before = await Promise.all(paths.map(snapshotFile));
    const manifest = JSON.parse(before[0].bytes.toString("utf8"));
    const lock = JSON.parse(before[1].bytes.toString("utf8"));
    if (validateNativePackagingLock(manifest, lock).length !== 0) throw new Error("invalid SDK metadata");
    scratch = await mkdtemp(join(tmpdir(), "bluey-browser-sdk-audit-"));
    const userConfig = join(scratch, "user.npmrc");
    const globalConfig = join(scratch, "global.npmrc");
    await Promise.all([
      ...[userConfig, globalConfig].map((path) =>
        writeFile(path, "", { flag: "wx", mode: 0o600 })),
      writeFile(join(scratch, "package.json"), before[0].bytes, { flag: "wx", mode: 0o600 }),
      writeFile(join(scratch, "package-lock.json"), before[1].bytes, { flag: "wx", mode: 0o600 }),
    ]);
    const result = await auditRunner(packagingAuditArguments(userConfig, globalConfig), {
      // No SDK/ancestor project .npmrc, workspace overrides or installed modules.
      cwd: scratch, env: packagingAuditEnvironment(scratch),
      stdio: ["ignore", "ignore", "ignore"], timeout: 120_000, killSignal: "SIGKILL", shell: false,
    });
    if (result?.error || result?.signal || result?.status !== 0) throw new Error("audit refused");
    const after = await Promise.all(paths.map(snapshotFile));
    if (before.some((value, index) => value.identity !== after[index].identity)) {
      throw new Error("metadata changed during audit");
    }
    return Object.freeze({
      auditClean: true, nativePublicationAllowed: false,
      installedGraphQualified: false, installedSdkExecuted: false,
    });
  } catch {
    // Never surface npm output, local paths, credentials or arbitrary child errors.
    throw new Error(auditRefusal);
  } finally {
    if (scratch) {
      try { await rm(scratch, { recursive: true, force: true }); }
      catch { throw new Error(auditRefusal); }
    }
  }
}

async function snapshotFile(path) {
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size < 1 || stat.size > 8 * 1024 * 1024 ||
      await realpath(path) !== path) throw new Error("invalid metadata file");
  const bytes = await readFile(path);
  const after = await lstat(path);
  const identity = (value) => [value.dev, value.ino, value.size, value.mtimeMs, value.ctimeMs].join(":");
  if (identity(stat) !== identity(after) || bytes.length !== stat.size) throw new Error("metadata file changed");
  return { bytes, identity: `${identity(stat)}:${createHash("sha256").update(bytes).digest("hex")}` };
}

async function runAudit(args, options) {
  // Independently derive npm from this Node distribution. Never consult PATH,
  // npm_execpath or an installed native SDK. Execute no shell or install scripts.
  const node = await realpath(process.execPath);
  const distributionRoot = resolve(dirname(node), "..");
  for (const candidate of packagingNpmCandidates(node)) {
    let path;
    let before;
    try {
      path = await realpath(candidate);
      const contained = relative(distributionRoot, path);
      if (contained.startsWith("..") || isAbsolute(contained)) continue;
      before = await snapshotFile(path);
    } catch { continue; }
    const result = spawnSync(node, [path, ...args], options);
    const after = await snapshotFile(path);
    if (before.identity !== after.identity) throw new Error(auditRefusal);
    return result;
  }
  throw new Error(auditRefusal);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  if (process.argv.length !== 2) {
    console.error(auditRefusal);
    process.exitCode = 1;
  } else {
    auditPackagingMetadata().then((result) => {
      console.log(JSON.stringify(result));
    }).catch(() => {
      console.error(auditRefusal);
      process.exitCode = 1;
    });
  }
}
