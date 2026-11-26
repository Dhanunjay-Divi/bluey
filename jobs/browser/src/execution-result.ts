import { createHash } from "node:crypto";
import {
  type EvidenceObjectUpload,
  type ExecutionResult,
  type MaterializedDocument,
  type NormalizedJob,
  PlaywrightBrowserPage,
} from "@bluey/jobs-automation";

export async function evidenceObject(
  document: MaterializedDocument,
  kind: "resume" | "cover_letter" | "attachment",
  mediaType: string,
): Promise<EvidenceObjectUpload> {
  const bytes = Buffer.from(document.bytesBase64, "base64");
  return {
    original_key: document.path,
    kind,
    media_type: mediaType,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    bytes_base64: bytes.toString("base64"),
  };
}

export async function handoffExecution(
  page: PlaywrightBrowserPage,
  reason: string,
  resume: boolean,
): Promise<ExecutionResult> {
  const body = await page.bodyText();
  const confirmed = /application (?:has been |was )?(?:submitted|received)|thank(?:s| you) for applying/i.test(body)
    || /(?:thank|confirmation|submitted|success|complete)/i.test(new URL(page.url()).pathname);
  if (resume && confirmed) {
    return {
      adapter: "semantic",
      adapterVersion: "2026.07.1-handoff",
      receipt: {
        status: "submitted",
        confirmationText: body.replace(/\s+/g, " ").trim().slice(0, 500),
        confirmationUrl: page.url(),
        submittedAt: new Date().toISOString(),
        issues: [],
      },
    };
  }
  return {
    adapter: "semantic",
    adapterVersion: "2026.07.1-handoff",
    receipt: {
      status: "needs_input",
      issues: [],
      intervention: {
        kind: "browser_takeover",
        title: "Finish on this job site",
        detail: reason,
        resolution: { kind: "browser_takeover", resumeAfter: true },
      },
    },
  };
}

export async function resolveJob(
  adapter: string,
  page: PlaywrightBrowserPage,
): Promise<NormalizedJob> {
  const title = await page.title();
  const parts = title.split(/[|\-–—]/).map((value) => value.trim()).filter(Boolean);
  return {
    externalId: new URL(page.url()).pathname.split("/").filter(Boolean).at(-1) || page.url(),
    canonicalUrl: page.url(),
    company: parts.at(-1) || "Employer",
    title: parts[0] || "Open role",
    location: "",
    workplace: "unknown",
    description: "",
    source: adapter as NormalizedJob["source"],
  };
}
