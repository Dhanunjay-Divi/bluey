import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { lookup as dnsLookup } from "node:dns/promises";
import { request as httpsRequest } from "node:https";
import { BlockList, isIP } from "node:net";
import { Readable } from "node:stream";

import {
  extractSmartRecruitersDescription,
  inferWorkplace,
  leverCompensation,
  leverDescription,
  normalizeEmploymentType,
  stripMarkup,
  type FetchResponse,
  type JobsFetch,
} from "./public-ats.js";

const SAFE_IDENTIFIER = /^[A-Za-z0-9_-]+$/;
const SHA256 = /^[a-f0-9]{64}$/;
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;
const MAX_PROVIDER_JSON_DEPTH = 32;
const MAX_PROVIDER_JSON_NODES = 50_000;
const MAX_PROVIDER_ARRAY_ITEMS = 10_000;
const MAX_PROVIDER_OBJECT_FIELDS = 512;
const MAX_PROVIDER_JSON_STRING_BYTES = 128 * 1024;
const MAX_OBSERVED_DESCRIPTION_BYTES = 128 * 1024;
const MAX_OBSERVED_HEADER_BYTES = 4_096;
const DEFAULT_TIMEOUT_MS = 8_000;
const DEFAULT_MAX_ATTEMPTS = 3;
const OBSERVATION_AUDIENCE = "bluey-jobs-original-source-observation-v1";

const NON_PUBLIC_PROVIDER_ADDRESSES = new BlockList();
for (const [network, prefix] of [
  ["0.0.0.0", 8],
  ["10.0.0.0", 8],
  ["100.64.0.0", 10],
  ["127.0.0.0", 8],
  ["169.254.0.0", 16],
  ["172.16.0.0", 12],
  ["192.0.0.0", 24],
  ["192.0.2.0", 24],
  ["192.31.196.0", 24],
  ["192.52.193.0", 24],
  ["192.88.99.0", 24],
  ["192.168.0.0", 16],
  ["192.175.48.0", 24],
  ["198.18.0.0", 15],
  ["198.51.100.0", 24],
  ["203.0.113.0", 24],
  ["224.0.0.0", 4],
  ["240.0.0.0", 4],
] as const) {
  NON_PUBLIC_PROVIDER_ADDRESSES.addSubnet(network, prefix, "ipv4");
}
for (const [network, prefix] of [
  ["::", 128],
  ["::1", 128],
  ["::ffff:0:0", 96],
  ["64:ff9b::", 96],
  ["64:ff9b:1::", 48],
  ["100::", 64],
  ["2001::", 23],
  ["2001:db8::", 32],
  ["2002::", 16],
  ["2620:4f:8000::", 48],
  ["3fff::", 20],
  ["5f00::", 16],
  ["fc00::", 7],
  ["fe80::", 10],
  ["ff00::", 8],
] as const) {
  NON_PUBLIC_PROVIDER_ADDRESSES.addSubnet(network, prefix, "ipv6");
}

export type OriginalSourceProviderFamily =
  | "ashby"
  | "greenhouse"
  | "lever"
  | "smartrecruiters"
  | "workday";

export interface OriginalSourceProviderTarget {
  host: string;
  tenant: string;
  job: string;
  variant: string;
}

export interface OriginalSourceExpectedJob {
  company: string;
  title: string;
  location: string;
  workplace: "hybrid" | "onsite" | "remote" | "unknown";
  description: string;
  compensation: string;
  employment_type: string;
  posted_at_ms: number | null;
  availability_status: string;
}

export interface OriginalSourceVerificationSubject {
  schema_version: 1;
  canonical_job_id: string;
  employer_id: string | null;
  original_url: string;
  provider_family: OriginalSourceProviderFamily;
  provider_record_id: string;
  provider_target: OriginalSourceProviderTarget;
  expected: OriginalSourceExpectedJob;
}

export type OriginalSourceCompletionResult =
  | "closed"
  | "indeterminate"
  | "mismatch"
  | "open"
  | "quarantined";

export type OriginalSourceRetrievalStatus =
  | "absent"
  | "gone"
  | "not_found"
  | "observed"
  | "preflight_rejected"
  | "unreachable";

export type OriginalSourceVerificationFailureCode =
  | "auth_required"
  | "captcha_required"
  | "invalid_assignment"
  | "parse_ambiguous"
  | "provider_unavailable"
  | "rate_limited"
  | "source_untrusted"
  | "unreachable";

export interface OriginalSourceVerificationObservation {
  assurance: "original_verified";
  result: OriginalSourceCompletionResult;
  evidence_sha256: string;
  error_code: OriginalSourceVerificationFailureCode | null;
  requested_url: string | null;
  canonical_observed_url: string | null;
  canonical_application_url: string | null;
  application_domain: string | null;
  retrieval_status: OriginalSourceRetrievalStatus;
  http_status: number | null;
  http_semantics_digest: string;
  redirect_chain_digest: string;
  headers_digest: string;
  content_digest: string;
  parser_version: string;
  parser_digest: string;
  provider_record_id: string | null;
  company: string | null;
  title: string | null;
  location: string | null;
  workplace: string | null;
  description: string | null;
  compensation: string | null;
  employment_type: string | null;
  posted_at_ms: number | null;
  mismatched_fields: string[];
}

export type OriginalSourceVerificationOutcome =
  | { kind: "complete"; observation: OriginalSourceVerificationObservation }
  | {
      kind: "fail";
      error_code: OriginalSourceVerificationFailureCode;
      observation: OriginalSourceVerificationObservation;
    };

export interface OriginalSourceVerifierOptions {
  fetch?: JobsFetch;
  lookup?: OriginalSourceDnsLookup;
  maxAttempts?: number;
  responseBytes?: number;
  sleep?: (milliseconds: number, signal?: AbortSignal) => Promise<void>;
  timeoutMs?: number;
}

export interface OriginalSourceDnsAddress {
  address: string;
  family: 4 | 6;
}

export type OriginalSourceDnsLookup = (
  hostname: string,
) => Promise<readonly OriginalSourceDnsAddress[]>;

interface ObservedJob {
  providerRecordId: string;
  canonicalUrl: string;
  applicationUrl: string;
  company: string;
  title: string;
  location: string;
  workplace: "hybrid" | "onsite" | "remote" | "unknown";
  description: string;
  compensation: string | null;
  employmentType: string | null;
  postedAtMs: number | null;
}

interface ProviderRequest {
  family: OriginalSourceProviderFamily;
  parserDigest: string;
  parserVersion: string;
  url: string;
  init: RequestInit;
  parse(value: unknown): ObservedJob | "closed";
}

interface OriginalSourceRetrievalEvidence {
  requested_url: string | null;
  retrieval_status: OriginalSourceRetrievalStatus;
  http_status: number | null;
  http_semantics_digest: string;
  redirect_chain_digest: string;
  headers_digest: string;
  content_digest: string;
  parser_version: string;
  parser_digest: string;
}

type ProviderRead =
  | { kind: "json"; value: unknown; evidence: OriginalSourceRetrievalEvidence }
  | { kind: "closed"; evidence: OriginalSourceRetrievalEvidence }
  | {
      kind: "fail";
      errorCode: OriginalSourceVerificationFailureCode;
      evidence: OriginalSourceRetrievalEvidence;
    };

interface BoundedProviderBody {
  bytes: Uint8Array;
  text: string;
}

export class OriginalSourceVerifier {
  private readonly fetcher: JobsFetch;
  private readonly maxAttempts: number;
  private readonly responseBytes: number;
  private readonly sleep: (
    milliseconds: number,
    signal?: AbortSignal,
  ) => Promise<void>;
  private readonly timeoutMs: number;

  constructor(options: OriginalSourceVerifierOptions = {}) {
    this.fetcher =
      options.fetch ??
      secureProviderFetch(options.lookup ?? lookupProviderAddresses);
    this.maxAttempts = boundedInteger(
      options.maxAttempts ?? DEFAULT_MAX_ATTEMPTS,
      1,
      5,
      "maximum attempts",
    );
    this.responseBytes = boundedInteger(
      options.responseBytes ?? MAX_RESPONSE_BYTES,
      1_024,
      MAX_RESPONSE_BYTES,
      "response size",
    );
    this.timeoutMs = boundedInteger(
      options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      100,
      30_000,
      "request timeout",
    );
    this.sleep = options.sleep ?? abortableSleep;
  }

