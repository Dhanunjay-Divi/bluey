import { describe, expect, it } from "vitest";
import type { EmploymentEntry } from "../types";
import { experienceRange } from "./search-policy";

function role(start_date: string, end_date: string, current = false): EmploymentEntry {
  return {
    id: `${start_date}-${end_date}`,
    company: "Example",
    title: "Software Engineer",
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
});
