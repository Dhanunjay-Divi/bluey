// Synthetic durable bootstrap only. There is no listener, network client,
// seeder invocation, or production route in this coordination module.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

const email = "candidate@bluey-qa.test";
const role = "Software Engineer";
const location = "New York, NY";
const employer = "Synthetic Qualification Employer";
const standardDailyLimit = 10;
const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const sha = /^[a-f0-9]{64}$/;
const plain = object => object && Object.getPrototypeOf(object) === Object.prototype;
const empty = list => Array.isArray(list) && list.length === 0;
const oneOf = (list, value) => Array.isArray(list) && list.length === 1 && list[0] === value;
const c2cFitChecks = Object.freeze(["employment_type_allowed", "engagement_type_allowed",
  "track_employment_type_allowed", "track_engagement_type_allowed"]);
export const hasNoQaDispatch = workspace =>
  empty(workspace?.browser_sessions) && empty(workspace?.auto_submit_authorizations)
  && workspace.entitlement?.plan === "free" && workspace.entitlement.local_browser === false
  && workspace.entitlement.cloud_browser === false && workspace.entitlement.used_packets === 0;

function matchesSavedProfile(profile, expected) {
  if (profile?.headline !== expected.headline || profile.current_location !== expected.current_location
    || !Array.isArray(profile.skills) || profile.skills.length !== 1 || profile.skills[0] !== expected.skills[0]
    || !Array.isArray(profile.employment) || profile.employment.length !== 1) return false;
  const [actual] = profile.employment, [source] = expected.employment;
  return actual?.id === source.id && actual.company === source.company && actual.title === source.title;
}

function bindPrepared(workspace, application, resume, jobId, c2c) {
  assert(plain(application) && plain(resume) && Array.isArray(workspace?.applications)
    && workspace.applications.length === 1 && hasNoQaDispatch(workspace), "qa_draft_response_shape");
  const readback = workspace.applications[0];
  const fingerprint = application.receipt?.job_snapshot_fingerprint;
  for (const row of [application, readback]) {
    assert(uuid.test(row?.id) && row.id === application.id && row.job_id === jobId
      && row.state === "awaiting_review" && row.submission_mode === "review_first"
      && row.run_id === null && row.submitted_at_ms === null
      && uuid.test(row.resume_version_id) && row.resume_version_id === resume.id
      && row.receipt?.resume_version_id === resume.id && row.receipt?.confirmation === null
      && typeof fingerprint === "string" && sha.test(fingerprint)
      && row.receipt.job_snapshot_fingerprint === fingerprint, "qa_draft_application_binding");
    if (c2c) {
      assert(isQaC2cCandidateReceipt(row.receipt, jobId), "qa_draft_c2c_receipt");
    }
  }
  assert(resume.mode === "factual" && resume.job_id === jobId
    && resume.content?.provenance?.generated_for_job_id === jobId
    && resume.content.provenance.job_snapshot_fingerprint === fingerprint, "qa_draft_resume_binding");
  return Object.freeze({ applicationId: application.id, jobId, resumeId: resume.id,
    jobSnapshotFingerprint: fingerprint, ...(c2c ? { fixtureMode: "c2c" } : {}) });
}

function c2cCandidateOnly(match) {
  const decision = match?.eligibility;
  return match?.employment_type === "contract c2c" && decision?.can_prepare === true
    && decision.can_queue_local === false && decision.can_queue_cloud === false
    && decision.can_auto_submit === false && empty(decision.hard_failures)
    && Array.isArray(decision.passed_checks) && c2cFitChecks.every(check => decision.passed_checks.includes(check))
    && Array.isArray(decision.review_reasons)
    && decision.review_reasons.some(reason => reason?.code === "c2c_submission_requires_candidate");
}

export function isQaC2cCandidateReceipt(receipt, jobId) {
  return plain(receipt) && !Object.hasOwn(receipt, "approved_execution") && receipt.job_snapshot?.id === jobId
    && c2cCandidateOnly({ ...receipt.job_snapshot, eligibility: receipt.eligibility });
}

/**
 * Build one local factual draft through a verified synthetic candidate
 * session. Every operation is already owned by the caller. A failed write is
 * never retried; a partially seeded disposable root must be closed.
 */
