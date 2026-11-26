import { createHash } from "node:crypto";
import { mkdtemp, readFile, rename, rm, truncate, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { EvidenceObjectUpload, MaterializedDocument } from "@bluey/jobs-automation";
import { afterEach, describe, expect, it } from "vitest";
import { evidenceObject } from "../src/execution-result.js";

const directories: string[] = [];

afterEach(async () => {
  await Promise.all(directories.splice(0).map((directory) => rm(directory, {
    recursive: true,
    force: true,
  })));
});

describe("document evidence snapshots", () => {
  it("emits the validated bytes after a same-name replacement", async () => {
    const directory = await temporaryDirectory();
    const path = join(directory, "resume.pdf");
    const replacementPath = join(directory, "replacement.pdf");
    const validatedBytes = Buffer.from("%PDF-validated resume bytes");
    const replacementBytes = Buffer.from("%PDF-unvalidated replacement bytes");
    await writeFile(path, validatedBytes);
    const document = materializedDocument(path, validatedBytes);
    await writeFile(replacementPath, replacementBytes);

    await rename(replacementPath, path);

    expect(await readFile(path)).toEqual(replacementBytes);
    expectEvidence(await evidenceObject(document, "resume", "application/pdf"), validatedBytes);
  });

  it("emits the validated bytes after the same path is truncated", async () => {
    const directory = await temporaryDirectory();
    const path = join(directory, "resume.pdf");
    const validatedBytes = Buffer.from("%PDF-validated resume bytes");
    await writeFile(path, validatedBytes);
    const document = materializedDocument(path, validatedBytes);

    await truncate(path, 5);

    expect((await readFile(path)).byteLength).toBe(5);
    expectEvidence(await evidenceObject(document, "resume", "application/pdf"), validatedBytes);
  });

  it("emits each validated snapshot after resume and cover-letter paths are swapped", async () => {
    const directory = await temporaryDirectory();
    const resumePath = join(directory, "resume.pdf");
    const coverLetterPath = join(directory, "cover-letter.pdf");
    const swapPath = join(directory, "swap.pdf");
    const resumeBytes = Buffer.from("%PDF-validated resume bytes");
    const coverLetterBytes = Buffer.from("%PDF-validated cover-letter bytes");
    await writeFile(resumePath, resumeBytes);
    await writeFile(coverLetterPath, coverLetterBytes);
    const resume = materializedDocument(resumePath, resumeBytes);
    const coverLetter = materializedDocument(coverLetterPath, coverLetterBytes);

    await rename(resumePath, swapPath);
    await rename(coverLetterPath, resumePath);
    await rename(swapPath, coverLetterPath);

    expect(await readFile(resumePath)).toEqual(coverLetterBytes);
    expect(await readFile(coverLetterPath)).toEqual(resumeBytes);
    expectEvidence(await evidenceObject(resume, "resume", "application/pdf"), resumeBytes);
    expectEvidence(
      await evidenceObject(coverLetter, "cover_letter", "application/pdf"),
      coverLetterBytes,
    );
  });
});

async function temporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "bluey-browser-evidence-"));
  directories.push(directory);
  return directory;
}

function materializedDocument(path: string, bytes: Uint8Array): MaterializedDocument {
  return Object.freeze({
    path,
    // Deliberately stale: evidence construction must hash the retained bytes itself.
    sha256: "f".repeat(64),
    bytesBase64: Buffer.from(bytes).toString("base64"),
  });
}

function expectEvidence(evidence: EvidenceObjectUpload, bytes: Uint8Array): void {
  expect(evidence.bytes_base64).toBe(Buffer.from(bytes).toString("base64"));
  expect(evidence.sha256).toBe(createHash("sha256").update(bytes).digest("hex"));
  expect(evidence.sha256).not.toBe("f".repeat(64));
}
