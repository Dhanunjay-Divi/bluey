import { describe, expect, it, vi } from "vitest";
import type { BrowserPage } from "../src/contracts.js";
import {
  PrivateVisualParser,
  VisualFormObserver,
  visualObservationEnabled,
  type VisualObservationProvider,
} from "../src/visual-observation.js";

function page(): BrowserPage {
  return {
    url: () => "https://jobs.example.test/apply",
    title: async () => "Apply",
    locator: vi.fn(),
    controls: async () => [],
    bodyText: async () => "",
    waitForSettled: async () => undefined,
    screenshot: async () => new Uint8Array([1, 2, 3]),
  } as unknown as BrowserPage;
}

const provider: VisualObservationProvider = {
  name: "fixture",
  observe: vi.fn(async () => [{
    id: "email",
    label: "Email address",
    kind: "email",
    confidence: 0.96,
    bounds: { x: 10, y: 20, width: 200, height: 40 },
  }, {
    id: "guess",
    label: "Unknown",
    kind: "text",
    confidence: 0.4,
    bounds: { x: 10, y: 80, width: 200, height: 40 },
  }]),
};

describe("visual form observation", () => {
  it("is disabled unless the explicit feature flag is set", async () => {
    expect(visualObservationEnabled({})).toBe(false);
    expect(visualObservationEnabled({ BLUEY_JOBS_VISUAL_OBSERVATION_ENABLED: "1" })).toBe(true);
    const observer = new VisualFormObserver(provider, { enabled: false });
    await expect(observer.observe(page(), "fill")).resolves.toEqual([]);
  });

  it("filters low-confidence observations and requires a unique DOM binding", async () => {
    const observer = new VisualFormObserver(provider, { enabled: true, minimumConfidence: 0.9 });
    const observations = await observer.observe(page(), "validate");
    expect(observations.map((item) => item.id)).toEqual(["email"]);
    expect(observer.bindToDom(observations, [{
      selector: "input[name=email]",
      label: "Email address",
      kind: "email",
    }])).toEqual([{ observation: observations[0], selector: "input[name=email]" }]);
    expect(observer.bindToDom(observations, [{
      selector: "input[name=email]",
      label: "Email address",
      kind: "email",
    }, {
      selector: "input[name=backup_email]",
      label: "Email address",
      kind: "email",
    }])).toEqual([]);
  });

  it("does not expose a submit phase or click primitive", () => {
    const observer = new VisualFormObserver(provider, { enabled: true });
    expect("submit" in observer).toBe(false);
    expect("click" in observer).toBe(false);
  });

  it("calls a private parser with a redacted page URL and bounded screenshot", async () => {
    const fetcher = vi.fn(async (_input: URL | RequestInfo, init?: RequestInit) => new Response(JSON.stringify({
      schema_version: "bluey.visual-observation.v1",
      observations: [{
        id: "control-1",
        label: "Legal name",
        kind: "text",
        confidence: 0.97,
        bounds: { x: 12, y: 24, width: 240, height: 44 },
      }],
    }), { status: 200, headers: { "content-type": "application/json" } }));
    const parser = new PrivateVisualParser({
      endpoint: "http://127.0.0.1:7861/v1/observe",
      fetch: fetcher as typeof fetch,
    });

    await expect(parser.observe({
      phase: "fill",
      screenshot: new Uint8Array([1, 2, 3]),
      url: "https://jobs.example.test/apply?token=secret#step-2",
    })).resolves.toHaveLength(1);

    const init = fetcher.mock.calls[0][1];
    const body = JSON.parse(String(init?.body));
    expect(body.page_url).toBe("https://jobs.example.test/apply");
    expect(body.screenshot.base64).toBe("AQID");
    expect(body.phase).toBe("fill");
  });

  it("requires authenticated HTTPS outside loopback", () => {
    expect(() => new PrivateVisualParser({ endpoint: "http://parser.internal/v1/observe" }))
      .toThrow(/HTTPS/);
    expect(() => new PrivateVisualParser({ endpoint: "https://parser.example.test/v1/observe" }))
      .toThrow(/authentication/);
    expect(() => new PrivateVisualParser({
      endpoint: "https://parser.example.test/v1/observe",
      authorization: "Bearer scoped-token",
    })).not.toThrow();
  });

  it("rejects malformed, oversized, and over-broad parser responses", async () => {
    const malformed = new PrivateVisualParser({
      endpoint: "http://localhost:7861/v1/observe",
      fetch: vi.fn(async () => new Response(JSON.stringify({
        schema_version: "bluey.visual-observation.v1",
        observations: [{ id: "submit", label: "Submit", kind: "button", confidence: 1, bounds: {} }],
      }))) as typeof fetch,
    });
    await expect(malformed.observe({
      phase: "validate",
      screenshot: new Uint8Array([1]),
      url: "https://jobs.example.test/apply",
    })).rejects.toThrow(/malformed control/);

    const oversized = new PrivateVisualParser({
      endpoint: "http://localhost:7861/v1/observe",
      maxScreenshotBytes: 2,
    });
    await expect(oversized.observe({
      phase: "prepare",
      screenshot: new Uint8Array([1, 2, 3]),
      url: "https://jobs.example.test/apply",
    })).rejects.toThrow(/screenshot/);
  });
});
