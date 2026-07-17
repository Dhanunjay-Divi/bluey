import type { Paragraph } from "docx";
import type { jsPDF } from "jspdf";
import type { ResumeContent } from "../../types";

export async function exportResumeDocx(content: ResumeContent, filename: string): Promise<void> {
  const { Document, HeadingLevel, Packer, Paragraph, TextRun } = await import("docx");
  const children: Paragraph[] = [
    new Paragraph({
      heading: HeadingLevel.TITLE,
      children: [new TextRun({ text: content.contact?.name || "Resume", bold: true })],
    }),
    new Paragraph([content.contact?.email, content.contact?.phone, content.contact?.location].filter(Boolean).join(" | ")),
    new Paragraph({ heading: HeadingLevel.HEADING_1, text: content.headline || "Professional Summary" }),
    new Paragraph(content.summary || ""),
    new Paragraph({ heading: HeadingLevel.HEADING_1, text: "Skills" }),
    new Paragraph((content.skills || []).join(" | ")),
  ];
  for (const role of content.employment || []) {
    children.push(
      new Paragraph({ heading: HeadingLevel.HEADING_2, text: `${role.title} - ${role.company}` }),
      new Paragraph(`${role.start_date} - ${role.current ? "Present" : role.end_date}`),
      ...role.highlights.map((highlight) => new Paragraph({ text: highlight, bullet: { level: 0 } })),
    );
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
  y += 22;
  pdf.setFont("helvetica", "normal");
  pdf.setFontSize(9);
  pdf.text([content.contact?.email, content.contact?.phone, content.contact?.location].filter(Boolean).join(" | "), margin, y);
  y += 30;
  y = pdfSection(pdf, "SUMMARY", content.summary || "", margin, y);
  y = pdfSection(pdf, "SKILLS", (content.skills || []).join(" | "), margin, y);
  for (const role of content.employment || []) {
    y = pdfSection(pdf, `${role.title} - ${role.company}`, role.highlights.join("\n"), margin, y);
  }
  pdf.save(`${filename}.pdf`);
}

function pdfSection(pdf: jsPDF, title: string, body: string, margin: number, y: number): number {
  if (y > 700) {
    pdf.addPage();
    y = 58;
  }
  pdf.setFont("helvetica", "bold");
  pdf.setFontSize(10);
  pdf.text(title, margin, y);
  y += 16;
  pdf.setFont("helvetica", "normal");
  pdf.setFontSize(9);
  const lines = pdf.splitTextToSize(body, 500) as string[];
  pdf.text(lines, margin, y);
  return y + lines.length * 12 + 20;
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