  async verify(
    subject: OriginalSourceVerificationSubject,
    signal?: AbortSignal,
  ): Promise<OriginalSourceVerificationOutcome> {
    let exactSubject: OriginalSourceVerificationSubject;
    try {
      exactSubject = parseOriginalSourceVerificationSubject(subject);
    } catch {
      const family =
        providerFamilyFromUntrustedSubject(subject) ?? "assignment";
      const observation = buildFailureObservation(
        "invalid_assignment",
        preflightEvidence(family),
      );
      return { kind: "fail", error_code: "invalid_assignment", observation };
    }

    const trustedUrl = canonicalProviderUrl(exactSubject);
    if (!trustedUrl || trustedUrl !== canonicalUrl(exactSubject.original_url)) {
      return {
        kind: "complete",
        observation: observationForResult(
          "mismatch",
          null,
          ["original_url"],
          preflightEvidence(exactSubject.provider_family),
        ),
      };
    }

    const request = providerRequest(exactSubject);
    const read = await this.requestJson(request, signal);
    if (read.kind === "fail") {
      return {
        kind: "fail",
        error_code: read.errorCode,
        observation: buildFailureObservation(read.errorCode, read.evidence),
      };
    }
    if (read.kind === "closed") {
      return {
        kind: "complete",
        observation: observationForResult("closed", null, [], read.evidence),
      };
    }

    let observed: ObservedJob | "closed";
    try {
      observed = request.parse(read.value);
    } catch {
      const errorCode = "parse_ambiguous";
      return {
        kind: "fail",
        error_code: errorCode,
        observation: buildFailureObservation(errorCode, read.evidence),
      };
    }
    if (observed === "closed") {
      return {
        kind: "complete",
        observation: observationForResult("closed", null, [], {
          ...read.evidence,
          retrieval_status: "absent",
        }),
      };
    }

    const mismatchedFields = mismatchedFieldsFor(exactSubject, observed);
    return {
      kind: "complete",
      observation: observationForResult(
        mismatchedFields.length === 0 ? "open" : "mismatch",
        observed,
        mismatchedFields,
        read.evidence,
      ),
    };
  }

  private async requestJson(
    request: ProviderRequest,
    signal?: AbortSignal,
  ): Promise<ProviderRead> {
    const { url, init } = request;
    assertProviderRequest(url, init);
    for (let attempt = 1; attempt <= this.maxAttempts; attempt += 1) {
      if (signal?.aborted) {
        return {
          kind: "fail",
          errorCode: "unreachable",
          evidence: unreachableEvidence(request),
        };
      }
      const controller = new AbortController();
      const onAbort = (): void => controller.abort();
      signal?.addEventListener("abort", onAbort, { once: true });
      const timer = setTimeout(() => controller.abort(), this.timeoutMs);
      try {
        const response = await this.fetcher(url, {
          ...init,
          redirect: "error",
          signal: controller.signal,
        });
        let body: BoundedProviderBody;
        try {
          body = await boundedResponseBody(response, this.responseBytes);
        } catch (error) {
          return {
            kind: "fail",
            errorCode: "parse_ambiguous",
            evidence: responseEvidence(
              request,
              response,
              error instanceof ProviderBodyEvidenceError
                ? error.bytes
                : new Uint8Array(),
            ),
          };
        }
        const evidence = responseEvidence(request, response, body.bytes);
        if (response.status === 404 || response.status === 410) {
          return {
            kind: "closed",
            evidence: {
              ...evidence,
              retrieval_status: response.status === 404 ? "not_found" : "gone",
            },
          };
        }
        if (response.status === 401 || response.status === 403) {
          return { kind: "fail", errorCode: "auth_required", evidence };
        }
        if (response.status === 429) {
          if (attempt < this.maxAttempts) {
            await this.sleep(100 * 2 ** (attempt - 1), signal);
            continue;
          }
          return { kind: "fail", errorCode: "rate_limited", evidence };
        }
        if (response.status >= 500) {
          if (attempt < this.maxAttempts) {
            await this.sleep(100 * 2 ** (attempt - 1), signal);
            continue;
          }
          return { kind: "fail", errorCode: "provider_unavailable", evidence };
        }
        if (response.status >= 300 || !response.ok) {
          return { kind: "fail", errorCode: "source_untrusted", evidence };
        }
        if (!hasAllowedJsonResponseHeaders(response)) {
          const errorCode = captchaResponse(body.text)
            ? ("captcha_required" as const)
            : ("parse_ambiguous" as const);
          return { kind: "fail", errorCode, evidence };
        }
        try {
          return {
            kind: "json",
            value: parseBoundedProviderJson(body.text),
            evidence,
          };
        } catch {
          return { kind: "fail", errorCode: "parse_ambiguous", evidence };
        }
      } catch (error) {
        if (error instanceof UnsafeProviderResolutionError) {
          return {
            kind: "fail",
            errorCode: "source_untrusted",
            evidence: preflightEvidence(request.family),
          };
        }
        if (attempt < this.maxAttempts && !signal?.aborted) {
          await this.sleep(100 * 2 ** (attempt - 1), signal);
          continue;
        }
        return {
          kind: "fail",
          errorCode: "unreachable",
          evidence: unreachableEvidence(request),
        };
      } finally {
        clearTimeout(timer);
        signal?.removeEventListener("abort", onAbort);
      }
    }
    return {
      kind: "fail",
      errorCode: "unreachable",
      evidence: unreachableEvidence(request),
    };
  }
}

export function parseOriginalSourceVerificationSubject(
  value: unknown,
): OriginalSourceVerificationSubject {
  const row = exactRecord(
    value,
    [
      "canonical_job_id",
      "employer_id",
      "expected",
      "original_url",
      "provider_family",
      "provider_record_id",
      "provider_target",
      "schema_version",
    ],
    "subject",
  );
  if (row.schema_version !== 1)
    throw new Error("Original-source subject version is invalid");
  const providerFamily = providerFamilyValue(row.provider_family);
  const providerTarget = providerTargetValue(row.provider_target);
  const originalUrl = requiredHttpsUrl(row.original_url, "original URL");
  const expected = expectedJobValue(row.expected);
  return {
    schema_version: 1,
    canonical_job_id: requiredString(
      row.canonical_job_id,
      "canonical job ID",
      200,
    ),
    employer_id: optionalString(row.employer_id, "employer ID", 200),
    original_url: originalUrl,
    provider_family: providerFamily,
    provider_record_id: requiredString(
      row.provider_record_id,
      "provider record ID",
      512,
    ),
    provider_target: providerTarget,
    expected,
  };
}

export function canonicalOriginalSourceJson(value: unknown): string {
  return JSON.stringify(canonicalize(value)) + "\n";
}

export function originalSourceSha256(value: string | Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}

export function validOriginalSourceSha256(value: unknown): value is string {
  return typeof value === "string" && SHA256.test(value);
}

function providerRequest(
  subject: OriginalSourceVerificationSubject,
): ProviderRequest {
  switch (subject.provider_family) {
    case "greenhouse": {
      const board = encodeURIComponent(subject.provider_target.tenant);
      const record = encodeURIComponent(subject.provider_target.job);
      return {
        ...providerParserMetadata("greenhouse"),
        url: `https://boards-api.greenhouse.io/v1/boards/${board}/jobs/${record}?content=true`,
        init: jsonGet(),
        parse: (value) => parseGreenhouse(subject, value),
      };
    }
    case "lever": {
      const apiHost =
        subject.provider_target.host === "jobs.eu.lever.co"
          ? "api.eu.lever.co"
          : "api.lever.co";
      const site = encodeURIComponent(subject.provider_target.tenant);
      const record = encodeURIComponent(subject.provider_target.job);
      return {
        ...providerParserMetadata("lever"),
        url: `https://${apiHost}/v0/postings/${site}/${record}?mode=json`,
        init: jsonGet(),
        parse: (value) => parseLever(subject, value),
      };
    }
    case "ashby": {
      const board = encodeURIComponent(subject.provider_target.tenant);
      return {
        ...providerParserMetadata("ashby"),
        url: `https://api.ashbyhq.com/posting-api/job-board/${board}`,
        init: jsonGet(),
        parse: (value) => parseAshby(subject, value),
      };
    }
    case "smartrecruiters": {
      const company = encodeURIComponent(subject.provider_target.tenant);
      const record = encodeURIComponent(subject.provider_target.job);
      return {
        ...providerParserMetadata("smartrecruiters"),
        url: `https://api.smartrecruiters.com/v1/companies/${company}/postings/${record}`,
        init: jsonGet(),
        parse: (value) => parseSmartRecruiters(subject, value),
      };
    }
    case "workday": {
      const target = workdayUrlTarget(subject);
      const tenant = encodeURIComponent(subject.provider_target.tenant);
      const site = encodeURIComponent(target.site);
      return {
        ...providerParserMetadata("workday"),
        url: `https://${subject.provider_target.host}/wday/cxs/${tenant}/${site}${target.externalPath}`,
        init: {
          method: "GET",
          headers: {
            Accept: "application/json",
            "accept-language": target.locale,
          },
        },
        parse: (value) => parseWorkday(subject, value),
      };
    }
  }
}

function parseGreenhouse(
  subject: OriginalSourceVerificationSubject,
  value: unknown,
): ObservedJob {
  const row = record(value, "Greenhouse job");
  const canonicalUrl = greenhouseObservedUrl(payloadString(row.absolute_url));
  const title = aliasedPayloadString("Greenhouse title", row.title, row.name);
  return observedJob(subject, {
    providerRecordId: observedProviderRecordId(subject, scalarId(row.id)),
    canonicalUrl,
    applicationUrl: canonicalUrl,
    title: requiredPayloadString(title, "Greenhouse title"),
    location:
      payloadString(recordOrEmpty(row.location, "Greenhouse location").name) ||
      "Location not listed",
    workplace: workplaceValue(row.workplace_type),
    description: stripMarkup(payloadString(row.content)),
    compensation: null,
    employmentType: aliasedEmploymentType(
      "Greenhouse employment type",
      row.employment_type,
      row.employmentType,
    ),
    postedAtMs: parsedTimestamp(row.updated_at),
  });
}

