import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { isAbsolute, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { describe, expect, it } from "vitest";
import {
  PDFJS_STANDARD_FONT_DATA_PATH,
  pdfjsNodeDirectoryPath,
  pdfjsNodeDocumentOptions,
} from "../src/pdfjs-node-options.js";

const require = createRequire(import.meta.url);

describe("PDF.js Node document options", () => {
  it("normalizes native directory separators to PDF.js' required slash suffix", () => {
    expect(pdfjsNodeDirectoryPath("/private/fonts/")).toBe("/private/fonts/");
    expect(pdfjsNodeDirectoryPath("C:\\private\\fonts\\")).toBe("C:\\private\\fonts/");
    expect(pdfjsNodeDirectoryPath("C:\\private\\fonts\\/")).toBe("C:\\private\\fonts/");
  });

  it("uses the installed standard-font directory as a local trailing-slash path", async () => {
    const buildUrl = pathToFileURL(require.resolve("pdfjs-dist/legacy/build/pdf.mjs"));
    const packageDirectory = fileURLToPath(new URL("../../standard_fonts/", buildUrl));
    const packageDirectoryWithoutTrailingSeparator = packageDirectory.replace(/[\\/]+$/u, "");
    const expectedPath = `${packageDirectoryWithoutTrailingSeparator}/`;
    const options = pdfjsNodeDocumentOptions();

    expect(PDFJS_STANDARD_FONT_DATA_PATH).toBe(expectedPath);
    expect(options.standardFontDataUrl).toBe(expectedPath);
    expect(options.standardFontDataUrl.startsWith(packageDirectoryWithoutTrailingSeparator)).toBe(true);
    expect(isAbsolute(options.standardFontDataUrl)).toBe(true);
    expect(options.standardFontDataUrl.endsWith("/")).toBe(true);
    expect(options.standardFontDataUrl.endsWith("\\/")).toBe(false);
    expect(options.standardFontDataUrl).not.toMatch(/^file:/u);
    expect(options.useWorkerFetch).toBe(false);
    await expect(access(join(options.standardFontDataUrl, "LiberationSans-Regular.ttf"))).resolves.toBeUndefined();
  });
});
