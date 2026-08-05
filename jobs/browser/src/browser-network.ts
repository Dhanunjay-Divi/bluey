import type { BrowserContext } from "playwright";
import { assertPublicApplicationUrl } from "@bluey/jobs-automation";

export const LOCAL_BROWSER_SERVICE_WORKERS = "block" as const;

interface GuardedRequestRoute {
  request(): { url(): string };
  continue(): Promise<void>;
  abort(errorCode: "blockedbyclient"): Promise<void>;
}

interface GuardedWebSocketRoute {
  url(): string;
  connectToServer(): unknown;
  close(options: { code: number; reason: string }): Promise<void>;
}

export async function installBrowserNetworkGuard(context: BrowserContext): Promise<void> {
  await context.route("**/*", (route) => guardBrowserRequest(route));
  await context.routeWebSocket(/.*/, (route) => guardBrowserWebSocket(route));
}

export async function guardBrowserRequest(route: GuardedRequestRoute): Promise<void> {
  try {
    await assertSafeBrowserRequestUrl(route.request().url());
    await route.continue();
  } catch {
    await route.abort("blockedbyclient").catch(() => undefined);
  }
}

export async function guardBrowserWebSocket(route: GuardedWebSocketRoute): Promise<void> {
  await route.close({ code: 1008, reason: "Network target blocked" }).catch(() => undefined);
}

export async function assertSafeBrowserRequestUrl(rawUrl: string): Promise<void> {
  const url = new URL(rawUrl);
  if (["about:", "blob:", "data:"].includes(url.protocol)) return;
  if (url.protocol !== "https:") throw new Error("network_target_blocked");
  await assertPublicApplicationUrl(url.toString());
}

export async function assertSafeBrowserWebSocketUrl(_rawUrl: string): Promise<never> {
  throw new Error("network_target_blocked");
}
