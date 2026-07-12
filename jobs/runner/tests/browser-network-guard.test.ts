import { describe, expect, it, vi } from "vitest";
import type { BrowserContext } from "playwright";
import {
  assertPublicBrowserTarget,
  installBrowserNetworkGuard,
  NETWORK_EGRESS_REQUIREMENT,
} from "../src/browser-network-guard.js";

describe("browser network guard", () => {
  it("fails closed for navigation and every routed subresource kind", async () => {
    const context = new FakeBrowserContext();
    const validate = vi.fn(async (url: string) => {
      if (url.includes("blocked.example")) throw new Error("private target");
    });
    await installBrowserNetworkGuard(context as unknown as BrowserContext, validate);

    for (const resourceType of ["document", "xhr", "fetch", "image", "script", "stylesheet"]) {
      const route = new FakeHttpRoute(`https://blocked.example/${resourceType}`, resourceType);
      await context.httpHandler!(route as never);
      expect(route.aborted).toBe(true);
      expect(route.continued).toBe(false);
    }

    const allowed = new FakeHttpRoute("https://public.example/app", "document");
    await context.httpHandler!(allowed as never);
    expect(allowed.continued).toBe(true);
    expect(allowed.aborted).toBe(false);
    expect(validate).toHaveBeenCalledTimes(7);
  });

  it("validates WebSocket handshakes and never connects a denied target", async () => {
    const context = new FakeBrowserContext();
    const validate = vi.fn(async (url: string) => {
      if (url.includes("127.0.0.1")) throw new Error("private target");
    });
    await installBrowserNetworkGuard(context as unknown as BrowserContext, validate);

    const denied = new FakeWebSocketRoute("wss://127.0.0.1/socket");
    await context.webSocketHandler!(denied as never);
    expect(denied.connected).toBe(false);
    expect(denied.closed).toBe(true);

    const allowed = new FakeWebSocketRoute("wss://public.example/socket");
    await context.webSocketHandler!(allowed as never);
    expect(allowed.connected).toBe(true);
    expect(allowed.closed).toBe(false);
    expect(validate).toHaveBeenCalledWith("https://public.example/socket");
  });

  it("rejects credential-bearing and private targets through the production validator", async () => {
    await expect(assertPublicBrowserTarget("https://user:secret@example.com/resource"))
      .rejects.toThrow("credentials");
    await expect(assertPublicBrowserTarget("wss://127.0.0.1/socket"))
      .rejects.toThrow("Private network");
    expect(NETWORK_EGRESS_REQUIREMENT).toContain("deny private");
  });
});

class FakeBrowserContext {
  httpHandler?: (route: never) => Promise<void>;
  webSocketHandler?: (route: never) => Promise<void>;

  async route(_pattern: string, handler: (route: never) => Promise<void>): Promise<void> {
    this.httpHandler = handler;
  }

  async routeWebSocket(_pattern: RegExp, handler: (route: never) => Promise<void>): Promise<void> {
    this.webSocketHandler = handler;
  }
}

class FakeHttpRoute {
  aborted = false;
  continued = false;

  constructor(private readonly target: string, readonly resourceType: string) {}

  request(): { url(): string; resourceType(): string } {
    return { url: () => this.target, resourceType: () => this.resourceType };
  }

  async continue(): Promise<void> {
    this.continued = true;
  }

  async abort(): Promise<void> {
    this.aborted = true;
  }
}

class FakeWebSocketRoute {
  connected = false;
  closed = false;

  constructor(private readonly target: string) {}

  url(): string {
    return this.target;
  }

  connectToServer(): this {
    this.connected = true;
    return this;
  }

  async close(): Promise<void> {
    this.closed = true;
  }
}
