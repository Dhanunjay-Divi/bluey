import type { CareerProfile, EducationEntry, EmploymentEntry, ProjectEntry } from "../types";

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const CAREER_DATE_PATTERN = /^(?:19|20)\d{2}(?:-(?:0[1-9]|1[0-2]))?$/;

export function validateProfileIdentity(profile: CareerProfile): string {
  if (!profile.full_name.trim()) return "Add your full name to continue.";
  if (!profile.email.trim()) return "Add the contact email shown on your resume.";
  if (!EMAIL_PATTERN.test(profile.email.trim())) return "Enter a valid resume contact email address.";
  if (!profile.current_location.trim()) return "Add your current city and state so Bluey can answer location questions correctly.";
  const linkedInError = validateOptionalUrl(profile.linkedin_url, "LinkedIn");
  if (linkedInError) return linkedInError;
  return validateOptionalUrl(profile.portfolio_url, "Portfolio");
}

export function validateEmploymentEntries(entries: EmploymentEntry[]): string {
  for (const entry of entries) {
    if (!hasEmploymentContent(entry)) continue;
    if (!entry.company.trim() || !entry.title.trim()) {
      return "Each work-history entry needs both a company and title.";
    }
    const dateError = validateDateRange(entry.start_date, entry.current ? "" : entry.end_date, "work-history");
    if (dateError) return dateError;
  }
  return "";
}

export function validateEducationEntries(entries: EducationEntry[]): string {
  for (const entry of entries) {
    if (!hasEducationContent(entry)) continue;
    if (!entry.school.trim() || !(entry.degree.trim() || entry.field.trim())) {
      return "Each education entry needs a school and degree or field of study.";
    }
    const dateError = validateDateRange(entry.start_date, entry.end_date, "education");
    if (dateError) return dateError;
  }
  return "";
}

export function validateProjectEntries(entries: ProjectEntry[]): string {
  for (const entry of entries) {
    if (!hasProjectContent(entry)) continue;
    if (!entry.name.trim()) return "Each project entry needs a project name.";
    const urlError = validateOptionalUrl(entry.url, "Project");
    if (urlError) return urlError;
  }
  return "";
}

export function validateCareerProfile(profile: CareerProfile): string {
  return (
    validateProfileIdentity(profile) ||
    validateEmploymentEntries(profile.employment) ||
    validateEducationEntries(profile.education) ||
    validateProjectEntries(profile.projects)
  );
}

function validateOptionalUrl(value: string, label: string): string {
  if (!value.trim()) return "";
  try {
    const parsed = new URL(/^https?:\/\//i.test(value.trim()) ? value.trim() : `https://${value.trim()}`);
    if (!parsed.hostname.includes(".")) throw new Error("missing host");
    return "";
  } catch {
    return `Enter a valid ${label} URL.`;
  }
}

function validateDateRange(start: string, end: string, label: string): string {
  if (start && !CAREER_DATE_PATTERN.test(start)) return `Use YYYY or YYYY-MM for ${label} start dates.`;
  if (end && !CAREER_DATE_PATTERN.test(end)) return `Use YYYY or YYYY-MM for ${label} end dates.`;
  if (start && end && comparableDate(end) < comparableDate(start)) {
    return `The ${label} end date cannot be earlier than its start date.`;
  }
  return "";
}

function comparableDate(value: string): number {
  const [year, month = "01"] = value.split("-");
  return Number(year) * 100 + Number(month);
}

function hasEmploymentContent(entry: EmploymentEntry): boolean {
  return Boolean(entry.company || entry.title || entry.location || entry.start_date || entry.end_date || entry.highlights.length);
}

function hasEducationContent(entry: EducationEntry): boolean {
  return Boolean(entry.school || entry.degree || entry.field || entry.location || entry.start_date || entry.end_date);
}

function hasProjectContent(entry: ProjectEntry): boolean {
  return Boolean(entry.name || entry.role || entry.summary || entry.url || entry.technologies.length);
}
