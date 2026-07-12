import type {
  ApplicationPacket,
  ApplicationReceiptBundle,
  ApplicationState,
  NormalizedJob,
  RunnerKind,
  SubmissionReceipt,
  EvidenceObjectUpload,
} from "@bluey/jobs-automation";

export interface ApplicationWorkflowInput {
  accountId: string;
  applicationId: string;
  jobId: string;
  canonicalJobKey: string;
  packetId: string;
  applicationIdentityId: string;
  browserProfileId: string;
  packet: ApplicationPacket;
  job: NormalizedJob;
  runner: RunnerKind;
  url: string;
  idempotencyKey: string;
}

export interface ApplicationWorkflowResult {
  state: ApplicationState;
  receipt?: SubmissionReceipt;
  interventionId?: string;
}

export interface RunnerExecutionResult {
  receipt: SubmissionReceipt;
  receiptBundle?: ApplicationReceiptBundle;
  evidenceObjects?: EvidenceObjectUpload[];
}

export interface InterventionResolution {
  action: string;
  field?: string;
  answer?: string;
}

export interface JobsActivities {
  assertEntitlement(input: ApplicationWorkflowInput): Promise<void>;
  loadPacket(input: ApplicationWorkflowInput): Promise<void>;
  allocateBrowser(input: ApplicationWorkflowInput): Promise<{ browserSessionId: string }>;
  runApplication(input: ApplicationWorkflowInput & { browserSessionId: string }): Promise<RunnerExecutionResult>;
  resumeApplication(input: ApplicationWorkflowInput & {
    browserSessionId: string;
    requestId: string;
    resolution: InterventionResolution;
  }): Promise<RunnerExecutionResult>;
  persistReceipt(input: ApplicationWorkflowInput & {
    receiptBundle: ApplicationReceiptBundle;
    evidenceObjects: EvidenceObjectUpload[];
  }): Promise<void>;
  releaseBrowser(browserSessionId: string): Promise<void>;
  recordState(input: ApplicationWorkflowInput, state: ApplicationState): Promise<void>;
  createIntervention(input: ApplicationWorkflowInput, receipt: SubmissionReceipt): Promise<string>;
}
