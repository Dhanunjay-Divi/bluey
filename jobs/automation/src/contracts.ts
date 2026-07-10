export type AtsKind =
  | "workday"
  | "greenhouse"
  | "lever"
  | "ashby"
  | "smartrecruiters"
  | "semantic";

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

export type RunnerKind = "local" | "cloud";

export interface BrowserPage {
  url(): string;
  title(): Promise<string>;
  locator(selector: string): BrowserLocator;
  screenshot(options?: { fullPage?: boolean }): Promise<Uint8Array>;
}

export interface BrowserLocator {
  count(): Promise<number>;
  fill(value: string): Promise<void>;
  click(): Promise<void>;
  textContent(): Promise<string | null>;
}

export interface NormalizedJob {
  externalId: string;
  canonicalUrl: string;
  company: string;
  title: string;
  location: string;
  workplace: "onsite" | "hybrid" | "remote" | "unknown";
  description: string;
  source: AtsKind;
  postedAt?: string;
  compensation?: string;
  department?: string;
}

export interface ApplicationPacket {
  applicationId: string;
  jobId: string;
  resumeVersionId: string;
  resumePath: string;
  coverLetterPath?: string;
  answers: Record<string, string>;
  verifiedClaimIds: string[];
}

export interface InterventionRequest {
  kind:
    | "captcha"
    | "two_factor"
    | "assessment"
    | "unknown_question"
    | "missing_fact"
    | "sensitive_question"
    | "browser_takeover";
  title: string;
  detail: string;
  field?: string;
  choices?: string[];
}

export interface ValidationIssue {
  field: string;
  message: string;
  severity: "blocking" | "warning";
}

export interface SubmissionReceipt {
  status: "submitted" | "needs_input" | "failed";
  confirmationText?: string;
  confirmationUrl?: string;
  screenshotPath?: string;
  submittedAt?: string;
  issues: ValidationIssue[];
  intervention?: InterventionRequest;
}

export interface AdapterContext {
  runner: RunnerKind;
  runId: string;
  accountId: string;
  page: BrowserPage;
  packet: ApplicationPacket;
  log(event: string, details?: Record<string, unknown>): Promise<void>;
}

export interface ApplicationAdapter {
  readonly kind: AtsKind;
  readonly version: string;
  detect(url: URL): boolean;
  normalize(page: BrowserPage): Promise<NormalizedJob>;
  prepare(context: AdapterContext): Promise<void>;
  fill(context: AdapterContext): Promise<void>;
  validate(context: AdapterContext): Promise<ValidationIssue[]>;
  submit(context: AdapterContext): Promise<SubmissionReceipt>;
}

export interface DiscoveryQuery {
  roles: string[];
  locations: string[];
  remotePreference: string;
  excludedCompanies: string[];
  excludedTitles?: string[];
  sources?: PublicAtsSource[];
  pageSize?: number;
  maxPostingAgeDays?: number;
  cursor?: string;
}

export type PublicAtsSource =
  | { kind: "greenhouse"; boardToken: string; company?: string }
  | { kind: "lever"; site: string; company?: string }
  | { kind: "ashby"; boardName: string; company?: string }
  | { kind: "smartrecruiters"; companyIdentifier: string; company?: string }
  | {
      kind: "workday";
      tenant: string;
      instance: string;
      site: string;
      company?: string;
      locale?: string;
    };

export interface DiscoveryPage {
  jobs: NormalizedJob[];
  nextCursor?: string;
  warnings?: string[];
}

export interface DiscoveryProvider {
  readonly name: string;
  search(query: DiscoveryQuery): Promise<DiscoveryPage>;
}
