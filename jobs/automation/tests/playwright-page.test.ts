import { createHash, webcrypto } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BrowserContext, Locator, Page } from "playwright";
import type {
  CertifiedFinalSubmitAdapter,
  FormFileEvidence,
} from "../src/contracts.js";
import { FORM_FILE_READBACK_LIMITS } from "../src/form-readback.js";
import { PlaywrightBrowserPage } from "../src/playwright-page.js";

interface LocatorState {
  count: number;
  value?: string;
  selected?: string[];
  checked?: boolean;
  fileEvidence?: FormFileEvidence[];
  submitTarget?: {
    actionUrl: string;
    method: string;
    enctype: string;
    formTarget: string;
    formIdentity: string;
  };
  retainFill?: boolean;
  retainSelection?: boolean;
  retainChecked?: boolean;
}

function locatorFor(state: LocatorState): Locator {
  return {
    count: async () => state.count,
    fill: async (value: string) => {
      if (state.count !== 1) throw new Error("strict mode violation");
      if (state.retainFill !== false) state.value = value;
    },
    inputValue: async () => state.value || "",
    click: async () => {
      if (state.count !== 1) throw new Error("strict mode violation");
    },
    textContent: async () => null,
    getAttribute: async () => null,
    isVisible: async () => true,
    selectOption: async (value: string) => {
      if (state.count !== 1) throw new Error("strict mode violation");
      if (state.retainSelection !== false) state.selected = [value];
      return state.selected || [];
    },
    setChecked: async (checked: boolean) => {
      if (state.count !== 1) throw new Error("strict mode violation");
      if (state.retainChecked !== false) state.checked = checked;
    },
    isChecked: async () => state.checked || false,
    setInputFiles: async () => undefined,
    evaluateAll: async () => state.fileEvidence || [],
    evaluate: async () => state.submitTarget,
  } as unknown as Locator;
}

function browserPage(state: LocatorState): PlaywrightBrowserPage {
  const locator = locatorFor(state);
  const page = {
    locator: () => locator,
  } as unknown as Page;
  return new PlaywrightBrowserPage(page);
}

