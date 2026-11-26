import type { BrowserContext, Page } from "playwright";
import { PlaywrightBrowserPage, type NormalizedJob } from "@bluey/jobs-automation";
import { ProfileRecoveryBlockedError } from "./recovery-isolation.js";

export async function installCertifiedExactSubmitGuard(
  page: PlaywrightBrowserPage,
  job: NormalizedJob,
): Promise<void> {
  if (job.source === "greenhouse" || job.source === "lever") {
    await page.installExactSubmitGuard(job.source, job.canonicalUrl);
  }
}

export async function prepareFreshCloudPage(
  context: BrowserContext,
  job: NormalizedJob,
): Promise<{ page: Page; browserPage: PlaywrightBrowserPage }> {
  await context.setOffline(true);
  const existingPages = context.pages();
  const page = existingPages[0] ?? (await context.newPage());
  await page.goto("about:blank", {
    waitUntil: "domcontentloaded",
    timeout: 10_000,
  });
  for (const candidate of existingPages) {
    if (candidate !== page) await candidate.close();
  }
  const browserPage = new PlaywrightBrowserPage(page);
  await installCertifiedExactSubmitGuard(browserPage, job);
  assertQuiescedCloudContext(context, page);
  await context.setOffline(false);
  return { page, browserPage };
}

export async function wrapActiveCloudPage(
  context: BrowserContext,
  job: NormalizedJob,
): Promise<{ page: Page; browserPage: PlaywrightBrowserPage }> {
  const pages = context.pages();
  if (pages.length !== 1 || context.serviceWorkers().length > 0) {
    throw new ProfileRecoveryBlockedError();
  }
  const page = pages[0]!;
  const browserPage = new PlaywrightBrowserPage(page);
  await installCertifiedExactSubmitGuard(browserPage, job);
  return { page, browserPage };
}

export function assertQuiescedCloudContext(
  context: BrowserContext,
  page: Page,
): void {
  if (
    context.serviceWorkers().length > 0 ||
    context.pages().length !== 1 ||
    context.pages()[0] !== page
  ) {
    throw new ProfileRecoveryBlockedError();
  }
}
