import { mkdir, readdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium, type BrowserContext } from "playwright";
import {
  installBrowserNetworkGuard,
  LOCAL_BROWSER_SERVICE_WORKERS,
} from "./browser-network.js";
import { closeBrowserContexts } from "./context-shutdown.js";
import { LocalBrowserError } from "./local-failure.js";
import { identityContextKey, identityProfileDirectory } from "./profile.js";

export class BrowserContextRegistry {
  private readonly contexts = new Map<string, BrowserContext>();

  constructor(
    private readonly userDataDirectory: string,
    private readonly packaged: boolean,
    private readonly resourcesPath: string,
  ) {}

  async contextFor(accountId: string, applicationIdentityId: string): Promise<BrowserContext> {
    const contextKey = identityContextKey(accountId, applicationIdentityId);
    const existing = this.contexts.get(contextKey);
    if (existing) return existing;
    const profile = join(
      identityProfileDirectory(this.userDataDirectory, accountId, applicationIdentityId),
      "chromium-profile",
    );
    await mkdir(profile, { recursive: true });
    const executablePath = this.packaged ? await packagedChromiumExecutable(this.resourcesPath) : undefined;
    const context = await chromium.launchPersistentContext(profile, {
      headless: false,
      ...(executablePath ? { executablePath } : { channel: "chromium" }),
      viewport: null,
      acceptDownloads: true,
      serviceWorkers: LOCAL_BROWSER_SERVICE_WORKERS,
    });
    try {
      await installBrowserNetworkGuard(context);
    } catch {
      await context.close().catch(() => undefined);
      throw new LocalBrowserError("configuration_invalid");
    }
    this.contexts.set(contextKey, context);
    context.on("close", () => this.contexts.delete(contextKey));
    return context;
  }

  async closeAll(): Promise<void> {
    await closeBrowserContexts(this.contexts.values());
    this.contexts.clear();
  }
}

async function packagedChromiumExecutable(resourcesPath: string): Promise<string> {
  const root = join(resourcesPath, "playwright");
  const candidates = process.platform === "darwin"
    ? ["Chromium", "Google Chrome for Testing"]
    : process.platform === "win32"
      ? ["chrome.exe"]
      : ["chrome", "headless_shell"];
  const found = await findExecutable(root, new Set(candidates), 0);
  if (!found) throw new LocalBrowserError("configuration_invalid");
  return found;
}

async function findExecutable(
  directory: string,
  names: ReadonlySet<string>,
  depth: number,
): Promise<string | undefined> {
  if (depth > 7) return undefined;
  const entries = await readdir(directory, { withFileTypes: true }).catch(() => []);
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isFile() && names.has(entry.name)) return path;
    if (entry.isDirectory()) {
      const nested = await findExecutable(path, names, depth + 1);
      if (nested) return nested;
    }
  }
  return undefined;
}
