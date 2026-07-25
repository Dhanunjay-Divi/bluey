export interface ImportedResume {
  name: string;
  text: string;
  file_type?: "pdf" | "docx" | "txt";
  page_count?: number;
}

export const MAX_RESUME_BYTES = 10 * 1024 * 1024;
export const MAX_RESUME_PDF_PAGES = 20;
export const MAX_RESUME_TEXT_CHARACTERS = 200_000;
const MIN_RESUME_TEXT_CHARACTERS = 40;

interface PdfTextItem {
  str: string;
  transform?: number[];
  width?: number;
  hasEOL?: boolean;
}

export async function importResume(file: File): Promise<ImportedResume> {
  const extension = file.name.split(".").pop()?.toLowerCase();
  if (file.size > MAX_RESUME_BYTES) {
    throw new Error("That resume is larger than 10 MB. Choose a smaller PDF, DOCX, or TXT file.");
  }
  const buffer = await file.arrayBuffer();
  validateResumeFileBytes(file.name, buffer);
  if (extension === "pdf") {
    const [pdfjs, worker] = await Promise.all([
      import("pdfjs-dist"),
      import("pdfjs-dist/build/pdf.worker.min.mjs?url"),
    ]);
    pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
    const pdf = await pdfjs.getDocument({ data: buffer }).promise;
    if (pdf.numPages > MAX_RESUME_PDF_PAGES) {
      throw new Error(`That PDF has ${pdf.numPages} pages. Bluey supports resumes up to ${MAX_RESUME_PDF_PAGES} pages.`);
    }
    const pages: string[] = [];
    for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
      const page = await pdf.getPage(pageNumber);
      const content = await page.getTextContent();
      pages.push(pdfTextItemsToText(content.items as PdfTextItem[]));
      if (pages.reduce((total, pageText) => total + pageText.length, 0) > MAX_RESUME_TEXT_CHARACTERS) {
        throw new Error("That resume contains too much text. Choose a shorter resume and try again.");
      }
    }
    return {
      name: file.name,
      text: validateExtractedResumeText(pages.join("\n\n"), "PDF"),
      file_type: "pdf",
      page_count: pdf.numPages,
    };
  }
  if (extension === "docx") {
    const { default: mammoth } = await import("mammoth");
    const [result, raw] = await Promise.all([
      mammoth.convertToHtml({ arrayBuffer: buffer }),
      mammoth.extractRawText({ arrayBuffer: buffer }),
    ]);
    const htmlText = resumeHtmlToText(result.value);
    return {
      name: file.name,
      text: validateExtractedResumeText(mergeDocxTextSources(htmlText, raw.value), "DOCX"),
      file_type: "docx",
    };
  }
  if (extension === "txt") {
    return {
      name: file.name,
      text: validateExtractedResumeText(new TextDecoder().decode(buffer), "TXT"),
      file_type: "txt",
    };
  }
  throw new Error("Use a PDF, DOCX, or TXT resume.");
}

export function mergeDocxTextSources(htmlText: string, rawText: string): string {
  const htmlLines = normalizedTextLines(htmlText);
  const rawLines = normalizedTextLines(rawText);
  const sectionIndex = rawLines.findIndex((line) => /^(?:(?:professional|executive|career|work|project|relevant|core|key|personal|academic)\s+)?(?:summary|profile|objective|experience|employment|background|education|skills|expertise|proficiencies|certifications?|licenses?|projects?)\b/i.test(line));
  const preamble = rawLines.slice(0, sectionIndex >= 0 ? sectionIndex : Math.min(rawLines.length, 12));
  const known = new Set(htmlLines.slice(0, 16).map((line) => line.toLowerCase()));
  const missing = preamble.filter((line) => !known.has(line.toLowerCase()));
  return [...missing, ...htmlLines].join("\n");
}

