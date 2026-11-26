import type {
  CertifiedFinalSubmitAdapter,
  NormalizedJob,
} from "./contracts.js";
import { parseProviderApplicationTarget } from "./ats-target.js";

export type CertifiedProviderJobKeyPurpose = "submit" | "confirmation";

export class CertifiedProviderJobKeyError extends Error {
  constructor() {
    super("Invalid certified provider job URL");
    this.name = "CertifiedProviderJobKeyError";
  }
}

/**
 * Returns the exact provider-owned job identity shared by final-submit proof,
 * manual confirmation quarantine, and certified form-target validation.
 */
export function certifiedProviderJobKey(
  adapter: CertifiedFinalSubmitAdapter,
  rawUrl: string,
  purpose: CertifiedProviderJobKeyPurpose = "submit",
): string {
  const target = parseProviderApplicationTarget(rawUrl, purpose);
  if (!target || target.provider !== adapter) {
    throw new CertifiedProviderJobKeyError();
  }
  return target.providerJobKey;
}

/** Fail closed before navigating or interacting when a certified job URL drifts. */
export function assertCertifiedProviderNavigationJob(
  rawUrl: string,
  job: Pick<NormalizedJob, "canonicalUrl" | "source">,
): void {
  const expectedAdapter = job.source === "greenhouse" || job.source === "lever"
    ? job.source
    : undefined;
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new CertifiedProviderJobKeyError();
  }
  const host = url.hostname.toLowerCase();
  const navigationAdapter: CertifiedFinalSubmitAdapter | undefined =
    host === "boards.greenhouse.io" || host === "job-boards.greenhouse.io"
      ? "greenhouse"
      : host === "jobs.lever.co" || host === "jobs.eu.lever.co"
        ? "lever"
        : undefined;
  if (expectedAdapter === undefined && navigationAdapter === undefined) return;
  if (!expectedAdapter || navigationAdapter !== expectedAdapter) {
    throw new CertifiedProviderJobKeyError();
  }
  if (
    certifiedProviderJobKey(expectedAdapter, rawUrl, "submit") !==
    certifiedProviderJobKey(expectedAdapter, job.canonicalUrl, "submit")
  ) {
    throw new CertifiedProviderJobKeyError();
  }
}
