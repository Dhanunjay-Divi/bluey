import { parseFragment, type DefaultTreeAdapterMap } from "parse5";

import type { PublicAtsSource } from "./contracts.js";
import { submissionPolicy, type SubmissionCapability } from "./policy.js";

const MAX_FEED_BYTES = 2 * 1024 * 1024;
const MAX_FEED_ROWS = 20_000;
const DEFAULT_TIMEOUT_MS = 10_000;

export type CuratedFeedId =
  | "simplify-new-grad"
  | "prepai-internships"
  | "prepai-new-grad"
  | "zapply-new-grad";

export interface CuratedFeedDescriptor {
  id: CuratedFeedId;
  sourceCatalogId: string;
  name: string;
  rawUrl: string;
  defaultEmploymentType: CuratedEmploymentType;
}

export type CuratedEmploymentType =
  | "full_time"
  | "part_time"
  | "contract"
  | "temporary"
  | "internship"
  | "apprenticeship"
  | "seasonal"
  | "per_diem";

export type CuratedEngagementType = "w2" | "c2c" | "1099" | "direct_hire";

export interface CuratedFeedLead {
  feedId: CuratedFeedId;
  sourceCatalogId: string;
  company: string;
  title: string;
  location: string;
  originalUrl: string;
  postedLabel: string;
  ageDays: number | null;
  employmentType: CuratedEmploymentType;
  engagementType: CuratedEngagementType | null;
  categoryEvidence: "explicit" | "feed_default";
  submissionCapability: SubmissionCapability;
  atsSource: PublicAtsSource | null;
  requiresOriginalRevalidation: true;
}

export interface CuratedFeedResult {
  descriptor: CuratedFeedDescriptor;
  leads: CuratedFeedLead[];
  skippedClosed: number;
  skippedInvalid: number;
  duplicatesCollapsed: number;
  etag?: string;
}

export interface CuratedFeedFetchOptions {
  fetch?: typeof fetch;
  timeoutMs?: number;
  ifNoneMatch?: string;
}

export class CuratedFeedError extends Error {
  readonly code: "invalid_feed" | "not_modified" | "too_large" | "unavailable";

  constructor(code: CuratedFeedError["code"], message: string) {
    super(message);
    this.name = "CuratedFeedError";
    this.code = code;
  }
}

export const CURATED_JOB_FEEDS: readonly CuratedFeedDescriptor[] = [
  {
    id: "simplify-new-grad",
    sourceCatalogId: "feed-simplify-new-grad",
    name: "Simplify New Grad Positions",
    rawUrl: "https://raw.githubusercontent.com/SimplifyJobs/New-Grad-Positions/dev/README.md",
    defaultEmploymentType: "full_time",
  },
  {
    id: "prepai-internships",
    sourceCatalogId: "feed-prepai-internships",
    name: "PrepAIJobs Summer Internships",
    rawUrl: "https://raw.githubusercontent.com/PrepAIJobs/Summer2026-Internships/main/README.md",
    defaultEmploymentType: "internship",
  },
  {
    id: "prepai-new-grad",
    sourceCatalogId: "feed-prepai-new-grad",
    name: "PrepAIJobs New Grad",
    rawUrl: "https://raw.githubusercontent.com/PrepAIJobs/New-Grad-2026/main/README.md",
    defaultEmploymentType: "full_time",
  },
  {
    id: "zapply-new-grad",
    sourceCatalogId: "feed-zapply-new-grad",
    name: "Zapply New Grad Jobs",
    rawUrl: "https://raw.githubusercontent.com/zapplyjobs/New-Grad-Jobs-2027/main/README.md",
    defaultEmploymentType: "full_time",
  },
];

type Node = DefaultTreeAdapterMap["node"];
type Element = DefaultTreeAdapterMap["element"];

interface TableCell {
  text: string;
  links: string[];
}

interface TableRow {
  cells: TableCell[];
}

