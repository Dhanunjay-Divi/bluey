import * as fontkitModule from "@pdf-lib/fontkit";
import { createHash } from "node:crypto";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { PDFDocument, rgb, type PDFFont, type PDFPage } from "pdf-lib";
import type { ApplicationPacket } from "./contracts.js";

export interface MaterializedDocuments {
  packet: ApplicationPacket;
  resume: { path: string; sha256: string };
  coverLetter?: { path: string; sha256: string };
}

interface CandidateContact {
  name: string;
  details: string[];
}

interface DocumentSection {
  title: string;
  body: string;
  titleIsContent?: boolean;
}

interface FontFace {
  font: PDFFont;
  codePoints: ReadonlySet<number>;
}

interface TextRun {
  face: FontFace;
  value: string;
}

const FONT_ASSETS = [
  { file: "NotoSans-Regular.ttf" },
  { file: "NotoSansSC-Regular.ttf" },
  { file: "NotoSansKR-Regular.ttf" },
] as const;
// Node ESM exposes this CommonJS package as `default`; Vitest exposes its named API.
const fontkit = (fontkitModule as unknown as { default?: typeof fontkitModule }).default ?? fontkitModule;
const PDF_TIMESTAMP = new Date("2000-01-01T00:00:00.000Z");
const MAX_PDF_BYTES = 12 * 1024 * 1024;
const MAX_PDF_PAGES = 12;
const MAX_FIELD_CHARS = 20_000;
const MAX_DOCUMENT_CHARS = 120_000;
const MAX_DOCUMENT_LINES = 3_000;
const MAX_COLLECTION_ITEMS = 200;
const MAX_DOCUMENT_SECTIONS = 256;
const MIN_EXTRACTABLE_TEXT_CHARS = 16;
const RTL_OR_BIDI_CONTROL = /[\u0590-\u08FF\u200C-\u200F\u202A-\u202E\u2066-\u2069\uFB1D-\uFDFF\uFE70-\uFEFF\u{10800}-\u{10FFF}\u{1E800}-\u{1EEFF}]/u;
const CJK_CHARACTER = /[\u2E80-\u30FF\u31F0-\u31FF\u3400-\u4DBF\u4E00-\u9FFF\uF900-\uFAFF\uFF00-\uFFEF\u{20000}-\u{323AF}]/u;
const KOREAN_CHARACTER = /[\u1100-\u11FF\u3130-\u318F\uA960-\uA97F\uAC00-\uD7AF\uD7B0-\uD7FF]/u;
const graphemeSegmenter = new Intl.Segmenter("en", { granularity: "grapheme" });
let fontAssetBytesPromise: Promise<Uint8Array[]> | undefined;

export async function materializeApplicationDocuments(
  packet: ApplicationPacket,
  directory: string,
): Promise<MaterializedDocuments> {
  await mkdir(directory, { recursive: true });
  const resumePath = packet.resumePath || join(directory, `resume-${safeName(packet.resumeVersionId)}.pdf`);
  if (!packet.resumePath) {
    if (!packet.resumeContent) throw new Error("This application has no tailored resume content");
    await writeResumePdf(packet.resumeContent, resumePath);
  }
  const resume = await inspectPdf(resumePath, "Resume");

  let coverLetterPath = packet.coverLetterPath;
  if (!coverLetterPath && packet.coverLetterContent !== undefined) {
    const letter = normalizeText(packet.coverLetterContent, "Cover letter");
    if (!letter) throw new Error("Cover letter content is blank");
    const contact = coverLetterContact(packet);
    coverLetterPath = join(directory, "cover-letter.pdf");
    await writeLetterPdf(letter, contact, coverLetterPath);
  }

  return {
    packet: { ...packet, resumePath, coverLetterPath },
    resume,
    coverLetter: coverLetterPath ? await inspectPdf(coverLetterPath, "Cover letter") : undefined,
  };
}

async function writeResumePdf(content: Record<string, unknown>, path: string): Promise<void> {
  const contact = requiredContact(content);
  const sections = resumeSections(content);
  if (!sections.length) throw new Error("Resume content is blank beyond the required contact information");
  const values = [contact.name, ...contact.details, ...sections.flatMap((section) => [section.title, section.body])];
  assertDocumentBudget(values, sections.length);

  const { document, writer } = await createPdfWriter("Resume", values);
  writer.heading(contact.name, 19);
  writer.line(contact.details.join(" | "), 9);
  for (const section of sections) writer.section(section.title, section.body, section.titleIsContent);
  await savePdf(document, path, "Resume");
}

