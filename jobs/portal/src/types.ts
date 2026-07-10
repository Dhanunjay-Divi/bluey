export type ResumeMode = "factual" | "enhance";
export type SubmissionMode = "review_first" | "auto_submit";
export type ApplicationState =
  | "matched"
  | "preparing"
  | "needs_confirmation"
  | "awaiting_review"
  | "queued"
  | "running"
  | "needs_input"
  | "submitted"
  | "failed";

export interface EmploymentEntry {
  id: string;
  company: string;
  title: string;
  location: string;
  start_date: string;
  end_date: string;
  current: boolean;
  highlights: string[];
}

export interface EducationEntry {
  id: string;
  school: string;
  degree: string;
  field: string;
  start_date: string;
  end_date: string;
  location: string;
}

export interface ProjectEntry {
  id: string;
  name: string;
  role: string;
  summary: string;
  technologies: string[];
  url: string;
}

export interface CareerProfile {
  full_name: string;
  email: string;
  phone: string;
  headline: string;
  current_location: string;
  street_address: string;
  summary: string;
  linkedin_url: string;
  portfolio_url: string;
  work_authorization: string;
  sponsorship_required: boolean | null;
  salary_expectation: string;
  notice_period: string;
  skills: string[];
  certifications: string[];
  employment: EmploymentEntry[];
  education: EducationEntry[];
  projects: ProjectEntry[];
  reusable_answers: Record<string, string>;
  source_resume_name: string;
  source_resume_text: string;
  resume_mode: ResumeMode;
  review_new_claims: boolean;
  default_submission_mode: SubmissionMode;
  auto_submit_threshold: number;
  daily_limit: number;
  onboarding_step: number;
  onboarding_complete: boolean;
  updated_at_ms: number;
}

export interface CareerFact {
  id: string;
  category: string;
  label: string;
  value: unknown;
  source: "resume_import" | "user_entry" | "bluey_suggestion" | string;
  verification_status: "unverified" | "needs_confirmation" | "confirmed" | "rejected";
  confirmed_at_ms?: number;
  confirmed_by?: string;
  schema_version: number;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface JobPreferences {
  desired_roles: string[];
  desired_locations: string[];
  location_policy: "local" | "willing_to_relocate" | "remote_only" | "ask";
  remote_preference: string;
  employment_types: string[];
  minimum_compensation?: number;
  sponsorship: string;
  excluded_companies: string[];
  excluded_titles: string[];
  daily_limit: number;
  apply_once_per_company: boolean;
  updated_at_ms: number;
}

export interface CareerTrack {
  id: string;
  name: string;
  role: string;
  locations: string[];
  remote_preference: string;
  active: boolean;
  match_count: number;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface JobPosting {
  id: string;
  canonical_key: string;
  source: string;
  external_id: string;
  company: string;
  title: string;
  location: string;
  workplace: string;
  canonical_url: string;
  description: string;
  compensation: string;
  track_id: string;
  match_score: number;
  matched_reasons: string[];
  missing_requirements: string[];
  status: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface ResumeVersion {
  id: string;
  job_id: string;
  version_no: number;
  mode: ResumeMode;
  content: ResumeContent;
  diff: Record<string, unknown>;
  claim_ids: string[];
  checksum: string;
  created_at_ms: number;
}

export interface ResumeContent {
  target?: { company?: string; title?: string; location?: string };
  contact?: Record<string, string>;
  headline?: string;
  summary?: string;
  skills?: string[];
  employment?: EmploymentEntry[];
  education?: EducationEntry[];
  projects?: ProjectEntry[];
  certifications?: string[];
}

export interface JobApplication {
  id: string;
  job_id: string;
  resume_version_id?: string;
  state: ApplicationState;
  submission_mode: SubmissionMode;
  match_score: number;
  answers: Array<Record<string, unknown>>;
  cover_letter: string;
  receipt: Record<string, unknown>;
  run_id?: string;
  created_at_ms: number;
  updated_at_ms: number;
  submitted_at_ms?: number;
}

export interface BrowserSession {
  id: string;
  runner: "local" | "cloud";
  status: string;
  current_company: string;
  current_step: string;
  application_id?: string;
  takeover_url?: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface Intervention {
  id: string;
  application_id?: string;
  kind: string;
  status: string;
  title: string;
  detail: string;
  choices: string[];
  created_at_ms: number;
  resolved_at_ms?: number;
}

export interface JobsIntegration {
  id: string;
  provider: string;
  status: string;
  account_label: string;
  capabilities: string[];
  updated_at_ms: number;
}

export interface JobsEntitlement {
  plan: "free" | "pro" | "cloud";
  track_limit: number;
  monthly_packet_limit: number;
  used_packets: number;
  period_start_ms: number;
  period_end_ms: number;
  local_browser: boolean;
  cloud_browser: boolean;
  overage_cents: number;
}

export interface JobsWorkspace {
  profile: CareerProfile;
  preferences: JobPreferences;
  facts: CareerFact[];
  tracks: CareerTrack[];
  matches: JobPosting[];
  applications: JobApplication[];
  browser_sessions: BrowserSession[];
  interventions: Intervention[];
  integrations: JobsIntegration[];
  entitlement: JobsEntitlement;
}

export interface AccountSummary {
  email: string;
  balance_cents: number;
}

export interface PrepareApplicationResponse {
  application: JobApplication;
  resume_version: ResumeVersion;
}

export interface PacketCommitResult {
  newly_metered: boolean;
  included: boolean;
  amount_cents: number;
  used_packets: number;
  monthly_packet_limit: number;
}
