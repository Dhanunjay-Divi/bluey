import type { AnswerMemory, CompanyAnswerMemoryScope } from "../types";

/** Syntax only: current employer verification is performed by the server lookup/write. */
export function isCompanyAnswerScopeReference(scopeId: string | undefined): boolean {
  return /^company-v1-[a-f0-9]{64}$/u.test(scopeId ?? "");
}

/** Validate a lookup response, never mint a scope from a display name. */
export function validateCompanyAnswerScope(
  value: CompanyAnswerMemoryScope,
  selectedJobId: string,
  existing: AnswerMemory | null,
): CompanyAnswerMemoryScope {
  if (!value || value.job_id !== selectedJobId || !selectedJobId
    || !isCompanyAnswerScopeReference(value.scope_id)
    || typeof value.company !== "string" || !value.company.trim()
    || typeof value.domain !== "string" || !value.domain.trim()) {
    throw new Error("This employer could not be verified. Choose a current job and try again.");
  }
  if (existing?.scope === "company" && isCompanyAnswerScopeReference(existing.scope_id)
    && existing.scope_id !== value.scope_id) {
    throw new Error("This saved employer reference does not match. Add a new saved answer instead.");
  }
  return value;
}

export function companyMemoryInterventionReady(state: {
  binding: CompanyAnswerMemoryScope | null;
  jobId: string | undefined;
  interventionId: string | undefined;
  loadedInterventionId: string;
  confirmed: boolean;
  loading: boolean;
  error: string;
}): boolean {
  return Boolean(state.interventionId && state.binding?.job_id === state.jobId
    && state.loadedInterventionId === state.interventionId
    && state.confirmed && !state.loading && !state.error);
}
