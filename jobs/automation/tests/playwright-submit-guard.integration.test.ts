import { accessSync, constants, statSync } from "node:fs";
import { Buffer } from "node:buffer";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PDFDocument } from "pdf-lib";
import { chromium, type Browser, type BrowserContext } from "playwright";
import { describe, expect, it } from "vitest";
import { PlaywrightBrowserPage } from "../src/playwright-page.js";
import { AdditionalSubmitRequestError } from "../src/trusted-submit.js";

const GREENHOUSE_JOB_ID = "1234567";
const GREENHOUSE_JOB_URL = `https://boards.greenhouse.io/acme/jobs/${GREENHOUSE_JOB_ID}`;
const APPROVED_EMAIL = "ada@example.com";
const PLAYWRIGHT_CHROMIUM_EXECUTABLE = chromium.executablePath();

// This is the integration test's only skip condition. Do not silently replace
// Playwright's configured Chromium with an unrelated system browser.
const HAS_CONFIGURED_PLAYWRIGHT_CHROMIUM = executableFileExists(
  PLAYWRIGHT_CHROMIUM_EXECUTABLE,
);

describe("Playwright Chromium exact-submit guard integration", () => {
  it.skipIf(!HAS_CONFIGURED_PLAYWRIGHT_CHROMIUM)(
    "allows one exact Greenhouse multipart POST and blocks a delayed beacon",
    async () => {
      const tempRoot = await mkdtemp(join(tmpdir(), "bluey-playwright-submit-guard-"));
      let browser: Browser | undefined;
      let context: BrowserContext | undefined;
      try {
        const pdf = await PDFDocument.create();
        pdf.addPage([612, 792]);
        const pdfBytes = await pdf.save();
        const pdfSha256 = createHash("sha256").update(pdfBytes).digest("hex");
        const pdfName = `resume-${pdfSha256}.pdf`;
        const pdfPath = join(tempRoot, pdfName);
        await writeFile(pdfPath, pdfBytes, { mode: 0o400 });

        browser = await chromium.launch({
          executablePath: PLAYWRIGHT_CHROMIUM_EXECUTABLE,
          headless: true,
        });
        context = await browser.newContext({ serviceWorkers: "block" });

        const fixtureRequests: Array<{ method: string; url: string }> = [];
        const fixturePostRequests: import("playwright").Request[] = [];
        const fixturePostBodies: Buffer[] = [];
        const fixturePostHeaders: Array<Record<string, string>> = [];
        await context.route("**/*", async (route) => {
          const request = route.request();
          fixtureRequests.push({ method: request.method(), url: request.url() });
          if (request.url() !== GREENHOUSE_JOB_URL) {
            await route.abort("blockedbyclient");
            return;
          }
          if (request.method() === "GET") {
            await route.fulfill({
              status: 200,
              contentType: "text/html; charset=utf-8",
              body: greenhouseApplicationHtml(),
            });
            return;
          }
          if (request.method() === "POST") {
            fixturePostRequests.push(request);
            fixturePostBodies.push(request.postDataBuffer() ?? Buffer.alloc(0));
            fixturePostHeaders.push(await request.allHeaders());
            await route.fulfill({
              status: 200,
              contentType: "text/html; charset=utf-8",
              body: "<!doctype html><main id=confirmation>Application received</main>",
            });
            return;
          }
          await route.abort("blockedbyclient");
        });

        const page = await context.newPage();
        const browserPage = new PlaywrightBrowserPage(page);
        // The guard's later catch-all route must validate first, then fall back
        // to the earlier local fixture route above.
        await browserPage.installExactSubmitGuard("greenhouse", GREENHOUSE_JOB_URL);
        await page.goto(GREENHOUSE_JOB_URL, { waitUntil: "domcontentloaded" });

        await browserPage.beginExactSubmitGuard();
        await browserPage.locator("#email").fill(APPROVED_EMAIL);
        const selectedFiles = await browserPage.locator("#resume").setInputFiles([pdfPath]);
        const submit = browserPage.locator("#submit");
        const target = await submit.effectiveSubmitTarget("greenhouse");
        const evidence = await submit.successfulSubmitEvidence(
          [{ fieldName: "email", value: APPROVED_EMAIL }],
          target.providerJobKey,
        );
        const files = selectedFiles.map((file) => ({ fieldName: "resume", ...file }));

        expect(target).toMatchObject({
          actionUrl: GREENHOUSE_JOB_URL,
          method: "post",
          enctype: "multipart/form-data",
          formTarget: "_self",
          providerJobKey: `greenhouse:acme:${GREENHOUSE_JOB_ID}`,
        });
        expect(files).toEqual([{
          fieldName: "resume",
          name: pdfName,
          byteLength: pdfBytes.byteLength,
          sha256: pdfSha256,
        }]);
        expect(evidence.fields.map((field) => field.fieldName)).toEqual(["email", "job_id"]);
        expect(evidence.partOrder).toEqual([
          { kind: "field", index: 0 },
          { kind: "field", index: 1 },
          { kind: "file", index: 0 },
        ]);

        await expect(submit.clickWithExactSubmit({
          target,
          files,
          fields: evidence.fields,
          partOrder: evidence.partOrder,
        })).resolves.toBe(200);

        expect(await page.locator("#confirmation").textContent()).toBe("Application received");
        expect(fixtureRequests.filter(({ method }) => method === "POST")).toHaveLength(1);
        expect(fixturePostBodies).toHaveLength(1);
        expect(fixturePostBodies[0]?.includes(pdfBytes)).toBe(true);
        expect(fixturePostHeaders[0]?.["content-type"]).toContain("multipart/form-data; boundary=");
        const exposedContentLength = fixturePostHeaders[0]?.["content-length"];
        if (exposedContentLength !== undefined) {
          expect(Number(exposedContentLength)).toBe(fixturePostBodies[0]?.byteLength);
        }
        const interceptedSizes = await fixturePostRequests[0]!.sizes();
        expect(fixturePostBodies[0]?.byteLength)
          .toBe(interceptedSizes.requestBodySize + pdfBytes.byteLength);

        const blockedRequest = page.waitForEvent("requestfailed", {
          predicate: (request) => (
            request.method() === "POST" && request.url() === GREENHOUSE_JOB_URL
          ),
          timeout: 5_000,
        });
        await page.evaluate((url) => {
          navigator.sendBeacon(url, new Blob(["delayed mutation"], { type: "text/plain" }));
        }, GREENHOUSE_JOB_URL);
        const failed = await blockedRequest;

        expect(failed.method()).toBe("POST");
        expect(fixtureRequests.filter(({ method }) => method === "POST")).toHaveLength(1);
        await expect(browserPage.assertExactSubmitGuardClean())
          .rejects.toBeInstanceOf(AdditionalSubmitRequestError);
      } finally {
        await context?.close().catch(() => undefined);
        await browser?.close().catch(() => undefined);
        await rm(tempRoot, { recursive: true, force: true });
      }
    },
    30_000,
  );
});

function executableFileExists(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function greenhouseApplicationHtml(): string {
  return `<!doctype html>
<html lang="en">
  <body>
    <script>
      globalThis.FormData = class PageOwnedFormData {
        constructor() { throw new Error("page-owned FormData must not be trusted"); }
      };
      Object.defineProperty(HTMLInputElement.prototype, "name", {
        configurable: true,
        get() { return "page_owned_name"; },
      });
    </script>
    <form
      id="application-form"
      action="${GREENHOUSE_JOB_URL}"
      method="post"
      enctype="multipart/form-data"
      target="_self"
      data-greenhouse-job-id="${GREENHOUSE_JOB_ID}"
    >
      <label>Email <input id="email" name="email" type="email" required></label>
      <input name="job_id" type="hidden" value="${GREENHOUSE_JOB_ID}">
      <label>Resume <input id="resume" name="resume" type="file" accept="application/pdf" required></label>
      <button id="submit" type="submit">Submit application</button>
    </form>
  </body>
</html>`;
}
