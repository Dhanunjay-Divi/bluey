import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { PDFDocument, StandardFonts } from "pdf-lib";
import { afterEach, describe, expect, it } from "vitest";
import type { ApplicationPacket } from "../src/contracts.js";
import { materializeApplicationDocuments } from "../src/index.js";

const directories: string[] = [];

afterEach(async () => {
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

describe("application document materialization", () => {
  it("preserves supported Unicode in extractable text with deterministic bytes", async () => {
    const firstDirectory = await temporaryDirectory();
    const secondDirectory = await temporaryDirectory();
    const value = packet({
      resumeContent: {
        contact: {
          name: "José Álvarez 李美玲",
          email: "jose@example.com",
          location: "Montréal, QC",
        },
        summary: "Builds résumé systems for 東京.",
        skills: ["TypeScript", "Rust", "分布式システム"],
        employment: [{
          title: "소프트웨어 엔지니어",
          company: "서울 데이터",
          location: "東京",
          start_date: "2022",
          end_date: "2026",
          highlights: ["開発チームを率いて信頼性を改善。", "Crème brûlée data was retained exactly."],
        }],
      },
      coverLetterContent: [
        "Dear hiring team,",
        "",
        "I build reliable systems for 東京 and 서울.",
        "",
        "Sincerely,",
        "José Álvarez",
      ].join("\n"),
    });

    const first = await materializeApplicationDocuments(value, firstDirectory);
    const second = await materializeApplicationDocuments(value, secondDirectory);

    expect(first.packet.resumePath).toMatch(/resume-resume-version-1\.pdf$/);
    expect((await readFile(first.resume.path)).subarray(0, 5).toString()).toBe("%PDF-");
    expect(first.resume.sha256).toBe(second.resume.sha256);
    expect(first.coverLetter?.sha256).toBe(second.coverLetter?.sha256);
    expect(first.resume.sha256).toBe("6b26db284724ff0c9fabc920d0c22077207f810216b0ee24785ddd85cc96f3f0");
    expect(first.coverLetter?.sha256).toBe("6ec2ecdc6a8273eb19aefb367b949b384b736af1e05a6f60206a1a71624fda2c");

    const resumeText = await extractPdfText(first.resume.path);
    expect(resumeText).toContain("José Álvarez");
    expect(resumeText).toContain("李美玲");
    expect(resumeText).toContain("東京");
    expect(resumeText).toContain("서울 데이터");
    expect(resumeText).toContain("Crème brûlée data was retained exactly.");

    const coverLetterText = await extractPdfText(first.coverLetter!.path);
    expect(coverLetterText).toContain("José Álvarez");
    expect(coverLetterText).toContain("I build reliable systems for 東京 and 서울.");

    const baseFonts = pdfBaseFonts(await readFile(first.resume.path));
    expect(baseFonts.some((name) => name.includes("NotoSans-Regular"))).toBe(true);
    expect(baseFonts.some((name) => name.includes("NotoSansSC-Regular"))).toBe(true);
    expect(baseFonts.some((name) => name.includes("NotoSansKR-Regular"))).toBe(true);
    expect(baseFonts.every((name) => /^[A-Za-z][A-Za-z0-9_.+-]*$/.test(name))).toBe(true);
  }, 30_000);

  it("fails closed when generated documents are missing a name or contact channel", async () => {
    const directory = await temporaryDirectory();
    await expect(materializeApplicationDocuments(packet({
      resumeContent: {
        contact: { email: "ada@example.com" },
        summary: "Builds reliable systems.",
      },
    }), directory)).rejects.toThrow(/contact name is required/i);

    await expect(materializeApplicationDocuments(packet({
      resumeContent: {
        contact: { name: "Ada Lovelace", location: "London" },
        summary: "Builds reliable systems.",
      },
    }), directory)).rejects.toThrow(/contact information is required/i);
  });

  it("fails closed on blank resume or cover-letter content", async () => {
    const directory = await temporaryDirectory();
    await expect(materializeApplicationDocuments(packet({
      resumeContent: { contact: { name: "Ada Lovelace", email: "ada@example.com" } },
    }), directory)).rejects.toThrow(/resume content is blank/i);

    await expect(materializeApplicationDocuments(packet({ coverLetterContent: " \n\t " }), directory))
      .rejects.toThrow(/cover letter content is blank/i);
  });

  it("fails closed instead of dropping unsupported scripts, glyphs, or controls", async () => {
    const cases = [
      { name: "Ada Lovelace 😀", message: /unsupported glyph U\+1F600/i },
      { name: "Ada\u0000Lovelace", message: /unsupported control character U\+0000/i },
      { name: "أدا لوفلايس", message: /right-to-left script/i },
      { name: "עדה לאבלייס", message: /right-to-left script/i },
      { name: "एडा लवलेस", message: /unsupported glyph/i },
      { name: "เอดา เลิฟเลซ", message: /unsupported glyph/i },
    ];

    for (const value of cases) {
      const directory = await temporaryDirectory();
      await expect(materializeApplicationDocuments(packet({
        resumeContent: {
          contact: { name: value.name, email: "ada@example.com" },
          summary: "Builds reliable systems.",
        },
      }), directory)).rejects.toThrow(value.message);
    }
  }, 20_000);

  it("generates a cover letter for an existing resume using the application identity", async () => {
    const directory = await temporaryDirectory();
    const resumePath = join(directory, "source-resume.pdf");
    await writeTextPdf(resumePath, "Ada Lovelace software engineering experience from 2020 through 2026.");

    const result = await materializeApplicationDocuments(packet({
      resumePath,
      resumeContent: undefined,
      applicationEmail: "applications@example.com",
      coverLetterContent: "Dear hiring team,\n\nI am excited to apply for this role.",
    }), directory);

    expect(result.packet.resumePath).toBe(resumePath);
    expect(await extractPdfText(result.coverLetter!.path)).toContain("applications@example.com");
  }, 20_000);

  it("rejects blank, image-only, invalid, oversized, and over-page-limit PDFs", async () => {
    const directory = await temporaryDirectory();

    const blankPath = join(directory, "blank.pdf");
    await writeFile(blankPath, new Uint8Array());
    await expectReferencedResumeFailure(blankPath, directory, /resume PDF is blank/i);

    const imageOnlyPath = join(directory, "image-only.pdf");
    await writeImageOnlyPdf(imageOnlyPath);
    await expectReferencedResumeFailure(imageOnlyPath, directory, /meaningful extractable text layer/i);

    const invalidPath = join(directory, "invalid.pdf");
    await writeFile(invalidPath, "%PDF-this is not a valid document");
    await expectReferencedResumeFailure(invalidPath, directory, /resume PDF is invalid/i);

    const oversizedPath = join(directory, "oversized.pdf");
    const oversized = Buffer.alloc((12 * 1024 * 1024) + 1);
    oversized.write("%PDF-", 0, "ascii");
    await writeFile(oversizedPath, oversized);
    await expectReferencedResumeFailure(oversizedPath, directory, /resume PDF is too large/i);

    const tooManyPagesPath = join(directory, "too-many-pages.pdf");
    const tooManyPages = await PDFDocument.create();
    for (let page = 0; page < 13; page += 1) tooManyPages.addPage();
    await writeFile(tooManyPagesPath, await tooManyPages.save());
    await expectReferencedResumeFailure(tooManyPagesPath, directory, /resume PDF has too many pages/i);
  }, 30_000);

  it("enforces collection and field budgets before rendering", async () => {
    const directory = await temporaryDirectory();
    await expect(materializeApplicationDocuments(packet({
      resumeContent: {
        contact: { name: "Ada Lovelace", email: "ada@example.com" },
        summary: "x".repeat(20_001),
      },
    }), directory)).rejects.toThrow(/too long/i);

    await expect(materializeApplicationDocuments(packet({
      resumeContent: {
        contact: { name: "Ada Lovelace", email: "ada@example.com" },
        summary: "Builds reliable systems.",
        employment: Array.from({ length: 201 }, () => ({ title: "Engineer", company: "Acme" })),
      },
    }), directory)).rejects.toThrow(/too many repeated entries/i);
  });
});

function packet(overrides: Partial<ApplicationPacket> = {}): ApplicationPacket {
  return {
    applicationId: "application-1",
    jobId: "job-1",
    resumeVersionId: "resume-version-1",
    approvedPacketChecksum: "c".repeat(64),
    resumeContent: {
      contact: { name: "Ada Lovelace", email: "ada@example.com", location: "New York, NY" },
      summary: "Builds reliable systems.",
      skills: ["TypeScript", "Rust"],
      employment: [{ title: "Engineer", company: "Acme", highlights: ["Built an application platform."] }],
    },
    coverLetterContent: "Dear hiring team,\n\nI am excited to apply.",
    answers: {},
    verifiedClaimIds: [],
    ...overrides,
  };
}

async function temporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "bluey-documents-"));
  directories.push(directory);
  return directory;
}

