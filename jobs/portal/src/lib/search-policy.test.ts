import { describe, expect, it } from "vitest";
import type { EmploymentEntry } from "../types";
import { experienceRange, roleExperienceRange } from "./search-policy";

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
});
