import { createHash } from "node:crypto";
import type {
  AtsKind,
  DiscoveryPage,
  DiscoveryProvider,
  DiscoveryQuery,
  NormalizedJob,
  PublicAtsSource,
} from "./contracts.js";
import {
  fingerprintDescription,
  fingerprintSimilarity,
} from "./job-source-intelligence.js";
import { submissionPolicy } from "./policy.js";

const MAX_RESPONSE_BYTES = 5 * 1024 * 1024;
const SAFE_IDENTIFIER = /^[a-zA-Z0-9_-]+$/;
const CURSOR_VERSION = 2;
// Continuations carry exact opaque dedupe and cross-list evidence. The entry
// caps prevent unbounded history; 192 KiB accommodates one 500-row provider
// window plus source state while still rejecting oversized attacker input.
const MAX_CURSOR_LENGTH = 192 * 1024;
const MAX_CURSOR_HISTORY_ENTRIES = 512;
const MAX_PUBLIC_ATS_SOURCES = 24;
const SMARTRECRUITERS_OVERLAP_ROWS = 50;
const WORKDAY_OVERLAP_ROWS = 10;
const CROSS_LISTING_THRESHOLD = 0.92;
const SHA256_HEX = /^[a-f0-9]{64}$/;
const SHA256_BASE64URL = /^[a-zA-Z0-9_-]{43}$/;
const CROSS_LISTING_CANDIDATE =
  /^[a-zA-Z0-9_-]{43}\.[a-zA-Z0-9_-]{43}\.[a-f0-9]{16}$/;

interface PublicAtsSourceCursor {
  offset: number;
  advertisedTotal: number | null;
  prefixSha256: string | null;
  validationOffset: number;
  validationSha256: string | null;
  overlapCount: number;
  overlapSha256: string | null;
  done: boolean;
}

interface PublicAtsHistoryCursor {
  seenJobSha256: string[];
  crossListingCandidates: string[];
  seenCrossListingSha256: string[];
}

interface PublicAtsCursor {
  version: typeof CURSOR_VERSION;
  pageOffset: number;
  querySha256: string;
  windowSha256: string | null;
  sources: PublicAtsSourceCursor[];
  history: PublicAtsHistoryCursor;
  checksumSha256: string;
}

interface PublicAtsSourceWindow {
  jobs: NormalizedJob[];
  nextState: PublicAtsSourceCursor;
  warning?: string;
}

export interface FetchResponse {
  ok: boolean;
  status: number;
  headers?: Pick<Headers, "get">;
  body?: ReadableStream<Uint8Array> | null;
  text(): Promise<string>;
}

export type JobsFetch = (
  url: string,
  init: RequestInit,
) => Promise<FetchResponse>;

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
  employmentType?: string;
  engagementType?: string;
}

export class IncompletePublicAtsSnapshotError extends Error {
  readonly partialJobs: NormalizedJob[];

  constructor(
    provider: "smartrecruiters" | "workday",
    partialJobs: NormalizedJob[] = [],
  ) {
    super(
      `${provider} feed reached the bounded pagination cap before completion; results are partial`,
    );
    this.name = "IncompletePublicAtsSnapshotError";
    this.partialJobs = partialJobs;
  }
}

export class InvalidPublicAtsSnapshotError extends Error {
  constructor(provider: PublicAtsSource["kind"], rowIndex?: number) {
    super(
      rowIndex === undefined
        ? `${provider} snapshot payload did not contain the expected job list`
        : `${provider} snapshot contained an invalid listed row at index ${rowIndex}`,
    );
    this.name = "InvalidPublicAtsSnapshotError";
  }
}

export class InvalidPublicAtsCursorError extends Error {
  constructor() {
    super("Public ATS cursor is invalid or stale");
    this.name = "InvalidPublicAtsCursorError";
  }
}

export class PublicAtsContinuationHistoryLimitError extends Error {
  constructor() {
    super(
      "Public ATS continuation exceeded its bounded exact-history limit; narrow the configured sources",
    );
    this.name = "PublicAtsContinuationHistoryLimitError";
  }
}