async function extractPdfText(path: string): Promise<string> {
  const pdf = await getDocument({ data: new Uint8Array(await readFile(path)) }).promise;
  try {
    const pages: string[] = [];
    for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
      const content = await (await pdf.getPage(pageNumber)).getTextContent();
      pages.push(content.items.map((item) => "str" in item ? item.str : "").join(" "));
    }
    return pages.join("\n").replace(/\s+/gu, " ").trim();
  } finally {
    await pdf.destroy();
  }
}

async function writeTextPdf(path: string, value: string): Promise<void> {
  const pdf = await PDFDocument.create();
  const font = await pdf.embedFont(StandardFonts.Helvetica);
  pdf.addPage().drawText(value, { font });
  await writeFile(path, await pdf.save());
}

async function writeImageOnlyPdf(path: string): Promise<void> {
  const pdf = await PDFDocument.create();
  const image = await pdf.embedPng(Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
    "base64",
  ));
  pdf.addPage().drawImage(image, { x: 10, y: 10, width: 1, height: 1 });
  await writeFile(path, await pdf.save());
}

async function expectReferencedResumeFailure(path: string, directory: string, message: RegExp): Promise<void> {
  await expect(materializeApplicationDocuments(packet({
    resumePath: path,
    resumeContent: undefined,
    coverLetterContent: undefined,
  }), directory)).rejects.toThrow(message);
}

function pdfBaseFonts(bytes: Uint8Array): string[] {
  return [...Buffer.from(bytes).toString("latin1").matchAll(/\/BaseFont\s+\/([A-Za-z0-9_.+-]+)/g)]
    .map((match) => match[1]);
}
