import { accessSync, constants, statSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium, type Browser, type BrowserContext, type Page } from "playwright";
import { describe, expect, it } from "vitest";
import { executeApplication } from "../src/execute.js";
import type { AdapterContext } from "../src/contracts.js";
import { PlaywrightBrowserPage } from "../src/playwright-page.js";

const URL = "https://careers.example.test/jobs/123/apply";
const EXECUTABLE = chromium.executablePath();
const HAS_CHROMIUM = executableExists(EXECUTABLE);

describe("approved-packet filling in intercepted Chromium", () => {
  it.skipIf(!HAS_CHROMIUM)("fills two DOM steps and stops before final submit", async () => {
    await withFixture(`
      <label for="first">First name</label><input id="first" name="first_name" required>
      <label for="email">Email address</label><input id="email" name="email" type="email" required>
      <label for="resume">CV</label><input id="resume" name="resume" type="file" required>
      <label for="photo">Photo</label><input id="photo" name="resume_photo" type="file">
      <button type="button" id="next">Next</button>
      <script>
        document.querySelector('#next').onclick = () => {
          window.nextClicks += 1;
          window.firstStep = {
            first: document.querySelector('#first').value,
            email: document.querySelector('#email').value,
            resume: Array.from(document.querySelector('#resume').files, f => f.name),
            photoCount: document.querySelector('#photo').files.length
          };
          document.querySelector('main').innerHTML = \`
            <label for="country">Country</label><select id="country" name="country" required>
              <option value="">Select</option><option value="us">United States</option>
              <option value="ca">Canada</option></select>
            <label for="updates">Email updates</label><input id="updates" name="updates" type="checkbox" checked>
            <button type="submit">Submit application</button>\`;
        };
      </script>`, async ({ page, context, requests, resumeName }) => {
      const hooks: string[] = [];
      const result = await executeApplication({
        ...context,
        packet: {
          ...context.packet,
          answers: { first_name: "Ada", email: "ada@example.test", country: "United States", "Email updates": "no" },
        },
        beforeFinalSubmit: async () => { hooks.push("before"); },
      });
      expect(await page.evaluate("window.firstStep")).toEqual({
        first: "Ada", email: "ada@example.test", resume: [resumeName], photoCount: 0,
      });
      expect(await page.locator("#country").inputValue()).toBe("us");
      expect(await page.locator("#updates").isChecked()).toBe(false);
      expect(await page.evaluate("window.nextClicks")).toBe(1);
      expect(result.receipt.issues).toEqual([]);
      expect(result.receipt.intervention?.kind).toBe("browser_takeover");
      expect(result.receipt.status).toBe("needs_input");
      expect(hooks).toEqual([]);
      expect(requests).toEqual([`GET ${URL}`]);
    });
  }, 30_000);

  const blocked = [
    { name: "manager email", html: '<label for="target">Manager email</label><input id="target" name="email" value="existing@example.test" required>', answers: { email: "ada@example.test" } },
    { name: "unsupported date", html: '<label for="target">Start date</label><input id="target" type="date" value="2027-01-01" required>', answers: {} },
    { name: "unsupported number", html: '<label for="target">Years</label><input id="target" type="number" value="9" required>', answers: {} },
    { name: "ambiguous boolean", html: '<label for="target">Consent</label><input id="target" name="consent" type="checkbox" checked required>', answers: { consent: "maybe" } },
    { name: "duplicate options", html: '<label for="target">Choice</label><select id="target" name="choice" required><option value="a" selected>Yes</option><option value="b">YES</option></select>', answers: { choice: "yes" } },
    { name: "empty question", html: '<input id="target" value="existing" required>', answers: { first_name: "Ada" } },
    { name: "negated radio", html: '<label for="target">Authorization Not authorized</label><input id="target" type="radio" name="authorization" value="not authorized" checked required><label for="other">Authorization Other</label><input id="other" type="radio" name="authorization" value="other" required>', answers: { authorization: "authorized" } },
    { name: "unknown document", html: '<label for="target">Supporting document</label><input id="target" name="resume" type="file" required>', answers: {}, prefillFile: true },
  ];
  for (const fixture of blocked) {
    it.skipIf(!HAS_CHROMIUM)(`blocks ${fixture.name} without changing the prefill or advancing`, async () => {
      await withFixture(`${fixture.html}<button type="button" onclick="window.nextClicks++">Next</button>`,
        async ({ page, context, requests }) => {
          if (fixture.prefillFile) await page.locator("#target").setInputFiles(context.packet.resumePath!);
          const before = await controlSnapshot(page);
          await page.evaluate("window.writes = 0");
          const hooks: string[] = [];
          const result = await executeApplication({
            ...context,
            packet: { ...context.packet, answers: fixture.answers },
            beforeFinalSubmit: async () => { hooks.push("before"); },
          });
          expect(await controlSnapshot(page)).toEqual(before);
          expect(await page.evaluate("window.writes")).toBe(0);
          expect(await page.evaluate("window.nextClicks")).toBe(0);
          expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
          expect(result.receipt.status).toBe("needs_input");
          expect(hooks).toEqual([]);
          expect(requests).toEqual([`GET ${URL}`]);
        });
    }, 30_000);
  }
});

