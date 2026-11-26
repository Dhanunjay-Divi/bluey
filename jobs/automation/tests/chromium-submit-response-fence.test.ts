import { afterEach, describe, expect, it, vi } from "vitest";
import type { Page } from "playwright";
import {
  ChromiumSubmitResponseFence,
  ChromiumSubmitResponseCleanupError,
  ChromiumSubmitResponseFenceError,
} from "../src/chromium-submit-response-fence.js";

const URL = "https://boards.greenhouse.io/acme/jobs/1234567";

afterEach(() => vi.useRealTimers());

describe("Chromium submit response fence", () => {
  it.each([200, 204, 400, 500])("continues exact nonredirect status %i unchanged", async (status) => {
    const fixture = createFixture();
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    fixture.pause(status);
    await expect(fence.waitForDecision()).resolves.toBe(status);
    expect(fixture.commands).toContain("Fetch.continueResponse");
    expect(fixture.failed).not.toHaveBeenCalled();
  });

  it.each([300, 301, 302, 303, 304, 307, 308, 399])(
    "fails exact HTTP %i before a redirect can follow", async (status) => {
      const fixture = createFixture();
      const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
      fixture.pause(status);
      await expect(fence.waitForDecision()).rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
      expect(fixture.commands).toContain("Fetch.failRequest");
      expect(fixture.failed).toHaveBeenCalled();
    },
  );

  it("closes the page if setup fails before the click", async () => {
    const fixture = createFixture("Fetch.enable");
    await expect(ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed))
      .rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
    expect(fixture.closed()).toBe(true);
  });

  it.each([
    { request: { url: "https://other.example/", method: "POST" } },
    { request: { url: URL, method: "GET" } },
    { frameId: "other" },
    { resourceType: "XHR" },
    { responseStatusCode: undefined },
  ])("rejects a mismatched or incomplete response", async (change) => {
    const fixture = createFixture();
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    fixture.pause(200, change);
    await expect(fence.waitForDecision()).rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
    expect(fixture.commands).toContain("Fetch.failRequest");
  });

  it("invalidates an earlier decision if a second response appears", async () => {
    const fixture = createFixture();
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    fixture.pause(200);
    await expect(fence.waitForDecision()).resolves.toBe(200);
    fixture.pause(200);
    await Promise.resolve();
    await expect(fence.waitForDecision()).rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
  });

  it("closes on a lost response or disconnected session", async () => {
    vi.useFakeTimers();
    const fixture = createFixture();
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    const result = fence.waitForDecision();
    const assertion = expect(result).rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
    await vi.advanceTimersByTimeAsync(20_000);
    await assertion;
    expect(fixture.closed()).toBe(true);
    const second = createFixture();
    const otherFence = await ChromiumSubmitResponseFence.install(second.page, URL, second.failed);
    second.disconnect();
    await expect(otherFence.waitForDecision()).rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
    expect(second.closed()).toBe(true);
  });

  it("surfaces page-close rejection as cleanup unproven", async () => {
    const fixture = createFixture(undefined, "reject");
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    await expect(fence.abort()).rejects.toBeInstanceOf(ChromiumSubmitResponseCleanupError);
    await expect(fence.abort()).rejects.toBeInstanceOf(ChromiumSubmitResponseCleanupError);
    expect(fixture.closeCalls()).toBe(1);
  });

  it("bounds a hung close and latches cleanup unproven", async () => {
    vi.useFakeTimers();
    const fixture = createFixture(undefined, "hang");
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    const assertion = expect(fence.abort()).rejects.toBeInstanceOf(ChromiumSubmitResponseCleanupError);
    await vi.advanceTimersByTimeAsync(5_000);
    await assertion;
    await expect(fence.abort()).rejects.toBeInstanceOf(ChromiumSubmitResponseCleanupError);
    expect(fixture.closeCalls()).toBe(1);
  });

  it("shares a single close across concurrent aborts", async () => {
    const fixture = createFixture(undefined, "defer");
    const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
    const first = fence.abort();
    const second = fence.abort();
    expect(fixture.closeCalls()).toBe(1);
    fixture.releaseClose();
    await expect(Promise.all([first, second])).resolves.toEqual([undefined, undefined]);
  });

  it.each(["Fetch.failRequest", "Fetch.continueResponse"])(
    "bounds stalled %s and closes the page without accepting a response", async (method) => {
      vi.useFakeTimers();
      const fixture = createFixture(undefined, undefined, method);
      const fence = await ChromiumSubmitResponseFence.install(fixture.page, URL, fixture.failed);
      fixture.pause(method === "Fetch.failRequest" ? 302 : 200);
      const assertion = expect(fence.waitForDecision()).rejects
        .toBeInstanceOf(ChromiumSubmitResponseFenceError);
      await vi.advanceTimersByTimeAsync(5_000);
      await assertion;
      expect(fixture.closed()).toBe(true);
    },
  );

  it("keeps separate page sessions independent", async () => {
    const one = createFixture();
    const two = createFixture();
    const first = await ChromiumSubmitResponseFence.install(one.page, URL, one.failed);
    const second = await ChromiumSubmitResponseFence.install(two.page, URL, two.failed);
    one.pause(302);
    two.pause(200);
    await expect(first.waitForDecision()).rejects.toBeInstanceOf(ChromiumSubmitResponseFenceError);
    await expect(second.waitForDecision()).resolves.toBe(200);
  });
});

function createFixture(
  failMethod?: string,
  closeMode?: "reject" | "hang" | "defer",
  stallMethod?: string,
) {
  type Event = {
    requestId: string;
    request: { url: string; method: string };
    frameId: string;
    resourceType: string;
    responseStatusCode?: number;
  };
  let paused: ((event: Event) => void) | undefined;
  let disconnected: (() => void) | undefined;
  let pageClosed: (() => void) | undefined;
  let closed = false;
  let closeCount = 0;
  let completeClose: (() => void) | undefined;
  const commands: string[] = [];
  const failed = vi.fn();
  const session = {
    on(event: string, handler: ((value: Event) => void) | (() => void)) {
      if (event === "Fetch.requestPaused") paused = handler as (value: Event) => void;
      if (event === "close") disconnected = handler as () => void;
      return session;
    },
    async send(method: string) {
      commands.push(method);
      if (method === failMethod) throw new Error("synthetic CDP failure");
      if (method === stallMethod) return new Promise<never>(() => undefined);
      if (method === "Page.getFrameTree") return { frameTree: { frame: { id: "main" } } };
      return {};
    },
  };
  const page = {
    context: () => ({ newCDPSession: async () => session }),
    once: (_event: string, handler: () => void) => { pageClosed = handler; },
    isClosed: () => closed,
    close: async () => {
      closeCount += 1;
      if (closeMode === "reject") throw new Error("synthetic close failure");
      if (closeMode === "hang") return new Promise<void>(() => undefined);
      if (closeMode === "defer") {
        await new Promise<void>((resolve) => { completeClose = resolve; });
      }
      closed = true;
      pageClosed?.();
    },
  } as unknown as Page;
  return {
    page, commands, failed, closed: () => closed, closeCalls: () => closeCount,
    releaseClose: () => completeClose?.(),
    disconnect: () => disconnected?.(),
    pause: (status: number, change: Partial<Event> = {}) => paused?.({
      requestId: "response", request: { url: URL, method: "POST" },
      frameId: "main", resourceType: "Document", responseStatusCode: status, ...change,
    }),
  };
}
