import { describe, expect, it, vi } from "vitest";
import {
  LOCAL_BROWSER_SERVICE_WORKERS,
  assertSafeBrowserRequestUrl,
  assertSafeBrowserWebSocketUrl,
  guardBrowserRequest,
} from "../src/browser-network.js";

describe("local browser network guard", () => {
  it("blocks credentials, insecure schemes, loopback, link-local, and private targets", async () => {
    const blocked = [
      "http://93.184.216.34/application",
      "https://user:password@93.184.216.34/application",
      "https://127.0.0.1/application",
      "https://169.254.169.254/latest/meta-data",
      "https://10.0.0.8/application",
      "https://172.16.0.8/application",
      "https://192.168.1.8/application",
      "https://[::1]/application",
      "https://[fe80::1]/application",
      "file:///etc/passwd",
    ];

    for (const url of blocked) await expect(assertSafeBrowserRequestUrl(url)).rejects.toThrow();
  });

  it("allows public HTTPS and non-network document URLs", async () => {
    await expect(assertSafeBrowserRequestUrl("https://93.184.216.34/application.js")).resolves.toBeUndefined();
    await expect(assertSafeBrowserRequestUrl("about:blank")).resolves.toBeUndefined();
    await expect(assertSafeBrowserRequestUrl("blob:https://93.184.216.34/id")).resolves.toBeUndefined();
    await expect(assertSafeBrowserRequestUrl("data:text/plain,ok")).resolves.toBeUndefined();
    expect(LOCAL_BROWSER_SERVICE_WORKERS).toBe("block");
  });

  it("applies the guard to non-navigation subresources", async () => {
    const continueRequest = vi.fn(async () => undefined);
    const abort = vi.fn(async () => undefined);

    await guardBrowserRequest({
      request: () => ({
        url: () => "https://127.0.0.1/credential-probe.js",
        isNavigationRequest: () => false,
      }),
      continue: continueRequest,
      abort,
    });

    expect(continueRequest).not.toHaveBeenCalled();
    expect(abort).toHaveBeenCalledWith("blockedbyclient");
  });

  it("allows only public secure WebSocket targets", async () => {
    await expect(assertSafeBrowserWebSocketUrl("wss://93.184.216.34/socket")).resolves.toBeUndefined();
    await expect(assertSafeBrowserWebSocketUrl("ws://93.184.216.34/socket")).rejects.toThrow();
    await expect(assertSafeBrowserWebSocketUrl("wss://127.0.0.1/socket")).rejects.toThrow();
    await expect(assertSafeBrowserWebSocketUrl("wss://user:password@93.184.216.34/socket")).rejects.toThrow();
  });
});
