import type { SubmissionCapability } from "./policy.js";

export type JobSourceKind =
  | "public_ats"
  | "public_curated_feed"
  | "staffing_company"
  | "portal_handoff";

export type DiscoveryCapability =
  | "scheduled_public_feed"
  | "candidate_lead"
  | "pasted_link_only";

export interface JobSourceCatalogEntry {
  id: string;
  name: string;
  kind: JobSourceKind;
  domains: string[];
  aliases: string[];
  discoveryCapability: DiscoveryCapability;
  submissionCapability: SubmissionCapability;
  requiresCanonicalRevalidation: boolean;
  notes?: string;
}

function source(
  id: string,
  name: string,
  kind: JobSourceKind,
  discoveryCapability: DiscoveryCapability,
  submissionCapability: SubmissionCapability,
  domains: string[] = [],
  aliases: string[] = [],
  notes?: string,
): JobSourceCatalogEntry {
  return {
    id,
    name,
    kind,
    domains: domains.map(normalizeDomain),
    aliases,
    discoveryCapability,
    submissionCapability,
    requiresCanonicalRevalidation: kind !== "public_ats",
    notes,
  };
}

function staffing(id: string, name: string, domains: string[] = [], aliases: string[] = []) {
  return source(
    id,
    name,
    "staffing_company",
    "candidate_lead",
    "unknown_review",
    domains,
    aliases,
    "Treat listings as candidate leads until the original employer or canonical ATS page confirms the opening.",
  );
}

/**
 * Product-owned source metadata. A catalog entry describes where a lead came
 * from; it never grants permission to submit or turns a third-party listing
 * into application truth.
 */