function parseLever(
  subject: OriginalSourceVerificationSubject,
  value: unknown,
): ObservedJob {
  const row = record(value, "Lever job");
  const categories = recordOrEmpty(row.categories, "Lever categories");
  const destinations = leverObservedDestinations(
    subject,
    payloadString(row.hostedUrl),
    payloadString(row.applyUrl),
  );
  assertDescriptionAliasesAgree(
    "Lever description",
    row.descriptionPlain,
    row.description,
  );
  assertDescriptionAliasesAgree(
    "Lever additional description",
    row.additionalPlain,
    row.additional,
  );
  return observedJob(subject, {
    providerRecordId: observedProviderRecordId(subject, scalarId(row.id)),
    canonicalUrl: destinations.canonicalUrl,
    applicationUrl: destinations.applicationUrl,
    title: requiredPayloadString(row.text, "Lever title"),
    location:
      aliasedPayloadString(
        "Lever location",
        categories.location,
        row.location,
      ) || "Location not listed",
    workplace: workplaceValue(row.workplaceType),
    description: stripMarkup(validatedLeverDescription(row)),
    compensation: validatedLeverCompensation(row.salaryRange),
    employmentType:
      normalizeEmploymentType(payloadString(categories.commitment)) ?? null,
    postedAtMs: parsedTimestamp(row.createdAt),
  });
}

function parseAshby(
  subject: OriginalSourceVerificationSubject,
  value: unknown,
): ObservedJob | "closed" {
  const payload = record(value, "Ashby board");
  if (!Array.isArray(payload.jobs))
    throw new Error("Ashby job list is invalid");
  const rows = payload.jobs.map((item) => record(item, "Ashby job"));
  const ids = rows.map((row) =>
    aliasedScalarId("Ashby job ID", row.id, row.jobId),
  );
  if (ids.some((id) => id.length === 0) || new Set(ids).size !== ids.length) {
    throw new Error("Ashby job identity is ambiguous");
  }
  const matches = rows.filter(
    (row) =>
      aliasedScalarId("Ashby job ID", row.id, row.jobId) ===
      subject.provider_target.job,
  );
  if (matches.length === 0) return "closed";
  if (matches.length !== 1) throw new Error("Ashby job identity is ambiguous");
  const row = matches[0]!;
  optionalBoolean(row.isListed, "Ashby listed state");
  if (row.isListed === false) return "closed";
  const destinations = ashbyObservedDestinations(
    subject,
    payloadString(row.jobUrl),
    payloadString(row.applyUrl),
  );
  return observedJob(subject, {
    providerRecordId: observedProviderRecordId(
      subject,
      subject.provider_target.job,
    ),
    canonicalUrl: destinations.canonicalUrl,
    applicationUrl: destinations.applicationUrl,
    title: requiredPayloadString(row.title, "Ashby title"),
    location: payloadString(row.location) || "Location not listed",
    workplace: workplaceValue(row.workplaceType),
    description: aliasedDescription(
      "Ashby description",
      row.descriptionPlain,
      row.descriptionHtml,
    ),
    compensation: nullablePayloadString(row.compensation),
    employmentType: aliasedEmploymentType(
      "Ashby employment type",
      row.employmentType,
      row.employmentTypeLabel,
    ),
    postedAtMs: parsedTimestamp(row.publishedAt),
  });
}

function parseSmartRecruiters(
  subject: OriginalSourceVerificationSubject,
  value: unknown,
): ObservedJob {
  const row = record(value, "SmartRecruiters job");
  const location = recordOrEmpty(row.location, "SmartRecruiters location");
  const company = recordOrEmpty(row.company, "SmartRecruiters company");
  const employment = recordOrEmpty(
    row.typeOfEmployment,
    "SmartRecruiters employment type",
  );
  const destinations = smartRecruitersObservedDestinations(row);
  return observedJob(subject, {
    providerRecordId: observedProviderRecordId(subject, scalarId(row.id)),
    canonicalUrl: destinations.canonicalUrl,
    applicationUrl: destinations.applicationUrl,
    company: payloadString(company.name) || subject.expected.company,
    title: requiredPayloadString(row.name, "SmartRecruiters title"),
    location:
      [location.city, location.region, location.country]
        .map(payloadString)
        .filter(Boolean)
        .join(", ") || "Location not listed",
    workplace: aliasedWorkplace(
      "SmartRecruiters workplace",
      row.workplaceType,
      location.remote,
    ),
    description: stripMarkup(validatedSmartRecruitersDescription(row)),
    compensation: nullablePayloadString(row.compensation),
    employmentType: aliasedEmploymentType(
      "SmartRecruiters employment type",
      employment.label,
      row.employmentType,
    ),
    postedAtMs: parsedTimestamp(row.releasedDate),
  });
}

function parseWorkday(
  subject: OriginalSourceVerificationSubject,
  value: unknown,
): ObservedJob {
  const payload = record(value, "Workday response");
  const row =
    payload.jobPostingInfo === undefined
      ? payload
      : record(payload.jobPostingInfo, "Workday job");
  const canonicalUrl = workdayObservedUrl(
    subject,
    requiredPayloadString(row.externalUrl, "Workday external URL"),
  );
  return observedJob(subject, {
    providerRecordId: observedProviderRecordId(
      subject,
      aliasedScalarId("Workday job ID", row.jobReqId, row.id, row.jobId),
    ),
    canonicalUrl,
    applicationUrl: canonicalUrl,
    company:
      payloadString(
        recordOrEmpty(row.hiringOrganization, "Workday hiring organization")
          .name,
      ) || subject.expected.company,
    title: requiredPayloadString(row.title, "Workday title"),
    location:
      aliasedPayloadString(
        "Workday location",
        row.location,
        row.locationText,
      ) || "Location not listed",
    workplace: workplaceValue(row.workplaceType),
    description: aliasedDescription(
      "Workday description",
      row.jobDescription,
      row.description,
    ),
    compensation: nullablePayloadString(row.compensation),
    employmentType: aliasedEmploymentType(
      "Workday employment type",
      row.timeType,
      row.employmentType,
    ),
    postedAtMs: aliasedTimestamp(
      "Workday posted time",
      row.startDate,
      row.postedOn,
    ),
  });
}

