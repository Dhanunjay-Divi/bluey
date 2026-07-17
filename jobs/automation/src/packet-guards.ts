import type { ApplicationPacket } from "./contracts.js";
import type { ApplicationReceiptBundle } from "./receipts.js";

export function assertRunnablePacket(packet: ApplicationPacket): void {
  const missing = [
    ["applicationId", packet.applicationId],
    ["jobId", packet.jobId],
    ["resumeVersionId", packet.resumeVersionId],
    ["approvedPacketChecksum", packet.approvedPacketChecksum],
    ["applicationIdentityId", packet.applicationIdentityId],
    ["applicationEmail", packet.applicationEmail],
    ["browserProfileId", packet.browserProfileId],
  ]
    .filter(([, value]) => !String(value ?? "").trim())
    .map(([field]) => field);

  if (missing.length) {
    throw new Error(`Application packet is missing ${missing.join(", ")}`);
  }
}

export function assertSubmissionReceiptComplete(receipt: ApplicationReceiptBundle): void {
  const missing = [
    ["receiptId", receipt.receiptId],
    ["accountId", receipt.accountId],
    ["applicationId", receipt.applicationId],
    ["runId", receipt.runId],
    ["applicationIdentityId", receipt.applicationIdentityId],
    ["browserProfileId", receipt.browserProfileId],
    ["adapter", receipt.adapter],
    ["adapterVersion", receipt.adapterVersion],
    ["packet.applicationEmail", receipt.packet.applicationEmail],
    ["packet.resumeVersionId", receipt.packet.resumeVersionId],
    ["packet.approvedPacketChecksum", receipt.packet.approvedPacketChecksum],
  ]
    .filter(([, value]) => !String(value ?? "").trim())
    .map(([field]) => field);
  const resume = receipt.documents.find((document) => document.kind === "resume");
  if (!resume) missing.push("resume document");
  if (resume && !resume.storageKey.trim()) missing.push("resume storage key");
  if (resume && !/^[a-f0-9]{64}$/i.test(resume.sha256)) missing.push("resume checksum");
  for (const document of receipt.documents) {
    if (!document.storageKey.trim()) missing.push(`${document.kind} storage key`);
    if (!/^[a-f0-9]{64}$/i.test(document.sha256)) missing.push(`${document.kind} checksum`);
  }
  if (resume && resume.versionId && resume.versionId !== receipt.packet.resumeVersionId) {
    throw new Error("Submission receipt resume document does not match the packet resume version");
  }
  if (receipt.result.status === "submitted") {
    const hasConfirmation = Boolean(receipt.result.confirmationText?.trim() || receipt.result.confirmationUrl?.trim());
    if (!hasConfirmation) missing.push("submission confirmation");
    if (!receipt.screenshotKeys.length) missing.push("confirmation screenshot");
  }
  if (missing.length) {
    throw new Error(`Submission receipt is missing ${missing.join(", ")}`);
  }
}
