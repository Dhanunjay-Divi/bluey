import type { Paragraph } from "docx";
import type { jsPDF } from "jspdf";
import type { ResumeContent } from "../../types";

export interface ResumeExportBlock {
  heading: string;
  metadata: string;
  body: string[];
  bullets: string[];
}

export interface ResumeExportSection {
  title: string;
  blocks: ResumeExportBlock[];
}

export function buildResumeExportSections(content: ResumeContent): ResumeExportSection[] {
  const sections: ResumeExportSection[] = [];
  const employment = (content.employment || []).map((role) => ({
    heading: [role.title, role.company].filter(Boolean).join(" - "),
    metadata: [role.location, dateRange(role.start_date, role.end_date, role.current)].filter(Boolean).join(" | "),
    body: [],
    bullets: role.highlights.filter(Boolean),
  }));
  if (employment.length) sections.push({ title: "EXPERIENCE", blocks: employment });

  const projects = (content.projects || []).map((project) => ({
    heading: [project.name, project.role].filter(Boolean).join(" - "),
    metadata: project.url || "",
    body: [project.summary, project.technologies.length ? `Technologies: ${project.technologies.join(", ")}` : ""].filter(Boolean),
    bullets: [],
  }));
  if (projects.length) sections.push({ title: "PROJECTS", blocks: projects });

  const education = (content.education || []).map((entry) => ({
    heading: educationHeading(entry.degree, entry.field),
    metadata: [entry.school, entry.location, dateRange(entry.start_date, entry.end_date, false)].filter(Boolean).join(" | "),
    body: [],
    bullets: [],
  }));
  if (education.length) sections.push({ title: "EDUCATION", blocks: education });

  const certifications = (content.certifications || []).filter(Boolean);
  if (certifications.length) {
    sections.push({
      title: "CERTIFICATIONS",
      blocks: [{ heading: "", metadata: "", body: certifications, bullets: [] }],
    });
  }
  return sections;
}

export async function exportResumeDocx(content: ResumeContent, filename: string): Promise<void> {
  const { Document, HeadingLevel, Packer, Paragraph, TextRun } = await import("docx");
  const contact = contactLines(content);
  const children: Paragraph[] = [
    new Paragraph({
      heading: HeadingLevel.TITLE,
      children: [new TextRun({ text: content.contact?.name || "Resume", bold: true })],
    }),
    ...contact.map((line) => new Paragraph(line)),
  ];

  if (content.headline) children.push(new Paragraph({ children: [new TextRun({ text: content.headline, bold: true })] }));
  if (content.summary) {
    children.push(
      new Paragraph({ heading: HeadingLevel.HEADING_1, text: "PROFESSIONAL SUMMARY" }),
      new Paragraph(content.summary),
    );
  }
  const skills = (content.skills || []).filter(Boolean);
  if (skills.length) {
    children.push(
      new Paragraph({ heading: HeadingLevel.HEADING_1, text: "SKILLS" }),
      new Paragraph(skills.join(" | ")),
    );
  }

  for (const section of buildResumeExportSections(content)) {
    children.push(new Paragraph({ heading: HeadingLevel.HEADING_1, text: section.title }));
    for (const block of section.blocks) {
      if (block.heading) children.push(new Paragraph({ heading: HeadingLevel.HEADING_2, text: block.heading }));
      if (block.metadata) children.push(new Paragraph({ children: [new TextRun({ text: block.metadata, italics: true })] }));
      children.push(...block.body.map((line) => new Paragraph(line)));
      children.push(...block.bullets.map((line) => new Paragraph({ text: line, bullet: { level: 0 } })));
    }
  }

  const blob = await Packer.toBlob(new Document({ sections: [{ children }] }));
  downloadBlob(blob, `${filename}.docx`);
}

