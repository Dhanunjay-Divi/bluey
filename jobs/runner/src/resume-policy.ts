import {
  createDefaultAdapterRegistry,
  type ApplicationPacket,
  type ProviderAdapterRegistryOptions,
} from "@bluey/jobs-automation";

export function providerOptionsForResumeAction(
  action: string | undefined,
): ProviderAdapterRegistryOptions | undefined {
  if (action !== "approve_submission") return undefined;
  return {
    greenhouse: { finalReviewApproval: async () => true },
    lever: { finalReviewApproval: async () => true },
  };
}

export function providerRegistryForResumeAction(action: string | undefined) {
  const options = providerOptionsForResumeAction(action);
  return options ? createDefaultAdapterRegistry(undefined, options) : undefined;
}

export function providerRegistryForExecution(
  packet: Pick<
    ApplicationPacket,
    "approvedExecutionAdmission" | "approvedExecutionSchemaVersion"
  >,
  resumeAction: string | undefined,
) {
  const certifiedAuto = packet.approvedExecutionSchemaVersion === 3
    && packet.approvedExecutionAdmission?.kind === "track_auto_submit"
    && packet.approvedExecutionAdmission.ats_certification !== undefined;
  if (certifiedAuto) {
    return createDefaultAdapterRegistry(undefined, {
      greenhouse: { finalReviewApproval: async () => true },
      lever: { finalReviewApproval: async () => true },
    });
  }
  return providerRegistryForResumeAction(resumeAction);
}
