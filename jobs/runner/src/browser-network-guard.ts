import type { BrowserContext } from "playwright";
import { assertPublicApplicationUrl } from "@bluey/jobs-automation";

/**
 * Deployment requirement: run the browser in a network namespace whose egress
 * policy denies private, link-local, metadata, and control-plane destinations.
 * Application-level DNS validation is defense in depth, not a DNS TOCTOU fix.
 */
export const NETWORK_EGRESS_REQUIREMENT =
  "Browser network egress must deny private and infrastructure address ranges by default.";

type PublicNetworkValidator = (url: string) => Promise<unknown>;

export async function installBrowserNetworkGuard(
  context: Pick<BrowserContext, "route" | "routeWebSocket">,
  validate: PublicNetworkValidator = assertPublicApplicationUrl,
): Promise<void> {
  await context.route("**/*", async (route) => {
    try {
      await assertPublicBrowserTarget(route.request().url(), validate);
      await route.continue();
    } catch {
      await route.abort("blockedbyclient");
    }
  });

  await context.routeWebSocket(/^wss?:\/\//i, async (route) => {
    try {
      await assertPublicBrowserTarget(route.url(), validate);
      route.connectToServer();
    } catch {
      await route.close({ code: 1008 });
    }
  });
}

export async function assertPublicBrowserTarget(
  rawUrl: string,
  validate: PublicNetworkValidator = assertPublicApplicationUrl,
): Promise<void> {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new Error("Invalid browser request target");
  }

  if (url.protocol === "ws:" || url.protocol === "wss:") {
    url.protocol = url.protocol === "wss:" ? "https:" : "http:";
  }
  await validate(url.toString());
}
