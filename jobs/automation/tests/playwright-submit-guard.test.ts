import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import type {
  BrowserContext,
  Locator,
  Page,
  Request,
  Route,
  WebSocketRoute,
} from "playwright";
import type { ExactSubmitExpectation } from "../src/contracts.js";
import { PlaywrightBrowserPage } from "../src/playwright-page.js";
import {
  AdditionalSubmitRequestError,
  ExactSubmitEvidenceError,
} from "../src/trusted-submit.js";

const URL = "https://jobs.lever.co/acme/11111111-1111-4111-8111-111111111111/apply";
const BOUNDARY = "----bluey-playwright-fence";
const SUCCESSFUL_SUBMIT_HTTP_STATUSES = [200, 204, 299, 301, 302, 303, 307, 308] as const;
const UNSUCCESSFUL_SUBMIT_HTTP_STATUSES = [199, 300, 304, 305, 306, 309, 399, 400] as const;

describe("Playwright exact submit context fence", () => {
  it("reuses one Page-scoped guard across wrapper recreation", async () => {
    const fixture = pageFixture();
    const first = new PlaywrightBrowserPage(fixture.page);
    const second = new PlaywrightBrowserPage(fixture.page);

    await first.installExactSubmitGuard("lever", URL);
    await second.installExactSubmitGuard("lever", URL);
    await second.beginExactSubmitGuard();

    expect(fixture.context.routeInstalls).toBe(1);
    expect(fixture.context.webSocketRouteInstalls).toBe(1);
  });

  it("allows one exact main-frame request and blocks a delayed second mutation", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    await browserPage.beginExactSubmitGuard();
    fixture.click = async () => {
      await fixture.context.dispatch(request(fixture.frame), fixture.firstRoute);
    };

    await browserPage.locator("#submit").clickWithExactSubmit(expectation());
    const delayedRoute = fakeRoute();
    await fixture.context.dispatch(request(fixture.frame), delayedRoute);

    expect(fixture.firstRoute.fallbacks).toBe(1);
    expect(delayedRoute.aborts).toBe(1);
    await expect(browserPage.assertExactSubmitGuardClean())
      .rejects.toBeInstanceOf(AdditionalSubmitRequestError);
  });

  it.each(SUCCESSFUL_SUBMIT_HTTP_STATUSES)(
    "accepts exact-submit HTTP status %i",
    async (status) => {
      const fixture = pageFixture();
      const browserPage = new PlaywrightBrowserPage(fixture.page);
      await browserPage.installExactSubmitGuard("lever", URL);
      await browserPage.beginExactSubmitGuard();
      fixture.click = async () => {
        await fixture.context.dispatch(
          request(fixture.frame, { status }),
          fixture.firstRoute,
        );
      };

      await expect(browserPage.locator("#submit").clickWithExactSubmit(expectation()))
        .resolves.toBe(status);
    },
  );

  it.each(UNSUCCESSFUL_SUBMIT_HTTP_STATUSES)(
    "rejects exact-submit HTTP status %i",
    async (status) => {
      const fixture = pageFixture();
      const browserPage = new PlaywrightBrowserPage(fixture.page);
      await browserPage.installExactSubmitGuard("lever", URL);
      await browserPage.beginExactSubmitGuard();
      fixture.click = async () => {
        await fixture.context.dispatch(
          request(fixture.frame, { status }),
          fixture.firstRoute,
        );
      };

      await expect(browserPage.locator("#submit").clickWithExactSubmit(expectation()))
        .rejects.toThrow("submit response status");
    },
  );

  it("rejects an exact-looking fetch or beacon instead of treating it as form activation", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    await browserPage.beginExactSubmitGuard();
    const fetchRoute = fakeRoute();
    fixture.click = async () => {
      await fixture.context.dispatch(request(fixture.frame, { navigation: false }), fetchRoute);
    };

    await expect(browserPage.locator("#submit").clickWithExactSubmit(expectation()))
      .rejects.toBeInstanceOf(ExactSubmitEvidenceError);
    expect(fetchRoute.aborts).toBe(1);
  });

  it("blocks unsafe pre-submit effects and popup navigation after fill begins", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    await browserPage.beginExactSubmitGuard();
    const beaconRoute = fakeRoute();

    await fixture.context.dispatch(request(fixture.frame, { navigation: false }), beaconRoute);

    expect(beaconRoute.aborts).toBe(1);
    await expect(browserPage.assertExactSubmitGuardClean())
      .rejects.toBeInstanceOf(ExactSubmitEvidenceError);
  });

  it.each(["GET", "HEAD", "OPTIONS"])(
    "silently aborts %s subresources after applicant-data filling begins",
    async (method) => {
      const fixture = pageFixture();
      const browserPage = new PlaywrightBrowserPage(fixture.page);
      await browserPage.installExactSubmitGuard("lever", URL);
      await browserPage.beginExactSubmitGuard();
      const subresource = fakeRoute();

      await fixture.context.dispatch(
        request(fixture.frame, { method, navigation: false }),
        subresource,
      );

      expect(subresource.aborts).toBe(1);
      await expect(browserPage.assertExactSubmitGuardClean()).resolves.toBeUndefined();
    },
  );

  it("allows only a causal main-frame redirect after the exact request", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    await browserPage.beginExactSubmitGuard();
    fixture.click = async () => {
      await fixture.context.dispatch(request(fixture.frame), fixture.firstRoute);
    };
    await browserPage.locator("#submit").clickWithExactSubmit(expectation());
    const redirect = fakeRoute();
    await fixture.context.dispatch(request(fixture.frame, {
      method: "GET",
      navigation: true,
      redirectedFrom: fixture.firstRoute.requestValue,
    }), redirect);

    expect(redirect.fallbacks).toBe(1);
    await expect(browserPage.assertExactSubmitGuardClean()).resolves.toBeUndefined();
  });

  it.each([
    "https://jobs.lever.co/acme/22222222-2222-4222-8222-222222222222/apply",
    "https://attacker.example/confirmation",
  ])("blocks a causal redirect outside the approved provider job: %s", async (redirectUrl) => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    await browserPage.beginExactSubmitGuard();
    fixture.click = async () => {
      await fixture.context.dispatch(request(fixture.frame), fixture.firstRoute);
    };
    await browserPage.locator("#submit").clickWithExactSubmit(expectation());
    const redirect = fakeRoute();
    await fixture.context.dispatch(request(fixture.frame, {
      method: "GET",
      navigation: true,
      redirectedFrom: fixture.firstRoute.requestValue,
      url: redirectUrl,
    }), redirect);

    expect(redirect.aborts).toBe(1);
    await expect(browserPage.assertExactSubmitGuardClean())
      .rejects.toBeInstanceOf(AdditionalSubmitRequestError);
  });

  it("silently aborts post-submit GET subresources that are not causal redirects", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    await browserPage.beginExactSubmitGuard();
    fixture.click = async () => {
      await fixture.context.dispatch(request(fixture.frame), fixture.firstRoute);
    };
    await browserPage.locator("#submit").clickWithExactSubmit(expectation());
    const subresource = fakeRoute();
    await fixture.context.dispatch(
      request(fixture.frame, { method: "GET", navigation: false }),
      subresource,
    );

    expect(subresource.aborts).toBe(1);
    await expect(browserPage.assertExactSubmitGuardClean()).resolves.toBeUndefined();
  });

  it("closes a popup created after the guard is installed", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    let closes = 0;

    fixture.context.dispatchPage({
      close: async () => { closes += 1; },
    } as unknown as Page);
    await Promise.resolve();

    expect(closes).toBe(1);
    await expect(browserPage.assertExactSubmitGuardClean())
      .rejects.toBeInstanceOf(ExactSubmitEvidenceError);
  });

  it("denies public WebSockets even after the guarded page closes", async () => {
    const fixture = pageFixture();
    const browserPage = new PlaywrightBrowserPage(fixture.page);
    await browserPage.installExactSubmitGuard("lever", URL);
    fixture.closePage();
    const socket = fakeWebSocket();

    await fixture.context.dispatchWebSocket(socket);

    expect(socket.closes).toBe(1);
    expect(socket.connects).toBe(0);
  });
});

