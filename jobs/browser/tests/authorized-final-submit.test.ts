import { createHash } from "node:crypto";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  type ApplicationPacket,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import {
  authorizedFinalSubmitHooks,
  type FinalSubmitFetch,
} from "../src/authorized-final-submit.js";
import { finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import type { LocalRunDelivery } from "../src/local-run-contracts.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  vi.useRealTimers();
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("authorized final submit", () => {
  it("rejects an expired submit capability before network or marker acquisition", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() - 1),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "launch_expired",
    });
    expect(fetchMock).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("rechecks expiry after the live-authority request returns", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      vi.setSystemTime(2_000);
      return Response.json({ authorized: true, authorizedAtMs: 1_000 });
    });
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(2_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "launch_expired",
    });
    expect(fetchMock).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it("fails closed on an explicit live-authority denial", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => new Response(null, { status: 403 }));
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "launch_expired",
    });
    expect(fetchMock).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it("fails closed on a live-authority network error", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      throw new TypeError("network unavailable");
    });
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "launch_expired",
    });
    expect(fetchMock).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it.each([
    ["denied body", { authorized: false, authorizedAtMs: Date.now() }],
    ["missing timestamp", { authorized: true }],
    ["invalid timestamp", { authorized: true, authorizedAtMs: "now" }],
  ])("fails closed on malformed success: %s", async (_label, responseBody) => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => Response.json(responseBody));
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "launch_expired",
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it("acquires the durable marker before posting only the scoped submit proof", async () => {
    const runDirectory = await temporaryRunDirectory();
    const expiresAtMs = Date.now() + 60_000;
    const documents = await materializedDocuments(runDirectory, true);
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
      return Response.json({
        authorized: true,
        authorizedAtMs: Date.now(),
      });
    });
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings("lever"),
      delivery(expiresAtMs),
      documents,
      currentPageUrl("lever"),
      fetchMock,
    );

    await hooks.beforeFinalSubmit(providerProof("lever", true));
    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toBe("https://bluey.sh/api/jobs/local-runs/run-123/authorize-submit");
    expect(init).toMatchObject({
      method: "POST",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        capability: capability("submit", expiresAtMs),
        final_submit_proof: {
          schemaVersion: 3,
          adapter: "lever",
          adapterVersion: "2026.07.0-beta.1",
          control: "lever_application_submit",
          target: submitTarget("lever"),
          files: submitFiles(true),
          fields: submitFields(),
          partOrder: submitPartOrder(true),
          job: {
            approvedCanonicalUrl: "https://jobs.lever.co/acme/posting-123",
            pageUrl: "https://jobs.lever.co/acme/posting-123/apply",
          },
          documents: [
            { kind: "cover_letter", sha256: documents.coverLetter!.sha256 },
            {
              kind: "resume",
              versionId: "resume-version-123",
              sha256: documents.resume.sha256,
            },
          ],
        },
      }),
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it.each([
    ["missing provider proof", undefined],
    ["wrong provider control", {
      ...providerProof(),
      control: "lever_application_submit",
    }],
  ])("rejects %s before network access or marker acquisition", async (_label, proof) => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(proof as never)).rejects.toMatchObject({
      code: "launch_mismatch",
    });

    expect(fetchMock).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("rejects another job on the same provider host before network or marker acquisition", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      () => "https://boards.greenhouse.io/acme/jobs/456#app",
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "launch_mismatch",
    });
    expect(fetchMock).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it.each(["answer", "job", "cover_letter"] as const)(
    "rejects a post-approval %s mutation before network or marker acquisition",
    async (mutation) => {
      const runDirectory = await temporaryRunDirectory();
      const request = requestBindings();
      if (mutation === "answer") request.packet.answers.email = "changed@example.test";
      if (mutation === "job") request.job.title = "Changed after approval";
      if (mutation === "cover_letter") {
        (request.packet as ApplicationPacket).coverLetterContent = "Changed after approval";
      }
      const fetchMock = vi.fn<FinalSubmitFetch>();
      const hooks = authorizedFinalSubmitHooks(
        runDirectory,
        request,
        delivery(Date.now() + 60_000),
        await materializedDocuments(runDirectory),
        currentPageUrl(),
        fetchMock,
      );

      await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
        code: "launch_mismatch",
      });
      expect(fetchMock).not.toHaveBeenCalled();
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
    },
  );

  it("rejects a changed materialized snapshot before network or marker acquisition", async () => {
    const runDirectory = await temporaryRunDirectory();
    const documents = await materializedDocuments(runDirectory);
    await chmod(documents.resume.path, 0o600);
    await writeFile(documents.resume.path, "changed-after-materialization");
    const fetchMock = vi.fn<FinalSubmitFetch>();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      documents,
      currentPageUrl(),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toThrow(
      "Materialized document snapshot is unavailable or has changed",
    );
    expect(fetchMock).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("does not contact the server when durable marker acquisition fails", async () => {
    const runDirectory = await temporaryRunDirectory();
    const documents = await materializedDocuments(runDirectory);
    const first = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      documents,
      currentPageUrl(),
      vi.fn<FinalSubmitFetch>(async () => Response.json({
        authorized: true,
        authorizedAtMs: Date.now(),
      })),
    );
    await first.beforeFinalSubmit(providerProof());

    const fetchMock = vi.fn<FinalSubmitFetch>();
    const duplicate = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      documents,
      currentPageUrl(),
      fetchMock,
    );
    await expect(duplicate.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "submit_authority_exists",
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

function requestBindings(adapter: "greenhouse" | "lever" = "greenhouse") {
  const job: NormalizedJob = {
    externalId: "posting-123",
    canonicalUrl: adapter === "greenhouse"
      ? "https://boards.greenhouse.io/acme/jobs/123"
      : "https://jobs.lever.co/acme/posting-123",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build reliable systems.",
    source: adapter,
  };
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    runId: "run-123",
    packet: applicationPacket(job),
    job,
  };
}

function currentPageUrl(adapter: "greenhouse" | "lever" = "greenhouse"): () => string {
  return () => adapter === "greenhouse"
    ? "https://boards.greenhouse.io/acme/jobs/123#app"
    : "https://jobs.lever.co/acme/posting-123/apply";
}

function providerProof(
  adapter: "greenhouse" | "lever" = "greenhouse",
  withCoverLetter = false,
) {
  return adapter === "greenhouse" ? {
    adapter,
    adapterVersion: "2026.07.1-beta.1",
    control: "greenhouse_submit_application" as const,
    target: submitTarget("greenhouse"),
    files: submitFiles(withCoverLetter),
    fields: submitFields(),
    partOrder: submitPartOrder(withCoverLetter),
  } : {
    adapter,
    adapterVersion: "2026.07.0-beta.1",
    control: "lever_application_submit" as const,
    target: submitTarget("lever"),
    files: submitFiles(withCoverLetter),
    fields: submitFields(),
    partOrder: submitPartOrder(withCoverLetter),
  };
}

function submitTarget(adapter: "greenhouse" | "lever") {
  return adapter === "greenhouse" ? {
    actionUrl: "https://boards.greenhouse.io/acme/jobs/123",
    method: "post",
    enctype: "multipart/form-data",
    formTarget: "_self",
    providerJobKey: "greenhouse:acme:123",
    formIdentity: '[0,"application_form","","application-form","","123"]',
  } : {
    actionUrl: "https://jobs.lever.co/acme/posting-123/apply",
    method: "post",
    enctype: "multipart/form-data",
    formTarget: "_self",
    providerJobKey: "lever:jobs.lever.co:acme:posting-123",
    formIdentity: '[0,"application-form","","","application-form","posting-123"]',
  };
}

function submitFiles(withCoverLetter = false) {
  const resumeBytes = Buffer.from("resume-pdf-bytes");
  const resumeSha = createHash("sha256").update(resumeBytes).digest("hex");
  const files = [{
    fieldName: "resume",
    name: `resume-${resumeSha}.pdf`,
    byteLength: resumeBytes.byteLength,
    sha256: resumeSha,
  }];
  if (withCoverLetter) {
    const coverBytes = Buffer.from("cover-letter-pdf-bytes");
    const coverSha = createHash("sha256").update(coverBytes).digest("hex");
    files.push({
      fieldName: "coverLetter",
      name: `cover-letter-${coverSha}.pdf`,
      byteLength: coverBytes.byteLength,
      sha256: coverSha,
    });
  }
  return files;
}

function submitFields() {
  return [{
    fieldName: "job_id",
    valueByteLength: 3,
    valueSha256: "d".repeat(64),
  }];
}

function submitPartOrder(withCoverLetter = false) {
  return [
    { kind: "field" as const, index: 0 },
    { kind: "file" as const, index: 0 },
    ...(withCoverLetter ? [{ kind: "file" as const, index: 1 }] : []),
  ];
}

async function materializedDocuments(runDirectory: string, withCoverLetter = false) {
  const resume = await snapshot(runDirectory, "resume", Buffer.from("resume-pdf-bytes"));
  const coverLetter = withCoverLetter
    ? await snapshot(runDirectory, "cover-letter", Buffer.from("cover-letter-pdf-bytes"))
    : undefined;
  return {
    packet: applicationPacket({
      externalId: "posting-123",
      canonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
      company: "Acme",
      title: "Engineer",
      location: "Remote",
      workplace: "remote",
      description: "Build reliable systems.",
      source: "greenhouse",
    }),
    resume,
    ...(coverLetter ? { coverLetter } : {}),
  };
}

async function snapshot(runDirectory: string, kind: string, bytes: Buffer) {
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const path = join(runDirectory, `${kind}-${sha256}.pdf`);
  await writeFile(path, bytes, { mode: 0o400 });
  return Object.freeze({
    path,
    sha256,
    bytesBase64: bytes.toString("base64"),
  });
}

function applicationPacket(job: NormalizedJob) {
  const packet = {
    applicationId: "application-123",
    jobId: "job-123",
    resumeVersionId: "resume-version-123",
    approvedPacketChecksum: "",
    answers: {},
    verifiedClaimIds: [],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return packet;
}

function delivery(expiresAtMs: number): LocalRunDelivery {
  return {
    apiOrigin: "https://bluey.sh",
    capabilities: {
      runId: "run-123",
      expiresAtMs,
      result: capability("result", expiresAtMs),
      resume: capability("resume", expiresAtMs),
      submit: capability("submit", expiresAtMs),
    },
  };
}

function capability(operation: "result" | "resume" | "submit", expiresAtMs: number): string {
  const payload = Buffer.from(JSON.stringify({
    version: 1,
    audience: "bluey-jobs-local-run",
    account_id: "account-123",
    application_id: "application-123",
    run_id: "run-123",
    browser_profile_id: "profile-123",
    operation,
    expires_at_ms: expiresAtMs,
    nonce: "n".repeat(32),
  })).toString("base64url");
  return `${payload}.${"a".repeat(64)}`;
}

async function temporaryRunDirectory(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "bluey-authorized-submit-"));
  temporaryDirectories.push(root);
  const runDirectory = join(root, "run");
  await mkdir(runDirectory, { recursive: true });
  return runDirectory;
}
