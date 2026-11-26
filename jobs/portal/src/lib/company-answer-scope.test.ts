import { describe, expect, it } from "vitest";
import type { AnswerMemory, CompanyAnswerMemoryScope } from "../types";
import {
  companyMemoryInterventionReady, isCompanyAnswerScopeReference, validateCompanyAnswerScope,
} from "./company-answer-scope";

const scope: CompanyAnswerMemoryScope = {
  job_id: "job-one", scope_id: `company-v1-${"a".repeat(64)}`, company: "Example", domain: "example.test",
};

describe("verified company answer references", () => {
  it("never reuses confirmation for another intervention on the same job", () => {
    const state = { binding: scope, jobId: scope.job_id, interventionId: "question-one",
      loadedInterventionId: "question-one", confirmed: true, loading: false, error: "" };
    expect(companyMemoryInterventionReady(state)).toBe(true);
    expect(companyMemoryInterventionReady({ ...state, interventionId: "question-two" })).toBe(false);
    expect(companyMemoryInterventionReady({ ...state, interventionId: undefined })).toBe(false);
    expect(companyMemoryInterventionReady({ ...state, jobId: "job-two" })).toBe(false);
  });
  it("accepts the exact server reference for the selected job", () => {
    expect(validateCompanyAnswerScope(scope, "job-one", null)).toBe(scope);
  });

  it.each([undefined, "example", "company-v1-bad", `company-v1-${"A".repeat(64)}`])(
    "does not treat an old name or malformed reference as verified", (value) => {
      expect(isCompanyAnswerScopeReference(value)).toBe(false);
    },
  );

  it.each([
    { ...scope, job_id: "job-two" },
    { ...scope, scope_id: "example" },
    { ...scope, company: " " },
    { ...scope, domain: " " },
  ])("rejects mismatched or incomplete lookup responses", (value) => {
    expect(() => validateCompanyAnswerScope(value, "job-one", null)).toThrow();
  });

  it("does not rebind an existing verified answer to another employer", () => {
    const existing = { scope: "company", scope_id: `company-v1-${"b".repeat(64)}` } as AnswerMemory;
    expect(() => validateCompanyAnswerScope(scope, "job-one", existing)).toThrow("does not match");
  });

  it("allows explicit confirmation of a legacy record without using its name as identity", () => {
    const existing = { scope: "company", scope_id: "some-other-name" } as AnswerMemory;
    expect(validateCompanyAnswerScope(scope, "job-one", existing)).toBe(scope);
  });
});
