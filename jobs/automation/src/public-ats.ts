import type {
  AtsKind,
  DiscoveryPage,
  DiscoveryProvider,
  DiscoveryQuery,
  NormalizedJob,
  PublicAtsSource,
} from "./contracts.js";
import { submissionPolicy } from "./policy.js";

const MAX_RESPONSE_BYTES = 5 * 1024 * 1024;
const SAFE_IDENTIFIER = /^[a-zA-Z0-9_-]+$/;

export interface FetchResponse {
  ok: boolean;
  status: number;
  headers?: Pick<Headers, "get">;
  body?: ReadableStream<Uint8Array> | null;
  text(): Promise<string>;
}

export type JobsFetch = (url: string, init: RequestInit) => Promise<FetchResponse>;

export interface PublicAtsOptions {
  fetch?: JobsFetch;
  timeoutMs?: number;
  maxAttempts?: number;
  maxPages?: number;
  sleep?: (milliseconds: number) => Promise<void>;
}

interface RawJob {
  externalId: string;
  canonicalUrl: string;
  company: string;
  title: string;
  location: string;
  workplace?: string;
  description?: string;
  postedAt?: string;
  compensation?: string;
  department?: string;
}

export class IncompletePublicAtsSnapshotError extends Error {
  constructor(provider: "smartrecruiters" | "workday") {
    super(`${provider} snapshot reached the bounded pagination cap before completion`);
    this.name = "IncompletePublicAtsSnapshotError";
  }
}

/**
 * Public ATS discovery adapted from career-ops provider patterns. Each request is
 * host-pinned, redirect-free, bounded, and normalized before it reaches Bluey.
 */
export class PublicAtsDiscoveryProvider implements DiscoveryProvider {
  readonly name = "public-ats";
  private readonly fetcher: JobsFetch;
  private readonly timeoutMs: number;
  private readonly maxAttempts: number;
  private readonly maxPages: number;
  private readonly sleep: (milliseconds: number) => Promise<void>;

