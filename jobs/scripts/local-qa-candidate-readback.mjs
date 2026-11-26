// Candidate-only admission shared by the two isolated QA platform owners.
import { hasNoQaDispatch, isQaC2cCandidateReceipt } from "./local-qa-durable-draft.mjs";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const need = value => { if (!value) throw new Error("local_qa_session_refused"); };

export function requireLocalQaExpectedDraft(draft) {
  need(draft && Object.getPrototypeOf(draft) === Object.prototype);
  const keys = Object.keys(draft).sort().join(",");
  need((keys === "applicationId,jobId,jobSnapshotFingerprint,resumeId"
    || keys === "applicationId,fixtureMode,jobId,jobSnapshotFingerprint,resumeId" && draft.fixtureMode === "c2c")
    && typeof draft.applicationId === "string" && UUID.test(draft.applicationId)
    && typeof draft.resumeId === "string" && UUID.test(draft.resumeId)
    && typeof draft.jobId === "string" && /^[a-zA-Z0-9_-]{1,160}$/.test(draft.jobId)
    && typeof draft.jobSnapshotFingerprint === "string" && /^[0-9a-f]{64}$/.test(draft.jobSnapshotFingerprint));
  return Object.freeze({ ...draft });
}

export function assertLocalQaExpectedDraft(workspace, expected) {
  const binding = requireLocalQaExpectedDraft(expected);
  need(Array.isArray(workspace?.applications) && workspace.applications.length === 1);
  const [draft] = workspace.applications;
  need(draft?.id === binding.applicationId && draft.job_id === binding.jobId
    && draft.resume_version_id === binding.resumeId
    && draft.receipt?.job_snapshot_fingerprint === binding.jobSnapshotFingerprint);
  if (binding.fixtureMode === "c2c") need(draft.state === "awaiting_review" && draft.submission_mode === "review_first"
    && draft.run_id === null && draft.submitted_at_ms === null && draft.receipt.confirmation === null
    && draft.receipt.resume_version_id === binding.resumeId && hasNoQaDispatch(workspace)
    && isQaC2cCandidateReceipt(draft.receipt, binding.jobId));
}

// Only a fresh authenticated workspace can select a resume or match route.
export function bindLocalQaCandidateReadback(accountId, workspace, expectedDraft) {
  need(typeof accountId === "string" && UUID.test(accountId) && Array.isArray(workspace?.applications)
    && workspace.matches_page?.next_cursor === null);
  const { applications } = workspace;
  need(applications.length <= 1);
  if (expectedDraft !== undefined) assertLocalQaExpectedDraft(workspace, expectedDraft);
  if (!applications.length) return Object.freeze({ accountId, resumeId: null });
  const [draft] = applications, resumeId = draft?.resume_version_id;
  need(draft?.state === "awaiting_review" && draft.submission_mode === "review_first"
    && draft.run_id === null && draft.submitted_at_ms === null && typeof resumeId === "string" && UUID.test(resumeId)
    && draft.receipt?.resume_version_id === resumeId && draft.receipt.confirmation === null);
  if (expectedDraft?.fixtureMode === "c2c") {
    const matchId = draft.job_id;
    need(typeof matchId === "string" && UUID.test(matchId) && Array.isArray(workspace.matches)
      && workspace.matches.length === 1 && workspace.matches[0]?.id === matchId
      && workspace.matches[0]?.employment_type === "contract c2c");
    return Object.freeze({ accountId, resumeId, c2cMatchId: matchId, c2cApplicationId: draft.id });
  }
  return Object.freeze({ accountId, resumeId });
}

export function assertLocalQaCandidateResume(workspace, resume) {
  const [draft] = workspace.applications;
  const fingerprint = draft?.receipt?.job_snapshot_fingerprint;
  need(typeof draft?.job_id === "string" && draft.job_id.length > 0
    && typeof fingerprint === "string" && /^[0-9a-f]{64}$/.test(fingerprint)
    && resume?.id === draft.resume_version_id && resume.mode === "factual" && resume.job_id === draft.job_id
    && resume.content?.provenance?.generated_for_job_id === draft.job_id
    && resume.content.provenance.job_snapshot_fingerprint === fingerprint);
}
