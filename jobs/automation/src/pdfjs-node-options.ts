import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const PDFJS_LEGACY_BUILD_URL = pathToFileURL(require.resolve("pdfjs-dist/legacy/build/pdf.mjs"));
const PDFJS_STANDARD_FONT_DIRECTORY_URL = new URL("../../standard_fonts/", PDFJS_LEGACY_BUILD_URL);

/**
 * PDF.js' Node binary-data factory passes this value directly to fs.readFile,
 * rather than converting file URLs itself. Keep the required trailing slash
 * after converting the installed package URL to a local filesystem path.
 */
export const PDFJS_STANDARD_FONT_DATA_PATH = pdfjsNodeDirectoryPath(
  fileURLToPath(PDFJS_STANDARD_FONT_DIRECTORY_URL),
);

export function pdfjsNodeDirectoryPath(path: string): string {
  return `${path.replace(/[\\/]+$/u, "")}/`;
}

export function pdfjsNodeDocumentOptions(): {
  readonly standardFontDataUrl: string;
  readonly useWorkerFetch: false;
} {
  return {
    standardFontDataUrl: PDFJS_STANDARD_FONT_DATA_PATH,
    useWorkerFetch: false,
  };
}