function optionalBoolean(value: unknown, label: string): boolean | null {
  if (value === undefined || value === null) return null;
  if (typeof value !== "boolean") {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function validatedLeverDescription(row: Record<string, unknown>): string {
  if (row.lists !== undefined && row.lists !== null) {
    if (!Array.isArray(row.lists)) {
      throw new Error("Original-source Lever description list is invalid");
    }
    for (const item of row.lists) {
      const section = record(item, "Lever description section");
      payloadString(section.text);
      payloadString(section.content);
    }
  }
  return leverDescription(row);
}

function validatedLeverCompensation(value: unknown): string | null {
  if (value === undefined || value === null) return null;
  const range = record(value, "Lever salary range");
  payloadString(range.currency);
  payloadString(range.interval);
  const minimum = optionalFiniteNumber(range.min, "Lever salary minimum");
  const maximum = optionalFiniteNumber(range.max, "Lever salary maximum");
  if (minimum === null && maximum === null) {
    throw new Error("Original-source Lever salary range is invalid");
  }
  return leverCompensation(range) ?? null;
}

function optionalFiniteNumber(value: unknown, label: string): number | null {
  if (value === undefined || value === null) return null;
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function validatedSmartRecruitersDescription(
  row: Record<string, unknown>,
): string {
  if (row.jobAd !== undefined && row.jobAd !== null) {
    const jobAd = record(row.jobAd, "SmartRecruiters job ad");
    if (jobAd.sections !== undefined && jobAd.sections !== null) {
      const sections = record(
        jobAd.sections,
        "SmartRecruiters job ad sections",
      );
      for (const section of Object.values(sections)) {
        const item = record(section, "SmartRecruiters job ad section");
        payloadString(item.text);
        payloadString(item.description);
      }
    }
  }
  return extractSmartRecruitersDescription(row);
}

function observedJob(
  subject: OriginalSourceVerificationSubject,
  input: Omit<ObservedJob, "company"> & { company?: string },
): ObservedJob {
  if (
    !input.providerRecordId ||
    !input.canonicalUrl ||
    !input.applicationUrl ||
    !input.title
  ) {
    throw new Error("Provider job identity is incomplete");
  }
  const canonicalObservedUrl = canonicalUrl(input.canonicalUrl);
  const canonicalApplicationUrl = canonicalUrl(input.applicationUrl);
  if (
    Buffer.byteLength(canonicalObservedUrl, "utf8") > 4_096 ||
    Buffer.byteLength(canonicalApplicationUrl, "utf8") > 4_096
  ) {
    throw new Error("Provider canonical destination is too large");
  }
  return {
    ...input,
    providerRecordId: boundedObservedString(
      input.providerRecordId,
      "provider record ID",
      512,
      true,
    ),
    company: boundedObservedString(
      input.company ?? subject.expected.company,
      "company",
      512,
      true,
    ),
    canonicalUrl: canonicalObservedUrl,
    applicationUrl: canonicalApplicationUrl,
    title: boundedObservedString(input.title, "title", 512, true),
    location: boundedObservedString(
      input.location || "Location not listed",
      "location",
      1_024,
      true,
    ),
    description: boundedObservedString(
      input.description,
      "description",
      MAX_OBSERVED_DESCRIPTION_BYTES,
      false,
    ),
    compensation: boundedNullableObservedString(
      input.compensation,
      "compensation",
      2_048,
    ),
    employmentType: boundedNullableObservedString(
      input.employmentType,
      "employment type",
      256,
    ),
  };
}

function observedProviderRecordId(
  subject: OriginalSourceVerificationSubject,
  rawProviderId: string,
): string {
  return rawProviderId === subject.provider_target.job
    ? subject.provider_record_id
    : rawProviderId;
}

function mismatchedFieldsFor(
  subject: OriginalSourceVerificationSubject,
  observed: ObservedJob,
): string[] {
  const expected = subject.expected;
  const mismatches: string[] = [];
  if (canonicalUrl(subject.original_url) !== observed.canonicalUrl) {
    mismatches.push("original_url");
  }
  if (subject.provider_record_id !== observed.providerRecordId) {
    mismatches.push("provider_record_id");
  }
  const exactComparisons: Array<[string, string | null, string | null]> = [
    ["company", expected.company, observed.company],
    ["title", expected.title, observed.title],
    ["location", expected.location, observed.location],
    ["workplace", expected.workplace, observed.workplace],
  ];
  mismatches.push(
    ...exactComparisons
      .filter(([, left, right]) => comparable(left) !== comparable(right))
      .map(([field]) => field),
  );
  const expectedDescription = comparable(expected.description);
  const observedDescription = comparable(observed.description);
  const descriptionMatches =
    subject.provider_family === "workday"
      ? expectedDescription.length > 0 &&
        observedDescription.startsWith(expectedDescription)
      : subject.provider_family === "smartrecruiters" &&
          expectedDescription.length === 0
        ? true
        : expectedDescription === observedDescription;
  if (!descriptionMatches) mismatches.push("description");
  for (const [field, left, right] of [
    ["compensation", expected.compensation, observed.compensation],
    ["employment_type", expected.employment_type, observed.employmentType],
  ] as const) {
    if (comparable(left).length > 0 && comparable(left) !== comparable(right)) {
      mismatches.push(field);
    }
  }
  if (
    expected.posted_at_ms !== null &&
    expected.posted_at_ms !== observed.postedAtMs
  ) {
    mismatches.push("posted_at_ms");
  }
  return mismatches.sort();
}

function observationForResult(
  result: OriginalSourceCompletionResult,
  observed: ObservedJob | null,
  mismatchedFields: string[],
  evidence: OriginalSourceRetrievalEvidence,
  errorCode: OriginalSourceVerificationFailureCode | null = null,
): OriginalSourceVerificationObservation {
  const body = {
    assurance: "original_verified" as const,
    result,
    error_code: errorCode,
    requested_url: evidence.requested_url,
    canonical_observed_url: observed?.canonicalUrl ?? null,
    canonical_application_url: observed?.applicationUrl ?? null,
    application_domain: observed
      ? (safeUrl(observed.applicationUrl)?.hostname ?? null)
      : null,
    retrieval_status: evidence.retrieval_status,
    http_status: evidence.http_status,
    http_semantics_digest: evidence.http_semantics_digest,
    redirect_chain_digest: evidence.redirect_chain_digest,
    headers_digest: evidence.headers_digest,
    content_digest: evidence.content_digest,
    parser_version: evidence.parser_version,
    parser_digest: evidence.parser_digest,
    provider_record_id: observed?.providerRecordId ?? null,
    company: observed?.company ?? null,
    title: observed?.title ?? null,
    location: observed?.location ?? null,
    workplace: observed?.workplace ?? null,
    description: observed?.description ?? null,
    compensation: observed?.compensation ?? null,
    employment_type: observed?.employmentType ?? null,
    posted_at_ms: observed?.postedAtMs ?? null,
    mismatched_fields: [...new Set(mismatchedFields)].sort(),
  };
  return {
    ...body,
    evidence_sha256: originalSourceSha256(
      `${OBSERVATION_AUDIENCE}\0${canonicalOriginalSourceJson(body)}`,
    ),
  };
}

function buildFailureObservation(
  errorCode: OriginalSourceVerificationFailureCode,
  evidence: OriginalSourceRetrievalEvidence,
): OriginalSourceVerificationObservation {
  return observationForResult(
    ["invalid_assignment", "source_untrusted"].includes(errorCode)
      ? "quarantined"
      : "indeterminate",
    null,
    [],
    evidence,
    errorCode,
  );
}

export function originalSourceFailureObservation(
  subject: unknown,
  errorCode: OriginalSourceVerificationFailureCode,
): OriginalSourceVerificationObservation {
  if (errorCode === "invalid_assignment") {
    return buildFailureObservation(
      errorCode,
      preflightEvidence(
        providerFamilyFromUntrustedSubject(subject) ?? "assignment",
      ),
    );
  }
  try {
    return buildFailureObservation(
      errorCode,
      unreachableEvidence(
        providerRequest(parseOriginalSourceVerificationSubject(subject)),
      ),
    );
  } catch {
    return buildFailureObservation(errorCode, preflightEvidence("assignment"));
  }
}

function providerParserMetadata(
  family: OriginalSourceProviderFamily | "assignment",
): Pick<ProviderRequest, "family" | "parserDigest" | "parserVersion"> {
  const parserVersion = `${family}.original_source.v1`;
  return {
    family: family === "assignment" ? "greenhouse" : family,
    parserVersion,
    parserDigest: originalSourceSha256(
      `bluey-jobs-original-source-parser-v1\0${family}\0${parserVersion}\n`,
    ),
  };
}

function preflightEvidence(
  family: OriginalSourceProviderFamily | "assignment",
): OriginalSourceRetrievalEvidence {
  const parser = providerParserMetadata(family);
  return {
    requested_url: null,
    retrieval_status: "preflight_rejected",
    http_status: null,
    http_semantics_digest: emptyEvidenceDigest("http-semantics"),
    redirect_chain_digest: emptyEvidenceDigest("redirect-chain"),
    headers_digest: emptyEvidenceDigest("headers"),
    content_digest: emptyEvidenceDigest("content"),
    parser_version: parser.parserVersion,
    parser_digest: parser.parserDigest,
  };
}

function unreachableEvidence(
  request: ProviderRequest,
): OriginalSourceRetrievalEvidence {
  return {
    requested_url: request.url,
    retrieval_status: "unreachable",
    http_status: null,
    http_semantics_digest: emptyEvidenceDigest("http-semantics"),
    redirect_chain_digest: emptyEvidenceDigest("redirect-chain"),
    headers_digest: emptyEvidenceDigest("headers"),
    content_digest: emptyEvidenceDigest("content"),
    parser_version: request.parserVersion,
    parser_digest: request.parserDigest,
  };
}

function responseEvidence(
  request: ProviderRequest,
  response: FetchResponse,
  contentBytes: Uint8Array,
): OriginalSourceRetrievalEvidence {
  const safeHeaders = {
    cache_control: boundedHeaderEvidence(response, "cache-control"),
    content_encoding: boundedHeaderEvidence(response, "content-encoding"),
    content_length: boundedHeaderEvidence(response, "content-length"),
    content_type: boundedHeaderEvidence(response, "content-type"),
    etag: boundedHeaderEvidence(response, "etag"),
    last_modified: boundedHeaderEvidence(response, "last-modified"),
    location: boundedHeaderEvidence(response, "location"),
    retry_after: boundedHeaderEvidence(response, "retry-after"),
  };
  return {
    requested_url: request.url,
    retrieval_status: "observed",
    http_status: response.status,
    http_semantics_digest: originalSourceSha256(
      `bluey-jobs-original-source-http-semantics-v1\0${canonicalOriginalSourceJson(
        {
          method: "GET",
          requested_url: request.url,
          status: response.status,
        },
      )}`,
    ),
    redirect_chain_digest: originalSourceSha256(
      `bluey-jobs-original-source-redirect-chain-v1\0${canonicalOriginalSourceJson([])}`,
    ),
    headers_digest: originalSourceSha256(
      `bluey-jobs-original-source-headers-v1\0${canonicalOriginalSourceJson(safeHeaders)}`,
    ),
    content_digest: originalSourceSha256(
      Buffer.concat([
        Buffer.from("bluey-jobs-original-source-content-v1\0", "utf8"),
        Buffer.from(contentBytes),
      ]),
    ),
    parser_version: request.parserVersion,
    parser_digest: request.parserDigest,
  };
}

function emptyEvidenceDigest(
  evidenceClass: "content" | "headers" | "http-semantics" | "redirect-chain",
): string {
  return originalSourceSha256(
    `bluey-jobs-original-source-empty-${evidenceClass}-v1\0`,
  );
}

type BoundedHeaderEvidence =
  | { state: "absent" }
  | { state: "observed"; value: string }
  | { byte_length: number; sha256: string; state: "over_limit" };

function boundedHeaderEvidence(
  response: FetchResponse,
  name: string,
): BoundedHeaderEvidence {
  const value = response.headers?.get(name) ?? null;
  if (value === null) return { state: "absent" };
  const byteLength = Buffer.byteLength(value, "utf8");
  if (byteLength <= MAX_OBSERVED_HEADER_BYTES) {
    return { state: "observed", value };
  }
  return {
    byte_length: byteLength,
    sha256: originalSourceSha256(
      Buffer.concat([
        Buffer.from(`bluey-jobs-original-source-header-v1\0${name}\0`, "utf8"),
        Buffer.from(value, "utf8"),
      ]),
    ),
    state: "over_limit",
  };
}

function hasAllowedJsonResponseHeaders(response: FetchResponse): boolean {
  const contentType = response.headers?.get("content-type") ?? null;
  const contentEncoding = response.headers?.get("content-encoding") ?? null;
  if (
    contentType === null ||
    Buffer.byteLength(contentType, "utf8") > MAX_OBSERVED_HEADER_BYTES ||
    (contentEncoding !== null &&
      Buffer.byteLength(contentEncoding, "utf8") > MAX_OBSERVED_HEADER_BYTES)
  ) {
    return false;
  }

  if (
    contentEncoding !== null &&
    contentEncoding.trim().toLowerCase() !== "identity"
  ) {
    return false;
  }

  const parts = contentType.split(";").map((part) => part.trim());
  if (parts[0]?.toLowerCase() !== "application/json") return false;
  if (parts.length === 1) return true;
  if (parts.length !== 2) return false;
  return /^charset\s*=\s*(?:utf-8|"utf-8")$/iu.test(parts[1] ?? "");
}

function captchaResponse(text: string): boolean {
  const sample = text.slice(0, 32 * 1024).toLowerCase();
  return (
    sample.includes("captcha") ||
    sample.includes("verify you are human") ||
    sample.includes("cf-chl-") ||
    sample.includes("g-recaptcha")
  );
}

function providerFamilyFromUntrustedSubject(
  value: unknown,
): OriginalSourceProviderFamily | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const family = (value as Record<string, unknown>).provider_family;
  try {
    return providerFamilyValue(family);
  } catch {
    return null;
  }
}

function canonicalProviderUrl(
  subject: OriginalSourceVerificationSubject,
): string | null {
  const parsed = safeUrl(subject.original_url);
  if (!parsed || parsed.hostname !== subject.provider_target.host) return null;
  const segments = parsed.pathname.split("/").filter(Boolean);
  const { tenant, job, variant } = subject.provider_target;
  switch (subject.provider_family) {
    case "greenhouse":
      if (
        !["boards.greenhouse.io", "job-boards.greenhouse.io"].includes(
          parsed.hostname,
        ) ||
        segments[0] !== tenant ||
        segments[1]?.toLowerCase() !== "jobs" ||
        segments[2] !== job ||
        variant !== "greenhouse_public"
      )
        return null;
      break;
    case "lever":
      if (
        !["jobs.lever.co", "jobs.eu.lever.co"].includes(parsed.hostname) ||
        segments[0] !== tenant ||
        segments[1] !== job ||
        !["lever_posting", "lever_application"].includes(variant)
      )
        return null;
      break;
    case "ashby":
      if (
        parsed.hostname !== "jobs.ashbyhq.com" ||
        segments[0] !== tenant ||
        segments[1] !== job ||
        variant !== "ashby_posting"
      )
        return null;
      break;
    case "smartrecruiters":
      if (
        parsed.hostname !== "jobs.smartrecruiters.com" ||
        segments[0] !== tenant ||
        (segments[1] !== job && !segments[1]?.startsWith(`${job}-`)) ||
        variant !== "smartrecruiters_posting"
      )
        return null;
      break;
    case "workday":
      try {
        workdayUrlTarget(subject);
      } catch {
        return null;
      }
      if (
        !/^[a-z0-9-]+\.wd\d+\.myworkdayjobs\.com$/.test(parsed.hostname) ||
        variant !== "workday_posting"
      )
        return null;
      break;
  }
  return canonicalUrl(parsed.toString());
}

function greenhouseObservedUrl(raw: string): string {
  const parsed = safeUrl(raw);
  const segments = parsed?.pathname.split("/").filter(Boolean) ?? [];
  if (
    !parsed ||
    !["boards.greenhouse.io", "job-boards.greenhouse.io"].includes(
      parsed.hostname,
    ) ||
    segments.length !== 3 ||
    segments[1]?.toLowerCase() !== "jobs" ||
    !safeProviderPathIdentifier(segments[0]) ||
    !safeProviderPathIdentifier(segments[2])
  ) {
    return "";
  }
  return canonicalUrl(parsed.toString());
}

function leverObservedDestinations(
  subject: OriginalSourceVerificationSubject,
  hostedRaw: string,
  applyRaw: string,
): { canonicalUrl: string; applicationUrl: string } {
  const hosted = safeUrl(hostedRaw);
  const apply = safeUrl(applyRaw);
  const hostedSegments = hosted?.pathname.split("/").filter(Boolean) ?? [];
  const applySegments = apply?.pathname.split("/").filter(Boolean) ?? [];
  const expectedHost = subject.provider_target.host;
  if (
    !hosted ||
    !apply ||
    hosted.hostname !== expectedHost ||
    apply.hostname !== expectedHost ||
    hostedSegments.length !== 2 ||
    applySegments.length !== 3 ||
    applySegments[2] !== "apply" ||
    hostedSegments[0] !== applySegments[0] ||
    hostedSegments[1] !== applySegments[1] ||
    !safeProviderPathIdentifier(hostedSegments[0]) ||
    !safeProviderPathIdentifier(hostedSegments[1])
  ) {
    return { canonicalUrl: "", applicationUrl: "" };
  }
  return {
    canonicalUrl: canonicalUrl(
      (subject.provider_target.variant === "lever_application"
        ? apply
        : hosted
      ).toString(),
    ),
    applicationUrl: canonicalUrl(apply.toString()),
  };
}

function ashbyObservedDestinations(
  subject: OriginalSourceVerificationSubject,
  jobRaw: string,
  applyRaw: string,
): { canonicalUrl: string; applicationUrl: string } {
  const parsed = safeUrl(jobRaw);
  const apply = safeUrl(applyRaw);
  const target = subject.provider_target;
  if (
    !parsed ||
    !apply ||
    subject.provider_family !== "ashby" ||
    parsed.hostname !== "jobs.ashbyhq.com" ||
    apply.hostname !== "jobs.ashbyhq.com"
  ) {
    return { canonicalUrl: "", applicationUrl: "" };
  }
  const segments = parsed.pathname.split("/").filter(Boolean);
  const applySegments = apply.pathname.split("/").filter(Boolean);
  if (
    segments.length !== 2 ||
    applySegments.length !== 3 ||
    segments[0] !== applySegments[0] ||
    segments[1] !== applySegments[1] ||
    !["application", "apply"].includes(applySegments[2] ?? "") ||
    !safeProviderPathIdentifier(segments[0]) ||
    !safeProviderPathIdentifier(segments[1])
  ) {
    return { canonicalUrl: "", applicationUrl: "" };
  }
  // Preserve a same-provider identity drift as a typed mismatch instead of
  // silently replacing the provider-returned destination with the subject.
  if (segments[0] !== target.tenant || segments[1] !== target.job) {
    return {
      canonicalUrl: canonicalUrl(parsed.toString()),
      applicationUrl: canonicalUrl(apply.toString()),
    };
  }
  return {
    canonicalUrl: canonicalUrl(parsed.toString()),
    applicationUrl: canonicalUrl(apply.toString()),
  };
}

function smartRecruitersObservedDestinations(row: Record<string, unknown>): {
  canonicalUrl: string;
  applicationUrl: string;
} {
  const company = recordOrEmpty(row.company, "SmartRecruiters company");
  const companyIdentifier = requiredPayloadString(
    company.identifier,
    "SmartRecruiters company identifier",
  );
  const postingId = scalarId(row.id);
  const apply = safeUrl(
    requiredPayloadString(row.applyUrl, "SmartRecruiters apply URL"),
  );
  const applySegments = apply?.pathname.split("/").filter(Boolean) ?? [];
  if (
    !apply ||
    apply.hostname !== "www.smartrecruiters.com" ||
    applySegments.length !== 2 ||
    applySegments[0] !== companyIdentifier ||
    (applySegments[1] !== postingId &&
      !applySegments[1]?.startsWith(`${postingId}-`)) ||
    !safeProviderPathIdentifier(companyIdentifier) ||
    !safeProviderPathIdentifier(postingId)
  ) {
    return { canonicalUrl: "", applicationUrl: "" };
  }
  const reference = payloadString(row.ref);
  if (
    reference &&
    !smartRecruitersReferenceMatches(reference, companyIdentifier, postingId)
  ) {
    return { canonicalUrl: "", applicationUrl: "" };
  }
  // Discovery deliberately canonicalizes SmartRecruiters to the stable
  // tenant/posting identity. The API's apply URL carries a mutable title
  // slug; validate that destination above, then project the stable posting
  // URL so a title-only slug change cannot masquerade as identity drift.
  return {
    canonicalUrl: canonicalUrl(
      `https://jobs.smartrecruiters.com/${encodeURIComponent(companyIdentifier)}` +
        `/${encodeURIComponent(postingId)}`,
    ),
    applicationUrl: canonicalUrl(apply.toString()),
  };
}

function smartRecruitersReferenceMatches(
  raw: string,
  companyIdentifier: string,
  postingId: string,
): boolean {
  const reference = safeUrl(raw);
  if (!reference || reference.hostname !== "api.smartrecruiters.com")
    return false;
  const segments = reference.pathname.split("/").filter(Boolean);
  if (segments[0] === "v1") {
    return (
      segments.length === 5 &&
      segments[1] === "companies" &&
      segments[2] === companyIdentifier &&
      segments[3] === "postings" &&
      segments[4] === postingId
    );
  }
  return (
    segments.length === 5 &&
    segments[0] === "api-v1" &&
    segments[1] === "companies" &&
    segments[2] === companyIdentifier &&
    segments[3] === "postings" &&
    segments[4] === postingId
  );
}

function workdayObservedUrl(
  subject: OriginalSourceVerificationSubject,
  raw: string,
): string {
  const parsed = safeUrl(raw);
  const target = workdayUrlTarget(subject);
  if (!parsed || parsed.hostname !== subject.provider_target.host) return "";
  if (!parsed.pathname.endsWith(target.externalPath)) return "";
  return canonicalUrl(parsed.toString());
}

function safeProviderPathIdentifier(value: string | undefined): boolean {
  return typeof value === "string" && SAFE_IDENTIFIER.test(value);
}

function providerTargetValue(value: unknown): OriginalSourceProviderTarget {
  const row = exactRecord(
    value,
    ["host", "job", "tenant", "variant"],
    "provider target",
  );
  const host = requiredString(row.host, "provider host", 253).toLowerCase();
  if (
    !/^[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?$/.test(host) ||
    host.includes("..")
  ) {
    throw new Error("Original-source provider host is invalid");
  }
  return {
    host,
    tenant: safeProviderIdentifier(row.tenant, "provider tenant"),
    job: safeProviderIdentifier(row.job, "provider job"),
    variant: safeProviderIdentifier(row.variant, "provider variant"),
  };
}

function expectedJobValue(value: unknown): OriginalSourceExpectedJob {
  const row = exactRecord(
    value,
    [
      "availability_status",
      "company",
      "compensation",
      "description",
      "employment_type",
      "location",
      "posted_at_ms",
      "title",
      "workplace",
    ],
    "expected job",
  );
  const workplace = row.workplace;
  if (
    !(["hybrid", "onsite", "remote", "unknown"] as const).includes(
      workplace as never,
    )
  ) {
    throw new Error("Original-source expected workplace is invalid");
  }
  return {
    company: requiredString(row.company, "expected company", 512),
    title: requiredString(row.title, "expected title", 512),
    location: requiredString(row.location, "expected location", 1_024),
    workplace: workplace as OriginalSourceExpectedJob["workplace"],
    description: boundedString(
      row.description,
      "expected description",
      128 * 1024,
    ),
    compensation: boundedString(
      row.compensation,
      "expected compensation",
      2_048,
    ),
    employment_type: boundedString(
      row.employment_type,
      "expected employment type",
      256,
    ),
    posted_at_ms: optionalSafeInteger(row.posted_at_ms, "expected posted time"),
    availability_status: requiredString(
      row.availability_status,
      "availability status",
      64,
    ),
  };
}

function assertProviderRequest(url: string, init: RequestInit): void {
  const parsed = safeUrl(url);
  const allowedHosts = [
    "api.ashbyhq.com",
    "api.eu.lever.co",
    "api.lever.co",
    "api.smartrecruiters.com",
    "boards-api.greenhouse.io",
  ];
  const workdayHostAllowed = parsed
    ? /^[a-z0-9-]+\.wd\d+\.myworkdayjobs\.com$/.test(parsed.hostname)
    : false;
  const headers = new Headers(init.headers);
  if (
    !parsed ||
    (!allowedHosts.includes(parsed.hostname) && !workdayHostAllowed) ||
    (init.method ?? "GET") !== "GET" ||
    init.body !== undefined ||
    headers.has("authorization") ||
    headers.has("cookie")
  ) {
    throw new Error("Original-source provider request is not allowed");
  }
}

class UnsafeProviderResolutionError extends Error {
  constructor() {
    super("Original-source provider resolved outside the public network");
    this.name = "UnsafeProviderResolutionError";
  }
}

async function lookupProviderAddresses(
  hostname: string,
): Promise<OriginalSourceDnsAddress[]> {
  const addresses = await dnsLookup(hostname, { all: true, verbatim: true });
  return addresses.map(({ address, family }) => ({
    address,
    family: family === 6 ? 6 : 4,
  }));
}

function secureProviderFetch(lookup: OriginalSourceDnsLookup): JobsFetch {
  return async (url, init) => {
    assertProviderRequest(url, init);
    const parsed = new URL(url);
    const resolved = [
      ...(await boundedProviderLookup(
        lookup(parsed.hostname),
        init.signal ?? undefined,
      )),
    ];
    if (
      resolved.length === 0 ||
      resolved.length > 16 ||
      resolved.some(
        (entry) =>
          entry.family !== isIP(entry.address) ||
          !publicInternetAddress(entry.address),
      )
    ) {
      throw new UnsafeProviderResolutionError();
    }
    resolved.sort(
      (left, right) =>
        left.family - right.family || left.address.localeCompare(right.address),
    );
    const pinned = resolved[0]!;
    const headers: Record<string, string> = {};
    new Headers(init.headers).forEach((value, key) => {
      headers[key] = value;
    });
    headers.host = parsed.host;
    return new Promise<FetchResponse>((resolve, reject) => {
      const request = httpsRequest(
        {
          protocol: "https:",
          hostname: pinned.address,
          port: 443,
          method: init.method ?? "GET",
          path: `${parsed.pathname}${parsed.search}`,
          headers,
          servername: parsed.hostname,
          rejectUnauthorized: true,
          signal: init.signal ?? undefined,
        },
        (response) => {
          const responseHeaders = new Headers();
          for (const [name, value] of Object.entries(response.headers)) {
            for (const item of Array.isArray(value)
              ? value
              : value === undefined
                ? []
                : [value]) {
              responseHeaders.append(name, String(item));
            }
          }
          resolve({
            ok:
              response.statusCode !== undefined &&
              response.statusCode >= 200 &&
              response.statusCode < 300,
            status: response.statusCode ?? 0,
            headers: responseHeaders,
            body: Readable.toWeb(response) as ReadableStream<Uint8Array>,
            text: async () => {
              throw new Error(
                "Original-source response stream was already exposed",
              );
            },
          });
        },
      );
      request.once("error", reject);
      request.end();
    });
  };
}

function boundedProviderLookup(
  pending: Promise<readonly OriginalSourceDnsAddress[]>,
  signal: AbortSignal | undefined,
): Promise<readonly OriginalSourceDnsAddress[]> {
  if (!signal) return pending;
  if (signal.aborted)
    return Promise.reject(new Error("Original-source DNS lookup aborted"));
  return new Promise((resolve, reject) => {
    const aborted = (): void => {
      signal.removeEventListener("abort", aborted);
      reject(new Error("Original-source DNS lookup aborted"));
    };
    signal.addEventListener("abort", aborted, { once: true });
    pending.then(
      (value) => {
        signal.removeEventListener("abort", aborted);
        resolve(value);
      },
      (error: unknown) => {
        signal.removeEventListener("abort", aborted);
        reject(error);
      },
    );
  });
}

function publicInternetAddress(address: string): boolean {
  const family = isIP(address);
  if (family === 4)
    return !NON_PUBLIC_PROVIDER_ADDRESSES.check(address, "ipv4");
  if (family !== 6 || NON_PUBLIC_PROVIDER_ADDRESSES.check(address, "ipv6"))
    return false;

  // Limit verifier egress to globally routable unicast. The explicit block list above removes
  // transition, documentation, translation, local, multicast, and other special-purpose ranges
  // that happen to sit inside 2000::/3.
  const first = Number.parseInt(address.split(":", 1)[0] ?? "", 16);
  return Number.isFinite(first) && first >= 0x2000 && first <= 0x3fff;
}

function safeUrl(value: string): URL | null {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return null;
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.port ||
    url.hash
  )
    return null;
  return url;
}

function canonicalUrl(value: string): string {
  const url = safeUrl(value);
  if (!url) return "";
  for (const key of [...url.searchParams.keys()]) {
    if (/^(?:gh_src|lever-source|source|utm_.+)$/i.test(key))
      url.searchParams.delete(key);
  }
  url.pathname = url.pathname.replace(/\/+$/, "") || "/";
  return url.toString();
}

function requiredHttpsUrl(value: unknown, label: string): string {
  const raw = requiredString(value, label, 4_096);
  const canonical = canonicalUrl(raw);
  if (!canonical) throw new Error(`Original-source ${label} is invalid`);
  return canonical;
}

function workdayUrlTarget(subject: OriginalSourceVerificationSubject): {
  locale: string;
  site: string;
  externalPath: string;
} {
  const parsed = safeUrl(subject.original_url);
  if (!parsed || parsed.hostname !== subject.provider_target.host) {
    throw new Error("Original-source Workday URL is invalid");
  }
  const segments = parsed.pathname.split("/").filter(Boolean);
  const jobIndex = segments.indexOf("job");
  if (jobIndex < 2 || jobIndex === segments.length - 1) {
    throw new Error("Original-source Workday path is invalid");
  }
  const locale = safeProviderIdentifier(segments[0], "Workday locale");
  const site = safeProviderIdentifier(segments[1], "Workday site");
  const externalPath = `/${segments.slice(jobIndex).map(encodeURIComponent).join("/")}`;
  const urlRecord = segments.at(-1) ?? "";
  if (
    subject.provider_target.tenant.length === 0 ||
    (urlRecord !== subject.provider_target.job &&
      !urlRecord.endsWith(`_${subject.provider_target.job}`))
  ) {
    throw new Error("Original-source Workday target is invalid");
  }
  return { locale, site, externalPath };
}

class ProviderBodyEvidenceError extends Error {
  readonly bytes: Uint8Array;

  constructor(message: string, name: string, bytes: Uint8Array) {
    super(message);
    this.name = name;
    this.bytes = Uint8Array.from(bytes);
  }
}

class MalformedProviderEncodingError extends ProviderBodyEvidenceError {
  constructor(bytes: Uint8Array) {
    super(
      "Original-source response is not valid UTF-8",
      "MalformedProviderEncodingError",
      bytes,
    );
  }
}

class OversizedProviderBodyError extends ProviderBodyEvidenceError {
  constructor(bytes: Uint8Array) {
    super(
      "Original-source response exceeded the size limit",
      "OversizedProviderBodyError",
      bytes,
    );
  }
}

async function boundedResponseBody(
  response: FetchResponse,
  maximumBytes: number,
): Promise<BoundedProviderBody> {
  const declared = Number(response.headers?.get("content-length"));
  if (Number.isFinite(declared) && declared > maximumBytes) {
    throw new OversizedProviderBodyError(new Uint8Array());
  }
  if (!response.body) {
    throw new Error("Original-source response did not expose exact bytes");
  }
  const reader = response.body.getReader();
  let received = 0;
  const chunks: Buffer[] = [];
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      const previousReceived = received;
      received += value.byteLength;
      if (received > maximumBytes) {
        const captureLength = Math.max(0, maximumBytes + 1 - previousReceived);
        const evidence = Buffer.concat(
          [
            ...chunks,
            ...(captureLength > 0
              ? [Buffer.from(value).subarray(0, captureLength)]
              : []),
          ],
          Math.min(received, maximumBytes + 1),
        );
        await reader.cancel("response limit exceeded");
        throw new OversizedProviderBodyError(evidence);
      }
      chunks.push(Buffer.from(value));
    }
    const bytes = Buffer.concat(chunks, received);
    try {
      return {
        bytes,
        text: new TextDecoder("utf-8", { fatal: true }).decode(bytes),
      };
    } catch {
      throw new MalformedProviderEncodingError(bytes);
    }
  } finally {
    reader.releaseLock();
  }
}

