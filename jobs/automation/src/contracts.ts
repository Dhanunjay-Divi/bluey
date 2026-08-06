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
  | "side_effect_unknown"
  | "submitted"
  | "failed";

export type RunnerKind = "local" | "cloud";

export type FormControlKind =
  | "text"
  | "email"
  | "tel"
  | "url"
  | "textarea"
  | "select"
  | "checkbox"
  | "radio"
  | "file"
  | "hidden"
  | "other";

export interface FormFileEvidence {
  name: string;
  byteLength: number;
  sha256: string;
}

export interface FormControl {
  selector: string;
  kind: FormControlKind;
  label: string;
  name: string;
  placeholder: string;
  required: boolean;
  value: string;
  checked?: boolean;
  options?: Array<{ label: string; value: string }>;
  files?: FormFileEvidence[];
}

export interface BrowserPage {
  installExactSubmitGuard(
    adapter: CertifiedFinalSubmitAdapter,
    approvedCanonicalUrl: string,
  ): Promise<void>;
  beginExactSubmitGuard(): Promise<void>;
  assertExactSubmitGuardClean(): Promise<void>;
  url(): string;
  title(): Promise<string>;
  locator(selector: string): BrowserLocator;
  controls(): Promise<FormControl[]>;
  bodyText(): Promise<string>;
  waitForSettled(): Promise<void>;
  screenshot(options?: { fullPage?: boolean }): Promise<Uint8Array>;
}

export interface BrowserLocator {
  count(): Promise<number>;
  fill(value: string): Promise<void>;
  click(): Promise<void>;
  textContent(): Promise<string | null>;
  getAttribute(name: string): Promise<string | null>;
  isVisible(): Promise<boolean>;
  selectOption(value: string): Promise<void>;
  setChecked(checked: boolean): Promise<void>;
  setInputFiles(paths: string[]): Promise<FormFileEvidence[]>;
  effectiveSubmitTarget(
    adapter: CertifiedFinalSubmitAdapter,
  ): Promise<EffectiveSubmitTargetIdentity>;
  successfulSubmitEvidence(
    trustedFields: ReadonlyArray<Readonly<TrustedSubmitFieldValue>>,
    providerJobKey: string,
  ): Promise<Readonly<ExactSubmitFormEvidence>>;
  clickWithExactSubmit(expectation: ExactSubmitExpectation): Promise<number>;
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
  employmentType?: string;
  engagementType?: string;
}

export interface ApplicationPacket {
  applicationId: string;
  jobId: string;
  resumeVersionId: string;
  approvedPacketChecksum: string;
  resumePath?: string;
  resumeContent?: Record<string, unknown>;
  coverLetterPath?: string;
  coverLetterContent?: string;
  answers: Record<string, string>;
  verifiedClaimIds: string[];
  applicationIdentityId?: string;
  applicationEmail?: string;
  browserProfileId?: string;
  approvedExecutionSchemaVersion?: 1 | 2 | 3;
  approvedExecutionAdmission?:
    | { kind: "review_approval" }
    | {
        kind: "track_auto_submit";
        authorization_id: string;
        career_track_id: string;
        revision_no: number;
        authority_fingerprint: string;
        ats_certification?: AtsCertificationAdmission;
      };
}

/**
 * Server-derived ATS authority frozen into an approved schema-v3 Auto-submit
 * admission. These are opaque content identities, not reusable credentials;
 * the runner must still obtain and consume a short-lived pre-click binding.
 */
export interface AtsCertificationAdmission {
  schema_version: 1;
  provider: CertifiedFinalSubmitAdapter;
  adapter_version: string;
  variant_key: string;
  layout_contract_version: number;
  surface_sha256: string;
  manifest_sha256: string;
  activation_sha256: string;
  activation_generation: number;
  target_key_sha256: string;
  layout_set_sha256: string;
  adapter_bundle_sha256: string;
  runner_target_sha256s: string[];
  expires_at_ms: number;
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
  takeoverUrl?: string;
  resolution?: InterventionResolution;
}

export interface InterventionResolution {
  kind: "browser_takeover" | "email_otp_approval" | "answer";
  resumeAfter: boolean;
  expiresAt?: string;
  provider?: "gmail" | "outlook_email";
  messageId?: string;
}

export interface ValidationIssue {
  field: string;
  message: string;
  severity: "blocking" | "warning";
}