export function createLocalQaDurableDraft({
  readWorkspace, readTaxonomy, saveOnboarding, seedPosting, prepareDraft, fixtureMode = "w2",
}) {
  assert([readWorkspace, readTaxonomy, saveOnboarding, seedPosting, prepareDraft].every(
    fn => typeof fn === "function"), "qa_draft_operations");
  assert(fixtureMode === "w2" || fixtureMode === "c2c", "qa_draft_fixture_mode");
  const c2c = fixtureMode === "c2c";
  const employmentType = c2c ? "contract" : "full_time";
  const hasC2cPolicy = value => oneOf(value?.employment_types, "contract") && oneOf(value?.engagement_types, "c2c");
  let used = false;
  return async ({ accountId, candidateEmail }) => {
    assert(!used && typeof accountId === "string" && uuid.test(accountId)
      && candidateEmail === email, "qa_draft_candidate_scope");
    used = true;
    const initial = await readWorkspace();
    assert(plain(initial?.profile) && initial.profile.email === email
      && initial.profile.onboarding_complete === false && empty(initial.applications)
      && empty(initial.matches) && empty(initial.tracks) && hasNoQaDispatch(initial), "qa_draft_candidate_not_fresh");
    const identities = initial.application_identities;
    assert(Array.isArray(identities) && identities.length === 1 && uuid.test(identities[0]?.id)
      && identities[0]?.email === email && identities[0]?.verification_status === "verified"
      && identities[0]?.is_default === true,
    "qa_draft_candidate_identity");
    const taxonomy = await readTaxonomy();
    assert(typeof taxonomy?.taxonomyVersion === "string" && taxonomy.taxonomyVersion.length > 0
      && taxonomy.taxonomyVersion.length <= 128 && typeof taxonomy.taxonomySha256 === "string"
      && sha.test(taxonomy.taxonomySha256), "qa_draft_taxonomy");
    const employmentId = randomUUID(), trackId = randomUUID(), identityId = identities[0].id;
    const profile = { ...initial.profile, full_name: "Synthetic Candidate", email, phone: "555-0100",
      headline: role, current_location: location, skills: ["TypeScript"],
      employment: [{ id: employmentId, company: "Synthetic Systems", title: role,
        location, start_date: "2022-01", end_date: "", current: true, highlights: ["Built synthetic test systems"] }],
      resume_mode: "factual", review_new_claims: true, default_submission_mode: "review_first",
      daily_limit: standardDailyLimit, onboarding_complete: false };
    const preferences = { desired_roles: [role], desired_locations: [location], location_policy: "local",
      remote_preference: "any", employment_types: [employmentType], engagement_types: [fixtureMode],
      daily_limit: standardDailyLimit, apply_once_per_company: true };
    const track = { id: trackId, name: "Synthetic software engineer search", role, locations: [location],
      remote_preference: "any", application_identity_id: identityId, active: true, match_count: 0,
      created_at_ms: 0, updated_at_ms: 0,
      policy: { role_family: "", relevant_employment_ids: [employmentId],
        employment_types: [employmentType], engagement_types: [fixtureMode], work_authorizations: [] } };
    await saveOnboarding({ profile, preferences, track }, {
      "x-bluey-jobs-taxonomy-version": taxonomy.taxonomyVersion,
      "x-bluey-jobs-taxonomy-sha256": taxonomy.taxonomySha256,
    });
    const saved = await readWorkspace();
    assert(saved?.profile?.onboarding_complete === true && saved.profile.email === email
      && matchesSavedProfile(saved.profile, profile)
      && saved.profile.default_submission_mode === "review_first"
      && saved.profile.daily_limit === standardDailyLimit
      && saved.preferences?.daily_limit === standardDailyLimit
      && saved.preferences?.apply_once_per_company === true && Array.isArray(saved.tracks)
      && saved.tracks.length === 1 && saved.tracks[0]?.id === trackId
      && saved.tracks[0]?.role === role && saved.tracks[0]?.active === true
      && saved.tracks[0]?.match_count === 0
      && saved.tracks[0]?.application_identity_id === identityId
      && empty(saved.applications) && empty(saved.matches) && hasNoQaDispatch(saved), "qa_draft_onboarding_readback");
    if (c2c) {
      assert(hasC2cPolicy(saved.preferences) && hasC2cPolicy(saved.tracks[0].policy), "qa_draft_c2c_policy");
    }
    const jobId = await seedPosting(Object.freeze({ accountId, trackId }));
    assert(typeof jobId === "string" && /^[a-zA-Z0-9_-]{1,160}$/.test(jobId), "qa_draft_seed_binding");
    const matched = await readWorkspace();
    assert(matchesSavedProfile(matched?.profile, profile), "qa_draft_profile_drift_before_prepare");
    assert(Array.isArray(matched?.matches) && matched.matches.length === 1
      && matched.matches[0]?.id === jobId && matched.matches[0]?.company === employer
      && matched.matches[0]?.track_id === trackId && matched.matches[0]?.eligibility?.can_prepare === true
      && empty(matched.applications) && hasNoQaDispatch(matched), "qa_draft_verified_match");
    if (c2c) {
      assert(hasC2cPolicy(matched.preferences) && Array.isArray(matched.tracks) && matched.tracks.length === 1
        && matched.tracks[0]?.id === trackId && hasC2cPolicy(matched.tracks[0].policy)
        && c2cCandidateOnly(matched.matches[0]), "qa_draft_c2c_candidate_only");
    }
    const result = await prepareDraft(Object.freeze({ job_id: jobId, mode: "factual", submission_mode: "review_first" }));
    assert(result?.metering === undefined || result.metering === null, "qa_draft_unexpected_metering");
    const persisted = await readWorkspace();
    return bindPrepared(persisted, result?.application, result?.resume_version, jobId, c2c);
  };
}
