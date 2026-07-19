import { constants } from "node:fs";
import {
  chmod,
  lstat,
  mkdir,
  open,
  readFile,
  rename,
  rm,
} from "node:fs/promises";
import { dirname, join } from "node:path";
import { randomBytes } from "node:crypto";

const PREFERENCES_VERSION = 1;
const MAX_PREFERENCES_BYTES = 4 * 1024;

export interface BrowserPreferences {
  version: typeof PREFERENCES_VERSION;
  backgroundEnabled: boolean;
}

export type CloseDisposition = "hide" | "quit" | "close";

export function closeDisposition(input: {
  backgroundEnabled: boolean;
  quitting: boolean;
}): CloseDisposition {
  if (input.quitting) return "close";
  return input.backgroundEnabled ? "hide" : "quit";
}

export class BrowserPreferenceStore {
  private constructor(
    readonly path: string,
    private preferences: BrowserPreferences,
  ) {}

  static async open(userDataDirectory: string): Promise<BrowserPreferenceStore> {
    const directory = join(userDataDirectory, "preferences");
    await mkdir(directory, { recursive: true, mode: 0o700 });
    if (process.platform !== "win32") await chmod(directory, 0o700);
    const path = join(directory, "browser-controller-v1.json");
    const preferences = await readPreferences(path);
    return new BrowserPreferenceStore(path, preferences);
  }

  snapshot(): BrowserPreferences {
    return { ...this.preferences };
  }

  async setBackgroundEnabled(enabled: boolean): Promise<BrowserPreferences> {
    const next: BrowserPreferences = {
      version: PREFERENCES_VERSION,
      backgroundEnabled: Boolean(enabled),
    };
    await atomicPrivateWrite(this.path, Buffer.from(`${JSON.stringify(next)}\n`, "utf8"));
    this.preferences = next;
    return this.snapshot();
  }
}

export function parseBrowserPreferences(value: unknown): BrowserPreferences {
  if (!value || typeof value !== "object" || Array.isArray(value)) return defaults();
  const record = value as Record<string, unknown>;
  if (record.version !== PREFERENCES_VERSION || typeof record.backgroundEnabled !== "boolean") {
    return defaults();
  }
  return {
    version: PREFERENCES_VERSION,
    backgroundEnabled: record.backgroundEnabled,
  };
}

async function readPreferences(path: string): Promise<BrowserPreferences> {
  let metadata;
  try {
    metadata = await lstat(path);
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return defaults();
    throw error;
  }
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > MAX_PREFERENCES_BYTES) {
    return defaults();
  }
  try {
    return parseBrowserPreferences(JSON.parse(await readFile(path, "utf8")));
  } catch {
    return defaults();
  }
}

async function atomicPrivateWrite(path: string, bytes: Buffer): Promise<void> {
  if (bytes.length === 0 || bytes.length > MAX_PREFERENCES_BYTES) {
    throw new Error("Invalid Bluey Browser preferences");
  }
  const temporary = `${path}.${process.pid}.${randomBytes(8).toString("hex")}.tmp`;
  let handle;
  try {
    await mkdir(dirname(path), { recursive: true, mode: 0o700 });
    handle = await open(
      temporary,
      constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY,
      0o600,
    );
    await handle.writeFile(bytes);
    await handle.sync();
    await handle.close();
    handle = undefined;
    if (process.platform !== "win32") await chmod(temporary, 0o600);
    await rename(temporary, path);
  } finally {
    await handle?.close().catch(() => undefined);
    await rm(temporary, { force: true });
    bytes.fill(0);
  }
}

function defaults(): BrowserPreferences {
  return { version: PREFERENCES_VERSION, backgroundEnabled: false };
}

function nodeErrorCode(error: unknown): string | undefined {
  return error && typeof error === "object" && "code" in error
    ? String((error as { code?: unknown }).code)
    : undefined;
}