/**
 * Public ATS discovery adapted from career-ops provider patterns. Each request is
 * host-pinned, redirect-free, bounded, and normalized before it reaches Bluey.
 * Public feeds do not expose a stable snapshot token: ordered-prefix validation
 * detects changes observed while each prefix page is fetched, but cannot detect
 * a provider mutation after an already-validated page and before acquisition.
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
    this.maxPages = normalizeMaxPages(options.maxPages);
    this.sleep =
      options.sleep ??
      ((milliseconds) =>
        new Promise((resolve) => setTimeout(resolve, milliseconds)));
  }

  async search(query: DiscoveryQuery): Promise<DiscoveryPage> {
    const sources = query.sources ?? [];
    assertPublicAtsSourceLimit(sources);
    const pageSize = normalizePageSize(query.pageSize);
    const maximumAgeDays = clamp(query.maxPostingAgeDays ?? 14, 1, 60);
    const querySha256 = publicAtsQuerySha256(
      query,
      pageSize,
      maximumAgeDays,
      this.maxPages,
    );
    const cursor = decodePublicAtsCursor(query.cursor);
    if (cursor && cursor.querySha256 !== querySha256) {
      throw new InvalidPublicAtsCursorError();
    }

    const sourceStates =
      cursor?.sources ?? sources.map(() => initialPublicAtsSourceCursor());
    const history = cursor?.history ?? initialPublicAtsHistoryCursor();
    validatePublicAtsSourceCursors(sourceStates, sources);
    if (cursor && sourceStates.every((state) => state.done)) {
      throw new InvalidPublicAtsCursorError();
    }
    if (
      cursor?.pageOffset === 0 &&
      sourceStates.some((state) => !state.done && state.offset === 0)
    ) {
      throw new InvalidPublicAtsCursorError();
    }
    const results = await Promise.allSettled(
      sources.map((source, index) =>
        this.searchSourceWindow(source, sourceStates[index]!),
      ),
    );
    const structuralFailure = results.find(
      (result): result is PromiseRejectedResult =>
        result.status === "rejected" &&
        (result.reason instanceof InvalidPublicAtsCursorError ||
          result.reason instanceof PublicAtsContinuationHistoryLimitError),
    );
    if (structuralFailure) throw structuralFailure.reason;
    // Promise.allSettled preserves source-index order even when requests finish
    // out of order, and each adapter preserves the provider's advertised row
    // order. The cursor binds each complete acquisition window; an upstream
    // reorder makes an in-window continuation stale, while window-to-window
    // progress revalidates the exact ordered prefix, trailing overlap, and
    // advertised total within the provider's no-snapshot TOCTOU boundary.
    const pages = results.flatMap((result) =>
      result.status === "fulfilled" ? [result.value.jobs] : [],
    );
    const warnings = results.flatMap((result, index) =>
      result.status === "rejected"
        ? [`${sources[index]?.kind ?? "ATS"} source failed: ${errorMessage(result.reason)}`]
        : result.value.warning
          ? [result.value.warning]
          : [],
    );
    const attemptedSourceIndexes = sourceStates.flatMap((state, index) =>
      state.done ? [] : [index],
    );
    const anyAttemptSucceeded = attemptedSourceIndexes.some(
      (index) => results[index]?.status === "fulfilled",
    );
    if (attemptedSourceIndexes.length > 0 && !anyAttemptSucceeded) {
      throw new AggregateError(
        results
          .filter(
            (result): result is PromiseRejectedResult =>
              result.status === "rejected",
          )
          .map((result) => result.reason),
        `All configured ATS sources failed: ${warnings.join("; ")}`,
      );
    }
    const nextSourceStates = results.map((result, index) =>
      result.status === "fulfilled"
        ? result.value.nextState
        : { ...sourceStates[index]!, done: true },
    );
    const acquiredJobs = pages.flat();
    const recentJobs = acquiredJobs.filter((job) =>
      isRecentJob(job, maximumAgeDays),
    );
    const staleCount = acquiredJobs.length - recentJobs.length;
    if (staleCount > 0) {
      warnings.push(
        `${staleCount} old or undated job${staleCount === 1 ? " was" : "s were"} skipped.`,
      );
    }
    const eligibleJobs = recentJobs
      .filter((job) => job.title.length > 0 && job.canonicalUrl.length > 0)
      .filter((job) => matchesQuery(job, query));
    const seenJobSha256 = new Set(history.seenJobSha256);
    const jobs = deduplicateJobs(eligibleJobs).filter(
      (job) => !seenJobSha256.has(publicAtsDedupeSha256(job)),
    );
    const currentSeenJobSha256 = jobs.map(publicAtsDedupeSha256);
    const crossListingState = findNewCrossListingState(jobs, history);
    if (crossListingState.newSignalSha256.length > 0) {
      const count = crossListingState.newSignalSha256.length;
      warnings.push(publicAtsCrossListingWarning(count));
    }
    const nextHistory = advancePublicAtsHistory(
      history,
      currentSeenJobSha256,
      crossListingState.currentCandidates,
      crossListingState.newSignalSha256,
    );

    const windowSha256 = publicAtsSnapshotSha256(
      jobs,
      warnings,
      nextSourceStates,
      nextHistory,
    );
    const pageOffset = cursor?.pageOffset ?? 0;
    if (
      cursor?.windowSha256 &&
      (cursor.windowSha256 !== windowSha256 ||
        pageOffset >= jobs.length ||
        pageOffset % pageSize !== 0)
    ) {
      throw new InvalidPublicAtsCursorError();
    }
    const pageEnd = Math.min(pageOffset + pageSize, jobs.length);
    const hasMoreInWindow = pageEnd < jobs.length;
    const hasMoreUpstream = nextSourceStates.some((state) => !state.done);
    if (hasMoreUpstream) assertPublicAtsHistoryBounded(nextHistory);
    let nextCursor: string | undefined;
    if (hasMoreInWindow) {
      nextCursor = encodePublicAtsCursor({
        version: CURSOR_VERSION,
        pageOffset: pageEnd,
        querySha256,
        windowSha256,
        sources: sourceStates,
        history,
      });
    } else if (hasMoreUpstream) {
      nextCursor = encodePublicAtsCursor({
        version: CURSOR_VERSION,
        pageOffset: 0,
        querySha256,
        windowSha256: null,
        sources: nextSourceStates,
        history: nextHistory,
      });
    }
    if (nextCursor) {
      warnings.push(
        publicAtsPaginationWarning(
          pageOffset,
          pageEnd,
          jobs.length,
          hasMoreInWindow,
        ),
      );
    }
    return {
      jobs: jobs.slice(pageOffset, pageEnd),
      ...(nextCursor ? { nextCursor } : {}),
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
    const jobs = await this.searchSource(
      source,
      {
        roles: [],
        locations: [],
        remotePreference: "any",
        excludedCompanies: [],
        sources: [source],
      },
      true,
    );
    const invalidRow = jobs.findIndex(
      (job) =>
        job.externalId.length === 0 ||
        job.title.length === 0 ||
        job.canonicalUrl.length === 0,
    );
    if (invalidRow >= 0) {
      // A scheduled snapshot is closure evidence. Silently dropping even one
      // provider-listed row would turn a parser/URL failure into false proof
      // that the corresponding job closed. Search mode remains best-effort,
      // but scheduled publication must fail closed and preserve the last good
      // snapshot in full.
      throw new InvalidPublicAtsSnapshotError(source.kind, invalidRow);
    }
    // Do not search-deduplicate a closure-authoritative snapshot. The server
    // owns external-ID/canonical reconciliation and rejects conflicting rows
    // atomically; dropping a row here would erase that evidence.
    return jobs;
  }

  private async searchSourceWindow(
    source: PublicAtsSource,
    state: PublicAtsSourceCursor,
  ): Promise<PublicAtsSourceWindow> {
    if (state.done) return { jobs: [], nextState: state };
    switch (source.kind) {
      case "greenhouse":
        return {
          jobs: await this.searchGreenhouse(source, false),
          nextState: { ...state, done: true },
        };
      case "lever":
        return {
          jobs: await this.searchLever(source, false),
          nextState: { ...state, done: true },
        };
      case "ashby":
        return {
          jobs: await this.searchAshby(source, false),
          nextState: { ...state, done: true },
        };
      case "smartrecruiters":
        return this.searchSmartRecruitersWindow(source, state, false);
      case "workday":
        return this.searchWorkdayWindow(source, state, false);
    }
  }

  private async searchSource(
    source: PublicAtsSource,
    _query: DiscoveryQuery,
    requireCompleteSnapshot = false,
  ): Promise<NormalizedJob[]> {
    switch (source.kind) {
      case "greenhouse":
        return this.searchGreenhouse(source, requireCompleteSnapshot);
      case "lever":
        return this.searchLever(source, requireCompleteSnapshot);
      case "ashby":
        return this.searchAshby(source, requireCompleteSnapshot);
      case "smartrecruiters":
        return this.searchSmartRecruiters(source, requireCompleteSnapshot);
      case "workday":
        return this.searchWorkday(source, requireCompleteSnapshot);
    }
  }

  private async searchGreenhouse(
    source: Extract<PublicAtsSource, { kind: "greenhouse" }>,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    assertIdentifier(source.boardToken, "Greenhouse board token");
    const host = "boards-api.greenhouse.io";
    const payload = asRecord(
      await this.requestJson(
        `https://${host}/v1/boards/${encodeURIComponent(source.boardToken)}/jobs?content=true`,
        [host],
      ),
    );
    if (requireCompleteSnapshot && !Array.isArray(payload.jobs)) {
      throw new InvalidPublicAtsSnapshotError("greenhouse");
    }
    return asArray(payload.jobs).map((value) => {
      const item = asRecord(value);
      const externalId = asString(item.id);
      return normalizeJob("greenhouse", {
        externalId,
        canonicalUrl: externalId
          ? `https://boards.greenhouse.io/${encodeURIComponent(
              source.boardToken,
            )}/jobs/${encodeURIComponent(externalId)}`
          : "",
        company: source.company ?? humanizeIdentifier(source.boardToken),
        title: asString(item.title || item.name),
        location: asString(asRecord(item.location).name),
        workplace: asString(item.workplace_type),
        description: asString(item.content),
        postedAt: asOptionalString(item.updated_at),
        department: asArray(item.departments)
          .map((department) => asString(asRecord(department).name))
          .filter(Boolean)
          .join(", "),
        employmentType: combineTypedFields(
          item.employment_type,
          item.employmentType,
        ),
      });
    });
  }

  private async searchLever(
    source: Extract<PublicAtsSource, { kind: "lever" }>,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    assertIdentifier(source.site, "Lever site");
    const host = "api.lever.co";
    const rawPayload = await this.requestJson(
      `https://${host}/v0/postings/${encodeURIComponent(source.site)}?mode=json`,
      [host],
    );
    if (requireCompleteSnapshot && !Array.isArray(rawPayload)) {
      throw new InvalidPublicAtsSnapshotError("lever");
    }
    const payload = asArray(rawPayload);
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
        employmentType: asOptionalString(categories.commitment),
        engagementType: combineTypedFields(
          categories.engagement,
          item.engagementType,
        ),
      });
    });
  }

  private async searchAshby(
    source: Extract<PublicAtsSource, { kind: "ashby" }>,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    assertIdentifier(source.boardName, "Ashby board name");
    const host = "api.ashbyhq.com";
    const payload = asRecord(
      await this.requestJson(
        `https://${host}/posting-api/job-board/${encodeURIComponent(source.boardName)}`,
        [host],
      ),
    );
    if (requireCompleteSnapshot && !Array.isArray(payload.jobs)) {
      throw new InvalidPublicAtsSnapshotError("ashby");
    }
    return asArray(payload.jobs)
      .filter((value) => asRecord(value).isListed !== false)
      .map((value) => {
        const item = asRecord(value);
        const externalId = asString(item.id || item.jobId);
        return normalizeJob("ashby", {
          externalId,
          canonicalUrl: ashbyCanonicalUrl(
            source.boardName,
            externalId,
            asString(item.jobUrl || item.applyUrl),
          ),
          company: source.company ?? humanizeIdentifier(source.boardName),
          title: asString(item.title),
          location: asString(item.location),
          workplace: asString(item.workplaceType),
          description: asString(item.descriptionPlain || item.descriptionHtml),
          postedAt: asOptionalString(item.publishedAt),
          department: asOptionalString(item.department),
          employmentType: combineTypedFields(
            item.employmentType,
            item.employmentTypeLabel,
          ),
          engagementType: asOptionalString(item.engagementType),
        });
      });
  }

  private async searchSmartRecruiters(
    source: Extract<PublicAtsSource, { kind: "smartrecruiters" }>,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    const window = await this.searchSmartRecruitersWindow(
      source,
      initialPublicAtsSourceCursor(),
      requireCompleteSnapshot,
    );
    if (!window.nextState.done) {
      throw new IncompletePublicAtsSnapshotError(
        "smartrecruiters",
        window.jobs,
      );
    }
    return window.jobs;
  }

  private async searchSmartRecruitersWindow(
    source: Extract<PublicAtsSource, { kind: "smartrecruiters" }>,
    state: PublicAtsSourceCursor,
    requireCompleteSnapshot: boolean,
  ): Promise<PublicAtsSourceWindow> {
    assertIdentifier(
      source.companyIdentifier,
      "SmartRecruiters company identifier",
    );
    const host = "api.smartrecruiters.com";
    const postingsBaseUrl =
      `https://${host}/v1/companies/` +
      `${encodeURIComponent(source.companyIdentifier)}/postings`;
    const jobs: NormalizedJob[] = [];
    const listedJobs: NormalizedJob[] = [];
    let advertisedTotal = state.advertisedTotal;
    let fetches = 0;
    let validationOffset = state.validationOffset;
    let validationSha256 = state.validationSha256;
    while (
      state.offset > 0 &&
      validationOffset < state.offset &&
      fetches < this.maxPages
    ) {
      const validationLimit = Math.min(100, state.offset - validationOffset);
      const payload = asRecord(
        await this.requestJson(
          `${postingsBaseUrl}?limit=${validationLimit}&offset=${validationOffset}`,
          [host],
        ),
      );
      const content = asArray(payload.content);
      const normalized = content.map((value) =>
        normalizeSmartRecruitersRow(source, value),
      );
      advertisedTotal = reconcileAdvertisedTotal(
        advertisedTotal,
        asNonNegativeInteger(payload.totalFound),
      );
      if (
        content.length === 0 ||
        content.length > validationLimit ||
        validationOffset + content.length > state.offset ||
        (advertisedTotal !== null && state.offset > advertisedTotal)
      ) {
        throw new InvalidPublicAtsCursorError();
      }
      validationSha256 = advancePublicAtsPrefixSha256(
        validationSha256,
        normalized,
      );
      validationOffset += content.length;
      fetches += 1;
    }
    if (state.offset > 0) {
      if (validationOffset < state.offset) {
        return {
          jobs: [],
          nextState: publicAtsSourceCursorDuringValidation(
            state,
            advertisedTotal,
            validationOffset,
            validationSha256,
          ),
          warning: publicAtsPrefixValidationWarning("smartrecruiters"),
        };
      }
      if (validationSha256 !== state.prefixSha256) {
        throw new InvalidPublicAtsCursorError();
      }
      if (fetches === this.maxPages) {
        // The provider has no stable snapshot token. When validation consumes
        // this invocation's entire fetch budget, acquisition resumes on the
        // next trusted cursor hop and remains subject to the documented TOCTOU.
        return {
          jobs: [],
          nextState: publicAtsSourceCursorDuringValidation(
            state,
            advertisedTotal,
            validationOffset,
            validationSha256,
          ),
          warning: publicAtsPrefixValidationWarning("smartrecruiters"),
        };
      }
    }

    let offset = state.offset - state.overlapCount;
    let overlapPending = state.overlapCount > 0;
    let prefixSha256 = state.prefixSha256;
    const limit = 100;
    while (fetches < this.maxPages) {
      const payload = asRecord(
        await this.requestJson(
          `${postingsBaseUrl}?limit=${limit}&offset=${offset}`,
          [host],
        ),
      );
      if (requireCompleteSnapshot && !Array.isArray(payload.content)) {
        throw new InvalidPublicAtsSnapshotError("smartrecruiters");
      }
      const content = asArray(payload.content);
      const normalized = content.map((value) =>
        normalizeSmartRecruitersRow(source, value),
      );
      listedJobs.push(...normalized);
      if (overlapPending) {
        if (
          normalized.length < state.overlapCount ||
          publicAtsJobsSha256(normalized.slice(0, state.overlapCount)) !==
            state.overlapSha256
        ) {
          throw new InvalidPublicAtsCursorError();
        }
        normalized.splice(0, state.overlapCount);
        overlapPending = false;
      }
      jobs.push(...normalized);
      prefixSha256 = advancePublicAtsPrefixSha256(prefixSha256, normalized);
      offset += content.length;
      fetches += 1;
      advertisedTotal = reconcileAdvertisedTotal(
        advertisedTotal,
        asNonNegativeInteger(payload.totalFound),
      );
      if (advertisedTotal !== null && offset > advertisedTotal) {
        throw new InvalidPublicAtsCursorError();
      }
      const hasMore =
        advertisedTotal === null
          ? content.length === limit
          : offset < advertisedTotal;
      if (!hasMore) {
        if (
          state.offset > 0 &&
          state.advertisedTotal !== null &&
          jobs.length === 0
        ) {
          throw new InvalidPublicAtsCursorError();
        }
        return {
          jobs,
          nextState: publicAtsSourceCursorAfterWindow(
            offset,
            advertisedTotal,
            listedJobs,
            SMARTRECRUITERS_OVERLAP_ROWS,
            true,
            prefixSha256,
          ),
        };
      }
      if (content.length === 0) {
        throw new InvalidPublicAtsCursorError();
      }
      if (fetches === this.maxPages) {
        if (offset <= state.offset) {
          throw new InvalidPublicAtsCursorError();
        }
        return {
          jobs,
          nextState: publicAtsSourceCursorAfterWindow(
            offset,
            advertisedTotal,
            listedJobs,
            SMARTRECRUITERS_OVERLAP_ROWS,
            false,
            prefixSha256,
          ),
          warning:
            "smartrecruiters source incomplete: bounded acquisition window " +
            "ended before the advertised feed; results are partial",
        };
      }
    }
    throw new InvalidPublicAtsCursorError();
  }

  private async searchWorkday(
    source: Extract<PublicAtsSource, { kind: "workday" }>,
    requireCompleteSnapshot: boolean,
  ): Promise<NormalizedJob[]> {
    const window = await this.searchWorkdayWindow(
      source,
      initialPublicAtsSourceCursor(),
      requireCompleteSnapshot,
    );
    if (!window.nextState.done) {
      throw new IncompletePublicAtsSnapshotError("workday", window.jobs);
    }
    return window.jobs;
  }

  private async searchWorkdayWindow(
    source: Extract<PublicAtsSource, { kind: "workday" }>,
    state: PublicAtsSourceCursor,
    requireCompleteSnapshot: boolean,
  ): Promise<PublicAtsSourceWindow> {
    assertIdentifier(source.tenant, "Workday tenant");
    assertIdentifier(source.instance, "Workday instance");
    assertIdentifier(source.site, "Workday site");
    const locale =
      source.locale && SAFE_IDENTIFIER.test(source.locale)
        ? source.locale
        : "en-US";
    const host = `${source.tenant}.${source.instance}.myworkdayjobs.com`;
    const endpoint =
      `https://${host}/wday/cxs/${encodeURIComponent(source.tenant)}/` +
      `${encodeURIComponent(source.site)}/jobs`;
    const jobs: NormalizedJob[] = [];
    const listedJobs: NormalizedJob[] = [];
    let advertisedTotal = state.advertisedTotal;
    let fetches = 0;
    let validationOffset = state.validationOffset;
    let validationSha256 = state.validationSha256;
    while (
      state.offset > 0 &&
      validationOffset < state.offset &&
      fetches < this.maxPages
    ) {
      const validationLimit = Math.min(20, state.offset - validationOffset);
      const payload = asRecord(
        await this.requestJson(endpoint, [host], {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "accept-language": locale,
          },
          // Raw worker role strings are not safe Workday acquisition filters:
          // they can omit canonical aliases before the server classifies them.
          body: JSON.stringify({
            appliedFacets: {},
            limit: validationLimit,
            offset: validationOffset,
            searchText: "",
          }),
        }),
      );
      const postings = asArray(payload.jobPostings);
      const normalized = postings.map((value) =>
        normalizeWorkdayRow(source, host, locale, value),
      );
      advertisedTotal = reconcileAdvertisedTotal(
        advertisedTotal,
        asNonNegativeInteger(payload.total),
      );
      if (
        postings.length === 0 ||
        postings.length > validationLimit ||
        validationOffset + postings.length > state.offset ||
        (advertisedTotal !== null && state.offset > advertisedTotal)
      ) {
        throw new InvalidPublicAtsCursorError();
      }
      validationSha256 = advancePublicAtsPrefixSha256(
        validationSha256,
        normalized,
      );
      validationOffset += postings.length;
      fetches += 1;
    }
    if (state.offset > 0) {
      if (validationOffset < state.offset) {
        return {
          jobs: [],
          nextState: publicAtsSourceCursorDuringValidation(
            state,
            advertisedTotal,
            validationOffset,
            validationSha256,
          ),
          warning: publicAtsPrefixValidationWarning("workday"),
        };
      }
      if (validationSha256 !== state.prefixSha256) {
        throw new InvalidPublicAtsCursorError();
      }
      if (fetches === this.maxPages) {
        return {
          jobs: [],
          nextState: publicAtsSourceCursorDuringValidation(
            state,
            advertisedTotal,
            validationOffset,
            validationSha256,
          ),
          warning: publicAtsPrefixValidationWarning("workday"),
        };
      }
    }

    const limit = 20;
    let offset = state.offset - state.overlapCount;
    let overlapPending = state.overlapCount > 0;
    let prefixSha256 = state.prefixSha256;
    while (fetches < this.maxPages) {
      const payload = asRecord(
        await this.requestJson(endpoint, [host], {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "accept-language": locale,
          },
          body: JSON.stringify({
            appliedFacets: {},
            limit,
            offset,
            searchText: "",
          }),
        }),
      );
      if (requireCompleteSnapshot && !Array.isArray(payload.jobPostings)) {
        throw new InvalidPublicAtsSnapshotError("workday");
      }
      const postings = asArray(payload.jobPostings);
      const normalized = postings.map((value) =>
        normalizeWorkdayRow(source, host, locale, value),
      );
      listedJobs.push(...normalized);
      if (overlapPending) {
        if (
          normalized.length < state.overlapCount ||
          publicAtsJobsSha256(normalized.slice(0, state.overlapCount)) !==
            state.overlapSha256
        ) {
          throw new InvalidPublicAtsCursorError();
        }
        normalized.splice(0, state.overlapCount);
        overlapPending = false;
      }
      jobs.push(...normalized);
      prefixSha256 = advancePublicAtsPrefixSha256(prefixSha256, normalized);
      offset += postings.length;
      fetches += 1;
      advertisedTotal = reconcileAdvertisedTotal(
        advertisedTotal,
        asNonNegativeInteger(payload.total),
      );
      if (advertisedTotal !== null && offset > advertisedTotal) {
        throw new InvalidPublicAtsCursorError();
      }
      const hasMore =
        advertisedTotal === null
          ? postings.length === limit
          : offset < advertisedTotal;
      if (!hasMore) {
        if (
          state.offset > 0 &&
          state.advertisedTotal !== null &&
          jobs.length === 0
        ) {
          throw new InvalidPublicAtsCursorError();
        }
        return {
          jobs,
          nextState: publicAtsSourceCursorAfterWindow(
            offset,
            advertisedTotal,
            listedJobs,
            WORKDAY_OVERLAP_ROWS,
            true,
            prefixSha256,
          ),
        };
      }
      if (postings.length === 0) {
        throw new InvalidPublicAtsCursorError();
      }
      if (fetches === this.maxPages) {
        if (offset <= state.offset) {
          throw new InvalidPublicAtsCursorError();
        }
        return {
          jobs,
          nextState: publicAtsSourceCursorAfterWindow(
            offset,
            advertisedTotal,
            listedJobs,
            WORKDAY_OVERLAP_ROWS,
            false,
            prefixSha256,
          ),
          warning:
            "workday source incomplete: bounded acquisition window ended " +
            "before the advertised feed; results are partial",
        };
      }
    }
    throw new InvalidPublicAtsCursorError();
  }

  private async requestJson(
    url: string,
    allowedHosts: string[],
    init: RequestInit = {},
  ): Promise<unknown> {
    const parsed = new URL(url);
    if (
      parsed.protocol !== "https:" ||
      !allowedHosts.includes(parsed.hostname)
    ) {
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
          if (
            (response.status === 429 || response.status >= 500) &&
            attempt < this.maxAttempts
          ) {
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

function ashbyCanonicalUrl(
  boardName: string,
  externalId: string,
  raw: string,
): string {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return "";
  }
  const segments = url.pathname.split("/").filter(Boolean);
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.port ||
    url.hostname.toLowerCase() !== "jobs.ashbyhq.com" ||
    segments.length < 2 ||
    segments[0] !== boardName ||
    segments[1] !== externalId ||
    !SAFE_IDENTIFIER.test(segments[0]) ||
    !SAFE_IDENTIFIER.test(segments[1])
  ) {
    return "";
  }
  url.hash = "";
  return url.toString();
}

function smartrecruitersCanonicalUrl(
  companyIdentifier: string,
  externalId: string,
): string {
  if (!SAFE_IDENTIFIER.test(externalId)) return "";
  return `https://jobs.smartrecruiters.com/${encodeURIComponent(companyIdentifier)}/${encodeURIComponent(externalId)}`;
}

function normalizeSmartRecruitersRow(
  source: Extract<PublicAtsSource, { kind: "smartrecruiters" }>,
  value: unknown,
): NormalizedJob {
  const item = asRecord(value);
  const location = asRecord(item.location);
  const company = asRecord(item.company);
  const externalId = asString(item.id);
  return normalizeJob("smartrecruiters", {
    externalId,
    canonicalUrl: smartrecruitersCanonicalUrl(
      source.companyIdentifier,
      externalId,
    ),
    company:
      source.company ||
      asString(company.name) ||
      humanizeIdentifier(source.companyIdentifier),
    title: asString(item.name),
    location: [location.city, location.region, location.country]
      .map(asOptionalString)
      .filter(Boolean)
      .join(", "),
    workplace: [
      location.remote === true
        ? "remote"
        : location.remote === false
          ? "not remote"
          : "",
      asString(item.workplaceType),
    ]
      .filter(Boolean)
      .join(" "),
    description: extractSmartRecruitersDescription(item),
    postedAt: asOptionalString(item.releasedDate),
    department: asOptionalString(asRecord(item.department).label),
    employmentType: combineTypedFields(
      asRecord(item.typeOfEmployment).label,
      item.employmentType,
    ),
    engagementType: asOptionalString(item.engagementType),
  });
}

function normalizeWorkdayRow(
  source: Extract<PublicAtsSource, { kind: "workday" }>,
  host: string,
  locale: string,
  value: unknown,
): NormalizedJob {
  const item = asRecord(value);
  const externalPath = asString(item.externalPath);
  return normalizeJob("workday", {
    externalId: asString(
      item.bulletFields ? asArray(item.bulletFields)[0] : externalPath,
    ),
    canonicalUrl: workdayCanonicalUrl(
      host,
      locale,
      source.site,
      externalPath,
    ),
    company: source.company ?? humanizeIdentifier(source.tenant),
    title: asString(item.title),
    location: asString(item.locationsText),
    workplace: asString(item.workplaceType),
    description: asString(item.descriptionPreview),
    postedAt: asOptionalString(item.postedOn),
    employmentType: combineTypedFields(item.timeType, item.employmentType),
    engagementType: asOptionalString(item.engagementType),
  });
}

function workdayCanonicalUrl(
  host: string,
  locale: string,
  site: string,
  externalPath: string,
): string {
  let url: URL;
  try {
    url = new URL(externalPath, `https://${host}`);
  } catch {
    return "";
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.port ||
    url.hostname !== host.toLowerCase()
  ) {
    return "";
  }

  if (url.pathname.startsWith("/job/")) {
    url.pathname = `/${encodeURIComponent(locale)}/${encodeURIComponent(site)}${url.pathname}`;
  } else {
    const segments = url.pathname.split("/");
    const jobIndex = segments.indexOf("job");
    if (
      jobIndex < 1 ||
      jobIndex === segments.length - 1 ||
      segments[jobIndex - 1] !== site
    ) {
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
    const key = [
      job.company,
      job.title,
      job.location,
      canonicalizeUrl(job.canonicalUrl),
    ]
      .map(normalizeComparable)
      .join("|");
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function normalizeJob(source: AtsKind, raw: RawJob): NormalizedJob {
  const canonicalUrl = canonicalizeUrl(raw.canonicalUrl);
  const description = stripMarkup(raw.description ?? "");
  // Only a provider's typed category can become hard-filter evidence. Titles
  // and descriptions remain raw review evidence; inferring "contract" from
  // "Contract Administrator" would manufacture an employment classification.
  const employmentType = normalizeEmploymentType(raw.employmentType);
  const engagementType = normalizeEngagementType(
    [raw.engagementType, raw.employmentType].filter(Boolean).join(" ") ||
      undefined,
  );
  return {
    externalId: raw.externalId || canonicalUrl,
    canonicalUrl,
    company: raw.company.trim(),
    title: raw.title.trim(),
    location: raw.location.trim() || "Location not listed",
    workplace: inferWorkplace(raw.workplace),
    description,
    source,
    postedAt: raw.postedAt,
    compensation: raw.compensation,
    department: raw.department || undefined,
    ...(employmentType ? { employmentType } : {}),
    ...(engagementType ? { engagementType } : {}),
  };
}

function normalizeEmploymentType(
  explicit: string | undefined,
): string | undefined {
  const value = (explicit ?? "").toLowerCase().replace(/[^a-z0-9+]+/g, " ");
  const negated = {
    internship: hasNegatedSignal(value, "interns?|internships?"),
    apprenticeship: hasNegatedSignal(value, "apprentices?|apprenticeships?"),
    perDiem: hasNegatedSignal(value, "per diem|perdiem"),
    seasonal: hasNegatedSignal(value, "seasonal"),
    partTime: hasNegatedSignal(value, "part time|parttime|pt"),
    fullTime: hasNegatedSignal(
      value,
      "full time|fulltime|ft|permanent|direct hire",
    ),
    contract: hasNegatedSignal(
      value,
      "contracts?|contractors?|consultants?|consulting",
    ),
    temporary: hasNegatedSignal(value, "temp|temporary"),
  };
  const kinds = [
    !negated.internship && /\b(intern(ship)?)\b/.test(value)
      ? "internship"
      : undefined,
    !negated.apprenticeship && /\b(apprentice(ship)?)\b/.test(value)
      ? "apprenticeship"
      : undefined,
    !negated.perDiem && /\b(per diem|perdiem)\b/.test(value)
      ? "per_diem"
      : undefined,
    !negated.seasonal && /\bseasonal\b/.test(value) ? "seasonal" : undefined,
    !negated.partTime && /\b(part time|parttime|pt)\b/.test(value)
      ? "part_time"
      : undefined,
    !negated.fullTime &&
    /\b(full time|fulltime|ft|permanent|direct hire)\b/.test(value)
      ? "full_time"
      : undefined,
    !negated.contract && /\b(contract(or)?|consultant|consulting)\b/.test(value)
      ? "contract"
      : undefined,
    !negated.temporary && /\b(temp(orary)?)\b/.test(value)
      ? "temporary"
      : undefined,
  ].filter((kind): kind is string => kind !== undefined);
  return kinds.length === 1 ? kinds[0] : undefined;
}

function normalizeEngagementType(
  explicit: string | undefined,
): string | undefined {
  const value = (explicit ?? "").toLowerCase().replace(/[^a-z0-9+]+/g, " ");
  const negated = {
    c2c: hasNegatedSignal(value, "c2c|corp to corp|corporation to corporation"),
    w2: hasNegatedSignal(value, "w 2|w2"),
    contractor1099: hasNegatedSignal(value, "1099|independent contractors?"),
    directHire: hasNegatedSignal(value, "direct hire|permanent hire"),
  };
  const kinds = [
    !negated.c2c &&
    /\b(c2c|corp to corp|corporation to corporation)\b/.test(value)
      ? "c2c"
      : undefined,
    !negated.w2 && /\b(w 2|w2)\b/.test(value) ? "w2" : undefined,
    !negated.contractor1099 && /\b(1099|independent contractor)\b/.test(value)
      ? "1099"
      : undefined,
    !negated.directHire && /\b(direct hire|permanent hire)\b/.test(value)
      ? "direct_hire"
      : undefined,
  ].filter((kind): kind is string => kind !== undefined);
  return kinds.length === 1 ? kinds[0] : undefined;
}

function hasNegatedSignal(value: string, signal: string): boolean {
  const subject = `(?:${signal})(?: candidates?| roles?| positions?| jobs?| work| options?)?`;
  const rejection = [
    "not(?: currently)?(?: allowed| accepted| available| offered| eligible| permitted| supported)?",
    "(?:currently |temporarily )?unavailable",
    "prohibited",
    "excluded",
    "disallowed",
    "disabled",
    "false",
    "none",
  ].join("|");
  return new RegExp(
    `\\b(?:` +
      `(?:no|not|non) (?:an? )?${subject}` +
      `|no longer (?:accepting |allowing |offering |supporting |permitting )?(?:an? )?${subject}` +
      `|not (?:fully|completely|entirely|exclusively|strictly|100(?: percent)?) (?:an? )?${subject}` +
      `|(?:cannot|can not) (?:be )?${subject}` +
      `|(?:will|must) not (?:be )?${subject}` +
      `|(?:do|does) not (?:accept|allow|support|offer|permit) ${subject}` +
      `|(?:will|must) not (?:accept|allow|support|offer|permit) ${subject}` +
      `|not eligible for ${subject}` +
      `|${subject}(?: (?:is|are))? (?:${rejection})` +
      `|${subject} no longer (?:accepted|allowed|available|offered|eligible|permitted|supported)` +
      `|${subject} (?:will|must) not (?:be )?` +
      `(?:accepted|allowed|available|offered|eligible|permitted|supported)` +
      `)\\b`,
  ).test(value);
}

function leverCompensation(value: unknown): string | undefined {
  const range = asRecord(value);
  const minimum = Number(range.min);
  const maximum = Number(range.max);
  if (!Number.isFinite(minimum) && !Number.isFinite(maximum)) return undefined;
  const currency = asString(range.currency) || "USD";
  const interval = asString(range.interval);
  const bounds =
    Number.isFinite(minimum) && Number.isFinite(maximum)
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
  const excludedCompanies = normalizedValues(query.excludedCompanies);
  const excludedTitles = normalizedValues(query.excludedTitles ?? []);
  if (excludedCompanies.some((value) => company.includes(value))) return false;
  if (excludedTitles.some((value) => title.includes(value))) return false;
  // Role and geography are server-authoritative taxonomy decisions. Raw
  // worker-side substring filters used to discard candidates permanently
  // before the server could classify aliases, typed regions, or ambiguity.
  // Keep every positive query value as an acquisition hint only. Even a raw
  // "Remote" location can name a city, and discarding an unknown posting here
  // would prevent the server taxonomy from making the authoritative decision.
  return true;
}

function extractSmartRecruitersDescription(
  item: Record<string, unknown>,
): string {
  const sections = asRecord(asRecord(item.jobAd).sections);
  return Object.values(sections)
    .map((section) =>
      asString(asRecord(section).text || asRecord(section).description),
    )
    .filter(Boolean)
    .join("\n\n");
}

function inferWorkplace(
  value: string | undefined,
): NormalizedJob["workplace"] {
  // A provider's explicit workplace category is typed evidence. Location text
  // stays raw for the server taxonomy: strings such as "Remote, OR" can name a
  // city and must never be promoted to an affirmative remote classification.
  const combined = (value ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
  if (!combined || hasCoordinatedWorkplaceRejection(combined)) return "unknown";
  const containsPhrase = (phrase: string) =>
    ` ${combined} `.includes(` ${phrase} `);
  const remote =
    !hasNegatedSignal(combined, "remote|work from home|wfh") &&
    ["remote", "work from home", "wfh"].some(containsPhrase);
  const hybrid =
    !hasNegatedSignal(combined, "hybrid") && containsPhrase("hybrid");
  const onsite =
    !hasNegatedSignal(combined, "on site|onsite|in office|office based") &&
    (["on site", "onsite", "in office", "office based"].some(containsPhrase) ||
      combined === "office");
  const kinds = [
    remote ? "remote" : undefined,
    hybrid ? "hybrid" : undefined,
    onsite ? "onsite" : undefined,
  ].filter(
    (kind): kind is "remote" | "hybrid" | "onsite" => kind !== undefined,
  );
  return kinds.length === 1 ? kinds[0] : "unknown";
}

function hasCoordinatedWorkplaceRejection(value: string): boolean {
  const subject =
    "(?:remote|work from home|wfh|hybrid|on site|onsite|in office|office based)";
  const rejection =
    "(?:not(?: currently)?(?: allowed| accepted| available| offered| eligible|" +
    " permitted| supported)?|(?:currently |temporarily )?unavailable|" +
    "prohibited|excluded|disallowed|disabled)";
  return new RegExp(
    `\\b${subject}(?: (?:and|or) ${subject})+` +
      `(?: candidates?| roles?| positions?| jobs?| work| options?)?` +
      `(?: (?:is|are))? ${rejection}\\b`,
  ).test(value);
}

function canonicalizeUrl(value: string): string {
  if (submissionPolicy(value).policy === "blocked") return "";
  try {
    const url = new URL(value);
    url.hash = "";
    for (const key of [...url.searchParams.keys()]) {
      if (/^(utm_|source$|sourceid$|gh_src$)/i.test(key))
        url.searchParams.delete(key);
    }
    return url.toString();
  } catch {
    return value.trim();
  }
}

function errorMessage(value: unknown): string {
  return value instanceof Error ? value.message : String(value);
}

function publicAtsQuerySha256(
  query: DiscoveryQuery,
  pageSize: number,
  maximumAgeDays: number,
  maxPages: number,
): string {
  return sha256Json({
    schemaVersion: CURSOR_VERSION,
    pageSize,
    maximumAgeDays,
    maxPages,
    roles: query.roles.map((value) => value.trim()),
    locations: query.locations.map((value) => value.trim()),
    remotePreference: query.remotePreference.trim(),
    excludedCompanies: query.excludedCompanies.map((value) => value.trim()),
    excludedTitles: (query.excludedTitles ?? []).map((value) => value.trim()),
    sources: (query.sources ?? []).map(canonicalCursorSource),
  });
}

function canonicalCursorSource(source: PublicAtsSource): unknown {
  switch (source.kind) {
    case "greenhouse":
      return {
        kind: source.kind,
        boardToken: source.boardToken,
        company: source.company ?? null,
      };
    case "lever":
      return {
        kind: source.kind,
        site: source.site,
        company: source.company ?? null,
      };
    case "ashby":
      return {
        kind: source.kind,
        boardName: source.boardName,
        company: source.company ?? null,
      };
    case "smartrecruiters":
      return {
        kind: source.kind,
        companyIdentifier: source.companyIdentifier,
        company: source.company ?? null,
      };
    case "workday":
      return {
        kind: source.kind,
        tenant: source.tenant,
        instance: source.instance,
        site: source.site,
        company: source.company ?? null,
        locale: source.locale ?? null,
      };
  }
}

function publicAtsSnapshotSha256(
  jobs: NormalizedJob[],
  warnings: string[],
  nextSourceStates: PublicAtsSourceCursor[],
  nextHistory: PublicAtsHistoryCursor,
): string {
  return sha256Json({
    jobs: jobs.map(canonicalNormalizedJob),
    warnings,
    nextSourceStates: nextSourceStates.map(canonicalSourceCursor),
    nextHistory: canonicalPublicAtsHistory(nextHistory),
  });
}

function canonicalNormalizedJob(job: NormalizedJob): unknown {
  return {
    externalId: job.externalId,
    canonicalUrl: job.canonicalUrl,
    company: job.company,
    title: job.title,
    location: job.location,
    workplace: job.workplace,
    description: job.description,
    source: job.source,
    postedAt: job.postedAt ?? null,
    compensation: job.compensation ?? null,
    department: job.department ?? null,
    employmentType: job.employmentType ?? null,
    engagementType: job.engagementType ?? null,
  };
}

function publicAtsPaginationWarning(
  offset: number,
  pageEnd: number,
  total: number,
  hasMoreInWindow: boolean,
): string {
  if (hasMoreInWindow) {
    return (
      "Public ATS results are partial: showing bounded candidates " +
      `${offset + 1}-${pageEnd} of ${total}; continue with nextCursor.`
    );
  }
  if (total === 0) {
    return (
      "Public ATS results are partial: this bounded acquisition window " +
      "returned no eligible candidates; continue with nextCursor for " +
      "remaining provider rows."
    );
  }
  return (
    "Public ATS results are partial: the bounded acquisition window ended " +
    `after candidates ${offset + 1}-${pageEnd}; continue with nextCursor ` +
    "for remaining provider rows."
  );
}

function publicAtsCrossListingWarning(count: number): string {
  return (
    `${count} possible cross-listed posting pair${count === 1 ? "" : "s"} ` +
    "kept separate for original-source comparison."
  );
}

function encodePublicAtsCursor(
  cursor: Omit<PublicAtsCursor, "checksumSha256">,
): string {
  const payload: PublicAtsCursor = {
    ...cursor,
    checksumSha256: publicAtsCursorChecksum(cursor),
  };
  const encoded = Buffer.from(JSON.stringify(payload), "utf8").toString(
    "base64url",
  );
  if (encoded.length > MAX_CURSOR_LENGTH) {
    throw new InvalidPublicAtsCursorError();
  }
  return encoded;
}

function decodePublicAtsCursor(
  value: string | undefined,
): PublicAtsCursor | undefined {
  if (value === undefined) return undefined;
  if (
    value.length === 0 ||
    value.length > MAX_CURSOR_LENGTH ||
    !/^[a-zA-Z0-9_-]+$/.test(value)
  ) {
    throw new InvalidPublicAtsCursorError();
  }

  let bytes: Buffer;
  let parsed: unknown;
  try {
    bytes = Buffer.from(value, "base64url");
    if (bytes.toString("base64url") !== value) {
      throw new InvalidPublicAtsCursorError();
    }
    parsed = JSON.parse(bytes.toString("utf8")) as unknown;
  } catch {
    throw new InvalidPublicAtsCursorError();
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new InvalidPublicAtsCursorError();
  }
  const record = parsed as Record<string, unknown>;
  if (
    Object.keys(record).sort().join(",") !==
      "checksumSha256,history,pageOffset,querySha256,sources,version,windowSha256" ||
    record.version !== CURSOR_VERSION ||
    typeof record.pageOffset !== "number" ||
    !Number.isSafeInteger(record.pageOffset) ||
    record.pageOffset < 0 ||
    typeof record.querySha256 !== "string" ||
    !SHA256_HEX.test(record.querySha256) ||
    !Array.isArray(record.sources) ||
    record.sources.length > MAX_PUBLIC_ATS_SOURCES ||
    !(
      record.windowSha256 === null ||
      (typeof record.windowSha256 === "string" &&
        SHA256_HEX.test(record.windowSha256))
    ) ||
    typeof record.checksumSha256 !== "string" ||
    !SHA256_HEX.test(record.checksumSha256)
  ) {
    throw new InvalidPublicAtsCursorError();
  }
  if (
    (record.pageOffset === 0 && record.windowSha256 !== null) ||
    (record.pageOffset > 0 && record.windowSha256 === null)
  ) {
    throw new InvalidPublicAtsCursorError();
  }
  const sourceCursors = record.sources.map(decodePublicAtsSourceCursor);
  const history = decodePublicAtsHistoryCursor(record.history);

  const cursor: PublicAtsCursor = {
    version: CURSOR_VERSION,
    pageOffset: record.pageOffset,
    querySha256: record.querySha256,
    windowSha256: record.windowSha256,
    sources: sourceCursors,
    history,
    checksumSha256: record.checksumSha256,
  };
  if (cursor.checksumSha256 !== publicAtsCursorChecksum(cursor)) {
    throw new InvalidPublicAtsCursorError();
  }
  return cursor;
}

function decodePublicAtsSourceCursor(value: unknown): PublicAtsSourceCursor {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new InvalidPublicAtsCursorError();
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).sort().join(",") !==
      "advertisedTotal,done,offset,overlapCount,overlapSha256,prefixSha256,validationOffset,validationSha256" ||
    typeof record.offset !== "number" ||
    !Number.isSafeInteger(record.offset) ||
    record.offset < 0 ||
    !(
      record.advertisedTotal === null ||
      (typeof record.advertisedTotal === "number" &&
        Number.isSafeInteger(record.advertisedTotal) &&
        record.advertisedTotal >= 0)
    ) ||
    (typeof record.advertisedTotal === "number" &&
      record.offset > record.advertisedTotal) ||
    !(
      record.prefixSha256 === null ||
      (typeof record.prefixSha256 === "string" &&
        SHA256_HEX.test(record.prefixSha256))
    ) ||
    typeof record.validationOffset !== "number" ||
    !Number.isSafeInteger(record.validationOffset) ||
    record.validationOffset < 0 ||
    record.validationOffset > record.offset ||
    !(
      record.validationSha256 === null ||
      (typeof record.validationSha256 === "string" &&
        SHA256_HEX.test(record.validationSha256))
    ) ||
    typeof record.overlapCount !== "number" ||
    !Number.isSafeInteger(record.overlapCount) ||
    record.overlapCount < 0 ||
    record.overlapCount > record.offset ||
    !(
      record.overlapSha256 === null ||
      (typeof record.overlapSha256 === "string" &&
        SHA256_HEX.test(record.overlapSha256))
    ) ||
    typeof record.done !== "boolean" ||
    (record.offset === 0 && record.prefixSha256 !== null) ||
    (record.offset > 0 && record.prefixSha256 === null) ||
    (record.validationOffset === 0 && record.validationSha256 !== null) ||
    (record.validationOffset > 0 && record.validationSha256 === null) ||
    (record.validationOffset === record.offset &&
      record.validationOffset > 0 &&
      record.validationSha256 !== record.prefixSha256) ||
    (record.done && record.validationOffset !== 0) ||
    (record.overlapCount === 0 && record.overlapSha256 !== null) ||
    (record.overlapCount > 0 && record.overlapSha256 === null)
  ) {
    throw new InvalidPublicAtsCursorError();
  }
  return {
    offset: record.offset,
    advertisedTotal: record.advertisedTotal,
    prefixSha256: record.prefixSha256,
    validationOffset: record.validationOffset,
    validationSha256: record.validationSha256,
    overlapCount: record.overlapCount,
    overlapSha256: record.overlapSha256,
    done: record.done,
  };
}

function decodePublicAtsHistoryCursor(value: unknown): PublicAtsHistoryCursor {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new InvalidPublicAtsCursorError();
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).sort().join(",") !==
      "crossListingCandidates,seenCrossListingSha256,seenJobSha256" ||
    !isBoundedUniqueStringArray(record.seenJobSha256, SHA256_BASE64URL) ||
    !isBoundedUniqueStringArray(
      record.crossListingCandidates,
      CROSS_LISTING_CANDIDATE,
    ) ||
    !isBoundedUniqueStringArray(
      record.seenCrossListingSha256,
      SHA256_BASE64URL,
    )
  ) {
    throw new InvalidPublicAtsCursorError();
  }
  return {
    seenJobSha256: record.seenJobSha256,
    crossListingCandidates: record.crossListingCandidates,
    seenCrossListingSha256: record.seenCrossListingSha256,
  };
}

function isBoundedUniqueStringArray(
  value: unknown,
  pattern: RegExp,
): value is string[] {
  return (
    Array.isArray(value) &&
    value.length <= MAX_CURSOR_HISTORY_ENTRIES &&
    value.every((entry) => typeof entry === "string" && pattern.test(entry)) &&
    new Set(value).size === value.length
  );
}

function publicAtsCursorChecksum(
  cursor: Omit<PublicAtsCursor, "checksumSha256">,
): string {
  // This is deliberately an unkeyed integrity checksum, not authentication.
  // It detects corruption and stale state only; search cursors must remain on
  // Bluey's trusted internal discovery boundary and never grant authority.
  return sha256Json({
    version: cursor.version,
    pageOffset: cursor.pageOffset,
    querySha256: cursor.querySha256,
    windowSha256: cursor.windowSha256,
    sources: cursor.sources.map(canonicalSourceCursor),
    history: canonicalPublicAtsHistory(cursor.history),
  });
}

function canonicalSourceCursor(state: PublicAtsSourceCursor): unknown {
  return {
    offset: state.offset,
    advertisedTotal: state.advertisedTotal,
    prefixSha256: state.prefixSha256,
    validationOffset: state.validationOffset,
    validationSha256: state.validationSha256,
    overlapCount: state.overlapCount,
    overlapSha256: state.overlapSha256,
    done: state.done,
  };
}

function canonicalPublicAtsHistory(history: PublicAtsHistoryCursor): unknown {
  return {
    seenJobSha256: history.seenJobSha256,
    crossListingCandidates: history.crossListingCandidates,
    seenCrossListingSha256: history.seenCrossListingSha256,
  };
}

function initialPublicAtsSourceCursor(): PublicAtsSourceCursor {
  return {
    offset: 0,
    advertisedTotal: null,
    prefixSha256: null,
    validationOffset: 0,
    validationSha256: null,
    overlapCount: 0,
    overlapSha256: null,
    done: false,
  };
}

function initialPublicAtsHistoryCursor(): PublicAtsHistoryCursor {
  return {
    seenJobSha256: [],
    crossListingCandidates: [],
    seenCrossListingSha256: [],
  };
}

function validatePublicAtsSourceCursors(
  states: PublicAtsSourceCursor[],
  sources: PublicAtsSource[],
): void {
  if (states.length !== sources.length) {
    throw new InvalidPublicAtsCursorError();
  }
  states.forEach((state, index) => {
    const kind = sources[index]?.kind;
    if (
      kind !== "smartrecruiters" &&
      kind !== "workday" &&
      (state.offset !== 0 ||
        state.advertisedTotal !== null ||
        state.prefixSha256 !== null ||
        state.validationOffset !== 0 ||
        state.validationSha256 !== null ||
        state.overlapCount !== 0 ||
        state.overlapSha256 !== null)
    ) {
      throw new InvalidPublicAtsCursorError();
    }
    if (
      kind === "smartrecruiters" &&
      (state.overlapCount > SMARTRECRUITERS_OVERLAP_ROWS ||
        (!state.done &&
          state.offset > 0 &&
          state.overlapCount !== SMARTRECRUITERS_OVERLAP_ROWS))
    ) {
      throw new InvalidPublicAtsCursorError();
    }
    if (
      kind === "workday" &&
      (state.overlapCount > WORKDAY_OVERLAP_ROWS ||
        (!state.done &&
          state.offset > 0 &&
          state.overlapCount !== WORKDAY_OVERLAP_ROWS))
    ) {
      throw new InvalidPublicAtsCursorError();
    }
    if (
      (kind === "smartrecruiters" || kind === "workday") &&
      !state.done &&
      state.offset > MAX_CURSOR_HISTORY_ENTRIES
    ) {
      throw new PublicAtsContinuationHistoryLimitError();
    }
  });
}

function reconcileAdvertisedTotal(
  expected: number | null,
  observed: number | undefined,
): number | null {
  if (expected !== null) {
    if (observed !== expected) throw new InvalidPublicAtsCursorError();
    return expected;
  }
  return observed ?? null;
}

function publicAtsSourceCursorAfterWindow(
  offset: number,
  advertisedTotal: number | null,
  listedJobs: NormalizedJob[],
  requiredOverlap: number,
  done: boolean,
  prefixSha256: string | null,
): PublicAtsSourceCursor {
  if (
    (offset === 0 && prefixSha256 !== null) ||
    (offset > 0 && prefixSha256 === null)
  ) {
    throw new InvalidPublicAtsCursorError();
  }
  if (!done && offset > MAX_CURSOR_HISTORY_ENTRIES) {
    throw new PublicAtsContinuationHistoryLimitError();
  }
  const overlapCount = Math.min(requiredOverlap, listedJobs.length);
  if (!done && overlapCount !== requiredOverlap) {
    throw new InvalidPublicAtsCursorError();
  }
  const overlap = listedJobs.slice(listedJobs.length - overlapCount);
  return {
    offset,
    advertisedTotal,
    prefixSha256,
    validationOffset: 0,
    validationSha256: null,
    overlapCount,
    overlapSha256: overlapCount > 0 ? publicAtsJobsSha256(overlap) : null,
    done,
  };
}

function publicAtsSourceCursorDuringValidation(
  state: PublicAtsSourceCursor,
  advertisedTotal: number | null,
  validationOffset: number,
  validationSha256: string | null,
): PublicAtsSourceCursor {
  if (
    state.done ||
    state.offset === 0 ||
    state.offset > MAX_CURSOR_HISTORY_ENTRIES ||
    validationOffset <= 0 ||
    validationOffset > state.offset ||
    validationSha256 === null ||
    (validationOffset === state.offset &&
      validationSha256 !== state.prefixSha256)
  ) {
    throw new InvalidPublicAtsCursorError();
  }
  return {
    ...state,
    advertisedTotal,
    validationOffset,
    validationSha256,
  };
}

function publicAtsPrefixValidationWarning(
  provider: "smartrecruiters" | "workday",
): string {
  return (
    `${provider} source incomplete: validating the bounded ordered provider ` +
    "prefix before continuation; results are partial"
  );
}

function advancePublicAtsPrefixSha256(
  currentSha256: string | null,
  jobs: NormalizedJob[],
): string | null {
  let digest = currentSha256;
  for (const job of jobs) {
    digest = sha256Json({
      previousSha256:
        digest ?? sha256Json({ schemaVersion: CURSOR_VERSION, prefix: [] }),
      job: canonicalNormalizedJob(job),
    });
  }
  return digest;
}

function publicAtsJobsSha256(jobs: NormalizedJob[]): string {
  return sha256Json(jobs.map(canonicalNormalizedJob));
}

function publicAtsDedupeSha256(job: NormalizedJob): string {
  const key = [
    job.company,
    job.title,
    job.location,
    canonicalizeUrl(job.canonicalUrl),
  ]
    .map(normalizeComparable)
    .join("|");
  return sha256Base64Url(key);
}

function findNewCrossListingState(
  jobs: NormalizedJob[],
  history: PublicAtsHistoryCursor,
): {
  currentCandidates: string[];
  newSignalSha256: string[];
} {
  const previous = history.crossListingCandidates.map(
    decodeCrossListingCandidate,
  );
  const currentCandidates = [
    ...new Set(
      jobs
        .map(encodeCrossListingCandidate)
        .filter((value): value is string => value !== undefined),
    ),
  ];
  const current = currentCandidates.map(decodeCrossListingCandidate);
  const seenSignals = new Set(history.seenCrossListingSha256);
  const newSignalSha256: string[] = [];

  current.forEach((candidate, index) => {
    for (const other of [...previous, ...current.slice(0, index)]) {
      if (
        candidate.urlSha256 === other.urlSha256 ||
        candidate.companySha256 === other.companySha256 ||
        fingerprintSimilarity(candidate.fingerprint, other.fingerprint) <
          CROSS_LISTING_THRESHOLD
      ) {
        continue;
      }
      const signalSha256 = sha256Base64Url(
        [
          [candidate.companySha256, other.companySha256].sort().join("|"),
          [candidate.fingerprint, other.fingerprint].sort().join("|"),
        ].join("::"),
      );
      if (seenSignals.has(signalSha256)) continue;
      seenSignals.add(signalSha256);
      newSignalSha256.push(signalSha256);
    }
  });
  return { currentCandidates, newSignalSha256 };
}

function encodeCrossListingCandidate(job: NormalizedJob): string | undefined {
  const fingerprint = fingerprintDescription(job.description);
  if (!fingerprint) return undefined;
  return [
    sha256Base64Url(crossListingUrlComparable(job.canonicalUrl)),
    sha256Base64Url(crossListingCompanyComparable(job.company)),
    fingerprint,
  ].join(".");
}

function decodeCrossListingCandidate(value: string): {
  urlSha256: string;
  companySha256: string;
  fingerprint: string;
} {
  const [urlSha256, companySha256, fingerprint] = value.split(".");
  if (!urlSha256 || !companySha256 || !fingerprint) {
    throw new InvalidPublicAtsCursorError();
  }
  return { urlSha256, companySha256, fingerprint };
}

function crossListingUrlComparable(value: string): string {
  try {
    const parsed = new URL(value);
    parsed.hash = "";
    return parsed.toString().replace(/\/$/, "").toLowerCase();
  } catch {
    return value.trim().toLowerCase();
  }
}

function crossListingCompanyComparable(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function advancePublicAtsHistory(
  history: PublicAtsHistoryCursor,
  currentSeenJobSha256: string[],
  currentCrossListingCandidates: string[],
  newSignalSha256: string[],
): PublicAtsHistoryCursor {
  return {
    seenJobSha256: appendUnique(
      history.seenJobSha256,
      currentSeenJobSha256,
    ),
    crossListingCandidates: appendUnique(
      history.crossListingCandidates,
      currentCrossListingCandidates,
    ),
    seenCrossListingSha256: appendUnique(
      history.seenCrossListingSha256,
      newSignalSha256,
    ),
  };
}

function appendUnique(current: string[], additions: string[]): string[] {
  const values = new Set(current);
  additions.forEach((value) => values.add(value));
  return [...values];
}

function assertPublicAtsHistoryBounded(history: PublicAtsHistoryCursor): void {
  if (
    history.seenJobSha256.length > MAX_CURSOR_HISTORY_ENTRIES ||
    history.crossListingCandidates.length > MAX_CURSOR_HISTORY_ENTRIES ||
    history.seenCrossListingSha256.length > MAX_CURSOR_HISTORY_ENTRIES
  ) {
    throw new PublicAtsContinuationHistoryLimitError();
  }
}

function sha256Json(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function sha256Base64Url(value: string): string {
  return createHash("sha256").update(value).digest("base64url");
}

function assertPublicAtsSourceLimit(sources: PublicAtsSource[]): void {
  if (sources.length > MAX_PUBLIC_ATS_SOURCES) {
    throw new RangeError(
      `Public ATS discovery supports at most ${MAX_PUBLIC_ATS_SOURCES} sources`,
    );
  }
}

function normalizePageSize(value: number | undefined): number {
  const candidate = value ?? 100;
  return Number.isFinite(candidate)
    ? Math.trunc(clamp(candidate, 1, 250))
    : 100;
}

function normalizeMaxPages(value: number | undefined): number {
  const candidate = value ?? 5;
  return Number.isFinite(candidate)
    ? Math.trunc(clamp(candidate, 1, 100))
    : 5;
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
  if (!SAFE_IDENTIFIER.test(value))
    throw new Error(`${label} contains unsupported characters`);
}

function humanizeIdentifier(value: string): string {
  return value
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function normalizeComparable(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function normalizedValues(values: string[]): string[] {
  return values.map(normalizeComparable).filter(Boolean);
}

export function isRecentJob(
  job: NormalizedJob,
  maximumAgeDays: number,
  now = new Date(),
): boolean {
  const postedAt = parsePostedAt(job.postedAt, now);
  if (!postedAt) return false;
  const ageMs = Math.max(0, now.getTime() - postedAt.getTime());
  return ageMs <= clamp(maximumAgeDays, 1, 60) * 24 * 60 * 60 * 1_000;
}

export function parsePostedAt(
  value: string | undefined,
  now = new Date(),
): Date | undefined {
  const raw = value?.trim();
  if (!raw) return undefined;
  if (/^\d{10,13}$/.test(raw)) {
    const numeric = Number(raw);
    const date = new Date(raw.length === 10 ? numeric * 1_000 : numeric);
    return Number.isNaN(date.getTime()) ? undefined : date;
  }
  const normalized = raw.toLowerCase();
  if (normalized.includes("today")) return new Date(now);
  if (normalized.includes("yesterday"))
    return new Date(now.getTime() - 24 * 60 * 60 * 1_000);
  const relative = normalized.match(/(\d+)\+?\s*(hour|day|week)s?\s*ago/);
  if (relative) {
    const count = Number(relative[1]);
    const unitMs =
      relative[2] === "hour"
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
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
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

function combineTypedFields(...values: unknown[]): string | undefined {
  const combined = values.map(asOptionalString).filter(Boolean).join(" ");
  return combined || undefined;
}

function asNonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= 0
    ? value
    : undefined;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