function parseBoundedProviderJson(text: string): unknown {
  assertNoDuplicateProviderJsonKeys(text);
  const value = JSON.parse(text) as unknown;
  const pending: Array<{ depth: number; value: unknown }> = [
    { depth: 0, value },
  ];
  let visited = 0;
  while (pending.length > 0) {
    const current = pending.pop()!;
    visited += 1;
    if (
      visited > MAX_PROVIDER_JSON_NODES ||
      current.depth > MAX_PROVIDER_JSON_DEPTH
    ) {
      throw new Error("Original-source provider JSON exceeded parser limits");
    }
    if (typeof current.value === "string") {
      if (
        Buffer.byteLength(current.value, "utf8") >
        MAX_PROVIDER_JSON_STRING_BYTES
      ) {
        throw new Error(
          "Original-source provider string exceeded parser limits",
        );
      }
      continue;
    }
    if (Array.isArray(current.value)) {
      if (current.value.length > MAX_PROVIDER_ARRAY_ITEMS) {
        throw new Error(
          "Original-source provider array exceeded parser limits",
        );
      }
      for (const item of current.value) {
        pending.push({ depth: current.depth + 1, value: item });
      }
      continue;
    }
    if (current.value && typeof current.value === "object") {
      const entries = Object.entries(current.value as Record<string, unknown>);
      if (entries.length > MAX_PROVIDER_OBJECT_FIELDS) {
        throw new Error(
          "Original-source provider object exceeded parser limits",
        );
      }
      for (const [key, item] of entries) {
        if (Buffer.byteLength(key, "utf8") > 256) {
          throw new Error(
            "Original-source provider key exceeded parser limits",
          );
        }
        pending.push({ depth: current.depth + 1, value: item });
      }
    }
  }
  return value;
}

