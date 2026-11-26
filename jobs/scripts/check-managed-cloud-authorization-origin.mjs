// Validate metadata captured by a trusted caller, not metadata bundled with an artifact.
// Use GitHub's exact run-attempt and exact attempt-jobs endpoints, then list run artifacts.
import { createHash } from "node:crypto";

const WORKFLOW_PATH = ".github/workflows/jobs-managed-cloud-release.yml";
const DECLARED_ENVIRONMENT = "jobs-managed-cloud-release";
const COMPLETED_STEPS = [
  "Check out only the verifier",
  "Read the verified candidate source commit",
  "Check out candidate policy files as data only",
  "Verify the protected Observability policy before handling release signatures",
  "Reverify and authorize with protected trust material",
  "Upload authorized bytes",
];
const MAX_LIST_ITEMS = 100;
const SHA1 = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;

function requireOrigin(condition, reason) {
  if (!condition) throw new Error(reason);
}

function isPositiveInteger(value) {
  return Number.isSafeInteger(value) && value > 0;
}

function isCompleted(value) {
  return value?.status === "completed" && value?.conclusion === "success";
}

function completeList(value, key, label = "authorization") {
  const rows = value?.[key];
  requireOrigin(Array.isArray(rows) && rows.length > 0 && rows.length <= MAX_LIST_ITEMS
    && rows.length === value.total_count, label + " " + key + " response must be complete");
  return rows;
}

function requireCompletedSteps(job, names, label) {
  let previous = 0;
  for (const name of names) {
    const matches = job.steps.filter((step) => step?.name === name);
    const [step] = matches;
    requireOrigin(matches.length === 1 && isCompleted(step) && isPositiveInteger(step.number) &&
      step.number > previous, label + " prerequisite did not finish in order: " + name);
    previous = step.number;
  }
}

function selectAttemptArtifact(artifacts, {
  artifactName, artifactMaxBytes, defaultBranch, headSha, repositoryId, runId, label,
}) {
  const matching = completeList(artifacts, "artifacts", label).filter((row) => row?.name === artifactName);
  requireOrigin(matching.length === 1, "expected exactly one attempt-qualified " + label + " artifact");
  const [artifact] = matching;
  const source = artifact.workflow_run;
  requireOrigin(artifact.expired === false && isPositiveInteger(artifact.id)
    && isPositiveInteger(artifact.size_in_bytes) && artifact.size_in_bytes <= artifactMaxBytes
    && typeof artifact.digest === "string" && DIGEST.test(artifact.digest)
    && source?.id === runId && source.repository_id === repositoryId
    && source.head_repository_id === repositoryId && source.head_branch === defaultBranch
    && source.head_sha === headSha, label + " artifact does not bind its exact source run and digest");
  return {
    artifactDigest: artifact.digest, artifactId: artifact.id, artifactName, artifactSizeBytes: artifact.size_in_bytes,
  };
}

/**
 * The expected source and repository come from the trusted release context.
 * The job attempt comes from the endpoint; if GitHub also supplies a job-row
 * attempt, it must agree. These objects never attest environment settings.
 */
export function selectManagedCloudAuthorizationOrigin({
  artifacts, expected, jobs, run,
}) {
  const {
    artifactMaxBytes, authorizationWorkflowCommit, candidateSourceCommit,
    defaultBranch, repository, repositoryId, runAttempt, runId,
  } = expected;
  requireOrigin(typeof repository === "string" && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)
    && typeof defaultBranch === "string" && /^[A-Za-z0-9][A-Za-z0-9._/-]{0,127}$/.test(defaultBranch)
    && !defaultBranch.includes("..") && !defaultBranch.includes("//") && !defaultBranch.endsWith("/")
    && typeof authorizationWorkflowCommit === "string" && SHA1.test(authorizationWorkflowCommit)
    && typeof candidateSourceCommit === "string" && SHA1.test(candidateSourceCommit)
    && [artifactMaxBytes, repositoryId, runAttempt, runId].every(isPositiveInteger),
  "invalid expected authorization identity");

  requireOrigin(isCompleted(run) && run.id === runId && run.run_attempt === runAttempt
    && run.event === "workflow_dispatch" && run.head_branch === defaultBranch
    && run.head_sha === authorizationWorkflowCommit
    && run.repository?.id === repositoryId && run.repository?.full_name === repository
    && (run.head_repository === undefined || run.head_repository?.id === repositoryId)
    && [WORKFLOW_PATH, WORKFLOW_PATH + "@" + defaultBranch].includes(run.path),
  "authorization run must be the successful exact-source workflow attempt");

  const selectedJobs = completeList(jobs, "jobs").filter((row) => row?.name === "authorize");
  requireOrigin(selectedJobs.length === 1, "expected exactly one authorization job");
  const [job] = selectedJobs;
  requireOrigin(isCompleted(job) && isPositiveInteger(job.id) && job.run_id === runId
    && (job.run_attempt === undefined || job.run_attempt === runAttempt)
    && job.head_sha === authorizationWorkflowCommit
    && (job.head_branch === undefined || job.head_branch === defaultBranch)
    && Array.isArray(job.steps) && job.steps.length <= MAX_LIST_ITEMS,
  "authorization job did not succeed in the exact attempt");
  requireCompletedSteps(job, COMPLETED_STEPS, "authorization");
  const artifactName = "jobs-managed-cloud-authorized-" + runId + "-" + runAttempt;
  return {
    ...selectAttemptArtifact(artifacts, {
      artifactName, artifactMaxBytes, defaultBranch, headSha: authorizationWorkflowCommit,
      repositoryId, runId, label: "authorization",
    }),
    authorizationWorkflowCommit, candidateSourceCommit,
    environmentName: DECLARED_ENVIRONMENT, environmentProtectionStatus: "workflow_declared_settings_not_attested",
    jobId: job.id, repository, repositoryId, runAttempt, runId, workflowPath: WORKFLOW_PATH,
  };
}