async function writeLetterPdf(letter: string, contact: CandidateContact, path: string): Promise<void> {
  const values = [contact.name, ...contact.details, letter];
  assertDocumentBudget(values, 1);
  const { document, writer } = await createPdfWriter("Cover Letter", values);
  writer.heading(contact.name || "Cover Letter", 17);
  writer.line(contact.details.join(" | "), 9);
  writer.section("COVER LETTER", letter);
  await savePdf(document, path, "Cover letter");
}

async function createPdfWriter(title: string, values: string[]): Promise<{ document: PDFDocument; writer: PdfWriter }> {
  const document = await PDFDocument.create({ updateMetadata: false });
  document.registerFontkit(fontkit);
  document.setTitle(title);
  document.setCreator("Bluey Jobs");
  document.setProducer("Bluey Jobs");
  document.setCreationDate(PDF_TIMESTAMP);
  document.setModificationDate(PDF_TIMESTAMP);

  const assets = await loadFontAssets();
  const requiredIndexes = requiredFontIndexes(values.join("\n"));
  const faces = await Promise.all(requiredIndexes.map(async (index): Promise<FontFace> => {
    const font = await document.embedFont(assets[index], { subset: true });
    return { font, codePoints: new Set(font.getCharacterSet()) };
  }));
  return { document, writer: new PdfWriter(document, faces) };
}

function requiredFontIndexes(value: string): number[] {
  const indexes = [0];
  if (CJK_CHARACTER.test(value)) indexes.push(1);
  if (KOREAN_CHARACTER.test(value)) indexes.push(2);
  return indexes;
}

class PdfWriter {
  private page!: PDFPage;
  private y = 0;
  private readonly margin = 48;
  private readonly width = 516;

  constructor(
    private readonly document: PDFDocument,
    private readonly faces: FontFace[],
  ) {
    this.newPage();
  }

  heading(value: string, size: number): void {
    const normalized = normalizeText(value, "Document heading");
    if (!normalized) return;
    for (const line of wrap(normalized, this.faces, size, this.width)) {
      this.ensure(size + 8);
      this.draw(line, size, rgb(0.04, 0.08, 0.1));
      this.y -= size + 4;
    }
    this.y -= 4;
  }

  section(title: string, value: string, titleIsContent = false): void {
    const normalizedTitle = normalizeText(title, "Section title");
    const normalizedValue = normalizeText(value, "Section content");
    if (!normalizedValue && !titleIsContent) return;
    if (!normalizedTitle && !normalizedValue) return;

    this.ensure(42);
    for (const line of wrap(normalizedTitle, this.faces, 9, this.width)) {
      this.ensure(14);
      this.draw(line, 9, rgb(0.08, 0.35, 0.48));
      this.y -= 13;
    }
    if (normalizedValue) this.line(normalizedValue, 10);
    this.y -= 7;
  }

  line(value: string, size: number): void {
    const normalized = normalizeText(value, "Document text");
    for (const paragraph of normalized.split("\n")) {
      const lines = wrap(paragraph, this.faces, size, this.width);
      for (const line of lines.length ? lines : [""]) {
        this.ensure(size + 5);
        if (line) this.draw(line, size, rgb(0.12, 0.15, 0.17));
        this.y -= size + 4;
      }
    }
  }

  private draw(value: string, size: number, color: ReturnType<typeof rgb>): void {
    let x = this.margin;
    for (const run of textRuns(value, this.faces)) {
      this.page.drawText(run.value, { x, y: this.y, size, font: run.face.font, color });
      x += run.face.font.widthOfTextAtSize(run.value, size);
    }
  }

  private ensure(height: number): void {
    if (this.y - height < this.margin) this.newPage();
  }

  private newPage(): void {
    this.page = this.document.addPage([612, 792]);
    this.y = 744;
  }
}

function wrap(value: string, faces: FontFace[], size: number, width: number): string[] {
  if (!value) return [];
  const lines: string[] = [];
  let current = "";
  for (const token of value.split(/( +)/u).filter(Boolean)) {
    if (/^ +$/u.test(token)) {
      if (current && !current.endsWith(" ")) current += " ";
      continue;
    }

    const candidate = `${current}${token}`;
    if (measure(candidate, faces, size) <= width) {
      current = candidate;
      continue;
    }
    if (current.trim()) lines.push(current.trimEnd());
    current = "";

    if (measure(token, faces, size) <= width) {
      current = token;
      continue;
    }
    for (const grapheme of graphemes(token)) {
      const fragment = `${current}${grapheme}`;
      if (current && measure(fragment, faces, size) > width) {
        lines.push(current);
        current = grapheme;
      } else {
        current = fragment;
      }
    }
  }
  if (current.trim()) lines.push(current.trimEnd());
  return lines;
}