export async function fetchCuratedFeed(
  feedId: CuratedFeedId,
  options: CuratedFeedFetchOptions = {},
): Promise<CuratedFeedResult> {
  const descriptor = descriptorFor(feedId);
  const fetcher = options.fetch ?? fetch;
  const timeoutMs = boundedTimeout(options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const headers = new Headers({ Accept: "text/plain, text/markdown;q=0.9" });
    if (options.ifNoneMatch) headers.set("If-None-Match", options.ifNoneMatch);
    const response = await fetcher(descriptor.rawUrl, {
      method: "GET",
      headers,
      redirect: "error",
      signal: controller.signal,
    });
    if (response.status === 304) {
      throw new CuratedFeedError("not_modified", "Curated feed has not changed");
    }
    if (!response.ok) {
      throw new CuratedFeedError("unavailable", `Curated feed returned HTTP ${response.status}`);
    }
    const declaredLength = Number(response.headers.get("content-length") ?? "0");
    if (Number.isFinite(declaredLength) && declaredLength > MAX_FEED_BYTES) {
      throw new CuratedFeedError("too_large", "Curated feed exceeds the response limit");
    }
    const content = await readBoundedBody(response, MAX_FEED_BYTES);
    return {
      ...parseCuratedFeed(feedId, content),
      etag: response.headers.get("etag") ?? undefined,
    };
  } catch (error) {
    if (error instanceof CuratedFeedError) throw error;
    throw new CuratedFeedError(
      "unavailable",
      controller.signal.aborted ? "Curated feed timed out" : "Curated feed could not be read",
    );
  } finally {
    clearTimeout(timer);
  }
}

export function parseCuratedFeed(feedId: CuratedFeedId, content: string): CuratedFeedResult {
  if (Buffer.byteLength(content, "utf8") > MAX_FEED_BYTES) {
    throw new CuratedFeedError("too_large", "Curated feed exceeds the response limit");
  }
  const descriptor = descriptorFor(feedId);
  const tables = [...extractHtmlTables(content), ...extractMarkdownTables(content)];
  let skippedClosed = 0;
  let skippedInvalid = 0;
  let duplicatesCollapsed = 0;
  let inheritedCompany = "";
  const leads = new Map<string, CuratedFeedLead>();
  let observedRows = 0;

  for (const table of tables) {
    if (table.length < 2) continue;
    const headers = table[0]!.cells.map((cell) => normalizedHeader(cell.text));
    const companyIndex = headerIndex(headers, ["company"]);
    const titleIndex = headerIndex(headers, ["role", "title", "position"]);
    const locationIndex = headerIndex(headers, ["location", "locations"]);
    const applyIndex = headerIndex(headers, ["application", "apply", "link"]);
    const postedIndex = headerIndex(headers, ["age", "posted", "date"]);
    if ([companyIndex, titleIndex, locationIndex, applyIndex].some((value) => value < 0)) continue;

    for (const row of table.slice(1)) {
      observedRows += 1;
      if (observedRows > MAX_FEED_ROWS) {
        throw new CuratedFeedError("too_large", "Curated feed exceeds the row limit");
      }
      const rowText = row.cells.map((cell) => cell.text).join(" ");
      if (isClosedRow(rowText)) {
        skippedClosed += 1;
        continue;
      }
      let company = cleanText(row.cells[companyIndex]?.text ?? "");
      if (company === "↳" || company === "") company = inheritedCompany;
      else inheritedCompany = company;
      const title = cleanText(row.cells[titleIndex]?.text ?? "");
      const location = cleanText(row.cells[locationIndex]?.text ?? "");
      const postedLabel = postedIndex >= 0 ? cleanText(row.cells[postedIndex]?.text ?? "") : "";
      const originalUrl = selectOriginalApplicationUrl(row.cells[applyIndex]?.links ?? []);
      if (!company || !title || !location || !originalUrl) {
        skippedInvalid += 1;
        continue;
      }
      if (leads.has(originalUrl)) {
        duplicatesCollapsed += 1;
        continue;
      }
      const categories = inferCuratedCategories(rowText, descriptor.defaultEmploymentType);
      leads.set(originalUrl, {
        feedId,
        sourceCatalogId: descriptor.sourceCatalogId,
        company,
        title,
        location,
        originalUrl,
        postedLabel,
        ageDays: parseAgeDays(postedLabel),
        employmentType: categories.employmentType,
        engagementType: categories.engagementType,
        categoryEvidence: categories.explicit ? "explicit" : "feed_default",
        submissionCapability: submissionPolicy(originalUrl).capability,
        atsSource: publicAtsSourceFromUrl(originalUrl, company),
        requiresOriginalRevalidation: true,
      });
    }
  }

  if (tables.length === 0) {
    throw new CuratedFeedError("invalid_feed", "Curated feed contains no supported job tables");
  }
  return {
    descriptor,
    leads: [...leads.values()],
    skippedClosed,
    skippedInvalid,
    duplicatesCollapsed,
  };
}

