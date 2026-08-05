import type {
  CertifiedFinalSubmitAdapter,
  NormalizedJob,
} from "./contracts.js";

export type CertifiedProviderJobKeyPurpose = "submit" | "confirmation";

const GREENHOUSE_JOB_QUERY_ALIASES = new Set([
  "gh_jid",
  "token",
  "job_id",
  "jobid",
  "posting_id",
  "postingid",
]);
const LEVER_JOB_QUERY_ALIASES = new Set([
  "posting_id",
  "postingid",
  "job_id",
  "jobid",
  "lever_job_id",
]);

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
  const url = publicHttpsUrl(rawUrl);
  return adapter === "greenhouse"
    ? greenhouseJobKey(url, purpose)
    : leverJobKey(url, purpose);
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
  if (certifiedProviderJobKey(expectedAdapter, rawUrl, "submit")
    !== certifiedProviderJobKey(expectedAdapter, job.canonicalUrl, "submit")) {
    throw new CertifiedProviderJobKeyError();
  }
}

function publicHttpsUrl(rawUrl: string): URL {
  if (rawUrl.length < 1 || rawUrl.length > 2_048 || rawUrl.trim() !== rawUrl) {
    throw new CertifiedProviderJobKeyError();
  }
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new CertifiedProviderJobKeyError();
  }
  if (url.protocol !== "https:" || url.username || url.password || url.port) {
    throw new CertifiedProviderJobKeyError();
  }
  return url;
}

function greenhouseJobKey(
  url: URL,
  purpose: CertifiedProviderJobKeyPurpose,
): string {
  const host = url.hostname.toLowerCase();
  if (host !== "boards.greenhouse.io" && host !== "job-boards.greenhouse.io") {
    throw new CertifiedProviderJobKeyError();
  }
  const segments = url.pathname.split("/").filter(Boolean);
  const jobIndexes = segments
    .map((segment, index) => segment.toLowerCase() === "jobs" ? index : -1)
    .filter((index) => index >= 0);
  if (jobIndexes.length > 1) throw new CertifiedProviderJobKeyError();
  const jobIndex = jobIndexes[0];
  if (jobIndex !== undefined) {
    if (jobIndex !== 1 || segments.length < 3) throw new CertifiedProviderJobKeyError();
    const suffix = segments.slice(3).map((segment) => segment.toLowerCase());
    const validSuffix = purpose === "confirmation"
      ? suffix.length === 1 && suffix[0] === "confirmation"
      : suffix.length === 0;
    if (!validSuffix) throw new CertifiedProviderJobKeyError();
  } else if (segments.length !== 2
    || segments[0]?.toLowerCase() !== "embed"
    || segments[1]?.toLowerCase() !== "job_app"
    || purpose !== "submit") {
    throw new CertifiedProviderJobKeyError();
  }
  const pathTenant = jobIndex === 1 ? segments[0] : undefined;
  const pathJob = jobIndex === 1 ? segments[2] : undefined;
  const tenant = oneIdentifier([
    ...queryAliasValues(url, new Set(["for"])),
    pathTenant,
  ]);
  const job = oneIdentifier([
    ...queryAliasValues(url, GREENHOUSE_JOB_QUERY_ALIASES),
    pathJob,
  ]);
  // Greenhouse's two official hosts are equivalent front doors for the same
  // tenant/job. Deliberately omit host so a legitimate host migration cannot
  // turn observed confirmation into a duplicate submit.
  return `greenhouse:${tenant}:${job}`;
}

function leverJobKey(
  url: URL,
  purpose: CertifiedProviderJobKeyPurpose,
): string {
  const host = url.hostname.toLowerCase();
  if (host !== "jobs.lever.co" && host !== "jobs.eu.lever.co") {
    throw new CertifiedProviderJobKeyError();
  }
  const segments = url.pathname.split("/").filter(Boolean);
  const suffix = segments.length === 3 ? segments[2]?.toLowerCase() : undefined;
  const validPath = purpose === "confirmation"
    ? segments.length === 3 && suffix === "confirmation"
    : segments.length === 2 || segments.length === 3 && suffix === "apply";
  if (!validPath) throw new CertifiedProviderJobKeyError();
  const tenant = identifier(segments[0]);
  const job = identifier(segments[1]);
  const aliases = queryAliasValues(url, LEVER_JOB_QUERY_ALIASES).map(identifier);
  if (aliases.some((value) => value !== job)) throw new CertifiedProviderJobKeyError();
  return `lever:${host}:${tenant}:${job}`;
}

function queryAliasValues(url: URL, aliases: ReadonlySet<string>): string[] {
  const values: string[] = [];
  for (const [key, value] of url.searchParams) {
    if (aliases.has(key.toLowerCase())) values.push(value);
  }
  return values;
}

function oneIdentifier(values: Array<string | undefined>): string {
  const present = values.filter((value): value is string => value !== undefined);
  if (present.length < 1) throw new CertifiedProviderJobKeyError();
  const checked = present.map(identifier);
  if (checked.some((value) => value !== checked[0])) {
    throw new CertifiedProviderJobKeyError();
  }
  return checked[0]!;
}

function identifier(value: string | undefined): string {
  if (!value || !/^[A-Za-z0-9_-]{1,160}$/u.test(value)) {
    throw new CertifiedProviderJobKeyError();
  }
  return value;
}
