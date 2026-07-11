import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { materializeApplicationDocuments } from "../src/index.js";

describe("application document materialization", () => {
  it("creates a deterministic ATS-readable PDF for the frozen resume version", async () => {
    const directory = await mkdtemp(join(tmpdir(), "bluey-resume-"));
    const result = await materializeApplicationDocuments({
      applicationId: "application-1",
      jobId: "job-1",
      resumeVersionId: "resume-version-1",
      resumeContent: {
        contact: { name: "Ada Lovelace", email: "ada@example.com", location: "New York, NY" },
        summary: "Builds reliable systems.",
        skills: ["TypeScript", "Rust"],
        employment: [{ title: "Engineer", company: "Acme", highlights: ["Built an application platform."] }],
      },
      coverLetterContent: "Dear hiring team,\n\nI am excited to apply.",
      answers: {},
      verifiedClaimIds: [],
    }, directory);
    expect(result.packet.resumePath).toMatch(/resume-resume-version-1\.pdf$/);
    expect((await readFile(result.resume.path)).subarray(0, 4).toString()).toBe("%PDF");
    expect(result.resume.sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(result.coverLetter?.sha256).toMatch(/^[a-f0-9]{64}$/);
    await rm(directory, { recursive: true, force: true });
  });
});
