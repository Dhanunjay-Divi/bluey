import {
  CANONICAL_ROLE_SUGGESTIONS,
  canonicalRoleSuggestions,
  resolveTargetRole,
} from "../lib/canonical-taxonomy";

export const TARGET_ROLE_SUGGESTIONS = [...CANONICAL_ROLE_SUGGESTIONS];

export const ROLE_SUGGESTIONS = [
  ...TARGET_ROLE_SUGGESTIONS,
  "Senior Software Engineer",
  "Staff Software Engineer",
  "Principal Software Engineer",
  "Senior Data Engineer",
  "Lead Clinical Research Analyst",
  "Director of Clinical Operations",
];

// Compatibility display helper. The server and resolveTargetRole retain canonical/review authority.
export function canonicalizeTargetRole(value: string): string {
  const resolution = resolveTargetRole(value);
  if (resolution.status === "resolved") return resolution.role.label;
  return resolution.input;
}

export function canonicalTargetRoles(values: string[]): string[] {
  return mergeCareerSuggestions(values.map(canonicalizeTargetRole));
}

export function targetRoleSuggestions(query: string, selected: string[] = [], limit = 8): string[] {
  return canonicalRoleSuggestions(query, selected, limit);
}

export const LOCATION_SUGGESTIONS = [
  "Remote - United States",
  "United States",
  "New York, NY",
  "San Francisco, CA",
  "San Jose, CA",
  "Los Angeles, CA",
  "San Diego, CA",
  "Seattle, WA",
  "Austin, TX",
  "Dallas, TX",
  "Houston, TX",
  "Chicago, IL",
  "Boston, MA",
  "Washington, DC",
  "Arlington, VA",
  "Falls Church, VA",
  "Atlanta, GA",
  "Denver, CO",
  "Raleigh, NC",
  "Charlotte, NC",
  "Philadelphia, PA",
  "Pittsburgh, PA",
  "Phoenix, AZ",
  "Portland, OR",
  "Minneapolis, MN",
  "Detroit, MI",
  "Columbus, OH",
  "Indianapolis, IN",
  "Nashville, TN",
  "Miami, FL",
  "Tampa, FL",
  "Salt Lake City, UT",
  "Kansas City, MO",
  "St. Louis, MO",
  "Hyderabad, India",
  "Bengaluru, India",
  "Mumbai, India",
  "Pune, India",
  "Chennai, India",
  "Visakhapatnam, India",
  "Toronto, Canada",
  "Vancouver, Canada",
  "London, United Kingdom",
];

export const SKILL_SUGGESTIONS = [
  "Clinical Research",
  "Clinical Operations",
  "Clinical Documentation",
  "Health Informatics",
  "Epic EMR",
  "REDCap",
  "HIPAA",
  "Good Clinical Practice",
  "Protocol Development",
  "Regulatory Compliance",
  "Patient Recruitment",
  "Data Management",
  "SQL",
  "Python",
  "R",
  "JavaScript",
  "TypeScript",
  "React",
  "Node.js",
  "Java",
  "C#",
  "Rust",
  "AWS",
  "Azure",
  "Google Cloud",
  "Docker",
  "Kubernetes",
  "PostgreSQL",
  "Machine Learning",
  "Data Analysis",
  "Project Management",
  "Product Management",
  "Agile",
  "Figma",
  "Tableau",
  "Power BI",
  "Salesforce",
];

export const CERTIFICATION_SUGGESTIONS = [
  "Certified Clinical Research Professional (CCRP)",
  "Certified Clinical Research Coordinator (CCRC)",
  "Good Clinical Practice (GCP)",
  "Epic Ambulatory",
  "Project Management Professional (PMP)",
  "Certified ScrumMaster (CSM)",
  "AWS Certified Solutions Architect",
  "AWS Certified Solutions Architect, Professional",
  "AWS Certified Machine Learning, Specialty",
  "Microsoft Certified: Azure Fundamentals",
  "Google Professional Cloud Architect",
  "Certified Information Systems Security Professional (CISSP)",
  "Certified Public Accountant (CPA)",
];

export function mergeCareerSuggestions(...groups: Array<Array<string | undefined>>): string[] {
  const seen = new Set<string>();
  const merged: string[] = [];
  for (const value of groups.flat()) {
    const clean = value?.trim();
    if (!clean) continue;
    const key = clean.toLocaleLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(clean);
  }
  return merged;
}

export function filterCareerSuggestions(
  query: string,
  suggestions: string[],
  selected: string[] = [],
  limit = 6,
): string[] {
  const needle = query.trim().toLocaleLowerCase();
  const selectedSet = new Set(selected.map((value) => value.trim().toLocaleLowerCase()));
  return mergeCareerSuggestions(suggestions)
    .filter((value) => !selectedSet.has(value.toLocaleLowerCase()))
    .map((value, index) => {
      const normalized = value.toLocaleLowerCase();
      const wordStarts = normalized.split(/[^a-z0-9]+/).some((word) => word.startsWith(needle));
      const rank = !needle
        ? 3
        : normalized.startsWith(needle)
          ? 0
          : wordStarts
            ? 1
            : normalized.includes(needle)
              ? 2
              : 4;
      return { value, rank, index };
    })
    .filter((item) => item.rank < 4)
    .sort((left, right) => left.rank - right.rank || left.index - right.index)
    .slice(0, limit)
    .map((item) => item.value);
}