export function inferCuratedCategories(
  sourceText: string,
  defaultEmploymentType: CuratedEmploymentType,
): {
  employmentType: CuratedEmploymentType;
  engagementType: CuratedEngagementType | null;
  explicit: boolean;
} {
  const value = ` ${sourceText.toLowerCase().replace(/[^a-z0-9]+/g, " ")} `;
  const employmentType: CuratedEmploymentType | null =
    /\b(intern|internship|co op)\b/.test(value) ? "internship"
      : /\b(apprentice|apprenticeship)\b/.test(value) ? "apprenticeship"
        : /\b(per diem|prn)\b/.test(value) ? "per_diem"
          : /\b(seasonal)\b/.test(value) ? "seasonal"
            : /\b(part time)\b/.test(value) ? "part_time"
              : /\b(temp|temporary)\b/.test(value) ? "temporary"
                : /\b(contract|contractor|c2c|corp to corp|1099)\b/.test(value) ? "contract"
                  : /\b(full time)\b/.test(value) ? "full_time"
                    : null;
  const engagementType: CuratedEngagementType | null =
    /\b(c2c|corp to corp)\b/.test(value) ? "c2c"
      : /\b(w2|w 2)\b/.test(value) ? "w2"
        : /\b1099\b/.test(value) ? "1099"
          : /\b(direct hire|permanent hire)\b/.test(value) ? "direct_hire"
            : null;
  return {
    employmentType: employmentType ?? defaultEmploymentType,
    engagementType,
    explicit: employmentType !== null || engagementType !== null,
  };
}

export function publicAtsSourceFromUrl(rawUrl: string, company?: string): PublicAtsSource | null {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    return null;
  }
  const host = url.hostname.toLowerCase().replace(/^www\./, "");
  const parts = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  if ((host === "boards.greenhouse.io" || host === "job-boards.greenhouse.io") && safeIdentifier(parts[0])) {
    return { kind: "greenhouse", boardToken: parts[0]!, ...(company ? { company } : {}) };
  }
  if (host === "jobs.lever.co" && safeIdentifier(parts[0])) {
    return { kind: "lever", site: parts[0]!, ...(company ? { company } : {}) };
  }
  if (host === "jobs.ashbyhq.com" && safeIdentifier(parts[0])) {
    return { kind: "ashby", boardName: parts[0]!, ...(company ? { company } : {}) };
  }
  if (host === "jobs.smartrecruiters.com" && safeIdentifier(parts[0])) {
    return { kind: "smartrecruiters", companyIdentifier: parts[0]!, ...(company ? { company } : {}) };
  }
  const workday = host.match(/^([a-z0-9-]+)\.([a-z0-9-]+)\.myworkdayjobs\.com$/);
  if (workday && safeIdentifier(workday[1]) && safeIdentifier(workday[2]) && safeIdentifier(parts[0])) {
    return {
      kind: "workday",
      tenant: workday[1]!,
      instance: workday[2]!,
      site: parts[0]!,
      ...(company ? { company } : {}),
    };
  }
  return null;
}

function descriptorFor(feedId: CuratedFeedId): CuratedFeedDescriptor {
  const descriptor = CURATED_JOB_FEEDS.find((candidate) => candidate.id === feedId);
  if (!descriptor) throw new CuratedFeedError("invalid_feed", "Curated feed is not allowlisted");
  return descriptor;
}

function extractHtmlTables(content: string): TableRow[][] {
  const document = parseFragment(content);
  return descendants(document, "table").map((table) => descendants(table, "tr").map((row) => ({
    cells: childElements(row)
      .filter((element) => element.tagName === "td" || element.tagName === "th")
      .map(cellFromElement),
  })).filter((row) => row.cells.length > 0));
}

function extractMarkdownTables(content: string): TableRow[][] {
  const lines = content.split(/\r?\n/);
  const tables: TableRow[][] = [];
  for (let index = 0; index + 1 < lines.length; index += 1) {
    if (!isMarkdownRow(lines[index]!) || !isMarkdownSeparator(lines[index + 1]!)) continue;
    const table: TableRow[] = [{ cells: splitMarkdownRow(lines[index]!).map(cellFromMarkdown) }];
    index += 2;
    while (index < lines.length && isMarkdownRow(lines[index]!)) {
      table.push({ cells: splitMarkdownRow(lines[index]!).map(cellFromMarkdown) });
      index += 1;
    }
    tables.push(table);
    index -= 1;
  }
  return tables;
}

function descendants(root: Node, tagName: string): Element[] {
  const output: Element[] = [];
  const visit = (node: Node): void => {
    if (isElement(node) && node.tagName === tagName) output.push(node);
    for (const child of childNodes(node)) visit(child);
  };
  visit(root);
  return output;
}

