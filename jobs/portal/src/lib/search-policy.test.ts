import { describe, expect, it } from "vitest";
import type { EmploymentEntry } from "../types";
import { experienceRange, roleExperienceAssessment, roleExperienceRange } from "./search-policy";

function role(
  start_date: string,
  end_date: string,
  current = false,
  title = "Software Engineer",
): EmploymentEntry {
  return {
    id: `${title}-${start_date}-${end_date}`,
    company: "Example",
    title,
    location: "Arlington, VA",
    start_date,
    end_date,
    current,
    highlights: [],
  };
}

describe("Bluey search policy", () => {
  it("targets roughly one year below through two years above candidate experience", () => {
    expect(experienceRange(
      [role("2022-01", "2024-01")],
      new Date("2026-01-01T00:00:00Z"),
    )).toEqual({ years: 2, minimum: 1, maximum: 4 });
  });

  it("counts the completed end month inclusively", () => {
    expect(experienceRange(
      [role("2022-01", "2023-12")],
      new Date("2026-01-01T00:00:00Z"),
    )).toEqual({ years: 2, minimum: 1, maximum: 4 });
  });

  it("does not double-count overlapping roles", () => {
    expect(experienceRange(
      [role("2022-01", "2024-01"), role("2023-01", "2025-01")],
      new Date("2026-01-01T00:00:00Z"),
    )).toEqual({ years: 3, minimum: 2, maximum: 5 });
  });

  it("uses only employment relevant to the target role family", () => {
    expect(roleExperienceRange(
      [
        role("2022-01", "2024-01"),
        role("2015-01", "2020-01", false, "Clinical Research Coordinator"),
      ],
      "SWE",
      new Date("2026-01-01T00:00:00Z"),
    )).toEqual({ years: 2, minimum: 1, maximum: 4 });
  });

  it("keeps overlapping relevant roles merged after filtering", () => {
    expect(roleExperienceRange(
      [
        role("2022-01", "2024-01", false, "Backend Developer"),
        role("2023-01", "2025-01", false, "Software Engineer"),
        role("2015-01", "2020-01", false, "Clinical Operations Manager"),
      ],
      "Software Engineer",
      new Date("2026-01-01T00:00:00Z"),
    )).toEqual({ years: 3, minimum: 2, maximum: 5 });
  });

  it("uses token-safe posting-title spans without classifying highlights", () => {
    const unrelated = {
      ...role("2015-01", "2020-01", false, "Sweeper"),
      highlights: ["Prepared SWE status reports"],
    };
    expect(roleExperienceRange(
      [unrelated, role("2022-01", "2024-01")],
      "Software Engineer",
      new Date("2026-01-01T00:00:00Z"),
    )).toEqual({ years: 2, minimum: 1, maximum: 4 });
  });

  it("counts composite titles only when their posting evidence proves one target family", () => {
    expect(roleExperienceAssessment(
      [
        role("2020-01", "2022-01", false, "Software Engineer, Backend"),
        role("2017-01", "2019-01", false, "Product Manager / Project Manager"),
      ],
      "Software Engineer",
      new Date("2026-01-01T00:00:00Z"),
    )).toMatchObject({
      range: { years: 2, minimum: 1, maximum: 4 },
      relevant_entry_count: 1,
    });
  });

  it("does not count terse QA or DE aliases embedded in unrelated employment titles", () => {
    expect(roleExperienceAssessment(
      [
        role("2015-01", "2020-01", false, "QA Coordinator"),
        role("2020-01", "2024-01", false, "DE&I Specialist"),
      ],
      "Software Engineer",
      new Date("2026-01-01T00:00:00Z"),
    )).toMatchObject({
      range: { years: 0, minimum: 0, maximum: 2 },
      relevant_entry_count: 0,
    });
  });

  it("marks ambiguous and custom targets for review without silently assigning a family", () => {
    const employment = [role("2022-01", "2024-01")];
    expect(roleExperienceAssessment(
      employment,
      "PM",
      new Date("2026-01-01T00:00:00Z"),
    )).toMatchObject({
      review_required: true,
      relevant_entry_count: 1,
      target_role: { status: "ambiguous" },
    });
    expect(roleExperienceAssessment(
      employment,
      "Clinical AI Workflow Specialist",
      new Date("2026-01-01T00:00:00Z"),
    )).toMatchObject({
      review_required: true,
      relevant_entry_count: 1,
      target_role: {
        status: "custom",
        custom_label: "Clinical AI Workflow Specialist",
      },
    });
  });
});
