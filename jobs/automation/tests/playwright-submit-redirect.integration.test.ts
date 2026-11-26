import { createHash } from "node:crypto";
import { createServer, get, type Server } from "node:http";
import { accessSync, constants, statSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium, type Browser, type BrowserContext, type Page, type Request } from "playwright";
import { describe, expect, it } from "vitest";
import { PlaywrightBrowserPage } from "../src/playwright-page.js";

const JOB = "https://boards.greenhouse.io/acme/jobs/1234567";
const CHROMIUM = chromium.executablePath();
const EMAIL = "synthetic@example.test";
const DOCUMENT = Buffer.from("Synthetic redirect sentinel document; no personal data.");
// Chromium must not introduce a separate favicon request into the network census.
const INLINE_ICON = '<link rel="icon" type="image/gif" href="data:image/gif;base64,'
  + 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7">';

describe("real Chromium exact-submit redirect boundary", () => {
  it.each([301, 302, 303, 307, 308])(
    "prevents HTTP %i redirect requests and bodies reaching loopback",
    async (status) => {
      // Missing Chromium is a failure, not a skipped qualification.
      expect(chromiumAvailable()).toBe(true);
      const root = await mkdtemp(join(tmpdir(), "bluey-redirect-sentinel-"));
      let server: Server | undefined;
      let browser: Browser | undefined;
      let context: BrowserContext | undefined;
      let controlContext: BrowserContext | undefined;
      const received: Array<{ path: string; method: string; bytes: number; document: boolean }> = [];
      let postCount = 0;
      let exactPost = false;
      let redirectEmitted = false;
      let terminalObserved = false;
      let guardRejected = false;
      let cleanupSafe = true;
      try {
        server = createServer((request, response) => {
          const chunks: Buffer[] = [];
          const record = { path: request.url ?? "", method: request.method ?? "", bytes: 0, document: false };
          received.push(record);
          request.on("data", (chunk: Buffer) => { record.bytes += chunk.length; chunks.push(chunk); });
          request.on("error", () => undefined);
          request.on("end", () => {
            record.document = Buffer.concat(chunks).includes(DOCUMENT);
            if (record.path === "/capture") {
              response.writeHead(200, { "content-type": "text/html; charset=utf-8" })
                .end(`<!doctype html><head>${INLINE_ICON}</head><body>Synthetic capture control</body>`);
            } else {
              response.writeHead(200).end("synthetic");
            }
          });
        });
        await new Promise<void>((resolve, reject) => {
          server!.once("error", reject);
          server!.listen(0, "127.0.0.1", () => {
            server!.off("error", reject);
            resolve();
          });
        });
        const address = server.address();
        if (!address || typeof address === "string") throw new Error("No loopback listener");
        const capture = `http://127.0.0.1:${address.port}/capture`;
        await new Promise<void>((resolve, reject) => {
          const probe = get(`http://127.0.0.1:${address.port}/probe`, (response) => {
            response.resume();
            response.on("end", () => response.statusCode === 200 ? resolve() : reject(new Error("Probe failed")));
          });
          probe.setTimeout(5_000, () => probe.destroy(new Error("Probe timed out")));
          probe.on("error", reject);
        });
        expect(received).toHaveLength(1);
        expect(received[0]?.path).toBe("/probe");
        received.length = 0;
        const hash = createHash("sha256").update(DOCUMENT).digest("hex");
        const path = join(root, `resume-${hash}.pdf`);
        await writeFile(path, DOCUMENT, { mode: 0o400 });
        browser = await chromium.launch({ executablePath: CHROMIUM, headless: true });
        const configure = async (selected: BrowserContext, guarded: boolean) => {
          await selected.route("**/*", async (route) => {
          const request = route.request();
          if (request.url() === JOB && request.method() === "GET") {
            return route.fulfill({
              status: 200, contentType: "text/html",
              body: `<!doctype html><head>${INLINE_ICON}</head>
                <form id="application_form" action="${JOB}" method="post"
                enctype="multipart/form-data" data-greenhouse-job-id="1234567">
                <input type="hidden" name="job_id" value="1234567">
                <label for="email">Email</label><input id="email" name="email" type="email">
                <label for="resume">Resume</label><input id="resume" name="resume" type="file">
                <button id="submit" type="submit">Submit application</button></form>`,
            });
          }
          if (request.url() === JOB && request.method() === "POST") {
            if (guarded) postCount += 1;
            const body = request.postDataBuffer();
            if (guarded) {
              exactPost = body !== null && body.includes(DOCUMENT) && body.includes(Buffer.from(EMAIL));
              redirectEmitted = true;
            }
            return route.fulfill({ status, headers: { location: capture }, body: "" });
          }
          // The independently observed owned listener must be reachable if the
          // guard leaks; this fixture handler must not itself block that request.
          if (request.url() === capture) return route.fallback();
          return route.abort("blockedbyclient");
          });
        };
        // Prove the identical browser and response can actually reach the
        // listener before interpreting any guarded zero-request observation.
        controlContext = await browser.newContext({ serviceWorkers: "block" });
        await configure(controlContext, false);
        const control = await controlContext.newPage();
        await control.goto(JOB, { waitUntil: "domcontentloaded", timeout: 10_000 });
        await control.locator("#email").fill(EMAIL);
        await control.locator("#resume").setInputFiles(path);
        await Promise.all([
          control.waitForURL(capture, { waitUntil: "load", timeout: 10_000 }),
          control.locator("#submit").click({ timeout: 10_000 }),
        ]);
        expect(received).toHaveLength(1);
        expect(received[0]?.path).toBe("/capture");
        expect(received[0]?.method).toBe(status === 307 || status === 308 ? "POST" : "GET");
        if (status === 307 || status === 308) {
          expect(received[0]?.document).toBe(true);
          expect(received[0]!.bytes).toBeGreaterThan(DOCUMENT.length);
        }
        await controlContext.close();
        controlContext = undefined;
        received.length = 0;

        context = await browser.newContext({ serviceWorkers: "block" });
        await configure(context, true);
        const page = await context.newPage();
        await page.goto(JOB, { waitUntil: "domcontentloaded", timeout: 10_000 });
        const guarded = new PlaywrightBrowserPage(page);
        await guarded.installExactSubmitGuard("greenhouse", JOB);
        await guarded.beginExactSubmitGuard();
        await guarded.locator("#email").fill(EMAIL);
        const files = await guarded.locator("#resume").setInputFiles([path]);
        const submit = guarded.locator("#submit");
        const target = await submit.effectiveSubmitTarget("greenhouse");
        const fields = await submit.successfulSubmitEvidence(
          [{ fieldName: "email", value: EMAIL }], target.providerJobKey,
        );
        if (files.length !== 1 || !files[0]) throw new Error("Synthetic file was not selected");
        const terminal = observeRedirectTerminal(page, capture);
        try {
          await submit.clickWithExactSubmit({
            target, fields: fields.fields, partOrder: fields.partOrder,
            files: [{ fieldName: "resume", ...files[0] }],
          });
        } catch (error) {
          // A blocked redirect can reject activation; it is not the zero-network oracle.
          void error;
        }
        terminalObserved = await terminal;
        guardRejected = await guarded.assertExactSubmitGuardClean().then(() => false, () => true);
      } finally {
        try {
          try {
            await context?.close();
            await controlContext?.close();
          } catch (error) {
            cleanupSafe = false;
            throw error;
          }
        } finally {
          try {
            try {
              await browser?.close();
            } catch (error) {
              cleanupSafe = false;
              throw error;
            }
          } finally {
            try {
              if (server?.listening) {
                await new Promise<void>((resolve, reject) => {
                  server!.close((error) => error ? reject(error) : resolve());
                  server!.closeAllConnections();
                });
              }
            } catch (error) {
              cleanupSafe = false;
              throw error;
            } finally {
              if (cleanupSafe) await rm(root, { recursive: true, force: true });
            }
          }
        }
      }
      // These assertions prevent a setup error or a never-activated submit from
      // masquerading as a successful zero-network result.
      expect(postCount).toBe(1);
      expect(exactPost).toBe(true);
      expect(redirectEmitted).toBe(true);
      expect(terminalObserved).toBe(true);
      expect(guardRejected).toBe(true);
      expect(received).toEqual([]);
    },
    60_000,
  );
});

function observeRedirectTerminal(page: Page, capture: string): Promise<boolean> {
  return new Promise((resolve) => {
    let timer: ReturnType<typeof setTimeout>;
    const finish = (result: boolean) => {
      clearTimeout(timer);
      page.off("requestfailed", failed);
      page.off("requestfinished", finished);
      page.off("framenavigated", navigated);
      resolve(result);
    };
    const isRedirect = (request: Request) =>
      request.url() === capture && request.redirectedFrom()?.url() === JOB;
    const failed = (request: Request) => {
      if (isRedirect(request) || (request.url() === JOB && request.method() === "POST")) finish(true);
    };
    const finished = (request: Request) => { if (isRedirect(request)) finish(true); };
    const navigated = () => { if (page.url() === capture) finish(true); };
    page.on("requestfailed", failed);
    page.on("requestfinished", finished);
    page.on("framenavigated", navigated);
    timer = setTimeout(() => finish(false), 8_000);
  });
}

function chromiumAvailable(): boolean {
  try {
    accessSync(CHROMIUM, constants.X_OK);
    return statSync(CHROMIUM).isFile();
  } catch {
    return false;
  }
}