export async function exportResumePdf(content: ResumeContent, filename: string): Promise<void> {
  const { jsPDF } = await import("jspdf");
  const pdf = new jsPDF({ unit: "pt", format: "letter" });
  const margin = 54;
  let y = 58;
  pdf.setFont("helvetica", "bold");
  pdf.setFontSize(20);
  pdf.text(content.contact?.name || "Resume", margin, y);
  y += 18;
  for (const line of contactLines(content)) {
    y = pdfWrappedText(pdf, line, margin, y, { fontSize: 9 });
  }
  if (content.headline) {
    y += 3;
    y = pdfWrappedText(pdf, content.headline, margin, y, { bold: true, fontSize: 10 });
  }
  y += 8;
  if (content.summary) y = pdfSimpleSection(pdf, "PROFESSIONAL SUMMARY", content.summary, margin, y);
  const skills = (content.skills || []).filter(Boolean);
  if (skills.length) y = pdfSimpleSection(pdf, "SKILLS", skills.join(" | "), margin, y);

  for (const section of buildResumeExportSections(content)) {
    y = pdfStructuredSection(pdf, section, margin, y);
  }
  pdf.save(`${filename}.pdf`);
}

function pdfSimpleSection(pdf: jsPDF, title: string, body: string, margin: number, y: number): number {
  y = pdfSectionHeading(pdf, title, margin, y);
  y = pdfWrappedText(pdf, body, margin, y, { fontSize: 9 });
  return y + 10;
}

function pdfStructuredSection(pdf: jsPDF, section: ResumeExportSection, margin: number, y: number): number {
  y = pdfSectionHeading(pdf, section.title, margin, y);
  for (const block of section.blocks) {
    if (block.heading) y = pdfWrappedText(pdf, block.heading, margin, y, { bold: true, fontSize: 9 });
    if (block.metadata) y = pdfWrappedText(pdf, block.metadata, margin, y, { fontSize: 8 });
    for (const line of block.body) y = pdfWrappedText(pdf, line, margin, y, { fontSize: 9 });
    for (const bullet of block.bullets) y = pdfWrappedText(pdf, `- ${bullet}`, margin + 10, y, { fontSize: 9, width: 490 });
    y += 6;
  }
  return y + 4;
}

function pdfSectionHeading(pdf: jsPDF, title: string, margin: number, y: number): number {
  y = ensurePdfSpace(pdf, y, 28);
  pdf.setFont("helvetica", "bold");
  pdf.setFontSize(10);
  pdf.text(title, margin, y);
  return y + 16;
}

function pdfWrappedText(
  pdf: jsPDF,
  text: string,
  x: number,
  y: number,
  options: { bold?: boolean; fontSize: number; width?: number },
): number {
  if (!text) return y;
  pdf.setFont("helvetica", options.bold ? "bold" : "normal");
  pdf.setFontSize(options.fontSize);
  const lines = pdf.splitTextToSize(text, options.width ?? 500) as string[];
  for (const line of lines) {
    y = ensurePdfSpace(pdf, y, 14);
    pdf.text(line, x, y);
    y += 12;
  }
  return y;
}

function ensurePdfSpace(pdf: jsPDF, y: number, needed: number): number {
  if (y + needed <= 738) return y;
  pdf.addPage();
  return 58;
}

function contactLines(content: ResumeContent): string[] {
  const primary = [content.contact?.email, content.contact?.phone, content.contact?.location].filter(Boolean).join(" | ");
  const links = [content.contact?.linkedin_url, content.contact?.portfolio_url].filter(Boolean).join(" | ");
  return [primary, links].filter(Boolean);
}

function dateRange(start: string, end: string, current: boolean): string {
  return [start, current ? "Present" : end].filter(Boolean).join(" - ");
}

function educationHeading(degree: string, field: string): string {
  const cleanDegree = degree.trim();
  const cleanField = field.trim();
  if (!cleanDegree) return cleanField || "Education";
  if (!cleanField || cleanDegree.toLowerCase().includes(cleanField.toLowerCase())) return cleanDegree;
  return `${cleanDegree}, ${cleanField}`;
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
