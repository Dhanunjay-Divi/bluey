import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setImmediate } from "node:timers/promises";
import { chromium } from "playwright";
import { afterEach, expect, it, vi } from "vitest";
import { BrowserContextRegistry } from "../src/browser-context-registry.js";
import { BrowserProfilePathPolicy, browserIdentityScope } from "../src/browser-profile-policy.js";

vi.mock("playwright", () => ({
  chromium: { launchPersistentContext: vi.fn() },
}));

const launch = vi.mocked(chromium.launchPersistentContext);
const roots: string[] = [];

afterEach(async () => {
  launch.mockReset();
  vi.unstubAllEnvs();
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function makeRegistry() {
  const root = await mkdtemp(join(tmpdir(), "jobs-browser-launch-"));
  roots.push(root);
  return { root, registry: new BrowserContextRegistry(root, false, root) };
}

function browserContext() {
  return {
    route: vi.fn(async () => undefined),
    routeWebSocket: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    on: vi.fn(),
  };
}

it("launches the persistent Chromium context sandboxed and offline before installing guards", async () => {
  vi.stubEnv("BLUEY_BROWSER_TEST_PRIVATE_VALUE", "synthetic-parent-only");
  vi.stubEnv("TZ", "UTC");
  const { root, registry } = await makeRegistry();
  const context = browserContext();
  const guardEntered = Promise.withResolvers<void>();
  const releaseGuard = Promise.withResolvers<void>();
  context.routeWebSocket.mockImplementationOnce(async () => {
    guardEntered.resolve();
    await releaseGuard.promise;
  });
  launch.mockResolvedValueOnce(context as unknown as Awaited<ReturnType<typeof launch>>);

  try {
    const pending = registry.contextFor("account-a", "identity-a");
    expect(registry.contextFor("account-a", "identity-a")).toBe(pending);
    let settled = false;
    void pending.then(() => { settled = true; }, () => { settled = true; });
    await Promise.race([guardEntered.promise, pending]);
    await setImmediate();
    expect(settled).toBe(false);
    expect(context.routeWebSocket).toHaveBeenCalledOnce();
    expect(context.on).not.toHaveBeenCalled();
    releaseGuard.resolve();
    expect(await pending).toBe(context);
    expect(launch).toHaveBeenCalledOnce();
    const identity = browserIdentityScope("account-a", "identity-a");
    expect(launch.mock.calls[0]?.[0]).toBe(
      new BrowserProfilePathPolicy(root).profileFor(identity).chromiumUserDataDirectory,
    );
    const options = launch.mock.calls[0]?.[1];
    expect(options).toMatchObject({
      chromiumSandbox: true,
      offline: true,
      serviceWorkers: "block",
      channel: "chromium",
      headless: false,
      viewport: null,
      acceptDownloads: true,
      env: { TZ: "UTC" },
    });
    expect(options?.env).not.toHaveProperty("BLUEY_BROWSER_TEST_PRIVATE_VALUE");
    expect(options?.ignoreDefaultArgs).toBeUndefined();
    expect(options?.executablePath).toBeUndefined();
    expect(options?.args).toEqual([
      "--disable-quic",
      "--disable-features=WebTransport",
      "--force-webrtc-ip-handling-policy=disable_non_proxied_udp",
    ]);
    expect(context.route).toHaveBeenCalledWith("**/*", expect.any(Function));
    expect(context.routeWebSocket).toHaveBeenCalledWith(/.*/, expect.any(Function));
    expect(context.on).toHaveBeenCalledWith("close", expect.any(Function));
  } finally {
    releaseGuard.resolve();
    await registry.closeAll();
  }
  expect(context.close).toHaveBeenCalledOnce();
});

it("closes and evicts a context if guard installation fails without returning its error details", async () => {
  const { registry } = await makeRegistry();
  const rejected = browserContext();
  const recovered = browserContext();
  rejected.routeWebSocket.mockRejectedValueOnce(new Error("synthetic private guard detail"));
  launch
    .mockResolvedValueOnce(rejected as unknown as Awaited<ReturnType<typeof launch>>)
    .mockResolvedValueOnce(recovered as unknown as Awaited<ReturnType<typeof launch>>);

  try {
    await expect(registry.contextFor("account-a", "identity-a")).rejects.toMatchObject({
      code: "configuration_invalid",
      message: "Bluey Browser is not configured for this application launch.",
    });
    expect(rejected.close).toHaveBeenCalledOnce();
    expect(await registry.contextFor("account-a", "identity-a")).toBe(recovered);
    expect(launch).toHaveBeenCalledTimes(2);
    expect(launch.mock.calls[1]?.[0]).toBe(launch.mock.calls[0]?.[0]);
  } finally {
    await registry.closeAll();
  }
  expect(rejected.close).toHaveBeenCalledOnce();
  expect(recovered.close).toHaveBeenCalledOnce();
});
