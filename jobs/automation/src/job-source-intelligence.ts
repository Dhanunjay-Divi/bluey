import { createHash } from "node:crypto";

import type { NormalizedJob } from "./contracts.js";

const MIN_FINGERPRINT_TEXT = 200;
const DEFAULT_CROSS_LISTING_THRESHOLD = 0.92;

const SUSPICIOUS_REDIRECT_HOSTS = new Set([
  "bit.ly",
  "cutt.ly",
  "forms.gle",
  "goo.gl",
  "rebrand.ly",
  "shorturl.at",
  "t.co",
  "tinyurl.com",
]);

const KNOWN_ATS_HOSTS = [
  "applytojob.com",
  "ashbyhq.com",
  "bamboohr.com",
  "breezy.hr",
  "greenhouse.io",
  "icims.com",
  "jazz.co",
  "jobvite.com",
  "lever.co",
  "myworkdayjobs.com",
  "oraclecloud.com",
  "recruitee.com",
  "smartrecruiters.com",
  "successfactors.com",
  "taleo.net",
  "teamtailor.com",
  "workable.com",
  "workday.com",
];

const COMPANY_TOKEN_STOP_WORDS = new Set([
  "and",
  "company",
  "corp",
  "corporation",
  "global",
  "group",
  "inc",
  "international",
  "llc",
  "services",
  "solutions",
  "systems",
  "technologies",
  "technology",
  "the",
]);

export type SourceTrustFlag =
  | "missing_application_url"
  | "invalid_application_url"
  | "insecure_application_url"
  | "redirector_application_url"
  | "company_domain_mismatch";

export type SourceTrustLevel = "high" | "medium" | "low";

export interface SourceTrustAssessment {
  score: number;
  level: SourceTrustLevel;
  flags: SourceTrustFlag[];
  hostname: string | null;
  requiresOriginalRevalidation: true;
}

export interface PotentialCrossListing {
  firstExternalId: string;
  firstCanonicalUrl: string;
  firstCompany: string;
  secondExternalId: string;
  secondCanonicalUrl: string;
  secondCompany: string;
  similarity: number;
}

/**
 * Advisory source assessment only. Eligibility and submission authority remain
 * server-owned and always require a fresh check against the original source.
 */
export function assessJobSourceTrust(input: {
  applicationUrl?: string;
  company?: string;
}): SourceTrustAssessment {
  const applicationUrl = input.applicationUrl?.trim() ?? "";
  if (!applicationUrl) {
    return trustResult(45, ["missing_application_url"], null);
  }

  let parsed: URL;
  try {
    parsed = new URL(applicationUrl);
  } catch {
    return trustResult(30, ["invalid_application_url"], null);
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return trustResult(30, ["invalid_application_url"], parsed.hostname.toLowerCase() || null);
  }

  const hostname = parsed.hostname.toLowerCase();
  const flags: SourceTrustFlag[] = [];
  let score = 100;

  if (parsed.protocol !== "https:") {
    flags.push("insecure_application_url");
    score -= 35;
  }
  if (matchesHost(hostname, [...SUSPICIOUS_REDIRECT_HOSTS])) {
    flags.push("redirector_application_url");
    score -= 45;
  }

  const company = input.company?.trim() ?? "";
  if (company && !matchesHost(hostname, KNOWN_ATS_HOSTS) && !companyMatchesHost(company, hostname)) {
    flags.push("company_domain_mismatch");
    score -= 15;
  }

  return trustResult(score, flags, hostname);
}

/**
 * Detects near-identical descriptions under different URLs and employer names.
 * Signals are kept separate; they must never silently collapse applications.
 */
