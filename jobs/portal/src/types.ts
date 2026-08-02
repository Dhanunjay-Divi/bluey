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
  | "side_effect_unknown"
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
  max_posting_age_days: number;
  time_zone_offset_minutes?: number;
  updated_at_ms: number;
}

export interface CareerTrack {
  id: string;
  name: string;
  role: string;
  locations: string[];
  remote_preference: string;
  application_identity_id?: string;
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
  posted_at_ms?: number;
  last_verified_at_ms?: number;
  availability_status: "active" | "expired" | "unknown" | string;
  status: string;
  created_at_ms: number;
  updated_at_ms: number;
  eligibility?: JobEligibilityDecision;
}

export interface UserJobInput {
  canonical_url: string;
  pasted_description?: string;
  company: string;
  title: string;
  location?: string;
  workplace?: string;
  compensation?: string;
  track_id: string;
}

export interface EligibilityReason {
  code: string;
  message: string;
}

export type SubmissionCapability = "certified" | "beta_review" | "handoff" | "unknown_review" | "blocked";

export interface JobEligibilityDecision {
  capability: SubmissionCapability;
  can_prepare: boolean;
  can_auto_submit: boolean;
  can_queue_local: boolean;
  can_queue_cloud: boolean;
  hard_failures: EligibilityReason[];
  review_reasons: EligibilityReason[];
  passed_checks: string[];
  evaluated_at_ms: number;
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

export interface JobsBlueyHandoffIssueResponse {
  schema_version: 1;
  audience: "bluey-desktop-interview-prep-v1";
  nonce: string;
  deep_link_url: string;
  expires_at_ms: number;
  expires_in_seconds: number;
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
  resolution_kind: "browser_takeover" | "email_otp_approval" | "answer" | string;
  resume_after_resolution: boolean;
  provider: string;
  provider_message_id: string;
  expires_at_ms?: number;
  metadata: Record<string, unknown>;
  created_at_ms: number;
  resolved_at_ms?: number;
}

export type AnswerMemoryScope = "account" | "track" | "company";

export interface AnswerMemory {
  id: string;
  key: string;
  question: string;
  value: string;
  scope: AnswerMemoryScope;
  scope_id?: string;
  confirmed: boolean;
  source: "settings" | "intervention" | string;
  created_at_ms: number;
  updated_at_ms: number;
  last_used_at_ms?: number;
  use_count: number;
}

export interface InterventionResolutionResult {
  intervention: Intervention;
  answer_memory?: AnswerMemory;
  application?: JobApplication;
}

export interface ApplicationEvidence {
  id: string;
  application_id: string;
  kind: "resume" | "cover_letter" | "attachment" | "submission_confirmation" | "status_email" | "interview_event" | string;
  label: string;
  provider: string;
  file_name: string;
  media_type: string;
  storage_key: string;
  sha256: string;
  resume_version_id?: string;
  occurred_at_ms: number;
  metadata: Record<string, unknown>;
  created_at_ms: number;
}

export interface JobsIntegration {
  id: string;
  provider: string;
  status: string;
  account_label: string;
  capabilities: string[];
  updated_at_ms: number;
}

export interface ApplicationIdentity {
  id: string;
  email: string;
  label: string;
  verification_status: "pending" | "verified";
  is_default: boolean;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface MailboxConnection {
  id: string;
  provider: "gmail" | "outlook";
  status: "pending" | "connected" | "disconnected";
  account_label: string;
  aliases: string[];
  capabilities: string[];
  created_at_ms: number;
  updated_at_ms: number;
}

export type DiscoverySourceHealth = "healthy" | "degraded" | "paused" | "waiting";

export interface DiscoverySource {
  id: string;
  provider: string;
  config: {
    company: string;
  };
  status: "active" | "paused";
  health: DiscoverySourceHealth;
  last_success_at_ms: number | null;
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
  monthly_price_cents: number;
  application_identity_limit: number;
  connected_inbox_limit: number;
  additional_inbox_cents: number;
}

export interface JobsWorkspace {
  profile: CareerProfile;
  preferences: JobPreferences;
  facts: CareerFact[];
  tracks: CareerTrack[];
  matches: JobPosting[];
  applications: JobApplication[];
  application_evidence: ApplicationEvidence[];
  browser_sessions: BrowserSession[];
  interventions: Intervention[];
  answer_memory: AnswerMemory[];
  integrations: JobsIntegration[];
  application_identities: ApplicationIdentity[];
  mailbox_connections: MailboxConnection[];
  discovery_sources: DiscoverySource[];
  entitlement: JobsEntitlement;
}

export interface AccountSummary {
  email: string;
  balance_cents: number;
}

export interface PrepareApplicationResponse {
  application: JobApplication;
  resume_version: ResumeVersion;
  metering?: PacketCommitResult;
}

export interface PacketCommitResult {
  newly_metered: boolean;
  included: boolean;
  amount_cents: number;
  used_packets: number;
  monthly_packet_limit: number;
}

export interface ApproveApplicationResponse {
  application: JobApplication;
  metering: PacketCommitResult;
}

export interface QueueApplicationRunResponse {
  application: JobApplication;
  browser_session: BrowserSession;
  workflow_id: string;
  run_id: string;
  launch_url?: string;
}
