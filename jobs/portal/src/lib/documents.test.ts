import { describe, expect, it } from "vitest";
import { inferProfileFromResume, pdfTextItemsToText, summarizeResumeImport } from "./documents";
import type { CareerProfile } from "../types";

function emptyProfile(): CareerProfile {
  return {
    full_name: "",
    email: "",
    phone: "",
    headline: "",
    current_location: "",
    street_address: "",
    summary: "",
    linkedin_url: "",
    portfolio_url: "",
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

describe("resume import inference", () => {
  it("fills missing contact fields without overwriting confirmed profile data", () => {
    const profile: CareerProfile = {
      ...emptyProfile(),
      full_name: "Confirmed Name",
      skills: ["Confirmed skill"],
    };
    const result = inferProfileFromResume(profile, {
      name: "resume.pdf",
      text: "Different Name\ndev@example.com\n(212) 555-0199\nSKILLS\nRust, TypeScript",
    });
    expect(result.full_name).toBe("Confirmed Name");
    expect(result.email).toBe("dev@example.com");
    expect(result.phone).toContain("212");
    expect(result.skills).toEqual(["Confirmed skill"]);
    expect(result.source_resume_name).toBe("resume.pdf");
  });

  it("extracts a structured baseline from common resume sections", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "taylor-resume.pdf",
      text: `Taylor Morgan
Austin, TX | taylor@example.com | (512) 555-0184 | https://linkedin.com/in/taylor
Senior Software Engineer

SUMMARY
Product-minded engineer building reliable developer platforms.

EXPERIENCE
Senior Software Engineer
Acme Technologies
Jan 2021 - Present
• Led a release platform used by 40 engineering teams.
• Reduced deployment failures by 35%.
Software Engineer | Northstar Labs
Jun 2018 - Dec 2020
• Built TypeScript services and internal tools.

EDUCATION
University of Texas at Austin
Bachelor of Science in Computer Science
2014 - 2018

SKILLS
TypeScript, Rust, React, PostgreSQL

CERTIFICATIONS
AWS Certified Developer

PROJECTS
Release Guard
• Open-source deployment safety toolkit.
Technologies: Rust, React`,
    });

    expect(result.full_name).toBe("Taylor Morgan");
    expect(result.current_location).toBe("Austin, TX");
    expect(result.headline).toBe("Senior Software Engineer");
    expect(result.linkedin_url).toContain("linkedin.com/in/taylor");
    expect(result.summary).toContain("reliable developer platforms");
    expect(result.employment).toHaveLength(2);
    expect(result.employment[0]).toMatchObject({
      company: "Acme Technologies",
      title: "Senior Software Engineer",
      start_date: "2021-01",
      current: true,
    });
    expect(result.employment[0].highlights).toContain("Reduced deployment failures by 35%.");
    expect(result.education[0]).toMatchObject({
      school: "University of Texas at Austin",
      degree: "Bachelor of Science in Computer Science",
      field: "Computer Science",
      start_date: "2014",
      end_date: "2018",
    });
    expect(result.skills).toEqual(["TypeScript", "Rust", "React", "PostgreSQL"]);
    expect(result.certifications).toEqual(["AWS Certified Developer"]);
    expect(result.projects[0]).toMatchObject({
      name: "Release Guard",
      technologies: ["Rust", "React"],
    });
    expect(summarizeResumeImport(result)).toEqual({
      employment: 2,
      education: 1,
      skills: 4,
      certifications: 1,
      projects: 1,
    });
  });

  it("reconstructs PDF rows instead of flattening the full page", () => {
    const text = pdfTextItemsToText([
      { str: "Taylor Morgan", transform: [1, 0, 0, 1, 40, 720], width: 80 },
      { str: "Austin, TX", transform: [1, 0, 0, 1, 40, 700], width: 55 },
      { str: "taylor@example.com", transform: [1, 0, 0, 1, 120, 700], width: 100 },
      { str: "EXPERIENCE", transform: [1, 0, 0, 1, 40, 660], width: 70 },
    ]);
    expect(text).toBe("Taylor Morgan\nAustin, TX taylor@example.com\nEXPERIENCE");
  });
});
