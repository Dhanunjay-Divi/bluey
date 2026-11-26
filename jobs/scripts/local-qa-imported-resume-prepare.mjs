// Same-candidate imported résumé QA. All I/O is through the owned caller's
// fixed candidate operations; this module never opens a socket or launches a job.
import { randomUUID } from "node:crypto";
import { LOCAL_QA_SAMPLE_RESUME as sample } from "./local-qa-browser-sample-resume.mjs";
import { hasNoQaDispatch } from "./local-qa-durable-draft.mjs";

export const IMPORTED_QA_ROLE = "Data Engineer";
export const IMPORTED_QA_LOCATION = "New York, NY";
export const IMPORTED_QA_EMPLOYER = "Synthetic Qualification Employer";
export const IMPORTED_QA_TRACK = "Synthetic Data Engineer search";
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const SHA = /^[a-f0-9]{64}$/;
const JOB = /^[A-Za-z0-9_-]{1,160}$/;
const matches = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;
const plain = value => value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
const one = (list, value) => Array.isArray(list) && list.length === 1 && list[0] === value;
const empty = list => Array.isArray(list) && list.length === 0;
const record = value => Array.isArray(value) && value.length === 1 && plain(value[0]) ? value[0] : null;
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
function need(value, reason) { if (!value) throw new Error(`local_qa_imported_resume_prepare_refused:${reason}`); }

// The sealed v4 seeder changes the persisted plan. The owned API masks both
// browser capabilities because distribution is off. The private census
// independently proves the unmasked cloud entitlement and zero usage.
function hasNoImportedDispatch(workspace, mockReceipt) {
  if (mockReceipt !== true) return mockReceipt === false && hasNoQaDispatch(workspace);
  const entitlement = workspace?.entitlement;
  return empty(workspace?.browser_sessions) && empty(workspace?.auto_submit_authorizations)
    && entitlement?.plan === "cloud" && entitlement.local_browser === false
    && entitlement.cloud_browser === false && entitlement.used_packets === 0;
}

export function requireImportedQaProfile(profile, assetId) {
  const employment = record(profile?.employment);
  need(plain(profile) && matches(UUID, assetId) && profile.source_resume_asset_id === assetId
    && Object.entries(sample.expectedProfile).every(([key, value]) => profile[key] === value)
    && employment?.company === "Bluey QA Systems" && employment.title === IMPORTED_QA_ROLE && employment.current === true
    && matches(UUID, employment.id) && Array.isArray(profile.skills)
    && ["Python", "SQL"].every(skill => profile.skills.includes(skill)), "current_import");
  return employment;
}