export interface SubmissionReceipt {
  status: "submitted" | "needs_input" | "failed";
  submitHttpStatus?: number;
  confirmationText?: string;
  confirmationUrl?: string;
  screenshotPath?: string;
  submittedAt?: string;
  issues: ValidationIssue[];
  intervention?: InterventionRequest;
}

export type FinalSubmitActivationOutcome = "activated" | "activation_uncertain";

export type CertifiedFinalSubmitAdapter = "greenhouse" | "lever";

export type CertifiedFinalSubmitControl =
  | "greenhouse_submit_application"
  | "lever_application_submit";

export interface EffectiveSubmitTargetIdentity {
  actionUrl: string;
  method: string;
  enctype: string;
  formTarget: string;
  providerJobKey: string;
  formIdentity: string;
}

export interface ExactSubmitFileEvidence extends FormFileEvidence {
  fieldName: string;
}

export interface ExactSubmitFieldEvidence {
  fieldName: string;
  valueByteLength: number;
  valueSha256: string;
}

export interface ExactSubmitPartOrderEntry {
  kind: "field" | "file";
  index: number;
}

export interface ExactSubmitFormEvidence {
  fields: ReadonlyArray<Readonly<ExactSubmitFieldEvidence>>;
  partOrder: ReadonlyArray<Readonly<ExactSubmitPartOrderEntry>>;
}

export interface TrustedSubmitFieldValue {
  fieldName: string;
  value: string;
}

export interface ExactSubmitExpectation {
  target: EffectiveSubmitTargetIdentity;
  files: ReadonlyArray<Readonly<ExactSubmitFileEvidence>>;
  fields: ReadonlyArray<Readonly<ExactSubmitFieldEvidence>>;
  partOrder: ReadonlyArray<Readonly<ExactSubmitPartOrderEntry>>;
}

/**
 * Provider-owned evidence emitted only at the exact certified submit-control
 * transition. Document evidence is deliberately added by the runner after the
 * approved PDFs have been materialized and hashed.
 */
export interface ProviderFinalSubmitProof {
  adapter: CertifiedFinalSubmitAdapter;
  adapterVersion: string;
  control: CertifiedFinalSubmitControl;
  target: EffectiveSubmitTargetIdentity;
  files: ReadonlyArray<Readonly<ExactSubmitFileEvidence>>;
  fields: ReadonlyArray<Readonly<ExactSubmitFieldEvidence>>;
  partOrder: ReadonlyArray<Readonly<ExactSubmitPartOrderEntry>>;
}

export interface FinalSubmitDocumentProof {
  kind: "resume" | "cover_letter";
  versionId?: string;
  sha256: string;
}

export interface FinalSubmitJobProof {
  approvedCanonicalUrl: string;
  pageUrl: string;
}

export interface ReviewedFinalSubmitProof extends ProviderFinalSubmitProof {
  schemaVersion: 3;
  job: FinalSubmitJobProof;
  documents: FinalSubmitDocumentProof[];
}

export interface AtsFinalSubmitCertificationProof {
  schemaVersion: 1;
  provider: CertifiedFinalSubmitAdapter;
  adapterVersion: string;
  manifestSha256: string;
  activationSha256: string;
  activationGeneration: number;
  targetKeySha256: string;
  layoutSetSha256: string;
  adapterBundleSha256: string;
  runnerTargetSha256s: string[];
  expiresAtMs: number;
}

export interface AtsFinalSubmitObservedSurfaceProof {
  schemaVersion: 1;
  variantKey: string;
  layoutContractVersion: number;
  surfaceSha256: string;
}

export interface CertifiedFinalSubmitProof extends ProviderFinalSubmitProof {
  schemaVersion: 4;
  job: FinalSubmitJobProof;
  documents: FinalSubmitDocumentProof[];
  certification: AtsFinalSubmitCertificationProof;
  observedSurface: AtsFinalSubmitObservedSurfaceProof;
}

export type FinalSubmitProof = ReviewedFinalSubmitProof | CertifiedFinalSubmitProof;

export interface AdapterContext {
  runner: RunnerKind;
  runId: string;
  accountId: string;
  approvedCanonicalUrl: string;
  page: BrowserPage;
  packet: ApplicationPacket;
  log(event: string, details?: Record<string, unknown>): Promise<void>;
  beforeFinalSubmit?(proof: ProviderFinalSubmitProof): Promise<void>;
  afterFinalSubmit?(outcome: FinalSubmitActivationOutcome): Promise<void>;
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
