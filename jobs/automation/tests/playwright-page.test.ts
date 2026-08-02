import { describe, expect, it } from "vitest";
import type { Locator, Page } from "playwright";
import { PlaywrightBrowserPage } from "../src/playwright-page.js";

interface LocatorState {
  count: number;
  value?: string;
  selected?: string[];
  checked?: boolean;
  fileNames?: string[];
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
    evaluateAll: async () => state.fileNames || [],
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

  it("verifies the exact document names attached to file inputs", async () => {
    const accepted = browserPage({
      count: 1,
      fileNames: ["resume.pdf", "cover-letter.docx"],
    }).locator("input[type=file]");
    await expect(accepted.setInputFiles([
      "/private/session/resume.pdf",
      "/private/session/cover-letter.docx",
    ])).resolves.toBeUndefined();

    const rejected = browserPage({ count: 1, fileNames: ["different.pdf"] })
      .locator("input[type=file]");
    await expect(rejected.setInputFiles(["/private/session/resume.pdf"]))
      .rejects.toThrow("did not retain the requested application document");
  });
});