function measure(value: string, faces: FontFace[], size: number): number {
  return textRuns(value, faces).reduce(
    (total, run) => total + run.face.font.widthOfTextAtSize(run.value, size),
    0,
  );
}

function textRuns(value: string, faces: FontFace[]): TextRun[] {
  const runs: TextRun[] = [];
  let preferred: FontFace | undefined;
  for (const grapheme of graphemes(value)) {
    const codePoints = [...grapheme].map((character) => character.codePointAt(0)!);
    const face = preferred && supports(preferred, codePoints)
      ? preferred
      : faces.find((candidate) => supports(candidate, codePoints));
    if (!face) {
      const codes = codePoints.map(formatCodePoint).join(" ");
      throw new Error(`Document contains unsupported glyph ${codes}: ${JSON.stringify(grapheme)}`);
    }
    const previous = runs.at(-1);
    if (previous?.face === face) previous.value += grapheme;
    else runs.push({ face, value: grapheme });
    preferred = face;
  }
  return runs;
}

function supports(face: FontFace, codePoints: number[]): boolean {
  return codePoints.every((codePoint) => face.codePoints.has(codePoint));
}

function graphemes(value: string): string[] {
  return [...graphemeSegmenter.segment(value)].map((part) => part.segment);
}

function formatCodePoint(codePoint: number): string {
  return `U+${codePoint.toString(16).toUpperCase().padStart(4, "0")}`;
}

function requiredContact(content: Record<string, unknown> | undefined): CandidateContact {
  const contact = object(content?.contact);
  const name = text(contact.name);
  if (!name) throw new Error("Resume contact name is required for generated documents");
  const channels = [text(contact.email), text(contact.phone), text(contact.linkedin_url), text(contact.portfolio_url)]
    .filter(Boolean);
  if (!channels.length) throw new Error("Resume contact information is required for generated documents");
  const details = [
    text(contact.email),
    text(contact.phone),
    text(contact.location),
    text(contact.linkedin_url),
    text(contact.portfolio_url),
  ].filter(Boolean);
  return { name, details };
}

function coverLetterContact(packet: ApplicationPacket): CandidateContact {
  if (packet.resumeContent) return requiredContact(packet.resumeContent);
  const email = normalizeText(packet.applicationEmail, "Application email");
  if (!email) {
    throw new Error("Application email is required to generate a cover letter for an existing resume");
  }
  return { name: "", details: [email] };
}

function resumeSections(content: Record<string, unknown>): DocumentSection[] {
  const sections: DocumentSection[] = [];
  addSection(sections, "SUMMARY", text(content.summary));
  addSection(sections, "SKILLS", strings(content.skills).join(" | "));

  for (const role of objects(content.employment)) {
    const title = [text(role.title), text(role.company)].filter(Boolean).join(" - ");
    const body = [
      [text(role.location), dateRange(role)].filter(Boolean).join(" | "),
      ...strings(role.highlights).map((value) => `\u2022 ${value}`),
    ].filter(Boolean).join("\n");
    addSection(sections, title || "EMPLOYMENT", body, Boolean(title));
  }
  for (const school of objects(content.education)) {
    const title = [text(school.degree), text(school.field)].filter(Boolean).join(" in ");
    const body = [text(school.school), text(school.location), dateRange(school)].filter(Boolean).join(" | ");
    addSection(sections, title || "EDUCATION", body, Boolean(title));
  }
  for (const project of objects(content.projects)) {
    const title = text(project.name);
    const body = [
      text(project.role),
      text(project.summary),
      strings(project.technologies).join(" | "),
    ].filter(Boolean).join("\n");
    addSection(sections, title || "PROJECT", body, Boolean(title));
  }
  addSection(sections, "CERTIFICATIONS", strings(content.certifications).join(" | "));
  return sections;
}

function addSection(
  sections: DocumentSection[],
  title: string,
  body: string,
  titleIsContent = false,
): void {
  if (body || titleIsContent) sections.push({ title, body, titleIsContent });
}