  constructor(options: PublicAtsOptions = {}) {
    this.fetcher = options.fetch ?? ((url, init) => fetch(url, init));
    this.timeoutMs = options.timeoutMs ?? 8_000;
    this.maxAttempts = options.maxAttempts ?? 3;
    this.maxPages = options.maxPages ?? 5;
    this.sleep = options.sleep ?? ((milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
  }

  async search(query: DiscoveryQuery): Promise<DiscoveryPage> {
    const sources = query.sources ?? [];
    const results = await Promise.allSettled(sources.map((source) => this.searchSource(source, query)));
    const pages = results
      .filter((result): result is PromiseFulfilledResult<NormalizedJob[]> => result.status === "fulfilled")
      .map((result) => result.value);
    const warnings = results.flatMap((result, index) => result.status === "rejected"
      ? [`${sources[index]?.kind ?? "ATS"} source failed: ${errorMessage(result.reason)}`]
      : []);
    if (sources.length > 0 && pages.length === 0) {
      throw new AggregateError(
        results.filter((result): result is PromiseRejectedResult => result.status === "rejected").map((result) => result.reason),
        `All configured ATS sources failed: ${warnings.join("; ")}`,
      );
    }
    const deduplicated = deduplicateJobs(pages.flat());
    const maximumAgeDays = clamp(query.maxPostingAgeDays ?? 14, 1, 60);
    const recentJobs = deduplicated.filter((job) => isRecentJob(job, maximumAgeDays));
    const staleCount = deduplicated.length - recentJobs.length;
    if (staleCount > 0) {
      warnings.push(`${staleCount} old or undated job${staleCount === 1 ? " was" : "s were"} skipped.`);
    }
    const jobs = recentJobs
      .filter((job) => job.title.length > 0 && job.canonicalUrl.length > 0)
      .filter((job) => matchesQuery(job, query));
    return {
      jobs: jobs.slice(0, clamp(query.pageSize ?? 100, 1, 250)),
      warnings: warnings.length ? warnings : undefined,
    };
  }

  /**
   * Returns one provider's complete public snapshot without user filters,
   * freshness filtering, or page-size truncation. Scheduled discovery uses
   * this path so an omitted job can be treated as provider evidence instead
   * of a search-filter side effect.
   */
  async snapshot(source: PublicAtsSource): Promise<NormalizedJob[]> {
    const jobs = await this.searchSource(source, {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [source],
    }, true);
    return deduplicateJobs(jobs)
      .filter((job) => job.externalId.length > 0)
      .filter((job) => job.title.length > 0 && job.canonicalUrl.length > 0);
  }

  private async searchSource(
    source: PublicAtsSource,
    query: DiscoveryQuery,
    requireCompleteSnapshot = false,
  ): Promise<NormalizedJob[]> {
    switch (source.kind) {
      case "greenhouse":
        return this.searchGreenhouse(source);
      case "lever":
        return this.searchLever(source);
      case "ashby":
        return this.searchAshby(source);
      case "smartrecruiters":
        return this.searchSmartRecruiters(source, requireCompleteSnapshot);
      case "workday":
        return this.searchWorkday(source, query, requireCompleteSnapshot);
    }
  }

  private async searchGreenhouse(source: Extract<PublicAtsSource, { kind: "greenhouse" }>): Promise<NormalizedJob[]> {
    assertIdentifier(source.boardToken, "Greenhouse board token");
    const host = "boards-api.greenhouse.io";
    const payload = asRecord(await this.requestJson(
      `https://${host}/v1/boards/${encodeURIComponent(source.boardToken)}/jobs?content=true`,
      [host],
    ));
    return asArray(payload.jobs).map((value) => {
      const item = asRecord(value);
      const externalId = asString(item.id);
      return normalizeJob("greenhouse", {
        externalId,
        canonicalUrl: externalId
          ? `https://boards.greenhouse.io/${encodeURIComponent(source.boardToken)}/jobs/${encodeURIComponent(externalId)}`
          : "",
        company: source.company ?? humanizeIdentifier(source.boardToken),
        title: asString(item.title || item.name),
        location: asString(asRecord(item.location).name),
        workplace: asString(item.workplace_type),
        description: asString(item.content),
        postedAt: asOptionalString(item.updated_at),
        department: asArray(item.departments).map((department) => asString(asRecord(department).name)).filter(Boolean).join(", "),
      });
    });
  }

  private async searchLever(source: Extract<PublicAtsSource, { kind: "lever" }>): Promise<NormalizedJob[]> {
    assertIdentifier(source.site, "Lever site");
    const host = "api.lever.co";
    const payload = asArray(await this.requestJson(
      `https://${host}/v0/postings/${encodeURIComponent(source.site)}?mode=json`,
      [host],
    ));
    return payload.map((value) => {
      const item = asRecord(value);
      const categories = asRecord(item.categories);
      const externalId = asString(item.id);
      return normalizeJob("lever", {
        externalId,
        canonicalUrl: externalId
          ? `https://jobs.lever.co/${encodeURIComponent(source.site)}/${encodeURIComponent(externalId)}`
          : "",
        company: source.company ?? humanizeIdentifier(source.site),
        title: asString(item.text),
        location: asString(categories.location || item.location),
        workplace: asString(item.workplaceType),
        description: leverDescription(item),
        postedAt: asOptionalString(item.createdAt),
        compensation: leverCompensation(item.salaryRange),
        department: asString(categories.department || categories.team),
      });
    });
  }

  private async searchAshby(source: Extract<PublicAtsSource, { kind: "ashby" }>): Promise<NormalizedJob[]> {
    assertIdentifier(source.boardName, "Ashby board name");
    const host = "api.ashbyhq.com";
    const payload = asRecord(await this.requestJson(
      `https://${host}/posting-api/job-board/${encodeURIComponent(source.boardName)}`,
      [host],
    ));
    return asArray(payload.jobs)
      .filter((value) => asRecord(value).isListed !== false)
      .map((value) => {
        const item = asRecord(value);
        return normalizeJob("ashby", {
          externalId: asString(item.id || item.jobId),
          canonicalUrl: asString(item.jobUrl || item.applyUrl),
          company: source.company ?? humanizeIdentifier(source.boardName),
          title: asString(item.title),
          location: asString(item.location),
          workplace: asString(item.workplaceType),
          description: asString(item.descriptionPlain || item.descriptionHtml),
          postedAt: asOptionalString(item.publishedAt),
          department: asOptionalString(item.department),
        });
      });
  }

  private async searchSmartRecruiters(
    source: Extract<PublicAtsSource, { kind: "smartrecruiters" }>,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    assertIdentifier(source.companyIdentifier, "SmartRecruiters company identifier");
    const host = "api.smartrecruiters.com";
    const jobs: NormalizedJob[] = [];
    let offset = 0;
    const limit = 100;
    for (let page = 0; page < this.maxPages; page += 1) {
      const payload = asRecord(await this.requestJson(
        `https://${host}/v1/companies/${encodeURIComponent(source.companyIdentifier)}/postings?limit=${limit}&offset=${offset}`,
        [host],
      ));
      const content = asArray(payload.content);
      for (const value of content) {
        const item = asRecord(value);
        const location = asRecord(item.location);
        const company = asRecord(item.company);
        const externalId = asString(item.id);
        jobs.push(normalizeJob("smartrecruiters", {
          externalId,
          canonicalUrl: smartrecruitersCanonicalUrl(source.companyIdentifier, externalId),
          company: source.company || asString(company.name) || humanizeIdentifier(source.companyIdentifier),
          title: asString(item.name),
          location: [location.city, location.region, location.country].map(asOptionalString).filter(Boolean).join(", "),
          workplace: location.remote === true ? "remote" : asString(item.workplaceType),
          description: extractSmartRecruitersDescription(item),
          postedAt: asOptionalString(item.releasedDate),
          department: asOptionalString(asRecord(item.department).label),
        }));
      }
      offset += content.length;
      const total = asNumber(payload.totalFound);
      const hasMore = total === undefined ? content.length === limit : offset < total;
      if (!hasMore) break;
      if (page === this.maxPages - 1) {
        if (requireCompleteSnapshot) throw new IncompletePublicAtsSnapshotError("smartrecruiters");
        break;
      }
    }
    return jobs;
  }

  private async searchWorkday(
    source: Extract<PublicAtsSource, { kind: "workday" }>,
    query: DiscoveryQuery,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    assertIdentifier(source.tenant, "Workday tenant");
    assertIdentifier(source.instance, "Workday instance");
    assertIdentifier(source.site, "Workday site");
    const locale = source.locale && SAFE_IDENTIFIER.test(source.locale) ? source.locale : "en-US";
    const host = `${source.tenant}.${source.instance}.myworkdayjobs.com`;
    const endpoint = `https://${host}/wday/cxs/${encodeURIComponent(source.tenant)}/${encodeURIComponent(source.site)}/jobs`;
    const jobs: NormalizedJob[] = [];
    const limit = 20;
    let offset = 0;
    for (let page = 0; page < this.maxPages; page += 1) {
      const payload = asRecord(await this.requestJson(endpoint, [host], {
        method: "POST",
        headers: { "content-type": "application/json", "accept-language": locale },
        body: JSON.stringify({ appliedFacets: {}, limit, offset, searchText: query.roles.join(" ") }),
      }));
      const postings = asArray(payload.jobPostings);
      for (const value of postings) {
        const item = asRecord(value);
        const externalPath = asString(item.externalPath);
        jobs.push(normalizeJob("workday", {
          externalId: asString(item.bulletFields ? asArray(item.bulletFields)[0] : externalPath),
          canonicalUrl: workdayCanonicalUrl(host, locale, source.site, externalPath),
          company: source.company ?? humanizeIdentifier(source.tenant),
          title: asString(item.title),
          location: asString(item.locationsText),
          workplace: asString(item.workplaceType),
          description: asString(item.descriptionPreview),
          postedAt: asOptionalString(item.postedOn),
        }));
      }
      offset += postings.length;
      const total = asNumber(payload.total);
      const hasMore = total === undefined ? postings.length === limit : offset < total;
      if (!hasMore) break;
      if (page === this.maxPages - 1) {
        if (requireCompleteSnapshot) throw new IncompletePublicAtsSnapshotError("workday");
        break;
      }
    }
    return jobs;
  }

  private async requestJson(url: string, allowedHosts: string[], init: RequestInit = {}): Promise<unknown> {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" || !allowedHosts.includes(parsed.hostname)) {
      throw new Error("ATS request target is not allowed");
    }

    let lastError: Error | undefined;
    for (let attempt = 1; attempt <= this.maxAttempts; attempt += 1) {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), this.timeoutMs);
      try {
        const response = await this.fetcher(parsed.toString(), {
          ...init,
          redirect: "error",
          signal: controller.signal,
        });
        const body = await boundedResponseText(response);
        if (!response.ok) {
          if ((response.status === 429 || response.status >= 500) && attempt < this.maxAttempts) {
            await this.sleep(100 * 2 ** (attempt - 1));
            continue;
          }
          throw new Error(`ATS request failed with status ${response.status}`);
        }
        return JSON.parse(body) as unknown;
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        if (attempt < this.maxAttempts) {
          await this.sleep(100 * 2 ** (attempt - 1));
          continue;
        }
      } finally {
        clearTimeout(timer);
      }
      break;
    }
    throw lastError ?? new Error("ATS request failed");
  }
}