describe("PlaywrightBrowserPage locator safety", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("preserves the full locator so ambiguous controls remain detectable", async () => {
    const locator = browserPage({ count: 2 }).locator("button[type=submit]");

    await expect(locator.count()).resolves.toBe(2);
    await expect(locator.click()).rejects.toThrow("strict mode violation");
  });

  it("verifies that text values survive browser writeback", async () => {
    const accepted = browserPage({ count: 1 }).locator("input[name=email]");
    await expect(accepted.fill("candidate@example.com")).resolves.toBeUndefined();

    const rejected = browserPage({ count: 1, retainFill: false })
      .locator("input[name=email]");
    await expect(rejected.fill("candidate@example.com"))
      .rejects.toThrow("did not retain the requested field value");
  });

  it("verifies select and checkbox writeback", async () => {
    const select = browserPage({ count: 1 }).locator("select[name=authorization]");
    await expect(select.selectOption("authorized")).resolves.toBeUndefined();

    const checkbox = browserPage({ count: 1 }).locator("input[name=consent]");
    await expect(checkbox.setChecked(true)).resolves.toBeUndefined();

    const rejected = browserPage({ count: 1, retainChecked: false })
      .locator("input[name=consent]");
    await expect(rejected.setChecked(true))
      .rejects.toThrow("did not retain the requested checked state");
  });

  it("verifies the exact content-addressed bytes attached to file inputs", async () => {
    const directory = await mkdtemp(join(tmpdir(), "bluey-playwright-page-"));
    const resumeBytes = Buffer.alloc(1_024, 0x61);
    const coverLetterBytes = Buffer.alloc(512, 0x62);
    const resumeSha = createHash("sha256").update(resumeBytes).digest("hex");
    const coverLetterSha = createHash("sha256").update(coverLetterBytes).digest("hex");
    const resumeName = `resume-${resumeSha}.pdf`;
    const coverLetterName = `cover-letter-${coverLetterSha}.pdf`;
    const resumePath = join(directory, resumeName);
    const coverLetterPath = join(directory, coverLetterName);
    const forgedPath = join(directory, `resume-${"c".repeat(64)}.pdf`);
    try {
      await writeFile(resumePath, resumeBytes);
      await writeFile(coverLetterPath, coverLetterBytes);
      await writeFile(forgedPath, resumeBytes);
      const accepted = browserPage({ count: 1 }).locator("input[type=file]");
      await expect(accepted.setInputFiles([
        resumePath,
        coverLetterPath,
      ])).resolves.toEqual([
        { name: resumeName, byteLength: 1_024, sha256: resumeSha },
        { name: coverLetterName, byteLength: 512, sha256: coverLetterSha },
      ]);

      const rejected = browserPage({ count: 1 }).locator("input[type=file]");
      await expect(rejected.setInputFiles([forgedPath]))
        .rejects.toThrow("could not verify");
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  it("hashes exact selected DOM File bytes into lowercase readback evidence", async () => {
    const bytes = new TextEncoder().encode("exact selected resume bytes");
    const page = browserControlsPage([domFile("resume.pdf", bytes)]);

    const controls = await page.controls();

    expect(controls[0]?.files).toEqual([{
      name: "resume.pdf",
      byteLength: bytes.byteLength,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    }]);
  });

  it.each([
    ["too many files", Array.from(
      { length: FORM_FILE_READBACK_LIMITS.maxFileCount + 1 },
      (_, index) => domFile(`resume-${index}.pdf`, new Uint8Array([index])),
    )],
    ["oversized file", [domFile(
      "resume.pdf",
      new Uint8Array([1]),
      FORM_FILE_READBACK_LIMITS.maxFileBytes + 1,
    )]],
  ] as const)("rejects %s before reading any file bytes", async (_name, files) => {
    const page = browserControlsPage([...files]);

    await expect(page.controls()).rejects.toThrow("could not verify");

    expect(files.every((file) => file.arrayBufferReads === 0)).toBe(true);
  });

  it("returns a bounded effective provider submit target identity", async () => {
    const actionUrl = "https://jobs.lever.co/acme/11111111-1111-4111-8111-111111111111/apply";
    const page = await certifiedBrowserPage("lever", actionUrl, {
      target: {
        actionUrl,
        method: "post",
        enctype: "multipart/form-data",
        formTarget: "_self",
        formIdentity: "[0,\"application-form\"]",
      },
      parts: [{ kind: "field", source: "visible", fieldName: "email", value: "a@b.test" }],
    });
    const target = page.locator("button[type=submit]");

    await expect(target.effectiveSubmitTarget("lever")).resolves.toEqual({
      actionUrl,
      method: "post",
      enctype: "multipart/form-data",
      formTarget: "_self",
      formIdentity: "[0,\"application-form\"]",
      providerJobKey: "lever:jobs.lever.co:acme:11111111-1111-4111-8111-111111111111",
    });
  });

  it("uses isolated-world values instead of a page-realm FormData monkeypatch", async () => {
    const providerJobId = "11111111-1111-4111-8111-111111111111";
    const actionUrl = `https://jobs.lever.co/acme/${providerJobId}/apply`;
    const page = await certifiedBrowserPage("lever", actionUrl, {
      target: submitTarget(actionUrl),
      parts: [
        { kind: "field", source: "visible", fieldName: "email", value: "ada@example.com" },
        { kind: "field", source: "hidden", fieldName: "postingId", value: providerJobId },
      ],
    }, locatorFor({ count: 1, submitTarget: {
      ...submitTarget(actionUrl),
      actionUrl: "https://attacker.invalid/submit",
    } }));

    const evidence = await page.locator("#submit").successfulSubmitEvidence(
      [{ fieldName: "email", value: "ada@example.com" }],
      `lever:jobs.lever.co:acme:${providerJobId}`,
    );

    expect(evidence).toEqual({
      fields: [
        fieldDigest("email", "ada@example.com"),
        fieldDigest("postingId", providerJobId),
      ],
      partOrder: [{ kind: "field", index: 0 }, { kind: "field", index: 1 }],
    });
  });

  it.each([
    ["duplicate trusted field", [
      ["email", "ada@example.com"],
      ["email", "mallory@example.com"],
    ]],
    ["unapproved visible field", [
      ["email", "ada@example.com"],
      ["tracking_answer", "injected"],
    ]],
  ] as const)("rejects a %s in the FormData entry list", async (_name, entries) => {
    const providerJobId = "11111111-1111-4111-8111-111111111111";
    const actionUrl = `https://jobs.lever.co/acme/${providerJobId}/apply`;
    const page = await certifiedBrowserPage("lever", actionUrl, {
      target: submitTarget(actionUrl),
      parts: entries.map(([fieldName, value]) => ({
        kind: "field" as const,
        source: "visible" as const,
        fieldName,
        value,
      })),
    });

    await expect(page.locator("#submit").successfulSubmitEvidence(
      [{ fieldName: "email", value: "ada@example.com" }],
      `lever:jobs.lever.co:acme:${providerJobId}`,
    )).rejects.toThrow("could not identify");
  });

  it.each([
    ["an unknown hidden field", "transfer_funds", "1"],
    ["a hidden provider job mismatch", "postingId", "22222222-2222-4222-8222-222222222222"],
  ])("rejects %s from the isolated certified form", async (_label, fieldName, value) => {
    const providerJobId = "11111111-1111-4111-8111-111111111111";
    const actionUrl = `https://jobs.lever.co/acme/${providerJobId}/apply`;
    const page = await certifiedBrowserPage("lever", actionUrl, {
      target: submitTarget(actionUrl),
      parts: [
        { kind: "field", source: "visible", fieldName: "email", value: "ada@example.com" },
        { kind: "field", source: "hidden", fieldName, value },
      ],
    });

    await expect(page.locator("#submit").successfulSubmitEvidence(
      [{ fieldName: "email", value: "ada@example.com" }],
      `lever:jobs.lever.co:acme:${providerJobId}`,
    )).rejects.toThrow();
  });
});

function fieldDigest(fieldName: string, value: string) {
  const bytes = Buffer.from(value, "utf8");
  return {
    fieldName,
    valueByteLength: bytes.byteLength,
    valueSha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

function submitTarget(actionUrl: string) {
  return {
    actionUrl,
    method: "post",
    enctype: "multipart/form-data",
    formTarget: "_self",
    formIdentity: "[0,\"application-form\"]",
  };
}

async function certifiedBrowserPage(
  adapter: CertifiedFinalSubmitAdapter,
  approvedUrl: string,
  snapshot: {
    target: ReturnType<typeof submitTarget>;
    parts: Array<Record<string, unknown>>;
  },
  locator: Locator = locatorFor({ count: 1 }),
): Promise<PlaywrightBrowserPage> {
  const context = {
    on: () => context,
    route: async () => undefined,
    routeWebSocket: async () => undefined,
    serviceWorkers: () => [],
    newCDPSession: async () => ({
      send: async (method: string) => {
        if (method === "Page.getFrameTree") return { frameTree: { frame: { id: "main" } } };
        if (method === "Page.createIsolatedWorld") return { executionContextId: 1 };
        if (method === "Runtime.evaluate") {
          return { result: { type: "object", value: snapshot } };
        }
        throw new Error(`unexpected CDP method: ${method}`);
      },
      detach: async () => undefined,
    }),
  };
  const page = {
    context: () => context as unknown as BrowserContext,
    locator: () => locator,
    url: () => approvedUrl,
    once: () => page,
    evaluate: async () => false,
  } as unknown as Page;
  const browserPage = new PlaywrightBrowserPage(page);
  await browserPage.installExactSubmitGuard(adapter, approvedUrl);
  return browserPage;
}

interface DomFileFixture {
  name: string;
  size: number;
  arrayBufferReads: number;
  arrayBuffer(): Promise<ArrayBuffer>;
}

function domFile(name: string, bytes: Uint8Array, reportedSize = bytes.byteLength): DomFileFixture {
  return {
    name,
    size: reportedSize,
    arrayBufferReads: 0,
    async arrayBuffer() {
      this.arrayBufferReads += 1;
      return bytes.slice().buffer;
    },
  };
}

function browserControlsPage(files: DomFileFixture[]): PlaywrightBrowserPage {
  class TestInputElement {
    disabled = false;
    dataset: Record<string, string> = {};
    id = "resume";
    type = "file";
    required = true;
    value = files.map((file) => file.name).join(", ");
    checked = false;
    files = files;

    getAttribute(name: string): string | null {
      if (name === "name") return "resume";
      return null;
    }

    closest(): null {
      return null;
    }
  }
  class TestTextAreaElement {}
  class TestSelectElement {}
  const control = new TestInputElement();
  vi.stubGlobal("HTMLInputElement", TestInputElement);
  vi.stubGlobal("HTMLTextAreaElement", TestTextAreaElement);
  vi.stubGlobal("HTMLSelectElement", TestSelectElement);
  vi.stubGlobal("CSS", { escape: (value: string) => value });
  vi.stubGlobal("crypto", webcrypto);
  vi.stubGlobal("document", {
    querySelectorAll: () => [control],
    querySelector: () => null,
    getElementById: () => null,
  });
  const page = {
    evaluate: async (callback: (limits: typeof FORM_FILE_READBACK_LIMITS) => unknown, limits: typeof FORM_FILE_READBACK_LIMITS) => (
      callback(limits)
    ),
  } as unknown as Page;
  return new PlaywrightBrowserPage(page);
}