function assertNoDuplicateProviderJsonKeys(text: string): void {
  let cursor = 0;
  let visited = 0;

  const skipWhitespace = (): void => {
    while (cursor < text.length && /\s/u.test(text[cursor]!)) cursor += 1;
  };
  const scanString = (): string => {
    const start = cursor;
    if (text[cursor] !== '"')
      throw new Error("Provider JSON string is invalid");
    cursor += 1;
    while (cursor < text.length) {
      const code = text.charCodeAt(cursor);
      const character = text[cursor]!;
      if (character === '"') {
        cursor += 1;
        return text.slice(start, cursor);
      }
      if (code <= 0x1f) throw new Error("Provider JSON string is invalid");
      if (character === "\\") {
        cursor += 1;
        const escape = text[cursor];
        if (escape === "u") {
          const scalar = text.slice(cursor + 1, cursor + 5);
          if (!/^[0-9a-fA-F]{4}$/.test(scalar)) {
            throw new Error("Provider JSON string escape is invalid");
          }
          cursor += 5;
          continue;
        }
        if (!escape || !'"\\/bfnrt'.includes(escape)) {
          throw new Error("Provider JSON string escape is invalid");
        }
        cursor += 1;
        continue;
      }
      cursor += 1;
    }
    throw new Error("Provider JSON string is unterminated");
  };
  const parseValue = (depth: number): void => {
    visited += 1;
    if (visited > MAX_PROVIDER_JSON_NODES || depth > MAX_PROVIDER_JSON_DEPTH) {
      throw new Error("Original-source provider JSON exceeded parser limits");
    }
    skipWhitespace();
    const character = text[cursor];
    if (character === "{") {
      cursor += 1;
      skipWhitespace();
      const keys = new Set<string>();
      let fields = 0;
      if (text[cursor] === "}") {
        cursor += 1;
        return;
      }
      while (cursor < text.length) {
        skipWhitespace();
        const rawKey = scanString();
        const key = JSON.parse(rawKey) as unknown;
        if (typeof key !== "string" || Buffer.byteLength(key, "utf8") > 256) {
          throw new Error(
            "Original-source provider key exceeded parser limits",
          );
        }
        fields += 1;
        if (fields > MAX_PROVIDER_OBJECT_FIELDS) {
          throw new Error(
            "Original-source provider object exceeded parser limits",
          );
        }
        if (keys.has(key)) {
          throw new Error(
            "Original-source provider JSON contains a duplicate object key",
          );
        }
        keys.add(key);
        skipWhitespace();
        if (text[cursor] !== ":")
          throw new Error("Provider JSON object is invalid");
        cursor += 1;
        parseValue(depth + 1);
        skipWhitespace();
        if (text[cursor] === "}") {
          cursor += 1;
          return;
        }
        if (text[cursor] !== ",")
          throw new Error("Provider JSON object is invalid");
        cursor += 1;
      }
      throw new Error("Provider JSON object is unterminated");
    }
    if (character === "[") {
      cursor += 1;
      skipWhitespace();
      let items = 0;
      if (text[cursor] === "]") {
        cursor += 1;
        return;
      }
      while (cursor < text.length) {
        items += 1;
        if (items > MAX_PROVIDER_ARRAY_ITEMS) {
          throw new Error(
            "Original-source provider array exceeded parser limits",
          );
        }
        parseValue(depth + 1);
        skipWhitespace();
        if (text[cursor] === "]") {
          cursor += 1;
          return;
        }
        if (text[cursor] !== ",")
          throw new Error("Provider JSON array is invalid");
        cursor += 1;
      }
      throw new Error("Provider JSON array is unterminated");
    }
    if (character === '"') {
      scanString();
      return;
    }
    const start = cursor;
    while (cursor < text.length && !/[\s,\]}]/u.test(text[cursor]!))
      cursor += 1;
    if (cursor === start) throw new Error("Provider JSON value is invalid");
  };

  skipWhitespace();
  parseValue(0);
  skipWhitespace();
  if (cursor !== text.length)
    throw new Error("Provider JSON has trailing content");
}