function object(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function objects(value: unknown): Array<Record<string, unknown>> {
  if (!Array.isArray(value)) return [];
  if (value.length > MAX_COLLECTION_ITEMS) throw new Error("Document contains too many repeated entries");
  return value.map(object);
}

function strings(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  if (value.length > MAX_COLLECTION_ITEMS) throw new Error("Document contains too many repeated entries");
  return value.map(text).filter(Boolean);
}

function text(value: unknown): string {
  return typeof value === "string" ? normalizeText(value, "Document content") : "";
}

function dateRange(value: Record<string, unknown>): string {
  return [text(value.start_date), text(value.end_date) || (value.current === true ? "Present" : "")]
    .filter(Boolean).join(" - ");
}

function normalizeText(value: unknown, label: string): string {
  if (typeof value !== "string") return "";
  const normalized = value.normalize("NFC").replace(/\r\n?|\u2028|\u2029/gu, "\n");
  if (normalized.length > MAX_FIELD_CHARS) throw new Error(`${label} is too long for ATS PDF export`);
  if (RTL_OR_BIDI_CONTROL.test(normalized)) {
    throw new Error(`${label} contains a right-to-left script that ATS PDF export does not yet support safely`);
  }
  for (const character of normalized) {
    if (character !== "\n" && character !== "\t" && /\p{Cc}/u.test(character)) {
      throw new Error(`${label} contains unsupported control character ${formatCodePoint(character.codePointAt(0)!)}`);
    }
  }
  return normalized
    .split("\n")
    .map((line) => line.replace(/[\p{Zs}\t]+/gu, " ").trim())
    .join("\n")
    .trim();
}

function assertDocumentBudget(values: string[], sectionCount: number): void {
  if (sectionCount > MAX_DOCUMENT_SECTIONS) throw new Error("Document contains too many sections");
  const characters = values.reduce((total, value) => total + value.length, 0);
  const lines = values.reduce((total, value) => total + value.split("\n").length, 0);
  if (characters > MAX_DOCUMENT_CHARS || lines > MAX_DOCUMENT_LINES) {
    throw new Error("Document content is too large for ATS PDF export");
  }
}

function safeName(value: string): string {
  return value.replace(/[^A-Za-z0-9_-]+/g, "-").slice(0, 80);
}

async function loadFontAssets(): Promise<Uint8Array[]> {
  fontAssetBytesPromise ??= Promise.all(FONT_ASSETS.map(async ({ file }) => (
    readFile(new URL(`../assets/fonts/${file}`, import.meta.url))
  )));
  return fontAssetBytesPromise;
}

async function savePdf(document: PDFDocument, path: string, label: string): Promise<void> {
  const bytes = await document.save({ useObjectStreams: false, addDefaultPage: false });
  await assertUsablePdf(bytes, label);
  await writeFile(path, bytes, { mode: 0o600 });
}

async function inspectPdf(path: string, label: string): Promise<{ path: string; sha256: string }> {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`${label} path is not a file`);
  if (metadata.size > MAX_PDF_BYTES) throw new Error(`${label} PDF is too large`);
  const bytes = await readFile(path);
  await assertUsablePdf(bytes, label);
  return { path, sha256: sha256(bytes) };
}

async function assertUsablePdf(bytes: Uint8Array, label: string): Promise<void> {
  if (!bytes.length) throw new Error(`${label} PDF is blank`);
  if (bytes.byteLength > MAX_PDF_BYTES) throw new Error(`${label} PDF is too large`);
  if (new TextDecoder("ascii").decode(bytes.subarray(0, 5)) !== "%PDF-") {
    throw new Error(`${label} file is not a PDF`);
  }
  let document: Awaited<ReturnType<typeof getDocument>["promise"]> | undefined;
  try {
    document = await getDocument({
      data: Uint8Array.from(bytes),
      disableFontFace: true,
      enableXfa: false,
      isImageDecoderSupported: false,
      isOffscreenCanvasSupported: false,
      stopAtErrors: true,
      useSystemFonts: false,
      useWasm: false,
    }).promise;
    if (document.numPages < 1) throw new Error(`${label} PDF is blank`);
    if (document.numPages > MAX_PDF_PAGES) throw new Error(`${label} PDF has too many pages`);
    let extracted = "";
    for (let pageNumber = 1; pageNumber <= document.numPages; pageNumber += 1) {
      const content = await (await document.getPage(pageNumber)).getTextContent();
      for (const item of content.items) {
        if ("str" in item && item.str) extracted += `${item.str} `;
        if (extracted.length > MAX_DOCUMENT_CHARS) throw new Error(`${label} PDF text is too large`);
      }
    }
    if (extracted.replace(/\s+/gu, "").length < MIN_EXTRACTABLE_TEXT_CHARS) {
      throw new Error(`${label} PDF does not contain a meaningful extractable text layer`);
    }
  } catch (error) {
    if (error instanceof Error && error.message.startsWith(`${label} PDF`)) throw error;
    throw new Error(`${label} PDF is invalid`);
  } finally {
    await document?.destroy();
  }
}

function sha256(value: Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}
