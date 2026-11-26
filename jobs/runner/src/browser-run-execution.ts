import { createHash } from "node:crypto";
import type { BrowserContext } from "playwright";
import {
  assertApprovedExecutionChecksum,
  assertMaterializedDocumentSnapshot,
  createApplicationReceipt,
  createFinalSubmitProof,
  executeApplication,
  materializeApplicationDocuments,
  type ApplicationPacket,
  type EvidenceObjectUpload,
  type MaterializedDocument,
  type ProviderFinalSubmitProof,
  type ReceiptDocument,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import { authorizeCloudFinalSubmitBeforeCheckpoint } from "./certified-final-submit.js";
import { ExecutionLeaseError, type ActiveExecutionLease } from "./execution-lease.js";
import { ProfileRecoveryBlockedError } from "./recovery-isolation.js";
import { providerRegistryForExecution } from "./resume-policy.js";
import type {
  CloudRunRequest,
  RunEvent,
  SubmissionReceiptAuthority,
} from "./server.js";
import type { ManagedProfileStorage } from "./subject-storage-manager.js";
import type { CloudRunCheckpoint } from "./run-checkpoint-store.js";
import {
  prepareFreshCloudPage,
  wrapActiveCloudPage,
} from "./cloud-browser-page.js";

/** Trusted in-process adapters supplied only by the runner's existing guards. */
export interface BrowserRunExecutionPorts {
  withProfileResidency<T>(
    operation: (storage: ManagedProfileStorage) => Promise<T>,
  ): Promise<T>;
  assertWorkMayContinue(): void;
  writeCheckpoint(input: {
    phase: CloudRunCheckpoint["phase"];
    status: CloudRunCheckpoint["workflow"]["status"];
    browserUrl: string;
    providerReview?: { adapter: string; adapterVersion?: string };
  }): Promise<unknown>;
  writeReceiptArtifact(
    runId: string,
    name: "final.png" | "receipt.json",
    contents: Buffer,
  ): Promise<string>;
  takeoverOrigin(): string | undefined;
}

export async function executeBrowserRun(
  input: CloudRunRequest,
  context: BrowserContext,
  events: RunEvent[],
  lease: ActiveExecutionLease,
  navigate: boolean,
  ports: BrowserRunExecutionPorts,
  resumeAction?: string,
) {
  assertApprovedExecutionChecksum(input.packet, input.job);
  const { documents, runDirectoryPath } = await ports.withProfileResidency(
    async (storage) => {
      const runDirectory = await storage.receipts.ensureChildDirectory(
        input.runId,
      );
      const documentDirectory =
        await runDirectory.ensureChildDirectory("documents");
      const documents = await materializeApplicationDocuments(
        input.packet,
        documentDirectory.canonicalPath,
      );
      return { documents, runDirectoryPath: runDirectory.canonicalPath };
    },
  );
  const runtimePacket: ApplicationPacket = {
    ...documents.packet,
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
  };
  const preparedPage = navigate
    ? await prepareFreshCloudPage(context, input.job)
    : await wrapActiveCloudPage(context, input.job);
  const { page, browserPage } = preparedPage;
  if (navigate)
    await page.goto(input.url, {
      waitUntil: "domcontentloaded",
      timeout: 45_000,
    });
  const adapterContext = {
    runner: "cloud",
    runId: input.runId,
    accountId: input.accountId,
    approvedCanonicalUrl: input.job.canonicalUrl,
    page: browserPage,
    packet: runtimePacket,
    async log(type: string, detail: Record<string, unknown> = {}) {
      events.push({
        id: `${input.runId}:${events.length + 1}`,
        occurredAt: new Date().toISOString(),
        type,
        detail,
      });
    },
    beforeFinalSubmit: async (providerProof: ProviderFinalSubmitProof) => {
      ports.assertWorkMayContinue();
      await ports.withProfileResidency(async (storage) => {
        const retainedRunDirectory = await storage.receipts.openChildDirectory(
          input.runId,
        );
        if (retainedRunDirectory.canonicalPath !== runDirectoryPath) {
          throw new ProfileRecoveryBlockedError();
        }
        await assertMaterializedDocumentSnapshot(documents.resume);
        if (documents.coverLetter) {
          await assertMaterializedDocumentSnapshot(documents.coverLetter);
        }
      });
      const finalSubmitProof = createFinalSubmitProof(
        providerProof,
        {
          resume: {
            versionId: input.packet.resumeVersionId,
            sha256: documents.resume.sha256,
          },
          ...(documents.coverLetter
            ? {
                coverLetter: { sha256: documents.coverLetter.sha256 },
              }
            : {}),
        },
        {
          approvedCanonicalUrl: input.job.canonicalUrl,
          pageUrl: browserPage.url(),
        },
        runtimePacket.approvedExecutionAdmission,
      );
      await authorizeCloudFinalSubmitBeforeCheckpoint(
        lease,
        finalSubmitProof,
        async () => {
          await ports.writeCheckpoint({
            phase: "final_submit_started",
            status: "side_effect_unknown",
            browserUrl: browserPage.url(),
          });
        },
        () => ports.assertWorkMayContinue(),
      );
    },
    afterFinalSubmit: async (outcome: "activated" | "activation_uncertain") => {
      await lease.afterFinalSubmit(outcome);
      await ports.writeCheckpoint({
        phase:
          outcome === "activated"
            ? "final_submit_activated"
            : "side_effect_unknown",
        status: "side_effect_unknown",
        browserUrl: browserPage.url(),
      });
    },
  } as const;
  const providerRegistry = providerRegistryForExecution(
    runtimePacket,
    resumeAction,
  );
  const execution = providerRegistry
    ? await executeApplication(adapterContext, providerRegistry)
    : await executeApplication(adapterContext);
  if (
    execution.receipt.status === "needs_input" &&
    !lease.finalSubmitAttempted
  ) {
    const providerReview =
      ["greenhouse", "lever"].includes(execution.adapter) &&
      execution.receipt.issues.length === 0
        ? {
            adapter: execution.adapter,
            adapterVersion: execution.adapterVersion,
          }
        : undefined;
    await ports.writeCheckpoint({
      phase: providerReview ? "provider_review" : "needs_input",
      status: providerReview ? "provider_review" : "needs_input",
      browserUrl: browserPage.url(),
      providerReview,
    });
  }
  const screenshotBytes = Buffer.from(
    await browserPage.screenshot({ fullPage: true }),
  );
  const screenshotPath = await ports.writeReceiptArtifact(
    input.runId,
    "final.png",
    screenshotBytes,
  );
  if (execution.receipt.status === "needs_input") {
    const takeoverOrigin = ports.takeoverOrigin()?.replace(
      /\/$/,
      "",
    );
    if (execution.receipt.intervention && takeoverOrigin) {
      const sessionId = encodeURIComponent(input.browserSessionId);
      execution.receipt.intervention.takeoverUrl = `${takeoverOrigin}/sessions/${sessionId}`;
    }
  }
  const receiptDocuments: ReceiptDocument[] = [
    {
      kind: "resume",
      versionId: input.packet.resumeVersionId,
      storageKey: documents.resume.path,
      sha256: documents.resume.sha256,
    },
  ];
  if (documents.coverLetter)
    receiptDocuments.push({
      kind: "cover_letter",
      storageKey: documents.coverLetter.path,
      sha256: documents.coverLetter.sha256,
    });
  const atsCertifiedReceiptAuthority = lease.atsCertifiedReceiptAuthority;
  const receipt = createApplicationReceipt({
    receiptId: `receipt-${input.runId}`,
    accountId: input.accountId,
    runId: input.runId,
    runner: "cloud",
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
    adapter: execution.adapter,
    adapterVersion: execution.adapterVersion,
    job: input.job,
    packet: input.packet,
    documents: receiptDocuments,
    events,
    result: execution.receipt,
    finalUrl: page.url(),
    screenshotKeys: [screenshotPath],
    ...(atsCertifiedReceiptAuthority ? { atsCertifiedReceiptAuthority } : {}),
  });
  const receiptPath = await ports.writeReceiptArtifact(
    input.runId,
    "receipt.json",
    Buffer.from(`${JSON.stringify(receipt, null, 2)}\n`, "utf8"),
  );
  const evidenceObjects: EvidenceObjectUpload[] = [
    await evidenceObject(documents.resume, "resume", "application/pdf"),
    ...(documents.coverLetter
      ? [
          await evidenceObject(
            documents.coverLetter,
            "cover_letter",
            "application/pdf",
          ),
        ]
      : []),
    {
      original_key: screenshotPath,
      kind: "screenshot",
      media_type: "image/png",
      sha256: createHash("sha256").update(screenshotBytes).digest("hex"),
      bytes_base64: screenshotBytes.toString("base64"),
    },
  ];
  const receiptAuthority = submissionReceiptAuthority(
    execution.receipt.status,
    lease,
  );
  return {
    accountId: input.accountId,
    applicationId: input.applicationId,
    applicationIdentityId: input.applicationIdentityId,
    browserSessionId: input.browserSessionId,
    runId: input.runId,
    receipt: execution.receipt,
    receiptBundle: receipt,
    evidenceObjects,
    ...(receiptAuthority ? { receiptAuthority } : {}),
    receiptPath,
  };
}

export function submissionReceiptAuthority(
  status: SubmissionReceipt["status"],
  lease: Pick<
    ActiveExecutionLease,
    | "checkpointMetadata"
    | "finalSubmitActivationOutcome"
    | "finalSubmitAttempted"
    | "finalSubmitAuthorized"
  >,
): SubmissionReceiptAuthority | undefined {
  if (status !== "submitted") return undefined;
  const authority = lease.checkpointMetadata();
  if (
    !lease.finalSubmitAttempted ||
    !lease.finalSubmitAuthorized ||
    !lease.finalSubmitActivationOutcome ||
    !/^[A-Za-z0-9_-]{43}$/.test(authority.leaseToken) ||
    !Number.isSafeInteger(authority.fence) ||
    authority.fence <= 0
  ) {
    throw new ExecutionLeaseError("irreversible", "invalid_state");
  }
  return { leaseToken: authority.leaseToken, fence: authority.fence };
}

async function evidenceObject(
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