export function createLocalQaImportedResumeMatch({
  readWorkspace, readTaxonomy, saveOnboarding, seedPosting, mockReceipt = false,
}) {
  need(typeof mockReceipt === "boolean"
    && [readWorkspace, readTaxonomy, saveOnboarding, seedPosting].every(fn => typeof fn === "function"), "operations");
  let used = false;
  return async ({ accountId, assetId }) => {
    need(!used && matches(UUID, accountId) && matches(UUID, assetId), "candidate");
    used = true;
    const initial = await readWorkspace();
    const employment = requireImportedQaProfile(initial?.profile, assetId);
    const importedFacts = structuredClone({ employment: initial.profile.employment, skills: initial.profile.skills });
    const identity = record(initial.application_identities);
    need(initial.profile.onboarding_complete === false && empty(initial.matches) && empty(initial.applications)
      && empty(initial.tracks) && hasNoQaDispatch(initial) && matches(UUID, identity?.id)
      && identity.email === sample.candidateEmail && identity.verification_status === "verified"
      && identity.is_default === true, "not_fresh");
    const taxonomy = await readTaxonomy();
    need(matches(/^[A-Za-z0-9._:-]{1,128}$/, taxonomy?.taxonomyVersion)
      && matches(SHA, taxonomy?.taxonomySha256), "taxonomy");
    const trackId = randomUUID();
    const profile = { ...initial.profile, current_location: IMPORTED_QA_LOCATION,
      resume_mode: "factual", review_new_claims: true, default_submission_mode: "review_first",
      daily_limit: 10, onboarding_complete: false };
    const preferences = { desired_roles: [IMPORTED_QA_ROLE], desired_locations: [IMPORTED_QA_LOCATION],
      location_policy: "local", remote_preference: "any", employment_types: ["full_time"],
      engagement_types: ["w2"], daily_limit: 10, apply_once_per_company: true };
    const track = { id: trackId, name: IMPORTED_QA_TRACK, role: IMPORTED_QA_ROLE,
      locations: [IMPORTED_QA_LOCATION], remote_preference: "any", application_identity_id: identity.id,
      active: true, match_count: 0, created_at_ms: 0, updated_at_ms: 0,
      policy: { role_family: "", relevant_employment_ids: [employment.id], employment_types: ["full_time"],
        engagement_types: ["w2"], work_authorizations: [] } };
    await saveOnboarding({ profile, preferences, track }, {
      "x-bluey-jobs-taxonomy-version": taxonomy.taxonomyVersion,
      "x-bluey-jobs-taxonomy-sha256": taxonomy.taxonomySha256,
    });
    const assertSaved = (workspace, cloudFixture) => {
      requireImportedQaProfile(workspace?.profile, assetId);
      const saved = record(workspace.tracks);
      need(workspace.profile.onboarding_complete === true && workspace.profile.resume_mode === "factual"
        && workspace.profile.default_submission_mode === "review_first" && same(workspace.profile.employment, importedFacts.employment)
        && same(workspace.profile.skills, importedFacts.skills) && saved?.id === trackId && saved.role === IMPORTED_QA_ROLE
        && saved.application_identity_id === identity.id && saved.active === true
        && one(saved.policy?.relevant_employment_ids, employment.id) && one(saved.policy?.employment_types, "full_time")
        && one(saved.policy?.engagement_types, "w2") && empty(workspace.applications)
        && hasNoImportedDispatch(workspace, cloudFixture),
      "onboarding_binding");
    };
    const saved = await readWorkspace();
    assertSaved(saved, false);
    need(empty(saved.matches), "preexisting_match");
    const jobId = await seedPosting(Object.freeze({ accountId, trackId, sourceResumeAssetId: assetId,
      sourceResumeSha256: sample.sha256 }));
    const workspace = await readWorkspace();
    assertSaved(workspace, mockReceipt);
    const job = record(workspace.matches);
    need(matches(JOB, jobId) && job?.id === jobId && job.title === IMPORTED_QA_ROLE
      && job.company === IMPORTED_QA_EMPLOYER && job.track_id === trackId && job.eligibility?.can_prepare === true,
    "match_binding");
    return Object.freeze({ jobId });
  };
}

export function requireImportedQaPrepared({ workspace, resume, jobId, assetId, applicationId, resumeId, mockReceipt = false }) {
  requireImportedQaProfile(workspace?.profile, assetId);
  const row = record(workspace.applications);
  const provenance = resume?.content?.provenance;
  const source = provenance?.career_track_policy_authority;
  need(matches(UUID, applicationId) && matches(UUID, resumeId) && matches(JOB, jobId)
    && row?.id === applicationId && row.job_id === jobId && row.resume_version_id === resumeId
    && row.state === "awaiting_review" && row.submission_mode === "review_first"
    && row.run_id === null && row.submitted_at_ms === null && plain(row.receipt)
    && row.receipt.confirmation === null && !Object.hasOwn(row.receipt, "approved_execution")
    && row.receipt.resume_version_id === resumeId && matches(SHA, row.receipt.job_snapshot_fingerprint)
    && resume?.id === resumeId && resume.mode === "factual" && resume.job_id === jobId
    && provenance?.generated_for_job_id === jobId && provenance?.job_snapshot_fingerprint === row.receipt.job_snapshot_fingerprint
    && source?.source_resume_asset_id === assetId && source?.source_resume_sha256 === sample.sha256
    && hasNoImportedDispatch(workspace, mockReceipt), "prepared_binding");
}