async function controlSnapshot(page: Page) {
  return page.locator("#target").evaluate((element) => {
    const input = element as HTMLInputElement;
    return {
      value: input.value,
      checked: input.checked,
      files: input.files ? Array.from(input.files, (file) => file.name) : undefined,
    };
  });
}

async function withFixture(
  html: string,
  run: (fixture: {
    page: Page;
    context: AdapterContext;
    requests: string[];
    resumeName: string;
  }) => Promise<void>,
): Promise<void> {
  const root = await mkdtemp(join(tmpdir(), "bluey-standard-browser-"));
  let browser: Browser | undefined;
  let browserContext: BrowserContext | undefined;
  try {
    const bytes = Buffer.from("Synthetic document bytes: not a real candidate resume");
    const sha = createHash("sha256").update(bytes).digest("hex");
    const resumeName = `resume-${sha}.pdf`;
    const resumePath = join(root, resumeName);
    await writeFile(resumePath, bytes, { mode: 0o400 });
    browser = await chromium.launch({ executablePath: EXECUTABLE, headless: true });
    browserContext = await browser.newContext({ serviceWorkers: "block" });
    const requests: string[] = [];
    await browserContext.route("**/*", async (route) => {
      const request = route.request();
      requests.push(`${request.method()} ${request.url()}`);
      if (request.url() === URL && request.method() === "GET" && requests.length === 1) {
        await route.fulfill({
          status: 200,
          contentType: "text/html; charset=utf-8",
          body: `<!doctype html><title>Engineer | Fixture</title><script>
            window.nextClicks=0; window.writes=0;
            document.addEventListener('input',()=>window.writes++);
            document.addEventListener('change',()=>window.writes++);
          </script><main>${html}</main>`,
        });
      } else await route.abort("blockedbyclient");
    });
    const page = await browserContext.newPage();
    await page.goto(URL, { waitUntil: "domcontentloaded" });
    const context: AdapterContext = {
      runner: "local", runId: "fixture-run", accountId: "fixture-account",
      approvedCanonicalUrl: URL, page: new PlaywrightBrowserPage(page),
      packet: {
        applicationId: "fixture-application", jobId: "fixture-job", resumeVersionId: "fixture-resume",
        approvedPacketChecksum: "c".repeat(64), resumePath,
        answers: {}, verifiedClaimIds: [], applicationIdentityId: "fixture-identity",
        applicationEmail: "ada@example.test", browserProfileId: "fixture-profile",
      },
      log: async () => undefined,
    };
    await run({ page, context, requests, resumeName });
  } finally {
    try {
      await browserContext?.close();
    } finally {
      await browser?.close();
    }
    await rm(root, { recursive: true, force: true });
  }
}

function executableExists(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return statSync(path).isFile();
  } catch { return false; }
}