export const JOB_SOURCE_CATALOG: readonly JobSourceCatalogEntry[] = [
  source("ats-greenhouse", "Greenhouse", "public_ats", "scheduled_public_feed", "beta_review", ["boards.greenhouse.io", "job-boards.greenhouse.io"]),
  source("ats-lever", "Lever", "public_ats", "scheduled_public_feed", "beta_review", ["jobs.lever.co"]),
  source("ats-ashby", "Ashby", "public_ats", "scheduled_public_feed", "beta_review", ["jobs.ashbyhq.com"]),
  source("ats-smartrecruiters", "SmartRecruiters", "public_ats", "scheduled_public_feed", "beta_review", ["jobs.smartrecruiters.com"]),
  source("ats-workday", "Workday", "public_ats", "scheduled_public_feed", "beta_review", ["myworkdayjobs.com"]),

  source("portal-linkedin", "LinkedIn", "portal_handoff", "pasted_link_only", "handoff", ["linkedin.com"]),
  source("portal-indeed", "Indeed", "portal_handoff", "pasted_link_only", "handoff", ["indeed.com"]),
  source("portal-ziprecruiter", "ZipRecruiter", "portal_handoff", "pasted_link_only", "unknown_review", ["ziprecruiter.com"]),
  source("portal-dice", "Dice", "portal_handoff", "pasted_link_only", "unknown_review", ["dice.com"]),
  source("portal-careerbuilder", "CareerBuilder", "portal_handoff", "pasted_link_only", "unknown_review", ["careerbuilder.com"]),

  source("feed-simplify-new-grad", "Simplify New Grad Positions", "public_curated_feed", "candidate_lead", "unknown_review", ["github.com"], ["SimplifyJobs/New-Grad-Positions"], "Revalidate every row against its original employer application URL."),
  source("feed-prepai-internships", "PrepAIJobs Summer Internships", "public_curated_feed", "candidate_lead", "unknown_review", ["github.com"], ["PrepAIJobs/Summer2026-Internships"], "Revalidate every row against its original employer application URL."),
  source("feed-prepai-new-grad", "PrepAIJobs New Grad", "public_curated_feed", "candidate_lead", "unknown_review", ["github.com"], ["PrepAIJobs/New-Grad-2026"], "Revalidate every row against its original employer application URL."),
  source("feed-remote-in-tech", "Remote in Tech", "public_curated_feed", "candidate_lead", "unknown_review", ["github.com"], ["remoteintech/remote-jobs"], "Company lists are discovery leads, not proof of a current opening."),
  source("feed-zapply-new-grad", "Zapply New Grad Jobs", "public_curated_feed", "candidate_lead", "unknown_review", ["github.com"], ["zapplyjobs/New-Grad-Jobs-2027"], "Revalidate every row against its original employer application URL."),

  staffing("staffing-apex", "Apex Systems", ["apexsystems.com"], ["Apex System"]),
  staffing("staffing-armada", "The Armada Group", [], ["Armada Group"]),
  staffing("staffing-arthur-lawrence", "Arthur Lawrence", [], ["Arthuer Alwrence"]),
  staffing("staffing-athenahealth", "athenahealth"),
  staffing("staffing-axelon", "Axelon Services"),
  staffing("staffing-beacon-hill", "Beacon Hill Technologies"),
  staffing("staffing-brooksource", "Brooksource"),
  staffing("staffing-business-plan-solutions", "Business Plan Solutions"),
  staffing("staffing-capgemini", "Capgemini Americas"),
  staffing("staffing-clearbridge", "ClearBridge Technology Group"),
  staffing("staffing-computer-futures", "Computer Futures"),
  staffing("staffing-consol-partners", "ConSol Partners"),
  staffing("staffing-consultsbc", "ConsultSBC", [], ["consultsbc"]),
  staffing("staffing-corporate-biz", "Corporate Biz Solutions"),
  staffing("staffing-cross-creek", "Cross Creek Systems"),
  staffing("staffing-css-tech", "CSS Tech", [], ["css-tech"]),
  staffing("staffing-dewinter", "DeWinter Technology"),
  staffing("staffing-eclaro", "Eclaro International"),
  staffing("staffing-ekodus", "Ekodus"),
  staffing("staffing-empiric", "Empiric Solutions"),
  staffing("staffing-expedite", "Expedite Technology"),
  staffing("staffing-experis", "Experis", [], ["Experies"]),
  staffing("staffing-flexcare", "FlexCare Medical Staffing"),
  staffing("staffing-genesis10", "Genesis10"),
  staffing("staffing-global-force", "Global Force US"),
  staffing("staffing-hireforce", "Hireforce"),
  staffing("staffing-horizontal", "Horizontal Integration"),
  staffing("staffing-host-ventures", "Host Ventures", ["hostventures.com"]),
  staffing("staffing-iconma", "ICONMA", ["iconma.com"]),
  staffing("staffing-indotronix", "Indotronix International"),
  staffing("staffing-insight-global", "Insight Global"),
  staffing("staffing-jsg", "Johnson Service Group", [], ["JSG"]),
  staffing("staffing-kds", "KDS Strategic"),
  staffing("staffing-kelly-it", "Kelly IT Resources"),
  staffing("staffing-kforce", "Kforce"),
  staffing("staffing-lawrence-harvey", "Lawrence Harvey"),
  staffing("staffing-leadstack", "LeadStack", [], ["leadstackinc"]),
  staffing("staffing-matchpoint", "MatchPoint Solutions"),
  staffing("staffing-maxonic", "Maxonic"),
  staffing("staffing-modis", "Modis", ["modis.com"], ["Akkodis"]),
  staffing("staffing-nessium", "Nessium Consulting", [], ["Nessiumconsulting"]),
  staffing("staffing-nexient", "Nexient"),
  staffing("staffing-optizm", "Optizm Global"),
  staffing("staffing-prairie", "Prairie Technology Recruiting"),
  staffing("staffing-quantum-leap", "Quantum Leap"),
  staffing("staffing-randstad", "Randstad Technologies", ["randstadusa.com"], ["Randstad USA"]),
  staffing("staffing-robert-half", "Robert Half Technology", ["roberthalf.com"], ["Robert Half"]),
  staffing("staffing-sd-engineering", "S&D Engineering Solutions"),
  staffing("staffing-sabre", "Sabre Corporation"),
  staffing("staffing-signature", "Signature Consultants"),
  staffing("staffing-softcom", "Softcom Systems"),
  staffing("staffing-sogeti", "Sogeti USA"),
  staffing("staffing-splunk", "Splunk"),
  staffing("staffing-sprucetech", "Spruce Technology", [], ["Sprucetech"]),
  staffing("staffing-staff-perm", "Staff Perm"),
  staffing("staffing-starpoint", "Starpoint Solutions", [], ["starpoint"]),
  staffing("staffing-synechron", "Synechron", ["synechron.com"]),
  staffing("staffing-systems-pros", "Systems Pros"),
  staffing("staffing-talentric", "Talentric"),
  staffing("staffing-tech-providers", "Tech Providers"),
  staffing("staffing-tekni-force", "Tekni Force"),
  staffing("staffing-teksystems", "TEKsystems", ["teksystems.com"], ["Tek Systems"]),
  staffing("staffing-judge-group", "The Judge Group"),
  staffing("staffing-midtown", "The Midtown Group"),
  staffing("staffing-three-bridge", "ThreeBridge", [], ["Three Bridge"]),
  staffing("staffing-twentypine", "TwentyPine"),
  staffing("staffing-ventas", "Ventas Consulting"),
  staffing("staffing-weinberg", "Weinberg & Associates"),
  staffing("staffing-xchange", "Xchange Software"),
];

export function findJobSourceByUrl(rawUrl: string): JobSourceCatalogEntry | undefined {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    return undefined;
  }
  const host = normalizeDomain(url.hostname);
  const path = url.pathname.toLowerCase().replace(/^\/+|\/+$/g, "");
  const pathSpecific = JOB_SOURCE_CATALOG.find((entry) => (
    entry.domains.some((domain) => host === domain || host.endsWith(`.${domain}`))
    && entry.kind === "public_curated_feed"
    && entry.aliases.some((alias) => path === alias.toLowerCase() || path.startsWith(`${alias.toLowerCase()}/`))
  ));
  if (pathSpecific) return pathSpecific;
  return JOB_SOURCE_CATALOG.find((entry) => entry.domains.some((domain) => (
    (host === domain || host.endsWith(`.${domain}`))
    && entry.kind !== "public_curated_feed"
  )));
}

export function findJobSourceByName(name: string): JobSourceCatalogEntry | undefined {
  const query = normalizeName(name);
  if (!query) return undefined;
  return JOB_SOURCE_CATALOG.find((entry) => [entry.name, ...entry.aliases]
    .some((candidate) => normalizeName(candidate) === query));
}

export function requiresCanonicalEmployerRevalidation(entry: JobSourceCatalogEntry): boolean {
  return entry.requiresCanonicalRevalidation;
}

function normalizeDomain(value: string): string {
  return value.trim().toLowerCase().replace(/^www\./, "").replace(/\.$/, "");
}

function normalizeName(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}
