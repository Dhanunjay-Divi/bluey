import {
  createDefaultAdapterRegistry,
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
