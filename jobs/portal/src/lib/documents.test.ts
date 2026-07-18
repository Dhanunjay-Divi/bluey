import { describe, expect, it } from "vitest";
import {
  applyResumeImport,
  inferProfileFromResume,
  pdfTextItemsToText,
  prepareResumeImport,
  resumeHtmlToText,
  summarizeResumeImport,
  validateExtractedResumeText,
  validateResumeFileBytes,
} from "./documents";
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

  it("separates company-first employment headers and preserves US locations", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "software-resume.pdf",
      text: `Taylor Example
Arlington, VA | taylor@example.com
EXPERIENCE
Capital One, Software Engineer
Richmond, VA
January 2021 - Present
• Built resilient payment services.
Infosys, Software Engineer
Indianapolis, IN
June 2018 - December 2020
• Delivered customer-facing systems.`,
    });

    expect(result.employment[0]).toMatchObject({
      company: "Capital One",
      title: "Software Engineer",
      location: "Richmond, VA",
      current: true,
    });
    expect(result.employment[1]).toMatchObject({
      company: "Infosys",
      title: "Software Engineer",
      location: "Indianapolis, IN",
    });
  });

  it("separates international locations and joins wrapped certification levels", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "clinical-resume.docx",
      text: `Jordan Example
Falls Church, VA | jordan@example.com
EXPERIENCE
Director of Clinical Operations, Hyderabad, India
Apollo Hospitals
January 2020 - October 2020
• Improved clinical documentation workflows.
CERTIFICATIONS
AWS Certified Solutions Architect
Professional
AWS Certified Machine Learning
Specialty`,
    });

    expect(result.employment[0]).toMatchObject({
      company: "Apollo Hospitals",
      title: "Director of Clinical Operations",
      location: "Hyderabad, India",
    });
    expect(result.certifications).toEqual([
      "AWS Certified Solutions Architect, Professional",
      "AWS Certified Machine Learning, Specialty",
    ]);
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

  it("preserves DOCX table cells, bullets, and manual line breaks", () => {
    expect(resumeHtmlToText(`
      <p><strong>CORE COMPETENCIES</strong></p>
      <table><tr><td><p>Clinical Research</p></td><td><p>REDCap</p></td></tr></table>
      <p><strong>Master of Science – Health Informatics<br />Example University, December 2023</strong></p>
      <ul><li>Verified source documentation.</li></ul>
    `)).toBe([
      "CORE COMPETENCIES",
      "Clinical Research",
      "REDCap",
      "Master of Science – Health Informatics",
      "Example University, December 2023",
      "• Verified source documentation.",
    ].join("\n"));
  });

  it("drops DOCX table category headers without dropping skill values", () => {
    expect(resumeHtmlToText(`
      <p><strong>CORE COMPETENCIES</strong></p>
      <table>
        <thead><tr><th>Clinical Research</th><th>Regulatory &amp; Compliance</th></tr></thead>
        <tbody><tr><td>Clinical Trial Operations</td><td>Good Clinical Practice</td></tr></tbody>
      </table>
    `)).toBe([
      "CORE COMPETENCIES",
      "Clinical Trial Operations",
      "Good Clinical Practice",
    ].join("\n"));
  });

  it("keeps contact details when a DOCX template places them in a table header", () => {
    expect(resumeHtmlToText(`
      <table>
        <thead><tr><th>Morgan Reed</th><th>morgan@example.com</th><th>https://linkedin.com/in/morgan</th></tr></thead>
        <tbody><tr><td>Clinical Research Analyst</td><td>Indianapolis, IN</td></tr></tbody>
      </table>
    `)).toBe([
      "Morgan Reed",
      "morgan@example.com",
      "https://linkedin.com/in/morgan",
      "Clinical Research Analyst",
      "Indianapolis, IN",
    ].join("\n"));
  });

  it("keeps internal experience headings with the bullets they introduce", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "clinical.docx",
      text: `Morgan Reed
PROFESSIONAL EXPERIENCE
Example Health System
Clinical Research Analyst, Falls Church, VA February 2024 – Present
• Coordinated research operations.
Neuro-Oncology Research Project – Technical Lead
• Built a longitudinal research dataset.
• Improved source verification.
Health Information Management
• Organized audit-ready documentation.
EDUCATION
Example University
Master of Science in Health Informatics
2023`,
    });

    expect(result.employment[0].highlights).toEqual([
      "Coordinated research operations.",
      "Neuro-Oncology Research Project – Technical Lead: Built a longitudinal research dataset.",
      "Improved source verification.",
      "Health Information Management: Organized audit-ready documentation.",
    ]);
  });

  it("keeps parenthetical experience subheadings separate and uses the current role location", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "clinical.docx",
      text: `Morgan Reed
PROFESSIONAL EXPERIENCE
Example Health System
Clinical Research Analyst, Falls Church, VA February 2024 – Present
• Coordinated research operations.
Health Information Management (HIM)
• Organized audit-ready documentation.`,
    });

    expect(result.current_location).toBe("Falls Church, VA");
    expect(result.employment[0].highlights).toEqual([
      "Coordinated research operations.",
      "Health Information Management (HIM): Organized audit-ready documentation.",
    ]);
  });

  it("separates company, title, and US or international location rows", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "clinical-resume.docx",
      text: `Morgan Reed
morgan@example.com | (703) 555-0148
PROFESSIONAL EXPERIENCE
Example Health System
PAA2, Falls Church, VA February 2024 – Present
• Supported clinical research operations and verified study records.
Example University School of Medicine
Clinical Research Coordinator, Indianapolis, IN January 2022 – December 2023
• Coordinated clinical research activities across multidisciplinary teams.
Meridian Hospitals
Director of Clinical Operations, Hyderabad, India January 2020 – October 2020
• Improved clinical documentation quality and operational reporting.
Harbor Dental Hospital
Director of Clinical Operations, Visakhapatnam, India, January 2019 – December 2019
• Coordinated patient care activities and documentation processes.
EDUCATION
Master of Science – Health Informatics
Example University, December 2023
CERTIFICATIONS
Epic Ambulatory Certified
TECHNICAL SKILLS
Clinical Systems: Epic EMR • REDCap
PROFESSIONAL AFFILIATIONS
Example Clinical Association - Peer Reviewer`,
    });

    expect(result.employment).toHaveLength(4);
    expect(result.employment[0]).toMatchObject({
      company: "Example Health System",
      title: "PAA2",
      location: "Falls Church, VA",
      start_date: "2024-02",
      current: true,
    });
    expect(result.employment[1]).toMatchObject({
      company: "Example University School of Medicine",
      title: "Clinical Research Coordinator",
      location: "Indianapolis, IN",
    });
    expect(result.employment[2]).toMatchObject({
      company: "Meridian Hospitals",
      title: "Director of Clinical Operations",
      location: "Hyderabad, India",
    });
    expect(result.employment[3]).toMatchObject({
      company: "Harbor Dental Hospital",
      title: "Director of Clinical Operations",
      location: "Visakhapatnam, India",
    });
    expect(result.education[0]).toMatchObject({
      school: "Example University",
      degree: "Master of Science – Health Informatics",
      field: "Health Informatics",
      end_date: "2023",
    });
    expect(result.skills).toEqual(["Epic EMR", "REDCap"]);
  });

  it("parses compact PDF rows with inline dates, grouped skills, and combined role-company headings", () => {
    const result = inferProfileFromResume(emptyProfile(), {
      name: "fictional-ai-resume.pdf",
      text: `Casey Morgan
Columbus, OH — casey@example.com — (614) 555-0135
linkedin.com/in/casey-morgan — github.com/casey-morgan
Summary
Early-career AI engineer building reliable document and retrieval systems.
Education
Northern University, School of Computing Aug 2022 – May 2024
M.S. in Computer Science
State Institute of Technology Jun 2018 – May 2022
B.Tech in Mechanical Engineering
Experience
Software Developer, AI, Meridian Insurance Feb 2024 – Present
• Built a document pipeline that reduced review time by 30% across customer operations.
AI Engineer, Northstar Communications Aug 2023 – Feb 2024
• Deployed retrieval systems across 100+ internal teams.
Research Intern, City Research Library May 2022 – Aug 2022
• Improved text recognition accu-
racy by 17% across historical documents.
Projects
Application Intelligence Platform
Built a multi-agent research platform for structured decision support.
Tech: Python, React, PostgreSQL
Skills
Languages: Python, Java, TypeScript
AI/ML: RAG, PyTorch, OCR
Backend: FastAPI, Kafka, PostgreSQL
Cloud/DevOps: AWS, Docker, Kubernetes`,
    });

    expect(result.current_location).toBe("Columbus, OH");
    expect(result.portfolio_url).toBe("https://github.com/casey-morgan");
    expect(result.headline).toBe("Software Developer, AI");
    expect(result.employment).toHaveLength(3);
    expect(result.employment[0]).toMatchObject({
      company: "Meridian Insurance",
      title: "Software Developer, AI",
      start_date: "2024-02",
      current: true,
    });
    expect(result.employment[1]).toMatchObject({
      company: "Northstar Communications",
      title: "AI Engineer",
    });
    expect(result.employment[2]).toMatchObject({
      company: "City Research Library",
      title: "Research Intern",
    });
    expect(result.employment[2].highlights).toEqual([
      "Improved text recognition accuracy by 17% across historical documents.",
    ]);
    expect(result.education).toEqual([
      expect.objectContaining({
        school: "Northern University, School of Computing",
        degree: "M.S. in Computer Science",
        start_date: "2022-08",
        end_date: "2024-05",
      }),
      expect.objectContaining({
        school: "State Institute of Technology",
        degree: "B.Tech in Mechanical Engineering",
        start_date: "2018-06",
        end_date: "2022-05",
      }),
    ]);
    expect(result.projects[0]).toMatchObject({
      name: "Application Intelligence Platform",
      summary: "Built a multi-agent research platform for structured decision support.",
      technologies: ["Python", "React", "PostgreSQL"],
    });
    expect(result.skills).toEqual([
      "Python",
      "Java",
      "TypeScript",
      "RAG",
      "PyTorch",
      "OCR",
      "FastAPI",
      "Kafka",
      "PostgreSQL",
      "AWS",
      "Docker",
      "Kubernetes",
    ]);
  });

  it("stages replacement separately from an explicit fill-blanks merge", () => {
    const first = prepareResumeImport(emptyProfile(), {
      name: "alex.pdf",
      text: `Alex Morgan
alex@example.com
EXPERIENCE
Software Engineer
Northstar Labs
January 2022 - Present
• Built reliable services.
SKILLS
Rust, TypeScript`,
    }).replacement;
    const second = prepareResumeImport(first, {
      name: "taylor.pdf",
      text: `Taylor Reed
taylor@example.com
EXPERIENCE
Clinical Research Analyst
Example Health System
January 2023 - Present
• Coordinated clinical studies.
SKILLS
REDCap, Epic EMR`,
    });

    expect(second.likely_different_person).toBe(true);
    expect(applyResumeImport(second, "replace")).toMatchObject({
      full_name: "Taylor Reed",
      email: "taylor@example.com",
      skills: ["REDCap", "Epic EMR"],
    });
    expect(applyResumeImport(second, "replace").employment[0].company).toBe("Example Health System");
    expect(() => applyResumeImport(second, "merge")).toThrow("current Career Profile");
  });

  it("clears candidate-specific application facts when replacing a different person", () => {
    const current = {
      ...emptyProfile(),
      full_name: "Alex Morgan",
      email: "alex@example.com",
      street_address: "100 Old Street",
      work_authorization: "US citizen",
      sponsorship_required: false,
      salary_expectation: "$170,000",
      notice_period: "Two weeks",
      reusable_answers: { authorization: "I am authorized." },
      resume_mode: "enhance" as const,
      default_submission_mode: "auto_submit" as const,
    };
    const preview = prepareResumeImport(current, {
      name: "taylor.pdf",
      text: `Taylor Reed
(212) 555-0199
EXPERIENCE
Clinical Research Analyst
Example Health System
January 2023 - Present
Improved participant screening by 20%`,
    });

    expect(preview.likely_different_person).toBe(true);
    expect(preview.replacement).toMatchObject({
      full_name: "Taylor Reed",
      email: "",
      street_address: "",
      work_authorization: "",
      sponsorship_required: null,
      salary_expectation: "",
      notice_period: "",
      reusable_answers: {},
      resume_mode: "enhance",
      default_submission_mode: "auto_submit",
    });
    expect(preview.replacement.employment[0].highlights).toContain("Improved participant screening by 20%");
  });

  it("fills blanks while adding new same-person history without duplicating confirmed facts", () => {
    const current = prepareResumeImport(emptyProfile(), {
      name: "alex-v1.pdf",
      text: `Alex Morgan
alex@example.com
EXPERIENCE
Software Engineer
Northstar Labs
January 2022 - Present
• Built reliable services.
SKILLS
Rust, TypeScript
CERTIFICATIONS
AWS Developer`,
    }).replacement;
    const preview = prepareResumeImport(current, {
      name: "alex-v2.pdf",
      text: `Alex Morgan
alex@example.com
EXPERIENCE
Software Engineer
Northstar Labs
January 2022 - Present
• Built reliable services.
• Reduced deployment failures by 30%.
Engineering Lead
Atlas Systems
February 2025 - Present
• Led platform delivery.
SKILLS
Rust, TypeScript, PostgreSQL
CERTIFICATIONS
AWS Developer, CKA
PROJECTS
Release Guard
• Deployment safety toolkit.
Technologies: Rust, PostgreSQL`,
    });
    const merged = applyResumeImport(preview, "merge");

    expect(preview.likely_different_person).toBe(false);
    expect(merged.employment).toHaveLength(2);
    expect(merged.employment[0].highlights).toEqual([
      "Built reliable services.",
      "Reduced deployment failures by 30%.",
    ]);
    expect(merged.skills).toEqual(["Rust", "TypeScript", "PostgreSQL"]);
    expect(merged.certifications).toEqual(["AWS Developer", "CKA"]);
    expect(merged.projects[0]).toMatchObject({ name: "Release Guard" });
    expect(merged.source_resume_name).toBe("alex-v2.pdf");
  });

  it("rejects renamed files and resumes without readable text", () => {
    const invalidPdf = new TextEncoder().encode("not a pdf");
    expect(() => validateResumeFileBytes("resume.pdf", exactBuffer(invalidPdf))).toThrow("not a valid PDF");

    const validPdf = new TextEncoder().encode("%PDF-1.7");
    expect(() => validateResumeFileBytes("resume.pdf", exactBuffer(validPdf))).not.toThrow();
    expect(() => validateExtractedResumeText("  ", "PDF")).toThrow("image-only or scanned PDF");
  });
});

function exactBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}
