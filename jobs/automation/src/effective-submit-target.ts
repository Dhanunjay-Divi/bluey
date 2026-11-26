import type {
  BrowserLocator,
  CertifiedFinalSubmitAdapter,
  EffectiveSubmitTargetIdentity,
  ExactSubmitFieldEvidence,
  ExactSubmitPartOrderEntry,
} from "./contracts.js";
import { certifiedProviderJobKey } from "./provider-job-key.js";

export function assertApprovedProviderJob(
  adapter: CertifiedFinalSubmitAdapter,
  approvedCanonicalUrl: string,
  currentUrl: string,
): string {
  let approvedKey: string;
  let currentKey: string;
  try {
    approvedKey = certifiedProviderJobKey(adapter, approvedCanonicalUrl, "submit");
    currentKey = certifiedProviderJobKey(adapter, currentUrl, "submit");
  } catch {
    throw new Error(`${providerLabel(adapter)} page does not match the approved provider job`);
  }
  if (approvedKey !== currentKey) {
    throw new Error(`${providerLabel(adapter)} page does not match the approved provider job`);
  }
  return currentKey;
}

export function assertApprovedProviderJobOrConfirmation(
  adapter: CertifiedFinalSubmitAdapter,
  approvedCanonicalUrl: string,
  currentUrl: string,
): string {
  let approvedKey: string;
  let currentKey: string;
  try {
    approvedKey = certifiedProviderJobKey(adapter, approvedCanonicalUrl, "submit");
    try {
      currentKey = certifiedProviderJobKey(adapter, currentUrl, "submit");
    } catch {
      currentKey = certifiedProviderJobKey(adapter, currentUrl, "confirmation");
    }
  } catch {
    throw new Error(`${providerLabel(adapter)} page does not match the approved provider job`);
  }
  if (approvedKey !== currentKey) {
    throw new Error(`${providerLabel(adapter)} page does not match the approved provider job`);
  }
  return currentKey;
}

export function isApprovedProviderConfirmation(
  adapter: CertifiedFinalSubmitAdapter,
  approvedCanonicalUrl: string,
  confirmationUrl: string,
  proofJobKey: string,
): boolean {
  try {
    const approvedKey = certifiedProviderJobKey(adapter, approvedCanonicalUrl, "submit");
    return approvedKey === proofJobKey
      && certifiedProviderJobKey(adapter, confirmationUrl, "confirmation") === proofJobKey;
  } catch {
    return false;
  }
}

export function sameExactSubmitFields(
  left: readonly ExactSubmitFieldEvidence[],
  right: readonly ExactSubmitFieldEvidence[],
): boolean {
  return left.length === right.length && left.every((field, index) => {
    const other = right[index];
    return field.fieldName === other?.fieldName
      && field.valueByteLength === other.valueByteLength
      && field.valueSha256 === other.valueSha256;
  });
}

export function sameExactSubmitPartOrder(
  left: readonly ExactSubmitPartOrderEntry[],
  right: readonly ExactSubmitPartOrderEntry[],
): boolean {
  return left.length === right.length && left.every((entry, index) => {
    const other = right[index];
    return entry.kind === other?.kind && entry.index === other.index;
  });
}

export async function captureEffectiveSubmitTarget(
  locator: BrowserLocator,
  adapter: CertifiedFinalSubmitAdapter,
  approvedCanonicalUrl: string,
  currentUrl: string,
): Promise<EffectiveSubmitTargetIdentity> {
  const approvedJobKey = assertApprovedProviderJob(
    adapter,
    approvedCanonicalUrl,
    currentUrl,
  );
  let target: EffectiveSubmitTargetIdentity;
  let pageUrl: URL;
  let actionUrl: URL;
  try {
    target = await locator.effectiveSubmitTarget(adapter);
    pageUrl = new URL(currentUrl);
    actionUrl = new URL(target.actionUrl);
  } catch {
    throw new Error(`${providerLabel(adapter)} submit target is invalid`);
  }
  let actionJobKey: string;
  try {
    actionJobKey = certifiedProviderJobKey(adapter, target.actionUrl, "submit");
  } catch {
    throw new Error(`${providerLabel(adapter)} submit target is invalid`);
  }
  if (target.method !== "post"
    || target.enctype !== "multipart/form-data"
    || target.formTarget !== "_self"
    || pageUrl.origin !== actionUrl.origin
    || target.providerJobKey !== actionJobKey
    || actionJobKey !== approvedJobKey
    || target.formIdentity.length < 1
    || target.formIdentity.length > 1_024) {
    throw new Error(`${providerLabel(adapter)} submit target is invalid`);
  }
  return Object.freeze({ ...target });
}

export function sameEffectiveSubmitTarget(
  left: EffectiveSubmitTargetIdentity,
  right: EffectiveSubmitTargetIdentity,
): boolean {
  return left.actionUrl === right.actionUrl
    && left.method === right.method
    && left.enctype === right.enctype
    && left.formTarget === right.formTarget
    && left.providerJobKey === right.providerJobKey
    && left.formIdentity === right.formIdentity;
}

function providerLabel(adapter: CertifiedFinalSubmitAdapter): "Greenhouse" | "Lever" {
  return adapter === "greenhouse" ? "Greenhouse" : "Lever";
}
