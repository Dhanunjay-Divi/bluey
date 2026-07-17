import type {
  CareerProfile,
  EducationEntry,
  EmploymentEntry,
  ProjectEntry,
} from "../../types";
import type { ImportedResume } from "./import";
import { inferProfileFromResume } from "./parser";

export interface ResumeImportSummary {
  employment: number;
  education: number;
  skills: number;
  certifications: number;
  projects: number;
}

export type ResumeImportMode = "replace" | "merge";

export interface ResumeImportPreview {
  imported: ImportedResume;
  parsed: CareerProfile;
  replacement: CareerProfile;
  merged: CareerProfile;
  summary: ResumeImportSummary;
  likely_different_person: boolean;
  changed_sections: string[];
}

export function prepareResumeImport(
  current: CareerProfile,
  imported: ImportedResume,
): ResumeImportPreview {
  const parsed = inferProfileFromResume(resumeImportBase(current), imported);
  const likelyDifferentPerson = Boolean(
    current.full_name &&
    parsed.full_name &&
    normalizePersonName(current.full_name) !== normalizePersonName(parsed.full_name)
  );
  const replacement: CareerProfile = {
    ...parsed,
    email: parsed.email || (likelyDifferentPerson ? "" : current.email),
    street_address: likelyDifferentPerson ? "" : current.street_address,
    work_authorization: likelyDifferentPerson ? "" : current.work_authorization,
    sponsorship_required: likelyDifferentPerson ? null : current.sponsorship_required,
    salary_expectation: likelyDifferentPerson ? "" : current.salary_expectation,
    notice_period: likelyDifferentPerson ? "" : current.notice_period,
    reusable_answers: likelyDifferentPerson ? {} : current.reusable_answers,
    resume_mode: current.resume_mode,
    review_new_claims: current.review_new_claims,
    default_submission_mode: current.default_submission_mode,
    auto_submit_threshold: current.auto_submit_threshold,
    daily_limit: current.daily_limit,
    onboarding_step: current.onboarding_step,
    onboarding_complete: current.onboarding_complete,
    updated_at_ms: current.updated_at_ms,
  };
  const merged = mergeCareerProfiles(current, parsed);
  const changedSections = [
    ["Contact", replacement.full_name || replacement.email || replacement.phone],
    ["Summary", replacement.summary],
    ["Experience", replacement.employment.length],
    ["Education", replacement.education.length],
    ["Skills", replacement.skills.length],
    ["Certifications", replacement.certifications.length],
    ["Projects", replacement.projects.length],
  ].filter(([, present]) => Boolean(present)).map(([label]) => String(label));
  return {
    imported,
    parsed,
    replacement,
    merged,
    summary: summarizeResumeImport(replacement),
    likely_different_person: likelyDifferentPerson,
    changed_sections: changedSections,
  };
}

export function applyResumeImport(preview: ResumeImportPreview, mode: ResumeImportMode): CareerProfile {
  if (mode === "merge" && preview.likely_different_person) {
    throw new Error("Fill blanks is available only when the resume belongs to the current Career Profile.");
  }
  return mode === "replace" ? preview.replacement : preview.merged;
}

export function summarizeResumeImport(profile: CareerProfile): ResumeImportSummary {
  return {
    employment: profile.employment?.length || 0,
    education: profile.education?.length || 0,
    skills: profile.skills?.length || 0,
    certifications: profile.certifications?.length || 0,
    projects: profile.projects?.length || 0,
  };
}

function resumeImportBase(profile: CareerProfile): CareerProfile {
  return {
    ...profile,
    full_name: "",
    email: "",
    phone: "",
    headline: "",
    current_location: "",
    street_address: "",
    summary: "",
    linkedin_url: "",
    portfolio_url: "",
    work_authorization: "",
    sponsorship_required: null,
    salary_expectation: "",
    notice_period: "",
    skills: [],
    certifications: [],
    employment: [],
    education: [],
    projects: [],
    reusable_answers: {},
    source_resume_name: "",
    source_resume_text: "",
  };
}

