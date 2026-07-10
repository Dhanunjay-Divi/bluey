import { describe, expect, it } from "vitest";
import { inferProfileFromResume } from "./documents";
import type { CareerProfile } from "../types";

describe("resume import inference", () => {
  it("fills missing contact fields without overwriting confirmed profile data", () => {
    const profile = {
      full_name: "Confirmed Name",
      email: "",
      phone: "",
    } as CareerProfile;
    const result = inferProfileFromResume(profile, {
      name: "resume.pdf",
      text: "Different Name\ndev@example.com\n(212) 555-0199",
    });
    expect(result.full_name).toBe("Confirmed Name");
    expect(result.email).toBe("dev@example.com");
    expect(result.phone).toContain("212");
    expect(result.source_resume_name).toBe("resume.pdf");
  });
});
