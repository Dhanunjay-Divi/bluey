import type { EmploymentEntry } from "../types";

export const BLUEY_DAILY_APPLICATION_LIMIT = 10;
export const BLUEY_AUTO_SUBMIT_THRESHOLD = 80;
export const BLUEY_MAX_POSTING_AGE_DAYS = 14;

export interface ExperienceRange {
  years: number;
  minimum: number;
  maximum: number;
}

type CareerRoleFamily =
  | "software_engineering"
  | "data_engineering"
  | "data_science_ml"
  | "product_management"
  | "clinical_research"
  | "generic";

export function experienceRange(employment: EmploymentEntry[], now = new Date()): ExperienceRange {
  const currentMonth = now.getUTCFullYear() * 12 + now.getUTCMonth();
  const intervals = employment
    .map((entry) => {
      const start = parseCareerMonth(entry.start_date);
      const parsedEnd = parseCareerMonth(entry.end_date);
      const end = entry.current ? currentMonth : parsedEnd === null ? null : parsedEnd + 1;
      return start !== null && end !== null && end >= start ? [start, end] as const : null;
    })
    .filter((value): value is readonly [number, number] => Boolean(value))
    .sort((left, right) => left[0] - right[0]);
  const merged: Array<[number, number]> = [];
  for (const [start, end] of intervals) {
    const previous = merged.at(-1);
    if (previous && start <= previous[1]) previous[1] = Math.max(previous[1], end);
    else merged.push([start, end]);
  }
  const months = merged.reduce((total, [start, end]) => total + end - start, 0);
  const years = Math.round(months / 12);
  return {
    years,
    minimum: Math.max(0, years - 1),
    maximum: Math.max(2, years + 2),
  };
}

export function roleExperienceRange(
  employment: EmploymentEntry[],
  targetRole: string,
  now = new Date(),
): ExperienceRange {
  const targetFamily = inferCareerRoleFamily(targetRole);
  const relevant = targetFamily === "generic"
    ? employment
    : employment.filter((entry) => (
      inferCareerRoleFamily(`${entry.title} ${entry.highlights.join(" ")}`) === targetFamily
    ));
  return experienceRange(relevant, now);
}

function inferCareerRoleFamily(value: string): CareerRoleFamily {
  const text = ` ${normalizeCareerText(value)} `;
  if (containsCareerPhrase(text, [" data engineer ", " data platform ", " analytics engineer ", " etl ", " data warehouse ", " de "])) {
    return "data_engineering";
  }
  if (containsCareerPhrase(text, [" machine learning ", " data scientist ", " applied scientist ", " artificial intelligence ", " ml engineer ", " mle "])) {
    return "data_science_ml";
  }
  if (containsCareerPhrase(text, [" product manager ", " product management ", " product owner ", " pm "])) {
    return "product_management";
  }
  if (containsCareerPhrase(text, [" clinical research ", " clinical operations ", " clinical trial ", " research coordinator ", " cra ", " crc "])) {
    return "clinical_research";
  }
  if (containsCareerPhrase(text, [
    " software ",
    " developer ",
    " frontend ",
    " front end ",
    " backend ",
    " back end ",
    " full stack ",
    " fullstack ",
    " devops ",
    " site reliability ",
    " cloud engineer ",
    " application engineer ",
    " swe ",
    " sde ",
  ])) {
    return "software_engineering";
  }
  return "generic";
}

function normalizeCareerText(value: string): string {
  return value
    .toLocaleLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function containsCareerPhrase(value: string, phrases: string[]): boolean {
  return phrases.some((phrase) => value.includes(phrase));
}

function parseCareerMonth(value: string): number | null {
  const match = value.trim().match(/^(\d{4})(?:-(\d{1,2}))?/);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Math.max(1, Math.min(12, Number(match[2] || 1)));
  return year * 12 + month - 1;
}
