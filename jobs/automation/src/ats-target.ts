import type { CertifiedFinalSubmitAdapter } from "./contracts.js";

export type ProviderApplicationTargetPurpose = "submit" | "confirmation";
export type ProviderApplicationTargetVariant =
  | "greenhouse_public"
  | "greenhouse_embedded"
  | "lever_posting"
  | "lever_application";

export interface ProviderApplicationTarget {
  provider: CertifiedFinalSubmitAdapter;
  host: string;
  tenant: string;
  job: string;
  purpose: ProviderApplicationTargetPurpose;
  variant: ProviderApplicationTargetVariant;
  providerJobKey: string;
}

const GREENHOUSE_HOSTS = new Set([
  "boards.greenhouse.io",
  "job-boards.greenhouse.io",
]);
const LEVER_HOSTS = new Set(["jobs.lever.co", "jobs.eu.lever.co"]);
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

/**
 * Parses the provider-owned target grammar used before navigation, adapter
 * selection, and final-submit proof. A provider hostname alone is never
 * enough: the target must be an exact public HTTPS job URL without user info,
 * an explicit port, or a path outside the provider's job/application shape.
 */
export function parseProviderApplicationTarget(
  rawUrl: string,
  purpose: ProviderApplicationTargetPurpose = "submit",
): ProviderApplicationTarget | undefined {
  const parsed = publicProviderUrl(rawUrl);
  if (!parsed) return undefined;
  const { url, pathSegments } = parsed;
  const host = url.hostname.toLowerCase();
  if (GREENHOUSE_HOSTS.has(host)) {
    return greenhouseTarget(url, host, pathSegments, purpose);
  }
  if (LEVER_HOSTS.has(host)) {
    return leverTarget(url, host, pathSegments, purpose);
  }
  return undefined;
}

function publicProviderUrl(
  rawUrl: string,
): { url: URL; pathSegments: string[] } | undefined {
  if (typeof rawUrl !== "string") return undefined;
  const raw = rawUrl;
  if (raw.length < 1
    || raw.length > 2_048
    || new TextEncoder().encode(raw).byteLength > 2_048
    || raw.trim() !== raw) {
    return undefined;
  }
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    return undefined;
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.port ||
    !hasExactProviderAuthority(raw, url)
  ) {
    return undefined;
  }
  const rawPath = rawProviderPath(raw);
  if (rawPath.includes("%") || rawPath.includes("//")) return undefined;
  const normalizedPath = rawPath.length > 1 && rawPath.endsWith("/")
    ? rawPath.slice(0, -1)
    : rawPath;
  const pathSegments = normalizedPath.split("/").filter(Boolean);
  if (pathSegments.length < 1 || pathSegments.some((segment) => !segment)) {
    return undefined;
  }
  return { url, pathSegments };
}

function hasExactProviderAuthority(rawUrl: string, url: URL): boolean {
  const scheme = rawUrl.slice(0, rawUrl.indexOf(":"));
  if (scheme.toLowerCase() !== "https" || rawUrl.slice(scheme.length, scheme.length + 3) !== "://") {
    return false;
  }
  const authorityStart = scheme.length + 3;
  const suffix = rawUrl.slice(authorityStart);
  const authorityEnd = suffix.search(/[/?#]/u);
  const authority = authorityEnd < 0 ? suffix : suffix.slice(0, authorityEnd);
  return authority.toLowerCase() === url.hostname.toLowerCase();
}

function rawProviderPath(rawUrl: string): string {
  const schemeEnd = rawUrl.indexOf("://") + 3;
  const authorityAndSuffix = rawUrl.slice(schemeEnd);
  const delimiter = authorityAndSuffix.search(/[/?#]/u);
  if (delimiter < 0 || authorityAndSuffix[delimiter] !== "/") return "/";
  const pathAndSuffix = authorityAndSuffix.slice(delimiter);
  const pathEnd = pathAndSuffix.search(/[?#]/u);
  return pathEnd < 0 ? pathAndSuffix : pathAndSuffix.slice(0, pathEnd);
}

function greenhouseTarget(
  url: URL,
  host: string,
  segments: string[],
  purpose: ProviderApplicationTargetPurpose,
): ProviderApplicationTarget | undefined {
  const publicTarget = segments.length === (purpose === "submit" ? 3 : 4)
    && segments[1]?.toLowerCase() === "jobs"
    && (purpose === "submit" || segments[3]?.toLowerCase() === "confirmation");
  const embeddedTarget = purpose === "submit"
    && segments.length === 2
    && segments[0]?.toLowerCase() === "embed"
    && segments[1]?.toLowerCase() === "job_app";
  if (!publicTarget && !embeddedTarget) return undefined;

  const pathTenant = publicTarget ? segments[0] : undefined;
  const pathJob = publicTarget ? segments[2] : undefined;
  const tenant = oneIdentifier([
    ...queryAliasValues(url, new Set(["for"])),
    pathTenant,
  ]);
  const job = oneIdentifier([
    ...queryAliasValues(url, GREENHOUSE_JOB_QUERY_ALIASES),
    pathJob,
  ]);
  if (!tenant || !job) return undefined;
  return {
    provider: "greenhouse",
    host,
    tenant,
    job,
    purpose,
    variant: embeddedTarget ? "greenhouse_embedded" : "greenhouse_public",
    providerJobKey: `greenhouse:${tenant}:${job}`,
  };
}

function leverTarget(
  url: URL,
  host: string,
  segments: string[],
  purpose: ProviderApplicationTargetPurpose,
): ProviderApplicationTarget | undefined {
  const suffix = segments[2]?.toLowerCase();
  const postingTarget = purpose === "submit" && segments.length === 2;
  const applicationTarget = segments.length === 3
    && suffix === (purpose === "submit" ? "apply" : "confirmation");
  if (!postingTarget && !applicationTarget) return undefined;
  const tenant = identifier(segments[0]);
  const job = identifier(segments[1]);
  if (!tenant || !job) return undefined;
  const aliases = queryAliasValues(url, LEVER_JOB_QUERY_ALIASES).map(identifier);
  if (aliases.some((value) => !value || value !== job)) return undefined;
  return {
    provider: "lever",
    host,
    tenant,
    job,
    purpose,
    variant: applicationTarget ? "lever_application" : "lever_posting",
    providerJobKey: `lever:${host}:${tenant}:${job}`,
  };
}

function queryAliasValues(url: URL, aliases: ReadonlySet<string>): string[] {
  const values: string[] = [];
  for (const [key, value] of url.searchParams) {
    if (aliases.has(key.toLowerCase())) values.push(value);
  }
  return values;
}

function oneIdentifier(values: Array<string | undefined>): string | undefined {
  const present = values.filter((value): value is string => value !== undefined);
  if (present.length < 1) return undefined;
  const checked = present.map(identifier);
  if (!checked[0] || checked.some((value) => value !== checked[0])) {
    return undefined;
  }
  return checked[0];
}

function identifier(value: string | undefined): string | undefined {
  return value && /^[A-Za-z0-9_-]{1,160}$/u.test(value) ? value : undefined;
}