function cellFromElement(element: Element): TableCell {
  return {
    text: cleanText(textContent(element)),
    links: descendants(element, "a").flatMap((anchor) => {
      const href = anchor.attrs.find((attribute) => attribute.name === "href")?.value;
      return href ? [href] : [];
    }),
  };
}

function cellFromMarkdown(value: string): TableCell {
  const fragment = parseFragment(value);
  const htmlLinks = descendants(fragment, "a").flatMap((anchor) => {
    const href = anchor.attrs.find((attribute) => attribute.name === "href")?.value;
    return href ? [href] : [];
  });
  const markdownLinks = [...value.matchAll(/\]\((https?:\/\/[^)\s]+)\)/g)].map((match) => match[1]!);
  const withoutMarkdown = value
    .replace(/!\[[^\]]*\]\([^)]+\)/g, " ")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[*_`]/g, " ");
  const renderedText = textContent(fragment)
    .replace(/!\[[^\]]*\]\([^)]+\)/g, " ")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[*_`]/g, " ");
  return {
    text: cleanText(renderedText || withoutMarkdown),
    links: [...htmlLinks, ...markdownLinks],
  };
}

function childNodes(node: Node): Node[] {
  return "childNodes" in node ? [...node.childNodes] : [];
}

function childElements(node: Node): Element[] {
  return childNodes(node).filter(isElement);
}

function isElement(node: Node): node is Element {
  return "tagName" in node;
}

function textContent(node: Node): string {
  if ("value" in node && typeof node.value === "string") return node.value;
  return childNodes(node).map(textContent).join(" ");
}

function isMarkdownRow(line: string): boolean {
  return line.trim().startsWith("|") && line.trim().endsWith("|");
}

function isMarkdownSeparator(line: string): boolean {
  if (!isMarkdownRow(line)) return false;
  const cells = splitMarkdownRow(line);
  return cells.length > 1 && cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()));
}

function splitMarkdownRow(line: string): string[] {
  const value = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  const cells: string[] = [];
  let cell = "";
  let escaped = false;
  for (const character of value) {
    if (escaped) {
      cell += character;
      escaped = false;
    } else if (character === "\\") {
      escaped = true;
      cell += character;
    } else if (character === "|") {
      cells.push(cell.trim());
      cell = "";
    } else {
      cell += character;
    }
  }
  cells.push(cell.trim());
  return cells;
}

function normalizedHeader(value: string): string {
  return cleanText(value).toLowerCase().replace(/[^a-z]+/g, " ").trim();
}

function headerIndex(headers: string[], candidates: string[]): number {
  return headers.findIndex((header) => candidates.some((candidate) => header === candidate || header.includes(candidate)));
}

function cleanText(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}

function isClosedRow(value: string): boolean {
  return value.includes("🔒") || /\b(application\s+)?closed\b/i.test(value);
}

function selectOriginalApplicationUrl(links: string[]): string | null {
  for (const raw of links) {
    let url: URL;
    try {
      url = new URL(raw);
    } catch {
      continue;
    }
    if (url.protocol !== "https:") continue;
    const host = url.hostname.toLowerCase().replace(/^www\./, "");
    if (["github.com", "raw.githubusercontent.com", "simplify.jobs", "prepai.dev", "zapply.jobs"].some((blocked) => host === blocked || host.endsWith(`.${blocked}`))) {
      continue;
    }
    for (const key of [...url.searchParams.keys()]) {
      if (key.toLowerCase().startsWith("utm_") || ["ref", "source", "gh_src"].includes(key.toLowerCase())) {
        url.searchParams.delete(key);
      }
    }
    url.hash = "";
    return url.toString();
  }
  return null;
}

function parseAgeDays(value: string): number | null {
  const days = value.trim().match(/^(\d{1,3})\s*d(?:ays?)?$/i);
  if (days) return Number(days[1]);
  const posted = value.match(/(?:posted\s+)?(\d{1,3})\s+days?\s+ago/i);
  return posted ? Number(posted[1]) : null;
}

function safeIdentifier(value: string | undefined): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]+$/.test(value);
}

function boundedTimeout(value: number): number {
  if (!Number.isInteger(value) || value < 100 || value > 60_000) {
    throw new CuratedFeedError("invalid_feed", "Curated feed timeout is invalid");
  }
  return value;
}

async function readBoundedBody(response: Response, maximumBytes: number): Promise<string> {
  if (!response.body) return "";
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let size = 0;
  let output = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > maximumBytes) {
      await reader.cancel();
      throw new CuratedFeedError("too_large", "Curated feed exceeds the response limit");
    }
    output += decoder.decode(value, { stream: true });
  }
  return output + decoder.decode();
}