function expectation(): ExactSubmitExpectation {
  const fileBytes = Buffer.from("approved resume bytes");
  const sha256 = createHash("sha256").update(fileBytes).digest("hex");
  const fieldBytes = Buffer.from("ada@example.com");
  return {
    target: {
      actionUrl: URL,
      method: "post",
      enctype: "multipart/form-data",
      formTarget: "_self",
      providerJobKey: "lever:jobs.lever.co:acme:11111111-1111-4111-8111-111111111111",
      formIdentity: "[0,\"application-form\"]",
    },
    files: [{
      fieldName: "resume",
      name: `resume-${sha256}.pdf`,
      byteLength: fileBytes.byteLength,
      sha256,
    }],
    fields: [{
      fieldName: "email",
      valueByteLength: fieldBytes.byteLength,
      valueSha256: createHash("sha256").update(fieldBytes).digest("hex"),
    }],
    partOrder: [{ kind: "field", index: 0 }, { kind: "file", index: 0 }],
  };
}

function request(
  frame: object,
  overrides: {
    method?: string;
    navigation?: boolean;
    redirectedFrom?: Request | null;
    status?: number;
    url?: string;
  } = {},
): Request {
  const approved = expectation();
  const file = approved.files[0]!;
  const body = Buffer.concat([
    Buffer.from(
      `--${BOUNDARY}\r\nContent-Disposition: form-data; name="email"\r\n\r\n`
        + "ada@example.com\r\n"
        + `--${BOUNDARY}\r\nContent-Disposition: form-data; name="resume"; `
        + `filename="${file.name}"\r\nContent-Type: application/pdf\r\n\r\n`,
      "utf8",
    ),
    Buffer.from("approved resume bytes"),
    Buffer.from(`\r\n--${BOUNDARY}--\r\n`, "ascii"),
  ]);
  return {
    method: () => overrides.method ?? "POST",
    url: () => overrides.url ?? URL,
    headers: () => ({ "content-type": `multipart/form-data; boundary=${BOUNDARY}` }),
    allHeaders: async () => ({
      "content-type": `multipart/form-data; boundary=${BOUNDARY}`,
      "content-length": String(body.byteLength),
    }),
    postDataBuffer: () => body,
    isNavigationRequest: () => overrides.navigation ?? true,
    frame: () => frame,
    serviceWorker: () => null,
    redirectedFrom: () => overrides.redirectedFrom ?? null,
    response: async () => ({ status: () => overrides.status ?? 200 }),
  } as unknown as Request;
}

