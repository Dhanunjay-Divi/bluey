import { describe, expect, it } from "vitest";
import type { CareerProfile } from "../types";
import {
  validateCareerProfile,
  validateEducationEntries,
  validateEmploymentEntries,
  validateProfileIdentity,
} from "./profile-validation";

function profile(): CareerProfile {
  return {
    full_name: "Taylor Rivera",
    email: "taylor@example.com",
    phone: "",
    headline: "Software Engineer",
    current_location: "Austin, TX",
    street_address: "",
    summary: "",
    linkedin_url: "linkedin.com/in/taylor",
    portfolio_url: "https://taylor.example.com",
    work_authorization: "",
    sponsorship_required: null,
    salary_expectation: "",
    notice_period: "",
    skills: [],
    certifications: [],
    employment: [],
    education: [],
    projects: [],
    reusable_answers: {},
    source_resume_name: "",
    source_resume_text: "",
    resume_mode: "factual",
    review_new_claims: false,
    default_submission_mode: "review_first",
    auto_submit_threshold: 80,
    daily_limit: 10,
    onboarding_step: 0,
    onboarding_complete: false,
    updated_at_ms: 0,
  };
}

describe("Career Profile validation", () => {
  it("requires a valid resume contact email", () => {
    expect(validateProfileIdentity({ ...profile(), email: "" })).toContain("contact email");
    expect(validateProfileIdentity({ ...profile(), email: "not-an-email" })).toContain("valid resume contact email");
    expect(validateProfileIdentity(profile())).toBe("");
  });

  it("rejects incomplete roles and reversed dates", () => {
    expect(validateEmploymentEntries([{
      id: "role",
      company: "Northstar",
      title: "",
      location: "",
      start_date: "2024-01",
      end_date: "",
      current: true,
      highlights: [],
    }])).toContain("company and title");
    expect(validateEmploymentEntries([{
      id: "role",
      company: "Northstar",
      title: "Engineer",
      location: "",
      start_date: "2024-01",
      end_date: "2023-12",
      current: false,
      highlights: [],
    }])).toContain("earlier");
  });

  it("rejects incomplete education but permits an empty optional section", () => {
    expect(validateEducationEntries([])).toBe("");
    expect(validateEducationEntries([{
      id: "school",
      school: "Example University",
      degree: "",
      field: "",
      start_date: "",
      end_date: "",
      location: "",
    }])).toContain("degree or field");
  });

  it("validates the complete profile before saving", () => {
    expect(validateCareerProfile({ ...profile(), portfolio_url: "not a url" })).toContain("Portfolio URL");
    expect(validateCareerProfile(profile())).toBe("");
  });
});