export function validateResumeFileBytes(name: string, buffer: ArrayBuffer): void {
  if (buffer.byteLength > MAX_RESUME_BYTES) {
    throw new Error("That resume is larger than 10 MB. Choose a smaller PDF, DOCX, or TXT file.");
  }
  const extension = name.split(".").pop()?.toLowerCase();
  const bytes = new Uint8Array(buffer);
  if (extension === "pdf" && !startsWithAscii(bytes, "%PDF-")) {
    throw new Error("That file is not a valid PDF. Choose the original PDF instead of a renamed file.");
  }
  if (
    extension === "docx" &&
    !(bytes[0] === 0x50 && bytes[1] === 0x4b && [0x03, 0x05, 0x07].includes(bytes[2]))
  ) {
    throw new Error("That file is not a valid DOCX. Choose the original Word document instead of a renamed file.");
  }
  if (!["pdf", "docx", "txt"].includes(extension || "")) {
    throw new Error("Use a PDF, DOCX, or TXT resume.");
  }
}

export function validateExtractedResumeText(text: string, label = "resume"): string {
  const normalized = text.trim();
  if (normalized.length > MAX_RESUME_TEXT_CHARACTERS) {
    throw new Error("That resume contains too much text. Choose a shorter resume and try again.");
  }
  if (normalized.replace(/\s/g, "").length < MIN_RESUME_TEXT_CHARACTERS) {
    const scanned = label.toLowerCase() === "pdf" ? " It may be an image-only or scanned PDF." : "";
    throw new Error(`Bluey could not find readable resume text.${scanned} Export it with selectable text and try again.`);
  }
  return normalized;
}

export function resumeHtmlToText(html: string): string {
  const withStructure = html
    .replace(/<thead\b[^>]*>[\s\S]*?<\/thead>/gi, (header) =>
      /@|https?:\/\/|linkedin\.com|\+?\d[\d\s().-]{8,}/i.test(header) ? header : "",
    )
    .replace(/<br\s*\/?\s*>/gi, "\n")
    .replace(/<li\b[^>]*>/gi, "\n• ")
    .replace(/<\/(?:p|li|td|th|tr|table|ul|ol|h[1-6])>/gi, "\n")
    .replace(/<[^>]+>/g, "");
  return decodeHtmlEntities(withStructure)
    .split(/\r?\n/)
    .map((line) => line.replace(/[ \t]+/g, " ").trim())
    .filter(Boolean)
    .join("\n");
}

export function pdfTextItemsToText(items: PdfTextItem[]): string {
  const positioned = items
    .filter((item) => item.str.trim())
    .map((item, index) => ({
      text: item.str.trim(),
      x: item.transform?.[4] ?? index * 10,
      y: item.transform?.[5] ?? 0,
      width: item.width ?? item.str.length * 5,
      index,
    }));
  if (!positioned.some((item) => item.y !== 0)) {
    return positioned.map((item) => item.text).join(" ").trim();
  }
  const lines: Array<{ y: number; items: typeof positioned }> = [];
  for (const item of positioned) {
    const line = lines.find((candidate) => Math.abs(candidate.y - item.y) <= 2);
    if (line) line.items.push(item);
    else lines.push({ y: item.y, items: [item] });
  }
  return lines
    .sort((left, right) => right.y - left.y)
    .map((line) => {
      const ordered = line.items.sort((left, right) => left.x - right.x || left.index - right.index);
      let value = "";
      let previousEnd = 0;
      for (const item of ordered) {
        const gap = item.x - previousEnd;
        if (value && gap > 2) value += " ";
        value += item.text;
        previousEnd = Math.max(previousEnd, item.x + item.width);
      }
      return value.trim();
    })
    .filter(Boolean)
    .join("\n");
}

function startsWithAscii(bytes: Uint8Array, value: string): boolean {
  return value.split("").every((character, index) => bytes[index] === character.charCodeAt(0));
}

function decodeHtmlEntities(value: string): string {
  const named: Record<string, string> = {
    amp: "&",
    apos: "'",
    gt: ">",
    lt: "<",
    nbsp: " ",
    quot: '"',
  };
  return value.replace(/&(?:#(\d+)|#x([0-9a-f]+)|([a-z]+));/gi, (entity, decimal, hex, name) => {
    if (decimal) return String.fromCodePoint(Number(decimal));
    if (hex) return String.fromCodePoint(Number.parseInt(hex, 16));
    return named[String(name).toLowerCase()] ?? entity;
  });
}

function normalizedTextLines(value: string): string[] {
  return value
    .replace(/[\u2028\u2029]/g, "\n")
    .split(/\r?\n/)
    .map((line) => line.replace(/[ \t]+/g, " ").trim())
    .filter(Boolean);
}
