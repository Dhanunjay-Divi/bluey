import { mkdir, readdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium, type BrowserContext } from "playwright";
import {
  installBrowserNetworkGuard,
  LOCAL_BROWSER_SERVICE_WORKERS,
} from "./browser-network.js";
import { closeBrowserContexts } from "./context-shutdown.js";
import { LocalBrowserError } from "./local-failure.js";
import {
  BrowserProfilePathPolicy,
  IdentityScopedBrowserContextRegistry,
  browserIdentityScope,
} from "./browser-profile-policy.js";

export class BrowserContextRegistry {
  private readonly contexts: IdentityScopedBrowserContextRegistry<Promise<BrowserContext>>;

  constructor(
    userDataDirectory: string,
    private readonly packaged: boolean,
    private readonly resourcesPath: string,
  ) {
    this.contexts = new IdentityScopedBrowserContextRegistry(
      new BrowserProfilePathPolicy(userDataDirectory),
    );
  }

  contextFor(accountId: string, applicationIdentityId: string): Promise<BrowserContext> {
    const identity = browserIdentityScope(accountId, applicationIdentityId);
    const existing = this.contexts.get(identity);
    if (existing) return existing;

    const profile = this.contexts.profileFor(identity).chromiumUserDataDirectory;
    const pending = this.launchContext(profile);
    this.contexts.bind(identity, pending);
    void pending.then(
      (context) => {
        context.on("close", () => this.contexts.release(identity, pending));
      },
      () => {
        this.contexts.release(identity, pending);
      },
    );
    return pending;
  }

  async closeAll(): Promise<void> {
    const pending = this.contexts.registrations().map(({ context }) => context);
    this.contexts.clear();
    const contexts: BrowserContext[] = [];
    for (const result of await Promise.allSettled(pending)) {
      if (result.status === "fulfilled") contexts.push(result.value);
    }
    await closeBrowserContexts(contexts);
  }

  private async launchContext(profile: string): Promise<BrowserContext> {
    await mkdir(profile, { recursive: true });
    const executablePath = this.packaged
      ? await packagedChromiumExecutable(this.resourcesPath)
      : undefined;
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
    return context;
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
