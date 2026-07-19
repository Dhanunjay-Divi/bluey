import type { Page } from "playwright";
import type { LocalProviderFinalReview } from "./provider-final-review.js";
import type { LocalRunDelivery, StartRunRequest } from "./local-run-contracts.js";

export interface ActiveLocalRun {
  request: StartRunRequest;
  delivery: LocalRunDelivery;
  page: Page;
  runDirectory: string;
  checkpointScope: string;
  checkpointCreatedAtMs: number;
  providerFinalReview?: LocalProviderFinalReview;
  approvedSubmitActionConsumed?: boolean;
  uiInterventionKind?: string;
  events: Array<{ event: string; details: Record<string, unknown>; at: string }>;
}