export function findPotentialCrossListings(
  jobs: readonly NormalizedJob[],
  threshold = DEFAULT_CROSS_LISTING_THRESHOLD,
): PotentialCrossListing[] {
  const candidates = jobs
    .map((job) => ({ job, fingerprint: fingerprintDescription(job.description) }))
    .filter((candidate) => candidate.fingerprint !== "");
  const matches: PotentialCrossListing[] = [];
  const seenSignals = new Set<string>();

  for (let leftIndex = 0; leftIndex < candidates.length; leftIndex += 1) {
    const left = candidates[leftIndex]!;
    for (let rightIndex = leftIndex + 1; rightIndex < candidates.length; rightIndex += 1) {
      const right = candidates[rightIndex]!;
      if (canonicalComparable(left.job.canonicalUrl) === canonicalComparable(right.job.canonicalUrl)) continue;
      if (companyComparable(left.job.company) === companyComparable(right.job.company)) continue;

      const similarity = fingerprintSimilarity(left.fingerprint, right.fingerprint);
      if (similarity < threshold) continue;
      const signalKey = [
        [companyComparable(left.job.company), companyComparable(right.job.company)].sort().join("|"),
        [left.fingerprint, right.fingerprint].sort().join("|"),
      ].join("::");
      if (seenSignals.has(signalKey)) continue;
      seenSignals.add(signalKey);
      matches.push({
        firstExternalId: left.job.externalId,
        firstCanonicalUrl: left.job.canonicalUrl,
        firstCompany: left.job.company,
        secondExternalId: right.job.externalId,
        secondCanonicalUrl: right.job.canonicalUrl,
        secondCompany: right.job.company,
        similarity,
      });
    }
  }

  return matches.sort((left, right) => right.similarity - left.similarity);
}

export function fingerprintDescription(text: string): string {
  const normalized = normalizeDescription(text);
  if (normalized.length < MIN_FINGERPRINT_TEXT) return "";
  const tokens = normalized.split(" ");
  if (tokens.length < 3) return "";

  const weights = new Array<number>(64).fill(0);
  for (let index = 0; index <= tokens.length - 3; index += 1) {
    const shingle = `${tokens[index]} ${tokens[index + 1]} ${tokens[index + 2]}`;
    const digest = createHash("sha1").update(shingle, "utf8").digest();
    for (let bit = 0; bit < 64; bit += 1) {
      const byte = digest[bit >> 3]!;
      weights[bit] += (byte >> (7 - (bit & 7))) & 1 ? 1 : -1;
    }
  }

  let fingerprint = 0n;
  for (let bit = 0; bit < weights.length; bit += 1) {
    if (weights[bit]! > 0) fingerprint |= 1n << BigInt(63 - bit);
  }
  return fingerprint.toString(16).padStart(16, "0");
}

export function fingerprintSimilarity(first: string, second: string): number {
  if (!/^[0-9a-f]{16}$/.test(first) || !/^[0-9a-f]{16}$/.test(second)) return 0;
  let difference = BigInt(`0x${first}`) ^ BigInt(`0x${second}`);
  let distance = 0;
  while (difference > 0n) {
    distance += Number(difference & 1n);
    difference >>= 1n;
  }
  return 1 - distance / 64;
}

function normalizeDescription(text: string): string {
  return text
    .toLowerCase()
    .replace(/<[^>]*>/g, " ")
    .replace(/&[a-z#0-9]+;/gi, " ")
    .replace(/https?:\/\/\S+/g, " ")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .replace(/\s{2,}/g, " ")
    .trim();
}

function companyMatchesHost(company: string, hostname: string): boolean {
  const normalized = company.toLowerCase().replace(/[^a-z0-9 ]/g, " ").trim();
  if (!normalized || !hostname) return true;
  const slug = normalized.replace(/\s+/g, "");
  if (slug.length >= 3 && hostname.includes(slug)) return true;

  return normalized
    .split(/\s+/)
    .filter((token) => token.length >= 3 && !COMPANY_TOKEN_STOP_WORDS.has(token))
    .some((token) => hostname.includes(token));
}

function matchesHost(hostname: string, domains: readonly string[]): boolean {
  return domains.some((domain) => hostname === domain || hostname.endsWith(`.${domain}`));
}

function trustResult(
  score: number,
  flags: SourceTrustFlag[],
  hostname: string | null,
): SourceTrustAssessment {
  const boundedScore = Math.max(0, Math.min(100, score));
  const level: SourceTrustLevel = boundedScore >= 90 ? "high" : boundedScore >= 60 ? "medium" : "low";
  return {
    score: boundedScore,
    level,
    flags,
    hostname,
    requiresOriginalRevalidation: true,
  };
}

function canonicalComparable(url: string): string {
  try {
    const parsed = new URL(url);
    parsed.hash = "";
    return parsed.toString().replace(/\/$/, "").toLowerCase();
  } catch {
    return url.trim().toLowerCase();
  }
}

function companyComparable(company: string): string {
  return company.toLowerCase().replace(/[^a-z0-9]/g, "");
}
