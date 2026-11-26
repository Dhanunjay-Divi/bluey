import { lstat, readFile, realpath } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import {
  BLUEY_BROWSER_APP_ID,
  verifyBrowserBuildProof,
  type BrowserBuildProof,
  type BrowserReleaseArchitecture,
  type BrowserReleasePlatform,
  type BrowserReleaseVerifyingKeys,
  type VerifiedBrowserBuildProof,
} from "./release-authority.js";

const RELEASE_DIRECTORY_NAME = "release";
const DESCRIPTOR_FILE = "build-descriptor.txt";
const SIGNATURE_FILE = "build-descriptor.sig";
const KEYRING_FILE = "build-public-keys.json";
const KEYRING_AUDIENCE = "bluey-jobs-browser-build-keyring-v1";
const MAX_DESCRIPTOR_BYTES = 4_096;
const MAX_SIGNATURE_BYTES = 256;
const MAX_KEYRING_BYTES = 16_384;

export interface BrowserPackagedRuntime {
  readonly isPackaged: boolean;
  readonly resourcesPath: string;
  readonly appVersion: string;
  readonly electronVersion: string;
  readonly platform: NodeJS.Platform;
  readonly architecture: string;
  readonly developmentReleaseDirectory?: string;
}

interface BrowserBuildKeyring {
  readonly version: 1;
  readonly audience: typeof KEYRING_AUDIENCE;
  readonly keys: BrowserReleaseVerifyingKeys;
}

export async function loadPackagedBrowserBuildProof(
  runtime: BrowserPackagedRuntime,
): Promise<VerifiedBrowserBuildProof | undefined> {
  const releaseDirectory = runtime.isPackaged
    ? join(runtime.resourcesPath, RELEASE_DIRECTORY_NAME)
    : runtime.developmentReleaseDirectory;
  if (!releaseDirectory) return undefined;

  const root = await requireSafeReleaseDirectory(
    releaseDirectory,
    runtime.isPackaged ? runtime.resourcesPath : undefined,
  );
  const [descriptorBytes, signatureBytes, keyringBytes] = await Promise.all([
    readBoundedRegularFile(join(root, DESCRIPTOR_FILE), MAX_DESCRIPTOR_BYTES),
    readBoundedRegularFile(join(root, SIGNATURE_FILE), MAX_SIGNATURE_BYTES),
    readBoundedRegularFile(join(root, KEYRING_FILE), MAX_KEYRING_BYTES),
  ]);
  const signature = strictUtf8Line(signatureBytes, "build descriptor signature");
  const keyring = parseBuildKeyring(keyringBytes);
  const verified = verifyBrowserBuildProof(
    {
      descriptor: descriptorBytes.toString("base64url"),
      signature,
    },
    keyring.keys,
  );
  requireRuntimeMatch(verified, runtime);
  return verified;
}

export function claimBrowserBuildProof(
  verified: VerifiedBrowserBuildProof | undefined,
): BrowserBuildProof | undefined {
  return verified?.proof;
}

function requireRuntimeMatch(
  verified: VerifiedBrowserBuildProof,
  runtime: BrowserPackagedRuntime,
): void {
  const platform = releasePlatform(runtime.platform);
  const architecture = releaseArchitecture(runtime.architecture);
  const descriptor = verified.descriptor;
  if (
    descriptor.appId !== BLUEY_BROWSER_APP_ID ||
    descriptor.appVersion !== runtime.appVersion ||
    descriptor.electronVersion !== runtime.electronVersion ||
    descriptor.platform !== platform ||
    descriptor.architecture !== architecture
  ) {
    throw new Error("Packaged Bluey Browser release identity does not match runtime");
  }
}

function releasePlatform(platform: NodeJS.Platform): BrowserReleasePlatform {
  if (platform === "darwin") return "darwin";
  if (platform === "win32") return "windows";
  throw new Error("This platform has no authorized Bluey Browser release target");
}

function releaseArchitecture(architecture: string): BrowserReleaseArchitecture {
  if (architecture === "arm64" || architecture === "x64") return architecture;
  throw new Error("This architecture has no authorized Bluey Browser release target");
}

async function requireSafeReleaseDirectory(
  path: string,
  resourcesPath: string | undefined,
): Promise<string> {
  const lexicalPath = resolve(path);
  const entry = await lstat(lexicalPath);
  if (!entry.isDirectory() || entry.isSymbolicLink()) {
    throw new Error("Invalid Bluey Browser release resource directory");
  }
  const canonicalPath = await realpath(lexicalPath);
  if (resourcesPath) {
    const canonicalResources = await realpath(resolve(resourcesPath));
    const child = relative(canonicalResources, canonicalPath);
    if (!child || child === ".." || child.startsWith(`..${sep}`)) {
      throw new Error("Bluey Browser release resources escape the application bundle");
    }
  }
  return canonicalPath;
}

async function readBoundedRegularFile(path: string, maximum: number): Promise<Buffer> {
  const entry = await lstat(path);
  if (!entry.isFile() || entry.isSymbolicLink() || entry.size < 1 || entry.size > maximum) {
    throw new Error(`Invalid Bluey Browser release resource: ${dirname(path)}`);
  }
  const bytes = await readFile(path);
  if (bytes.length !== entry.size) {
    throw new Error("Bluey Browser release resource changed while reading");
  }
  return bytes;
}

function strictUtf8Line(bytes: Buffer, label: string): string {
  let value: string;
  try {
    value = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(`Invalid ${label}`);
  }
  if (value.endsWith("\n")) value = value.slice(0, -1);
  if (!value || value.includes("\n") || value.includes("\r") || value.trim() !== value) {
    throw new Error(`Invalid ${label}`);
  }
  return value;
}

function parseBuildKeyring(bytes: Buffer): BrowserBuildKeyring {
  let input: unknown;
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    input = JSON.parse(text);
  } catch {
    throw new Error("Invalid Bluey Browser build keyring");
  }
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new Error("Invalid Bluey Browser build keyring");
  }
  const record = input as Record<string, unknown>;
  if (
    Object.keys(record).sort().join(",") !== "audience,keys,version" ||
    record.version !== 1 ||
    record.audience !== KEYRING_AUDIENCE ||
    !record.keys ||
    typeof record.keys !== "object" ||
    Array.isArray(record.keys)
  ) {
    throw new Error("Invalid Bluey Browser build keyring");
  }
  const rawKeys = record.keys as Record<string, unknown>;
  const entries = Object.entries(rawKeys);
  if (entries.length < 1 || entries.length > 16) {
    throw new Error("Invalid Bluey Browser build keyring");
  }
  const keys: Record<string, string> = {};
  for (const [keyId, publicKey] of entries) {
    if (
      !/^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/.test(keyId) ||
      typeof publicKey !== "string" ||
      !/^[A-Za-z0-9_-]+$/.test(publicKey) ||
      Buffer.from(publicKey, "base64url").length !== 32 ||
      Buffer.from(publicKey, "base64url").toString("base64url") !== publicKey
    ) {
      throw new Error("Invalid Bluey Browser build keyring");
    }
    keys[keyId] = publicKey;
  }
  return Object.freeze({
    version: 1,
    audience: KEYRING_AUDIENCE,
    keys: Object.freeze(keys),
  });
}