export function selectManagedCloudCandidateOrigin({ artifacts, expected, jobs, run }) {
  const { artifactMaxBytes, defaultBranch, repository, repositoryId, runAttempt, runId } = expected;
  requireOrigin(typeof repository === "string" && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)
    && typeof defaultBranch === "string" && /^[A-Za-z0-9][A-Za-z0-9._/-]{0,127}$/.test(defaultBranch)
    && !defaultBranch.includes("..") && !defaultBranch.includes("//") && !defaultBranch.endsWith("/")
    && [artifactMaxBytes, repositoryId, runAttempt, runId].every(isPositiveInteger),
  "invalid expected candidate identity");
  requireOrigin(isCompleted(run) && run.id === runId && run.run_attempt === runAttempt
    && run.event === "workflow_dispatch" && run.head_branch === defaultBranch
    && typeof run.head_sha === "string" && SHA1.test(run.head_sha)
    && run.repository?.id === repositoryId && run.repository?.full_name === repository
    && (run.head_repository === undefined || run.head_repository?.id === repositoryId)
    && [WORKFLOW_PATH, WORKFLOW_PATH + "@" + defaultBranch].includes(run.path),
  "candidate run must be the successful exact default-branch workflow attempt");
  const rows = completeList(jobs, "jobs", "candidate");
  const expectedJobs = {
    candidate: [
      "Bind successful exact-tip CI instead of synthetic candidate evidence",
      "Build each component exactly once",
      "Assemble the deterministic release candidate",
      "Upload the only candidate bytes",
    ],
    verify: ["Fetch candidate bytes from the build job", "Independently inspect the downloaded bytes"],
  };
  const ids = {};
  for (const [name, steps] of Object.entries(expectedJobs)) {
    const matches = rows.filter((job) => job?.name === name);
    const [job] = matches;
    requireOrigin(matches.length === 1 && isCompleted(job) && isPositiveInteger(job.id)
      && job.run_id === runId && (job.run_attempt === undefined || job.run_attempt === runAttempt)
      && job.head_sha === run.head_sha && (job.head_branch === undefined || job.head_branch === defaultBranch)
      && Array.isArray(job.steps) && job.steps.length <= MAX_LIST_ITEMS, "candidate " + name + " job did not succeed");
    requireCompletedSteps(job, steps, "candidate " + name);
    ids[name] = job.id;
  }
  requireOrigin(ids.candidate !== ids.verify, "candidate build and verification must be separate jobs");
  return {
    ...selectAttemptArtifact(artifacts, {
      artifactMaxBytes, artifactName: "jobs-managed-cloud-candidate-" + runId + "-" + runAttempt,
      defaultBranch, headSha: run.head_sha, label: "candidate", repositoryId, runId,
    }),
    candidateSourceCommit: run.head_sha, candidateBuildJobId: ids.candidate, candidateVerifyJobId: ids.verify,
    repository, repositoryId, runAttempt, runId, workflowPath: WORKFLOW_PATH,
  };
}

/**
 * Match the validated receipt from the selected immutable artifact to a separately
 * approved policy. The caller must validate the receipt and download custody;
 * comparing a digest on an arbitrary unpacked receipt is not origin authority.
 */
export function bindManagedCloudAuthorizationPolicy(selection, authorization, approvedPolicySha256) {
  requireOrigin(typeof approvedPolicySha256 === "string" && SHA256.test(approvedPolicySha256)
    && authorization?.version === 2 && authorization.audience === "bluey-jobs-managed-cloud-authorization-v2"
    && typeof selection?.candidateSourceCommit === "string" && SHA1.test(selection.candidateSourceCommit)
    && isPositiveInteger(selection.runId) && isPositiveInteger(selection.runAttempt)
    && authorization.authorizationId === "authorization-" + selection.runId + "-" + selection.runAttempt
    && authorization.sourceCommit === selection.candidateSourceCommit
    && authorization.observabilityPolicySha256 === approvedPolicySha256,
  "authorization receipt does not bind the protected approved policy and source");
  return { ...selection, observabilityPolicySha256: approvedPolicySha256 };
}

/** Check original archive bytes before any extraction. Does not write or unpack them. */
export async function verifyManagedCloudAuthorizationArchive(selection, input) {
  const { artifactDigest, artifactSizeBytes } = selection;
  requireOrigin(typeof artifactDigest === "string" && DIGEST.test(artifactDigest)
    && isPositiveInteger(artifactSizeBytes), "invalid selected authorization archive identity");
  const hash = createHash("sha256");
  let bytes = 0;
  for await (const chunk of input) {
    requireOrigin(Buffer.isBuffer(chunk) && chunk.length <= artifactSizeBytes - bytes,
      "authorization archive exceeds the selected byte count");
    hash.update(chunk);
    bytes += chunk.length;
  }
  const actualDigest = "sha256:" + hash.digest("hex");
  requireOrigin(bytes === artifactSizeBytes && actualDigest === artifactDigest,
    "authorization archive does not match the selected size and digest");
  return { artifactDigest: actualDigest, artifactSizeBytes: bytes };
}
