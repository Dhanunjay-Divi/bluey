import { describe, expect, it } from "vitest";
import {
  planApplicationForm,
  resolveMemory,
  type AnswerMemoryEntry,
  type ApplicationAnswerProfile,
  type ApplicationFormField,
} from "../src/index.js";

const profile: ApplicationAnswerProfile = {
  facts: {
    full_name: { value: "Avery Chen", verified: true, source: "resume" },
    email: { value: "avery@example.com", verified: true, source: "user" },
    current_title: { value: "Staff Engineer", verified: false, source: "bluey" },
  },
  resumePath: "/packets/job-1/resume.pdf",
};

describe("application form intelligence", () => {
  it("fills confirmed facts, uploads the job resume, and pauses on unknown required questions", () => {
    const fields: ApplicationFormField[] = [
      { id: "name", label: "Full name", type: "text", required: true },
      { id: "resume", label: "Resume", type: "file", required: true },
      { id: "why", label: "Why do you want to work here?", type: "textarea", required: true },
      { id: "site", label: "Personal site", type: "url", required: false },
    ];
    const plan = planApplicationForm(fields, profile, [], { trackId: "track-1", companyId: "acme", autoSubmit: false });

    expect(plan.actions).toEqual(expect.arrayContaining([
      expect.objectContaining({ fieldId: "name", action: "fill", value: "Avery Chen" }),
      expect.objectContaining({ fieldId: "resume", action: "upload", value: profile.resumePath }),
      expect.objectContaining({ fieldId: "why", action: "intervene" }),
      expect.objectContaining({ fieldId: "site", action: "skip" }),
    ]));
    expect(plan.canSubmit).toBe(false);
  });

  it("keeps unconfirmed generated facts out of Auto-submit", () => {
    const plan = planApplicationForm(
      [{ id: "title", label: "Current title", type: "text", required: true }],
      profile,
      [],
      { trackId: "track-1", companyId: "acme", autoSubmit: true },
    );
    expect(plan.actions[0]?.action).toBe("intervene");
    expect(plan.interventions[0]?.detail).toContain("not been confirmed");
  });

  it("uses company memory before track and account memory", () => {
    const entries: AnswerMemoryEntry[] = [
      { key: "Why Acme?", value: "Account answer", scope: "account", confirmed: true },
      { key: "Why Acme?", value: "Track answer", scope: "track", scopeId: "track-1", confirmed: true },
      { key: "Why Acme?", value: "Company answer", scope: "company", scopeId: "acme", confirmed: true },
    ];
    expect(resolveMemory("why acme", entries, { trackId: "track-1", companyId: "acme", autoSubmit: false })?.value)
      .toBe("Company answer");
  });

  it("accepts API-shaped answer memory without a worker-side rewrite", () => {
    const entries: AnswerMemoryEntry[] = [
      { id: "answer-1", key: "Why Acme?", value: "Saved company answer", scope: "company", scope_id: "acme", confirmed: true },
    ];
    expect(resolveMemory("why acme", entries, { trackId: "track-1", companyId: "acme", autoSubmit: false })?.value)
      .toBe("Saved company answer");
  });

  it("does not guess a required sensitive answer", () => {
    const plan = planApplicationForm(
      [{ id: "veteran", label: "Veteran status", type: "select", required: true, options: ["Yes", "No", "Decline"] }],
      profile,
      [],
      { trackId: "track-1", companyId: "acme", autoSubmit: false },
    );
    expect(plan.interventions[0]?.kind).toBe("sensitive_question");
    expect(plan.canSubmit).toBe(false);
  });

  it("pauses on sensitive questions even if a reusable answer exists", () => {
    const plan = planApplicationForm(
      [{ id: "gender", label: "Gender", type: "select", required: true, options: ["Woman", "Man", "Decline"] }],
      profile,
      [{ key: "Gender", value: "Decline", scope: "account", confirmed: true }],
      { trackId: "track-1", companyId: "acme", autoSubmit: true },
    );
    expect(plan.actions[0]?.action).toBe("intervene");
    expect(plan.interventions[0]?.kind).toBe("sensitive_question");
  });

  it("pauses on optional demographic questions instead of silently answering them", () => {
    const plan = planApplicationForm(
      [{ id: "ethnicity", label: "Ethnicity", type: "select", required: false, options: ["Decline"] }],
      profile,
      [],
      { trackId: "track-1", companyId: "acme", autoSubmit: false },
    );
    expect(plan.actions[0]?.action).toBe("intervene");
    expect(plan.canSubmit).toBe(false);
  });
});