function smartrecruitersCanonicalUrl(companyIdentifier: string, externalId: string): string {
  if (!SAFE_IDENTIFIER.test(externalId)) return "";
  return `https://jobs.smartrecruiters.com/${encodeURIComponent(companyIdentifier)}/${encodeURIComponent(externalId)}`;
}

function workdayCanonicalUrl(host: string, locale: string, site: string, externalPath: string): string {
  let url: URL;
  try {
    url = new URL(externalPath, `https://${host}`);
  } catch {
    return "";
  }
  if (
    url.protocol !== "https:"
    || url.username
    || url.password
    || url.port
    || url.hostname !== host.toLowerCase()
  ) {
    return "";
  }

  if (url.pathname.startsWith("/job/")) {
    url.pathname = `/${encodeURIComponent(locale)}/${encodeURIComponent(site)}${url.pathname}`;
  } else {
    const segments = url.pathname.split("/");
    const jobIndex = segments.indexOf("job");
    if (jobIndex < 1 || jobIndex === segments.length - 1 || segments[jobIndex - 1] !== site) {
      return "";
    }
  }
  url.hash = "";
  return url.toString();
}

async function boundedResponseText(response: FetchResponse): Promise<string> {
  const declaredLength = Number(response.headers?.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > MAX_RESPONSE_BYTES) {
    throw new Error("ATS response exceeded the size limit");
  }
  if (!response.body) {
    const body = await response.text();
    if (Buffer.byteLength(body, "utf8") > MAX_RESPONSE_BYTES) {
      throw new Error("ATS response exceeded the size limit");
    }
    return body;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const chunks: string[] = [];
  let received = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      if (received > MAX_RESPONSE_BYTES) {
        await reader.cancel("response limit exceeded");
        throw new Error("ATS response exceeded the size limit");
      }
      chunks.push(decoder.decode(value, { stream: true }));
    }
    chunks.push(decoder.decode());
    return chunks.join("");
  } finally {
    reader.releaseLock();
  }
}

