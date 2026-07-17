import type { JobPosting, JobsWorkspace, ResumeVersion, UserJobInput } from "../types";

export function previewPosting(input: UserJobInput): JobPosting {
  const now = Date.now();
  const url = input.canonical_url.toLowerCase();
  const capability = url.includes("linkedin.com") || url.includes("indeed.com")
    ? "handoff"
    : ["greenhouse.io", "lever.co", "ashbyhq.com", "smartrecruiters.com", "workday.com", "myworkdayjobs.com"].some((host) => url.includes(host))
      ? "beta_review"
      : "unknown_review";
  return {
    id: `job-${now}`,
    canonical_key: `preview-${now}`,
    source: "pasted_link",
    external_id: "",
    company: input.company,
    title: input.title,
    location: input.location || "",
    workplace: input.workplace || "Unknown",
    canonical_url: input.canonical_url,
    description: input.pasted_description || "",
    compensation: input.compensation || "",
    track_id: input.track_id,
    match_score: 84,
    matched_reasons: ["Matches your active Career Track"],
    missing_requirements: [],
    availability_status: "unknown",
    status: "matched",
    created_at_ms: now,
    updated_at_ms: now,
    eligibility: {
      capability,
      can_prepare: true,
      can_auto_submit: false,
      can_queue_local: false,
      can_queue_cloud: false,
      hard_failures: [],
      review_reasons: [
        { code: "availability_unverified", message: "Bluey has not verified that this pasted job is still accepting applications." },
        { code: "live_verification_required", message: "Bluey must confirm this job is still open before a runner starts." },
      ],
      passed_checks: [],
      evaluated_at_ms: now,
    },
  };
}

export function previewResume(
  workspace: JobsWorkspace,
  job: JobPosting,
  id: string,
  mode: string,
  accountEmail: string,
): ResumeVersion {
  const track = workspace.tracks.find((item) => item.id === job.track_id);
  const applicationIdentity = workspace.application_identities.find((item) => item.id === track?.application_identity_id)
    || workspace.application_identities.find((item) => item.is_default && item.verification_status === "verified");
  return {
    id,
    job_id: job.id,
    version_no: workspace.applications.filter((application) => application.job_id === job.id).length + 1,
    mode: mode === "enhance" ? "enhance" : "factual",
    content: {
      target: { company: job.company, title: job.title, location: job.location },
      contact: {
        name: workspace.profile.full_name,
        email: applicationIdentity?.email || accountEmail,
        phone: workspace.profile.phone,
        location: workspace.profile.current_location,
      },
      headline: workspace.profile.headline || job.title,
      summary: `${workspace.profile.summary} Focused for the ${job.title} opportunity at ${job.company}.`,
      skills: workspace.profile.skills,
      employment: workspace.profile.employment,
      education: workspace.profile.education,
      projects: workspace.profile.projects,
      certifications: workspace.profile.certifications,
    },
    diff: {
      summary: {
        before: workspace.profile.summary,
        after: `${workspace.profile.summary} Focused for the ${job.title} opportunity at ${job.company}.`,
      },
      claims_added: [],
      profile_fact_ids_used: workspace.facts.filter((fact) => fact.verification_status === "confirmed").map((fact) => fact.id),
    },
    claim_ids: workspace.facts.filter((fact) => fact.verification_status === "confirmed").map((fact) => fact.id),
    checksum: `${job.id}-${id}`,
    created_at_ms: Date.now(),
  };
}
