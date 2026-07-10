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
    const jobs = deduplicateJobs(pages.flat())
      .filter((job) => job.title.length > 0 && job.canonicalUrl.length > 0)
      .filter((job) => matchesQuery(job, query));
    return {
      jobs: jobs.slice(0, clamp(query.pageSize ?? 100, 1, 250)),
      warnings: warnings.length ? warnings : undefined,
    };
  }

  private async searchSource(source: PublicAtsSource, query: DiscoveryQuery): Promise<NormalizedJob[]> {
    switch (source.kind) {
      case "greenhouse":
        return this.searchGreenhouse(source);
      case "lever":
        return this.searchLever(source);
      case "ashby":
        return this.searchAshby(source);
      case "smartrecruiters":
        return this.searchSmartRecruiters(source);
      case "workday":
        return this.searchWorkday(source, query);
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
      return normalizeJob("greenhouse", {
        externalId: asString(item.id),
        canonicalUrl: asString(item.absolute_url),
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
      return normalizeJob("lever", {
        externalId: asString(item.id),
        canonicalUrl: asString(item.hostedUrl || item.applyUrl),
        company: source.company ?? humanizeIdentifier(source.site),
        title: asString(item.text),
        location: asString(categories.location || item.location),
        workplace: asString(item.workplaceType),
        description: asString(item.descriptionPlain || item.description),
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
        const sourceUrl = asString(item.ref || item.postingUrl);
        jobs.push(normalizeJob("smartrecruiters", {
          externalId: asString(item.id),
          canonicalUrl: sourceUrl.startsWith("http")
            ? sourceUrl
            : `https://jobs.smartrecruiters.com/${source.companyIdentifier}/${asString(item.id)}`,
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
      if (content.length < limit || (total !== undefined && offset >= total)) break;
    }
    return jobs;
  }

  private async searchWorkday(
    source: Extract<PublicAtsSource, { kind: "workday" }>,
    query: DiscoveryQuery,
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
          canonicalUrl: externalPath.startsWith("http") ? externalPath : `https://${host}${externalPath}`,
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
      if (postings.length < limit || (total !== undefined && offset >= total)) break;
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
        const body = await response.text();
        if (body.length > MAX_RESPONSE_BYTES) throw new Error("ATS response exceeded the size limit");
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