function jsonGet(): RequestInit {
  return { method: "GET", headers: { Accept: "application/json" } };
}

function providerFamilyValue(value: unknown): OriginalSourceProviderFamily {
  const families: OriginalSourceProviderFamily[] = [
    "ashby",
    "greenhouse",
    "lever",
    "smartrecruiters",
    "workday",
  ];
  if (
    typeof value !== "string" ||
    !families.includes(value as OriginalSourceProviderFamily)
  ) {
    throw new Error("Original-source provider family is invalid");
  }
  return value as OriginalSourceProviderFamily;
}

function exactRecord(
  value: unknown,
  keys: readonly string[],
  label: string,
): Record<string, unknown> {
  const row = record(value, label);
  const actual = Object.keys(row).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error(`Original-source ${label} shape is invalid`);
  }
  return row;
}

function record(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value as Record<string, unknown>;
}

function recordOrEmpty(value: unknown, label: string): Record<string, unknown> {
  if (value === undefined || value === null) return {};
  return record(value, label);
}

function requiredString(
  value: unknown,
  label: string,
  maximumBytes: number,
): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value !== value.trim() ||
    Buffer.byteLength(value, "utf8") > maximumBytes ||
    value.includes("\ufffd") ||
    /\p{Cc}/u.test(value)
  ) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function optionalString(
  value: unknown,
  label: string,
  maximumBytes: number,
): string | null {
  if (value === null) return null;
  return requiredString(value, label, maximumBytes);
}