function mergeCareerProfiles(current: CareerProfile, parsed: CareerProfile): CareerProfile {
  const fill = (existing: string, incoming: string) => existing.trim() ? existing : incoming;
  return {
    ...current,
    full_name: fill(current.full_name, parsed.full_name),
    email: fill(current.email, parsed.email),
    phone: fill(current.phone, parsed.phone),
    headline: fill(current.headline, parsed.headline),
    current_location: fill(current.current_location, parsed.current_location),
    summary: fill(current.summary, parsed.summary),
    linkedin_url: fill(current.linkedin_url, parsed.linkedin_url),
    portfolio_url: fill(current.portfolio_url, parsed.portfolio_url),
    skills: unionStrings(current.skills, parsed.skills),
    certifications: unionStrings(current.certifications, parsed.certifications),
    employment: mergeEmployment(current.employment, parsed.employment),
    education: mergeEducation(current.education, parsed.education),
    projects: mergeProjects(current.projects, parsed.projects),
    source_resume_name: parsed.source_resume_name,
    source_resume_text: parsed.source_resume_text,
  };
}

function mergeEmployment(current: EmploymentEntry[], parsed: EmploymentEntry[]): EmploymentEntry[] {
  const merged = current.map((entry) => ({ ...entry, highlights: [...entry.highlights] }));
  for (const incoming of parsed) {
    const index = merged.findIndex((entry) =>
      normalizedKey(entry.company, entry.title, entry.start_date) ===
      normalizedKey(incoming.company, incoming.title, incoming.start_date),
    );
    if (index < 0) {
      merged.push(incoming);
      continue;
    }
    const existing = merged[index];
    merged[index] = {
      ...existing,
      company: existing.company || incoming.company,
      title: existing.title || incoming.title,
      location: existing.location || incoming.location,
      start_date: existing.start_date || incoming.start_date,
      end_date: existing.end_date || incoming.end_date,
      current: existing.current || (!existing.end_date && incoming.current),
      highlights: unionStrings(existing.highlights, incoming.highlights),
    };
  }
  return merged;
}

function mergeEducation(current: EducationEntry[], parsed: EducationEntry[]): EducationEntry[] {
  const merged = current.map((entry) => ({ ...entry }));
  for (const incoming of parsed) {
    const index = merged.findIndex((entry) =>
      normalizedKey(entry.school, entry.degree, entry.field, entry.start_date) ===
      normalizedKey(incoming.school, incoming.degree, incoming.field, incoming.start_date),
    );
    if (index < 0) {
      merged.push(incoming);
      continue;
    }
    const existing = merged[index];
    merged[index] = {
      ...existing,
      school: existing.school || incoming.school,
      degree: existing.degree || incoming.degree,
      field: existing.field || incoming.field,
      start_date: existing.start_date || incoming.start_date,
      end_date: existing.end_date || incoming.end_date,
      location: existing.location || incoming.location,
    };
  }
  return merged;
}

function mergeProjects(current: ProjectEntry[], parsed: ProjectEntry[]): ProjectEntry[] {
  const merged = current.map((entry) => ({ ...entry, technologies: [...entry.technologies] }));
  for (const incoming of parsed) {
    const index = merged.findIndex((entry) =>
      normalizedKey(entry.name, entry.role) === normalizedKey(incoming.name, incoming.role),
    );
    if (index < 0) {
      merged.push(incoming);
      continue;
    }
    const existing = merged[index];
    merged[index] = {
      ...existing,
      name: existing.name || incoming.name,
      role: existing.role || incoming.role,
      summary: existing.summary || incoming.summary,
      technologies: unionStrings(existing.technologies, incoming.technologies),
      url: existing.url || incoming.url,
    };
  }
  return merged;
}

function normalizedKey(...parts: string[]): string {
  return parts.map((part) => part.toLocaleLowerCase().replace(/[^a-z0-9]/g, "")).join("|");
}

function unionStrings(existing: string[], incoming: string[]): string[] {
  const seen = new Set(existing.map((value) => value.toLocaleLowerCase().trim()));
  return [...existing, ...incoming.filter((value) => {
    const key = value.toLocaleLowerCase().trim();
    if (!key || seen.has(key)) return false;
    seen.add(key);
    return true;
  })];
}

function normalizePersonName(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}
