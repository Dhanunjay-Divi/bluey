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

  it("stores common abbreviations as canonical full role names", () => {
    expect(canonicalTargetRoles(["SWE", "SDE", "PM", "Data Engineer II"])).toEqual([
      "Software Engineer",
      "Product Manager",
      "Data Engineer",
    ]);
    expect(canonicalizeTargetRole("Capital One, Software Engineer")).toBe("Software Engineer");
    expect(targetRoleSuggestions("pm")).toEqual(expect.arrayContaining([
      "Product Manager",
      "Project Manager",
      "Program Manager",
    ]));
  });

  it("keeps custom full-form roles available for later review", () => {
    expect(canonicalizeTargetRole("Clinical AI Workflow Specialist")).toBe("Clinical AI Workflow Specialist");
  });
});