export function deduplicateJobs(jobs: NormalizedJob[]): NormalizedJob[] {
  const seen = new Set<string>();
  return jobs.filter((job) => {
    const key = [job.company, job.title, job.location, canonicalizeUrl(job.canonicalUrl)]
      .map(normalizeComparable)
      .join("|");
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function normalizeJob(source: AtsKind, raw: RawJob): NormalizedJob {
  const canonicalUrl = canonicalizeUrl(raw.canonicalUrl);
  return {
    externalId: raw.externalId || canonicalUrl,
    canonicalUrl,
    company: raw.company.trim(),
    title: raw.title.trim(),
    location: raw.location.trim() || "Location not listed",
    workplace: inferWorkplace(raw.workplace, raw.location),
    description: stripMarkup(raw.description ?? ""),
    source,
    postedAt: raw.postedAt,
    compensation: raw.compensation,
    department: raw.department || undefined,
  };
}

function leverCompensation(value: unknown): string | undefined {
  const range = asRecord(value);
  const minimum = Number(range.min);
  const maximum = Number(range.max);
  if (!Number.isFinite(minimum) && !Number.isFinite(maximum)) return undefined;
  const currency = asString(range.currency) || "USD";
  const interval = asString(range.interval);
  const bounds = Number.isFinite(minimum) && Number.isFinite(maximum)
    ? `${Math.round(minimum)}-${Math.round(maximum)}`
    : String(Math.round(Number.isFinite(minimum) ? minimum : maximum));
  return `${currency} ${bounds}${interval ? ` ${interval}` : ""}`;
}

function leverDescription(item: Record<string, unknown>): string {
  const sections = [asString(item.descriptionPlain || item.description)];
  for (const value of asArray(item.lists)) {
    const section = asRecord(value);
    sections.push(asString(section.text), asString(section.content));
  }
  sections.push(asString(item.additionalPlain || item.additional));
  return sections.filter(Boolean).join("\n\n");
}

function matchesQuery(job: NormalizedJob, query: DiscoveryQuery): boolean {
  const company = normalizeComparable(job.company);
  const title = normalizeComparable(job.title);
  const location = normalizeComparable(job.location);
  const excludedCompanies = normalizedValues(query.excludedCompanies);
  const excludedTitles = normalizedValues(query.excludedTitles ?? []);
  const roles = normalizedValues(query.roles);
  const locations = normalizedValues(query.locations);
  if (excludedCompanies.some((value) => company.includes(value))) return false;
  if (excludedTitles.some((value) => title.includes(value))) return false;
  if (roles.length && !roles.some((value) => title.includes(value))) return false;
  if (locations.length && job.workplace !== "remote" && !locations.some((value) => location.includes(value))) {
    return false;
  }
  if (query.remotePreference === "remote_only" && job.workplace !== "remote") return false;
  return true;
}

function extractSmartRecruitersDescription(item: Record<string, unknown>): string {
  const sections = asRecord(asRecord(item.jobAd).sections);
  return Object.values(sections)
    .map((section) => asString(asRecord(section).text || asRecord(section).description))
    .filter(Boolean)
    .join("\n\n");
}

function inferWorkplace(value: string | undefined, location: string): NormalizedJob["workplace"] {
  const combined = `${value ?? ""} ${location}`.toLowerCase();
  if (combined.includes("remote")) return "remote";
  if (combined.includes("hybrid")) return "hybrid";
  if (combined.includes("on-site") || combined.includes("onsite")) return "onsite";
  return "unknown";
}

function canonicalizeUrl(value: string): string {
  if (submissionPolicy(value).policy === "blocked") return "";
  try {
    const url = new URL(value);
    url.hash = "";
    for (const key of [...url.searchParams.keys()]) {
      if (/^(utm_|source$|sourceid$|gh_src$)/i.test(key)) url.searchParams.delete(key);
    }
    return url.toString();
  } catch {
    return value.trim();
  }
}

function errorMessage(value: unknown): string {
  return value instanceof Error ? value.message : String(value);
}

function stripMarkup(value: string): string {
  return value
    .replace(/<script[\s\S]*?<\/script>/gi, " ")
    .replace(/<style[\s\S]*?<\/style>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&#39;/g, "'")
    .replace(/&quot;/gi, '"')
    .replace(/\s+/g, " ")
    .trim();
}

function assertIdentifier(value: string, label: string): void {
  if (!SAFE_IDENTIFIER.test(value)) throw new Error(`${label} contains unsupported characters`);
}

function humanizeIdentifier(value: string): string {
  return value.replace(/[-_]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function normalizeComparable(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function normalizedValues(values: string[]): string[] {
  return values.map(normalizeComparable).filter(Boolean);
}

export function isRecentJob(job: NormalizedJob, maximumAgeDays: number, now = new Date()): boolean {
  const postedAt = parsePostedAt(job.postedAt, now);
  if (!postedAt) return false;
  const ageMs = Math.max(0, now.getTime() - postedAt.getTime());
  return ageMs <= clamp(maximumAgeDays, 1, 60) * 24 * 60 * 60 * 1_000;
}

export function parsePostedAt(value: string | undefined, now = new Date()): Date | undefined {
  const raw = value?.trim();
  if (!raw) return undefined;
  if (/^\d{10,13}$/.test(raw)) {
    const numeric = Number(raw);
    const date = new Date(raw.length === 10 ? numeric * 1_000 : numeric);
    return Number.isNaN(date.getTime()) ? undefined : date;
  }
  const normalized = raw.toLowerCase();
  if (normalized.includes("today")) return new Date(now);
  if (normalized.includes("yesterday")) return new Date(now.getTime() - 24 * 60 * 60 * 1_000);
  const relative = normalized.match(/(\d+)\+?\s*(hour|day|week)s?\s*ago/);
  if (relative) {
    const count = Number(relative[1]);
    const unitMs = relative[2] === "hour"
      ? 60 * 60 * 1_000
      : relative[2] === "week"
        ? 7 * 24 * 60 * 60 * 1_000
        : 24 * 60 * 60 * 1_000;
    return new Date(now.getTime() - count * unitMs);
  }
  const parsed = new Date(raw);
  return Number.isNaN(parsed.getTime()) ? undefined : parsed;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function asString(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return "";
}

function asOptionalString(value: unknown): string | undefined {
  const result = asString(value).trim();
  return result || undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