function boundedString(
  value: unknown,
  label: string,
  maximumBytes: number,
): string {
  if (
    typeof value !== "string" ||
    value !== value.trim() ||
    Buffer.byteLength(value, "utf8") > maximumBytes ||
    value.includes("\ufffd") ||
    /\p{Cc}/u.test(value)
  ) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function optionalSafeInteger(value: unknown, label: string): number | null {
  if (value === null) return null;
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value as number;
}

function safeProviderIdentifier(value: unknown, label: string): string {
  const item = requiredString(value, label, 160);
  if (!SAFE_IDENTIFIER.test(item))
    throw new Error(`Original-source ${label} is invalid`);
  return item;
}

function scalarId(value: unknown): string {
  if (value === undefined) return "";
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) {
    return String(value);
  }
  if (typeof value === "string") {
    const normalized = normalizeWhitespace(value);
    if (normalized) return normalized;
  }
  throw new Error("Original-source provider ID is invalid");
}

function aliasedScalarId(label: string, ...values: unknown[]): string {
  const supplied = values.map(scalarId).filter(Boolean);
  if (new Set(supplied).size > 1) {
    throw new Error(`Original-source ${label} aliases conflict`);
  }
  return supplied[0] ?? "";
}

function payloadString(value: unknown): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") return normalizeWhitespace(value);
  throw new Error("Original-source provider scalar is invalid");
}

function aliasedPayloadString(label: string, ...values: unknown[]): string {
  const supplied = values.map(payloadString).filter(Boolean);
  if (new Set(supplied.map((value) => comparable(value))).size > 1) {
    throw new Error(`Original-source ${label} aliases conflict`);
  }
  return supplied[0] ?? "";
}

function aliasedDescription(label: string, ...values: unknown[]): string {
  const supplied = values.map(payloadString).filter(Boolean).map(stripMarkup);
  if (new Set(supplied.map((value) => comparable(value))).size > 1) {
    throw new Error(`Original-source ${label} aliases conflict`);
  }
  return supplied[0] ?? "";
}

function assertDescriptionAliasesAgree(
  label: string,
  ...values: unknown[]
): void {
  aliasedDescription(label, ...values);
}

function aliasedEmploymentType(
  label: string,
  ...values: unknown[]
): string | null {
  const supplied = values.map(payloadString).filter(Boolean);
  const normalized = supplied.map(
    (value) => normalizeEmploymentType(value) ?? `unknown:${comparable(value)}`,
  );
  if (new Set(normalized).size > 1) {
    throw new Error(`Original-source ${label} aliases conflict`);
  }
  return supplied.length === 0
    ? null
    : (normalizeEmploymentType(supplied[0]!) ?? null);
}

function aliasedWorkplace(
  label: string,
  ...values: unknown[]
): ObservedJob["workplace"] {
  const supplied = values.filter((value) => {
    if (value === null || value === undefined) return false;
    if (typeof value !== "string" && typeof value !== "boolean") {
      throw new Error(`Original-source ${label} alias is invalid`);
    }
    return typeof value !== "string" || normalizeWhitespace(value).length > 0;
  });
  const normalized = supplied
    .map(workplaceValue)
    .filter((value) => value !== "unknown");
  if (new Set(normalized).size > 1) {
    throw new Error(`Original-source ${label} aliases conflict`);
  }
  return normalized[0] ?? "unknown";
}

function aliasedTimestamp(label: string, ...values: unknown[]): number | null {
  const supplied = values.filter(
    (value) =>
      value !== null &&
      value !== undefined &&
      (typeof value !== "string" || value.trim().length > 0),
  );
  const parsed = supplied.map(parsedTimestamp);
  if (
    supplied.length > 1 &&
    (parsed.some((value) => value === null) || new Set(parsed).size > 1)
  ) {
    throw new Error(`Original-source ${label} aliases conflict`);
  }
  return parsed[0] ?? null;
}

function requiredPayloadString(value: unknown, label: string): string {
  const result = payloadString(value);
  if (!result) throw new Error(`${label} is missing`);
  return result;
}

function nullablePayloadString(value: unknown): string | null {
  return nullableNormalizedString(payloadString(value));
}

function nullableNormalizedString(
  value: string | null | undefined,
): string | null {
  const normalized = normalizeWhitespace(value ?? "");
  return normalized || null;
}

function boundedObservedString(
  value: string,
  label: string,
  maximumBytes: number,
  required: boolean,
): string {
  const normalized = normalizeWhitespace(value);
  if (
    (required && normalized.length === 0) ||
    Buffer.byteLength(normalized, "utf8") > maximumBytes ||
    normalized.includes("\ufffd") ||
    /\p{Cc}/u.test(normalized)
  ) {
    throw new Error(`Original-source observed ${label} is invalid`);
  }
  return normalized;
}

function boundedNullableObservedString(
  value: string | null | undefined,
  label: string,
  maximumBytes: number,
): string | null {
  const normalized = boundedObservedString(
    value ?? "",
    label,
    maximumBytes,
    false,
  );
  return normalized || null;
}

function normalizeWhitespace(value: string): string {
  return value.replace(/\s+/gu, " ").trim();
}

function workplaceValue(value: unknown): ObservedJob["workplace"] {
  if (value === undefined || value === null || value === false)
    return "unknown";
  if (value === true) return "remote";
  if (typeof value !== "string") {
    throw new Error("Original-source provider workplace is invalid");
  }
  return inferWorkplace(payloadString(value));
}

function parsedTimestamp(value: unknown): number | null {
  if (value === undefined || value === null || value === "") return null;
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0)
    return value;
  if (typeof value !== "string" || !value.trim()) {
    throw new Error("Original-source provider timestamp is invalid");
  }
  const parsed = Date.parse(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error("Original-source provider timestamp is invalid");
  }
  return parsed;
}

function comparable(value: string | null): string {
  return normalizeWhitespace(value ?? "")
    .normalize("NFKC")
    .toLowerCase();
}

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value === null || typeof value === "boolean" || typeof value === "string")
    return value;
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) {
      throw new Error("Original-source canonical number is invalid");
    }
    return value;
  }
  if (value && typeof value === "object") {
    const result: Record<string, unknown> = {};
    const row = value as Record<string, unknown>;
    for (const key of Object.keys(row).sort())
      result[key] = canonicalize(row[key]);
    return result;
  }
  throw new Error("Original-source canonical value is invalid");
}

function boundedInteger(
  value: number,
  minimum: number,
  maximum: number,
  label: string,
): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function abortableSleep(
  milliseconds: number,
  signal?: AbortSignal,
): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted) return resolve();
    const timer = setTimeout(done, milliseconds);
    signal?.addEventListener("abort", done, { once: true });
    function done(): void {
      clearTimeout(timer);
      signal?.removeEventListener("abort", done);
      resolve();
    }
  });
}
