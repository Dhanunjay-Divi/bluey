import type { ApplicationState, RunnerKind, SubmissionReceipt } from "@bluey/jobs-automation";

export interface ApplicationWorkflowInput {
  accountId: string;
  applicationId: string;
  jobId: string;
  canonicalJobKey: string;
  packetId: string;
  runner: RunnerKind;
  url: string;
  idempotencyKey: string;
}

export interface ApplicationWorkflowResult {
  state: ApplicationState;
  receipt?: SubmissionReceipt;
  interventionId?: string;
}

export interface JobsActivities {
  assertEntitlement(input: ApplicationWorkflowInput): Promise<void>;
  loadPacket(input: ApplicationWorkflowInput): Promise<void>;
  allocateBrowser(input: ApplicationWorkflowInput): Promise<{ browserSessionId: string }>;
  runApplication(input: ApplicationWorkflowInput & { browserSessionId: string }): Promise<SubmissionReceipt>;
  persistReceipt(input: ApplicationWorkflowInput & { receipt: SubmissionReceipt }): Promise<void>;
  releaseBrowser(browserSessionId: string): Promise<void>;
  recordState(applicationId: string, state: ApplicationState): Promise<void>;
  createIntervention(applicationId: string, receipt: SubmissionReceipt): Promise<string>;
}
