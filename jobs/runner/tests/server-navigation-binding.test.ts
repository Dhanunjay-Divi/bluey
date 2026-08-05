import type { BrowserContext, Page } from "playwright";
import { describe, expect, it, vi } from "vitest";
import type { NormalizedJob } from "@bluey/jobs-automation";
import {
  assertCloudRecoveryNavigation,
  assertCloudRunCertifiedNavigation,
  prepareRecoveredCloudPage,
} from "../src/server.js";

describe("cloud run certified navigation binding", () => {
  it("accepts an exact provider application variant", () => {
    expect(() => assertCloudRunCertifiedNavigation({
      url: "https://job-boards.greenhouse.io/embed/job_app?for=acme&token=123",
      job: job(),
    })).not.toThrow();
  });

  it.each([
    ["same-host cross-job", "https://boards.greenhouse.io/acme/jobs/456", "greenhouse"],
    ["provider mismatch", "https://jobs.lever.co/acme/123/apply", "greenhouse"],
    ["source mismatch", "https://boards.greenhouse.io/acme/jobs/123", "semantic"],
  ] as const)("rejects %s before browser creation", (_label, url, source) => {
    expect(() => assertCloudRunCertifiedNavigation({
      url,
      job: { ...job(), source },
    })).toThrow();
  });

  it.each([
    ["checkpoint URL", "https://boards.greenhouse.io/acme/jobs/456", []],
    ["existing page URL", "https://boards.greenhouse.io/acme/jobs/123", [
      "https://boards.greenhouse.io/acme/jobs/456",
    ]],
  ] as const)("rejects a cross-job recovery %s before page reuse", (
    _label,
    recoveryUrl,
    existingPageUrls,
  ) => {
    expect(() => assertCloudRecoveryNavigation({
      recoveryUrl,
      existingPageUrls,
      job: job(),
    })).toThrow();
  });

  it("keeps the unique exact-job page offline until its guard is installed", async () => {
    const harness = fakeContext([
      "about:blank",
      "https://boards.greenhouse.io/acme/jobs/123",
    ]);
    const installGuard = vi.fn(async (page: Page) => {
      harness.events.push(`guard:${page.url()}`);
    });

    const selected = await prepareRecoveredCloudPage(
      harness.context,
      "https://boards.greenhouse.io/acme/jobs/123",
      job(),
      installGuard,
    );

    expect(selected.url()).toBe("https://boards.greenhouse.io/acme/jobs/123");
    expect(harness.events).toEqual([
      "offline:true",
      "guard:https://boards.greenhouse.io/acme/jobs/123",
      "close:about:blank",
      "offline:false",
    ]);
    expect(harness.context.pages()).toEqual([selected]);
  });

  it("never enables network when a restored service worker remains", async () => {
    const harness = fakeContext(
      ["https://boards.greenhouse.io/acme/jobs/123"],
      [{}],
    );

    await expect(prepareRecoveredCloudPage(
      harness.context,
      "https://boards.greenhouse.io/acme/jobs/123",
      job(),
      async () => undefined,
    )).rejects.toThrow();

    expect(harness.events).toEqual(["offline:true"]);
  });

  it("never enables network for ambiguous duplicate restored job pages", async () => {
    const harness = fakeContext([
      "https://boards.greenhouse.io/acme/jobs/123",
      "https://job-boards.greenhouse.io/acme/jobs/123",
    ]);
    const installGuard = vi.fn(async () => undefined);

    await expect(prepareRecoveredCloudPage(
      harness.context,
      "https://boards.greenhouse.io/acme/jobs/123",
      job(),
      installGuard,
    )).rejects.toThrow();

    expect(installGuard).not.toHaveBeenCalled();
    expect(harness.events).toEqual(["offline:true"]);
  });
});

function fakeContext(
  urls: string[],
  serviceWorkers: object[] = [],
): { context: BrowserContext; events: string[] } {
  const events: string[] = [];
  const pages: Page[] = [];
  for (const initialUrl of urls) {
    let currentUrl = initialUrl;
    const page = {
      url: () => currentUrl,
      goto: vi.fn(async (url: string) => {
        events.push(`goto:${url}`);
        currentUrl = url;
        return null;
      }),
      close: vi.fn(async () => {
        events.push(`close:${currentUrl}`);
        const index = pages.indexOf(page as Page);
        if (index >= 0) pages.splice(index, 1);
      }),
    } as unknown as Page;
    pages.push(page);
  }
  const context = {
    pages: () => [...pages],
    serviceWorkers: () => serviceWorkers,
    setOffline: vi.fn(async (offline: boolean) => {
      events.push(`offline:${offline}`);
    }),
    newPage: vi.fn(async () => {
      let currentUrl = "about:blank";
      const page = {
        url: () => currentUrl,
        goto: vi.fn(async (url: string) => {
          events.push(`goto:${url}`);
          currentUrl = url;
          return null;
        }),
        close: vi.fn(async () => {
          events.push(`close:${currentUrl}`);
          const index = pages.indexOf(page as Page);
          if (index >= 0) pages.splice(index, 1);
        }),
      } as unknown as Page;
      pages.push(page);
      return page;
    }),
  } as unknown as BrowserContext;
  return { context, events };
}

function job(): NormalizedJob {
  return {
    externalId: "123",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build reliable systems.",
    source: "greenhouse",
  };
}
