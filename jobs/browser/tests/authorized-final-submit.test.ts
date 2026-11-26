import { createHash } from "node:crypto";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  finalSubmitSurfaceSha256,
  type ApplicationPacket,
  type AtsCertifiedReceiptAuthority,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import {
  authorizedFinalSubmitHooks,
  certifiedAutoProviderApproval,
  type FinalSubmitFetch,
} from "../src/authorized-final-submit.js";
import { finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import { classifyLocalFailure } from "../src/local-failure.js";
import type { LocalRunDelivery } from "../src/local-run-contracts.js";
import {
  localRunCapabilitiesFixture,
  localRunCapabilityFixture,
  localRunReleaseFixture,
} from "./fixtures/local-run-capability.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  vi.useRealTimers();
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("authorized final submit", () => {
  it("clears provider review only for checksum-valid schema-3 certified Auto", () => {
    const certified = requestBindings("greenhouse", true);
    expect(certifiedAutoProviderApproval(certified.packet, certified.job)).toEqual({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
    });

    const review = requestBindings();
    expect(certifiedAutoProviderApproval(review.packet, review.job)).toBeUndefined();

    const schemaTwoAuto = requestBindings();
    schemaTwoAuto.packet.approvedExecutionSchemaVersion = 2;
    schemaTwoAuto.packet.approvedExecutionAdmission = {
      kind: "track_auto_submit",
      authorization_id: "authorization-123",
      career_track_id: "track-123",
      revision_no: 2,
      authority_fingerprint: "1".repeat(64),
    };
    schemaTwoAuto.packet.approvedPacketChecksum = approvedExecutionChecksum(
      schemaTwoAuto.packet,
      schemaTwoAuto.job,
    );
    expect(
      certifiedAutoProviderApproval(schemaTwoAuto.packet, schemaTwoAuto.job),
    ).toBeUndefined();
  });

  it("does not clear provider review for a mismatched or changed certified packet", () => {
    const providerMismatch = requestBindings("greenhouse", true);
    providerMismatch.packet.approvedExecutionAdmission!.ats_certification!.provider = "lever";
    providerMismatch.packet.approvedPacketChecksum = approvedExecutionChecksum(
      providerMismatch.packet,
      providerMismatch.job,
    );
    expect(
      certifiedAutoProviderApproval(providerMismatch.packet, providerMismatch.job),
    ).toBeUndefined();

    const changed = requestBindings("greenhouse", true);
    changed.packet.answers.email = "changed@example.com";
    expect(() => certifiedAutoProviderApproval(changed.packet, changed.job)).toThrow(
      "changed after review",
    );
  });

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

  it("rejects a checkpoint release rebind before live submit authorization", async () => {
    const runDirectory = await temporaryRunDirectory();
    const expiresAtMs = Date.now() + 60_000;
    const fetchMock = vi.fn<FinalSubmitFetch>();
    const original = delivery(expiresAtMs);
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      {
        ...original,
        capabilities: {
          ...original.capabilities,
          release: localRunReleaseFixture({
            activation_sha256: "e".repeat(64),
            activation_generation: 2,
            channel_sequence: 2,
          }),
        },
      },
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

  it("treats expiry after Phase B returns as terminal uncertainty", async () => {
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
    const activate = vi.fn();

    await expect(
      hooks.beforeFinalSubmit(providerProof()).then(() => activate()),
    ).rejects.toMatchObject({ code: "submit_outcome_unknown" });
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(activate).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("preserves recovery when Phase B commits before the local marker can be written", async () => {
    const runDirectory = await temporaryRunDirectory();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      vi.fn<FinalSubmitFetch>(async () => {
        await rm(runDirectory, { recursive: true, force: true });
        return Response.json({ authorized: true, authorizedAtMs: Date.now() });
      }),
    );

    const error = await hooks.beforeFinalSubmit(providerProof()).then(
      () => undefined,
      (cause: unknown) => cause,
    );
    expect(error).toMatchObject({ code: "submit_outcome_unknown" });
    await expect(classifyLocalFailure(runDirectory, error)).resolves.toMatchObject({
      status: "side_effect_unknown",
      code: "submit_outcome_unknown",
      preservePage: true,
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
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
    const activate = vi.fn();

    await expect(
      hooks.beforeFinalSubmit(providerProof()).then(() => activate()),
    ).rejects.toMatchObject({ code: "launch_expired" });
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(activate).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it.each([500, 502, 503, 504])(
    "treats ambiguous live-authority HTTP %i as terminal uncertainty",
    async (status) => {
      const runDirectory = await temporaryRunDirectory();
      const fetchMock = vi.fn<FinalSubmitFetch>(
        async () => new Response(null, { status }),
      );
      const hooks = authorizedFinalSubmitHooks(
        runDirectory,
        requestBindings(),
        delivery(Date.now() + 60_000),
        await materializedDocuments(runDirectory),
        currentPageUrl(),
        fetchMock,
      );
      const activate = vi.fn();

      const error = await hooks.beforeFinalSubmit(providerProof()).then(
        () => activate(),
        (cause: unknown) => cause,
      );
      expect(error).toMatchObject({ code: "submit_outcome_unknown" });
      expect(fetchMock).toHaveBeenCalledOnce();
      expect(activate).not.toHaveBeenCalled();
      await expect(classifyLocalFailure(runDirectory, error)).resolves.toMatchObject({
        status: "side_effect_unknown",
        code: "submit_outcome_unknown",
        preservePage: true,
      });
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
    },
  );

  it("treats a live-authority transport loss as terminal uncertainty", async () => {
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
    const activate = vi.fn();

    await expect(
      hooks.beforeFinalSubmit(providerProof()).then(() => activate()),
    ).rejects.toMatchObject({ code: "submit_outcome_unknown" });
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(activate).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("treats a live-authority timeout as terminal uncertainty", async () => {
    const runDirectory = await temporaryRunDirectory();
    const timeout = new AbortController();
    const fetchMock = vi.fn<FinalSubmitFetch>(async (_input, init) =>
      new Promise<Response>((_resolve, reject) => {
        const signal = init?.signal;
        if (!signal) {
          reject(new Error("missing authorization timeout signal"));
          return;
        }
        signal.addEventListener(
          "abort",
          () => reject(new Error("authorization timed out")),
          { once: true },
        );
        queueMicrotask(() => timeout.abort());
      }));
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
      () => timeout.signal,
    );
    const activate = vi.fn();

    await expect(
      hooks.beforeFinalSubmit(providerProof()).then(() => activate()),
    ).rejects.toMatchObject({ code: "submit_outcome_unknown" });
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(activate).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it.each([
    ["denied body", { authorized: false, authorizedAtMs: Date.now() }],
    ["missing timestamp", { authorized: true }],
    ["invalid timestamp", { authorized: true, authorizedAtMs: "now" }],
  ])("treats malformed Phase-B success as terminal uncertainty: %s", async (
    _label,
    responseBody,
  ) => {
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
    const activate = vi.fn();

    await expect(
      hooks.beforeFinalSubmit(providerProof()).then(() => activate()),
    ).rejects.toMatchObject({ code: "submit_outcome_unknown" });
    expect(activate).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("retains exact certified Phase-B authority before marker and activation", async () => {
    const runDirectory = await temporaryRunDirectory();
    const authorizedAtMs = Date.now();
    const authority = certifiedReceiptAuthority(authorizedAtMs);
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
      return Response.json({
        authorized: true,
        authorizedAtMs,
        atsCertifiedReceiptAuthority: authority,
      });
    });
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings("greenhouse", true),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      fetchMock,
    );
    const activate = vi.fn(() => {
      expect(hooks.atsCertifiedReceiptAuthority()).toEqual(authority);
    });

    await hooks.beforeFinalSubmit(providerProof()).then(() => activate());

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(activate).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
    const returned = hooks.atsCertifiedReceiptAuthority()!;
    returned.phaseBRequestId = "mutated-after-read";
    expect(hooks.atsCertifiedReceiptAuthority()?.phaseBRequestId).toBe(
      "phase-b-request-123",
    );
  });

  it.each([
    ["missing authority", (response: Record<string, unknown>) => {
      delete response.atsCertifiedReceiptAuthority;
    }],
    ["unknown response field", (response: Record<string, unknown>) => {
      response.reusableCredential = "forbidden";
    }],
    ["unknown authority field", (response: Record<string, unknown>) => {
      authorityRecord(response).reusableCredential = "forbidden";
    }],
    ["uppercase digest", (response: Record<string, unknown>) => {
      authorityRecord(response).bindingSha256 = "F".repeat(64);
    }],
    ["cross-run binding", (response: Record<string, unknown>) => {
      authorityRecord(response).runId = "run-other";
    }],
    ["cross-provider binding", (response: Record<string, unknown>) => {
      authorityRecord(response).provider = "lever";
    }],
    ["wrong observed surface", (response: Record<string, unknown>) => {
      authorityRecord(response).observedSurfaceSha256 = "d".repeat(64);
    }],
    ["invalid signed layout observation", (response: Record<string, unknown>) => {
      authorityRecord(response).layoutObservationSha256 = "D".repeat(64);
    }],
    ["cloud runner binding", (response: Record<string, unknown>) => {
      authorityRecord(response).runnerKind = "cloud";
    }],
    ["unapproved runner target", (response: Record<string, unknown>) => {
      authorityRecord(response).runnerTargetSha256 = "d".repeat(64);
    }],
    ["mismatched consume time", (response: Record<string, unknown>) => {
      authorityRecord(response).bindingConsumedAtMs = 2;
    }],
    ["control-bearing request ID", (response: Record<string, unknown>) => {
      authorityRecord(response).phaseBRequestId = "phase-b\nother";
    }],
    ["unsafe fence", (response: Record<string, unknown>) => {
      authorityRecord(response).bindingFence = Number.MAX_SAFE_INTEGER + 1;
    }],
  ] as const)("treats malformed certified success with %s as terminal uncertainty", async (
    _label,
    mutate,
  ) => {
    const runDirectory = await temporaryRunDirectory();
    const authorizedAtMs = Date.now();
    const response: Record<string, unknown> = {
      authorized: true,
      authorizedAtMs,
      atsCertifiedReceiptAuthority: certifiedReceiptAuthority(authorizedAtMs),
    };
    mutate(response);
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings("greenhouse", true),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      vi.fn<FinalSubmitFetch>(async () => Response.json(response)),
    );
    const activate = vi.fn();

    await expect(
      hooks.beforeFinalSubmit(providerProof()).then(() => activate()),
    ).rejects.toMatchObject({ code: "submit_outcome_unknown" });
    expect(activate).not.toHaveBeenCalled();
    expect(hooks.atsCertifiedReceiptAuthority()).toBeUndefined();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("keeps review-first response behavior authority-free", async () => {
    const runDirectory = await temporaryRunDirectory();
    const authorizedAtMs = Date.now();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      vi.fn<FinalSubmitFetch>(async () => Response.json({
        authorized: true,
        authorizedAtMs,
      })),
    );

    await hooks.beforeFinalSubmit(providerProof());

    expect(hooks.atsCertifiedReceiptAuthority()).toBeUndefined();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it("treats injected authority in a successful review response as terminal uncertainty", async () => {
    const runDirectory = await temporaryRunDirectory();
    const authorizedAtMs = Date.now();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      await materializedDocuments(runDirectory),
      currentPageUrl(),
      vi.fn<FinalSubmitFetch>(async () => Response.json({
        authorized: true,
        authorizedAtMs,
        atsCertifiedReceiptAuthority: certifiedReceiptAuthority(authorizedAtMs),
      })),
    );

    await expect(hooks.beforeFinalSubmit(providerProof())).rejects.toMatchObject({
      code: "submit_outcome_unknown",
    });

    expect(hooks.atsCertifiedReceiptAuthority()).toBeUndefined();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("consumes live authority before writing the durable scoped submit marker", async () => {
    const runDirectory = await temporaryRunDirectory();
    const expiresAtMs = Date.now() + 60_000;
    const documents = await materializedDocuments(runDirectory, true);
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
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
    const activate = vi.fn(async () => {
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
    });

    await hooks.beforeFinalSubmit(providerProof("lever", true)).then(() => activate());
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(activate).toHaveBeenCalledOnce();
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

function requestBindings(
  adapter: "greenhouse" | "lever" = "greenhouse",
  certified = false,
) {
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
    packet: applicationPacket(job, certified),
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

function applicationPacket(job: NormalizedJob, certified = false): ApplicationPacket {
  if (certified && job.source !== "greenhouse" && job.source !== "lever") {
    throw new Error("certified packet fixture needs an exact provider");
  }
  const packet: ApplicationPacket = {
    applicationId: "application-123",
    jobId: "job-123",
    resumeVersionId: "resume-version-123",
    approvedPacketChecksum: "",
    answers: {},
    verifiedClaimIds: [],
    ...(certified ? {
      approvedExecutionSchemaVersion: 3 as const,
      approvedExecutionAdmission: {
        kind: "track_auto_submit" as const,
        authorization_id: "authorization-123",
        career_track_id: "track-123",
        revision_no: 2,
        authority_fingerprint: "1".repeat(64),
        ats_certification: {
          schema_version: 1 as const,
          provider: job.source as "greenhouse" | "lever",
          adapter_version: job.source === "greenhouse"
            ? "2026.07.1-beta.1"
            : "2026.07.0-beta.1",
          variant_key: "public",
          layout_contract_version: 1,
          surface_sha256: finalSubmitSurfaceSha256(providerProof(job.source)),
          manifest_sha256: "2".repeat(64),
          activation_sha256: "3".repeat(64),
          activation_generation: 4,
          target_key_sha256: "5".repeat(64),
          layout_set_sha256: "6".repeat(64),
          adapter_bundle_sha256: "7".repeat(64),
          runner_target_sha256s: ["8".repeat(64), "9".repeat(64)],
          expires_at_ms: 9_007_199_254_740_000,
        },
      },
    } : {}),
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return packet;
}

function certifiedReceiptAuthority(
  bindingConsumedAtMs: number,
  overrides: Partial<AtsCertifiedReceiptAuthority> = {},
): AtsCertifiedReceiptAuthority {
  return {
    schemaVersion: 1,
    accountId: "account-123",
    applicationId: "application-123",
    runId: "run-123",
    provider: "greenhouse",
    adapter: "greenhouse",
    adapterVersion: "2026.07.1-beta.1",
    manifestSha256: "2".repeat(64),
    activationSha256: "3".repeat(64),
    activationGeneration: 4,
    targetKeySha256: "5".repeat(64),
    layoutSetSha256: "6".repeat(64),
    layoutObservationSha256: "d".repeat(64),
    observedSurfaceSha256: finalSubmitSurfaceSha256(providerProof()),
    adapterBundleSha256: "7".repeat(64),
    runnerKind: "local",
    runnerTargetSha256: "8".repeat(64),
    bindingSha256: "a".repeat(64),
    bindingFence: 5,
    bindingConsumedAtMs,
    applicationAttemptId: "attempt-123",
    phaseBRequestId: "phase-b-request-123",
    rolloutChannel: "canary",
    canaryReservationSha256: "b".repeat(64),
    meteringReservationSha256: "c".repeat(64),
    ...overrides,
  };
}

function authorityRecord(response: Record<string, unknown>): Record<string, unknown> {
  return response.atsCertifiedReceiptAuthority as Record<string, unknown>;
}

function delivery(expiresAtMs: number): LocalRunDelivery {
  return {
    apiOrigin: "https://bluey.sh",
    capabilities: localRunCapabilitiesFixture(expiresAtMs),
  };
}

function capability(operation: "result" | "resume" | "submit", expiresAtMs: number): string {
  return localRunCapabilityFixture(operation, expiresAtMs);
}

async function temporaryRunDirectory(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "bluey-authorized-submit-"));
  temporaryDirectories.push(root);
  const runDirectory = join(root, "run");
  await mkdir(runDirectory, { recursive: true });
  return runDirectory;
}
