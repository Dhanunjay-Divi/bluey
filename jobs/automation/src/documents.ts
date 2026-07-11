import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { PDFDocument, StandardFonts, rgb, type PDFFont, type PDFPage } from "pdf-lib";
import type { ApplicationPacket } from "./contracts.js";

export interface MaterializedDocuments {
  packet: ApplicationPacket;
  resume: { path: string; sha256: string };
  coverLetter?: { path: string; sha256: string };
}

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
  let coverLetterPath = packet.coverLetterPath;
  if (!coverLetterPath && packet.coverLetterContent?.trim()) {
    coverLetterPath = join(directory, "cover-letter.pdf");
    await writeLetterPdf(packet.coverLetterContent, coverLetterPath);
  }
  return {
    packet: { ...packet, resumePath, coverLetterPath },
    resume: { path: resumePath, sha256: await sha256File(resumePath) },
    coverLetter: coverLetterPath ? { path: coverLetterPath, sha256: await sha256File(coverLetterPath) } : undefined,
  };
}

async function writeResumePdf(content: Record<string, unknown>, path: string): Promise<void> {
  const document = await PDFDocument.create();
  const regular = await document.embedFont(StandardFonts.Helvetica);
  const bold = await document.embedFont(StandardFonts.HelveticaBold);
  const writer = new PdfWriter(document, regular, bold);
  const contact = object(content.contact);
  writer.heading(text(contact.name) || "Resume", 19);
  writer.line([
    text(contact.email), text(contact.phone), text(contact.location), text(contact.linkedin_url), text(contact.portfolio_url),
  ].filter(Boolean).join(" | "), 9);
  writer.section("SUMMARY", text(content.summary));
  writer.section("SKILLS", strings(content.skills).join(" | "));
  for (const role of objects(content.employment)) {
    writer.section(
      [text(role.title), text(role.company)].filter(Boolean).join(" - ").toUpperCase(),
      [
        [text(role.location), dateRange(role)].filter(Boolean).join(" | "),
        ...strings(role.highlights).map((value) => `• ${value}`),
      ].filter(Boolean).join("\n"),
    );
  }
  for (const school of objects(content.education)) {
    writer.section(
      [text(school.degree), text(school.field)].filter(Boolean).join(" in ").toUpperCase(),
      [text(school.school), text(school.location), dateRange(school)].filter(Boolean).join(" | "),
    );
  }
  for (const project of objects(content.projects)) {
    writer.section(text(project.name).toUpperCase(), [text(project.role), text(project.summary), strings(project.technologies).join(" | ")].filter(Boolean).join("\n"));
  }
  writer.section("CERTIFICATIONS", strings(content.certifications).join(" | "));
  await writeFile(path, await document.save(), { mode: 0o600 });
}

async function writeLetterPdf(letter: string, path: string): Promise<void> {
  const document = await PDFDocument.create();
  const regular = await document.embedFont(StandardFonts.Helvetica);
  const bold = await document.embedFont(StandardFonts.HelveticaBold);
  const writer = new PdfWriter(document, regular, bold);
  writer.heading("Cover Letter", 17);
  writer.line(letter, 11);
  await writeFile(path, await document.save(), { mode: 0o600 });
}

class PdfWriter {
  private page!: PDFPage;
  private y = 0;
  private readonly margin = 48;
  private readonly width = 516;

  constructor(
    private readonly document: PDFDocument,
    private readonly regular: PDFFont,
    private readonly bold: PDFFont,
  ) {
    this.newPage();
  }

  heading(value: string, size: number): void {
    if (!value) return;
    this.ensure(size + 10);
    this.page.drawText(clean(value), { x: this.margin, y: this.y, size, font: this.bold, color: rgb(0.04, 0.08, 0.1) });
    this.y -= size + 8;
  }

  section(title: string, value: string): void {
    if (!title || !value) return;
    this.ensure(42);
    this.page.drawText(clean(title), { x: this.margin, y: this.y, size: 9, font: this.bold, color: rgb(0.08, 0.35, 0.48) });
    this.y -= 14;
    this.line(value, 10);
    this.y -= 7;
  }

  line(value: string, size: number): void {
    for (const paragraph of clean(value).split("\n")) {
      const lines = wrap(paragraph, this.regular, size, this.width);
      for (const line of lines.length ? lines : [""]) {
        this.ensure(size + 5);
        this.page.drawText(line, { x: this.margin, y: this.y, size, font: this.regular, color: rgb(0.12, 0.15, 0.17) });
        this.y -= size + 4;
      }
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

function wrap(value: string, font: PDFFont, size: number, width: number): string[] {
  const words = value.split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current ? `${current} ${word}` : word;
    if (font.widthOfTextAtSize(candidate, size) <= width) current = candidate;
    else {
      if (current) lines.push(current);
      current = word;
    }
  }
  if (current) lines.push(current);
  return lines;
}

function object(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function objects(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value) ? value.map(object) : [];
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.map(text).filter(Boolean) : [];
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function dateRange(value: Record<string, unknown>): string {
  return [text(value.start_date), text(value.end_date) || (value.current ? "Present" : "")].filter(Boolean).join(" - ");
}

function clean(value: string): string {
  return value.normalize("NFKD").replace(/[^\x20-\x7E\n]/g, "").replace(/[ \t]+/g, " ").trim();
}

function safeName(value: string): string {
  return value.replace(/[^A-Za-z0-9_-]+/g, "-").slice(0, 80);
}

async function sha256File(path: string): Promise<string> {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}
