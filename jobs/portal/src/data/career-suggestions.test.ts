import { describe, expect, it } from "vitest";
import { filterCareerSuggestions, mergeCareerSuggestions } from "./career-suggestions";

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
});