function pageFixture() {
  const frame = {};
  const context = new FakeContext();
  let closeHandler: (() => void) | undefined;
  const fixture: {
    page: Page;
    frame: object;
    context: FakeContext;
    firstRoute: ReturnType<typeof fakeRoute>;
    click: () => Promise<void>;
    closePage: () => void;
  } = {
    page: undefined as unknown as Page,
    frame,
    context,
    firstRoute: fakeRoute(),
    click: async () => undefined,
    closePage: () => closeHandler?.(),
  };
  const locator = {
    click: async () => fixture.click(),
  } as unknown as Locator;
  fixture.page = {
    context: () => context as unknown as BrowserContext,
    url: () => URL,
    evaluate: async () => false,
    once: (event: string, handler: () => void) => {
      if (event === "close") closeHandler = handler;
      return fixture.page;
    },
    mainFrame: () => frame,
    locator: () => locator,
  } as unknown as Page;
  return fixture;
}

class FakeContext {
  routeInstalls = 0;
  webSocketRouteInstalls = 0;
  private routeHandler?: (route: Route) => Promise<void>;
  private webSocketHandler?: (route: WebSocketRoute) => Promise<void>;
  private pageHandler?: (page: Page) => void;

  on(event: string, handler: (page: Page) => void): this {
    if (event === "page") this.pageHandler = handler;
    return this;
  }

  serviceWorkers(): [] {
    return [];
  }

  async newCDPSession(): Promise<{
    send(method: string): Promise<unknown>;
    detach(): Promise<void>;
  }> {
    const approved = expectation();
    return {
      async send(method: string) {
        if (method === "Page.getFrameTree") return { frameTree: { frame: { id: "main" } } };
        if (method === "Page.createIsolatedWorld") return { executionContextId: 1 };
        if (method === "Runtime.evaluate") {
          return {
            result: {
              type: "object",
              value: {
                target: {
                  actionUrl: approved.target.actionUrl,
                  method: approved.target.method,
                  enctype: approved.target.enctype,
                  formTarget: approved.target.formTarget,
                  formIdentity: approved.target.formIdentity,
                },
                parts: [{
                  kind: "field",
                  source: "visible",
                  fieldName: "email",
                  value: "ada@example.com",
                }, {
                  kind: "file",
                  fieldName: approved.files[0]!.fieldName,
                  name: approved.files[0]!.name,
                  byteLength: approved.files[0]!.byteLength,
                  sha256: approved.files[0]!.sha256,
                  mediaType: "application/pdf",
                }],
              },
            },
          };
        }
        throw new Error(`unexpected CDP method: ${method}`);
      },
      async detach() {},
    };
  }

  async route(_pattern: string, handler: (route: Route) => Promise<void>): Promise<void> {
    this.routeInstalls += 1;
    this.routeHandler = handler;
  }

  async routeWebSocket(
    _pattern: RegExp,
    handler: (route: WebSocketRoute) => Promise<void>,
  ): Promise<void> {
    this.webSocketRouteInstalls += 1;
    this.webSocketHandler = handler;
  }

  async dispatch(requestValue: Request, route: ReturnType<typeof fakeRoute>): Promise<void> {
    if (!this.routeHandler) throw new Error("route is not installed");
    route.requestValue = requestValue;
    await this.routeHandler(route as unknown as Route);
  }

  async dispatchWebSocket(route: ReturnType<typeof fakeWebSocket>): Promise<void> {
    if (!this.webSocketHandler) throw new Error("WebSocket route is not installed");
    await this.webSocketHandler(route as unknown as WebSocketRoute);
  }

  dispatchPage(page: Page): void {
    this.pageHandler?.(page);
  }
}

function fakeRoute() {
  return {
    requestValue: undefined as unknown as Request,
    fallbacks: 0,
    aborts: 0,
    request() {
      return this.requestValue;
    },
    async fallback() {
      this.fallbacks += 1;
    },
    async abort() {
      this.aborts += 1;
    },
  };
}

function fakeWebSocket() {
  return {
    closes: 0,
    connects: 0,
    async close() {
      this.closes += 1;
    },
    connectToServer() {
      this.connects += 1;
      return this;
    },
  };
}
