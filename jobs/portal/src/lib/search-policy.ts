import type { EmploymentEntry } from "../types";
import {
  classifyPostingRole,
  resolveTargetRole,
  type TargetRoleResolution,
} from "./canonical-taxonomy";

export const BLUEY_DAILY_APPLICATION_LIMIT = 10;
export const BLUEY_AUTO_SUBMIT_THRESHOLD = 80;
export const BLUEY_MAX_POSTING_AGE_DAYS = 14;

export interface ExperienceRange {
  years: number;
  minimum: number;
  maximum: number;
}

export interface RoleExperienceAssessment {
  range: ExperienceRange;
  target_role: TargetRoleResolution;
  review_required: boolean;
  relevant_entry_count: number;
}

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
  // Existing presentation callers keep the range shape; authority callers must inspect the assessment.
  return roleExperienceAssessment(employment, targetRole, now).range;
}

export function roleExperienceAssessment(
  employment: EmploymentEntry[],
  targetRole: string,
  now = new Date(),
): RoleExperienceAssessment {
  const target = resolveTargetRole(targetRole);
  if (target.status !== "resolved") {
    return {
      range: experienceRange(employment, now),
      target_role: target,
      review_required: true,
      relevant_entry_count: employment.length,
    };
  }

  const relevant = employment.filter((entry) => {
    const postingRole = classifyPostingRole(entry.title);
    return postingRole.status === "known" && postingRole.family_id === target.role.family_id;
  });
  return {
    range: experienceRange(relevant, now),
    target_role: target,
    review_required: false,
    relevant_entry_count: relevant.length,
  };
}

function parseCareerMonth(value: string): number | null {
  const match = value.trim().match(/^(\d{4})(?:-(\d{1,2}))?/);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Math.max(1, Math.min(12, Number(match[2] || 1)));
  return year * 12 + month - 1;
}
