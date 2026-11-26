import { describe, expect, it } from "vitest";
import {
  canonicalTargetRoles,
  canonicalizeTargetRole,
  filterCareerSuggestions,
  mergeCareerSuggestions,
  targetRoleSuggestions,
} from "./career-suggestions";

describe("career suggestions", () => {
  it("ranks prefix matches before word and contains matches", () => {
    expect(filterCareerSuggestions("clinical", [
      "Lead Clinical Research Analyst",
      "Clinical Research Coordinator",
      "Healthcare Clinical Analyst",
    ])).toEqual([
      "Clinical Research Coordinator",
      "Lead Clinical Research Analyst",
      "Healthcare Clinical Analyst",
    ]);
  });

  it("deduplicates imported and built-in values and omits selected tags", () => {
    const merged = mergeCareerSuggestions(
      ["Hyderabad, India", "Indianapolis, IN"],
      ["hyderabad, india", "Falls Church, VA"],
    );
    expect(merged).toEqual(["Hyderabad, India", "Indianapolis, IN", "Falls Church, VA"]);
    expect(filterCareerSuggestions("in", merged, ["Indianapolis, IN"])).toEqual(["Hyderabad, India"]);
  });

  it("stores deterministic abbreviations canonically and preserves ambiguous ones for review", () => {
    expect(canonicalTargetRoles(["SWE", "SDE", "PM", "Data Engineer II"])).toEqual([
      "Software Engineer",
      "PM",
      "Data Engineer",
    ]);
    expect(canonicalizeTargetRole("Capital One, Software Engineer")).toBe(
      "Capital One, Software Engineer",
    );
    expect(canonicalizeTargetRole("Software Engineer, Product Manager")).toBe(
      "Software Engineer, Product Manager",
    );
    expect(canonicalizeTargetRole("TPM")).toBe("TPM");
    expect(targetRoleSuggestions("pm").slice(0, 3)).toEqual([
      "Product Manager",
      "Project Manager",
      "Program Manager",
    ]);
    expect(targetRoleSuggestions("tpm").slice(0, 2)).toEqual([
      "Technical Product Manager",
      "Technical Program Manager",
    ]);
  });

  it("keeps custom full-form roles available for later review", () => {
    expect(canonicalizeTargetRole("Clinical AI Workflow Specialist")).toBe("Clinical AI Workflow Specialist");
    expect(canonicalizeTargetRole("Clinical AI Workflow Specialist, Software Engineer")).toBe(
      "Clinical AI Workflow Specialist, Software Engineer",
    );
  });

  it("keeps the prior exact alias surface through the shared authority", () => {
    expect(canonicalTargetRoles([
      "Application Developer",
      "Frontend",
      "Back-end",
      "Fullstack",
      "DevOps",
      "Product Owner",
    ])).toEqual([
      "Software Engineer",
      "Frontend Engineer",
      "Backend Engineer",
      "Full-stack Engineer",
      "DevOps Engineer",
      "Product Manager",
    ]);
  });
});
