import { useRef, useState } from "react";
import { AlertCircle, CheckCircle2, FileCheck2, LoaderCircle, Sparkles } from "lucide-react";
import { InterviewPrepBalanceRequired, jobsApi, type InterviewPrepCompletionResponse } from "../api";
import {
  assertInterviewPrepDownloadScope,
  downloadInterviewPrepDraft,
  type InterviewPrepDownloadFormat,
  type InterviewPrepDownloadScope,
} from "../lib/interview-prep-download";
import type { JobApplication, JobsWorkspace, ResumeVersion } from "../types";
import { Dialog } from "./Dialog";

export interface InterviewPrepTarget {
  application: JobApplication;
  resume: ResumeVersion;
}

interface Props {
  target: InterviewPrepTarget | null;
  workspace: JobsWorkspace;
  currentResume: ResumeVersion | undefined;
  onClose(): void;
}

interface SubmittedApplicationSnapshot extends InterviewPrepDownloadScope {
  receiptFingerprint: string;
  company: string;
  jobTitle: string;
  submittedClaimCount: number;
  finalAnswerCount: number;
  evidenceCount: number;
}

export function InterviewPrepDialog({ target, workspace, currentResume, onClose }: Props) {
  const inFlight = useRef(new Map<string, Promise<InterviewPrepCompletionResponse>>());
  if (!target) return null;
  const original = target.application;
  const current = workspace.applications.find((item) => item.id === original.id);
  if (!current || current.state !== original.state || current.job_id !== original.job_id
    || current.resume_version_id !== original.resume_version_id
    || (current.receipt !== original.receipt && JSON.stringify(current.receipt) !== JSON.stringify(original.receipt))
    || !currentResume || currentResume.id !== target.resume.id || currentResume.job_id !== target.resume.job_id
    || currentResume.checksum !== target.resume.checksum) {
    return (
      <Dialog open title="Interview prep changed" onClose={onClose} size="large">
        <div className="prep-error" role="alert">
          <AlertCircle size={18} />
          <p>The submitted application or résumé changed or is no longer available. Close this dialog and open Interview Prep again.</p>
        </div>
      </Dialog>
    );
  }
  const scopeKey = interviewPrepScopeKey(target, workspace);

  const prepare = (
    selected: InterviewPrepTarget,
    snapshot: SubmittedApplicationSnapshot,
  ): Promise<InterviewPrepCompletionResponse> => {
    const existing = inFlight.current.get(scopeKey);
    if (existing) return existing;

    const request = new URLSearchParams(window.location.search).get("preview") === "1"
      ? Promise.resolve(previewResponse(snapshot))
      : jobsApi.prepareInterview(selected.application.id, true, snapshot.receiptFingerprint);
    let tracked: Promise<InterviewPrepCompletionResponse>;
    tracked = request.finally(() => {
      if (inFlight.current.get(scopeKey) === tracked) inFlight.current.delete(scopeKey);
    });
    inFlight.current.set(scopeKey, tracked);
    return tracked;
  };

  return (
    <InterviewPrepSession
      key={scopeKey}
      target={target}
      workspace={workspace}
      onClose={onClose}
      prepare={prepare}
    />
  );
}

function InterviewPrepSession({
  target,
  workspace,
  onClose,
  prepare,
}: Pick<Props, "workspace" | "onClose"> & {
  target: InterviewPrepTarget;
  prepare(
    selected: InterviewPrepTarget,
    snapshot: SubmittedApplicationSnapshot,
  ): Promise<InterviewPrepCompletionResponse>;
}) {
  const [response, setResponse] = useState<InterviewPrepCompletionResponse | null>(null);
  const [error, setError] = useState("");
  const [state, setState] = useState<"idle" | "generating" | "requested">("idle");
  const [consented, setConsented] = useState(false);
  const [factsChecked, setFactsChecked] = useState(false);
  const [reasoningChecked, setReasoningChecked] = useState(false);
  const [retryAfterFunding, setRetryAfterFunding] = useState(false);
  const generationStarted = useRef(false);
  let snapshot: SubmittedApplicationSnapshot | null = null;
  let snapshotError = "";
  try {
    snapshot = submittedApplicationSnapshot(target, workspace);
  } catch (cause) {
    snapshotError = cause instanceof Error ? cause.message : "Bluey could not verify this application packet.";
  }
  const busy = state === "generating";

  const generate = async () => {
    if (!snapshot || !consented || generationStarted.current) return;
    generationStarted.current = true;
    setState("generating");
    setError("");
    setRetryAfterFunding(false);
    try {
      const completion = await prepare(target, snapshot);
      assertInterviewPrepDownloadScope(completion, snapshot);
      setResponse(completion);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not generate the coaching brief.");
      if (cause instanceof InterviewPrepBalanceRequired) {
        setConsented(false);
        setRetryAfterFunding(true);
        generationStarted.current = false;
        setState("idle");
        return;
      }
    }
    setState("requested");
  };

  const downloadDraft = (format: InterviewPrepDownloadFormat) => {
    if (!response || !snapshot) return;
    try {
      downloadInterviewPrepDraft(
        response,
        snapshot,
        factsChecked && reasoningChecked ? { factsChecked, reasoningChecked } : undefined,
        format,
      );
    } catch {
      setError("Bluey could not download this interview preparation draft.");
    }
  };

  return (
    <Dialog
      open
      title={snapshot ? `Interview prep: ${snapshot.company}` : "Interview prep"}
      description={snapshot ? `${snapshot.jobTitle} · Submitted resume v${target.resume.version_no}` : undefined}
      onClose={onClose}
      size="large"
    >
        <div className="interview-prep">
          {snapshot && (
            <>
              <header className="prep-summary">
                <div><FileCheck2 size={19} /><span><b>Submitted application selected</b><small>Receipt {snapshot.receiptId} · Resume {snapshot.resumeVersionId}</small></span></div>
                <div><CheckCircle2 size={19} /><span><b>{snapshot.submittedClaimCount} submitted claim{snapshot.submittedClaimCount === 1 ? "" : "s"}</b><small>Only facts tied to the submitted resume are used.</small></span></div>
                <div><CheckCircle2 size={19} /><span><b>{snapshot.finalAnswerCount} final answer{snapshot.finalAnswerCount === 1 ? "" : "s"}</b><small>{snapshot.evidenceCount} evidence record{snapshot.evidenceCount === 1 ? "" : "s"} attached.</small></span></div>
              </header>

              <section className="prep-plan">
                <div className="prep-section-heading"><p>GROUNDING</p><h3>Verify the submitted application before preparing</h3></div>
                <p>When you select Generate, Bluey checks the locked receipt, exact resume version, final answers, and stored submission evidence before preparing your coaching brief.</p>
              </section>

              {!response && (
                <div className="prep-generate">
                  <div><Sparkles size={18} /><span><b>Build my coaching brief</b><small>May use your balance when you select Generate.</small></span></div>
                  <label className="prep-consent">
                    <input
                      type="checkbox"
                      checked={consented}
                      disabled={state !== "idle"}
                      onChange={(event) => setConsented(event.target.checked)}
                    />
                    <span>Prepare for this interview using my submitted application and résumé.</span>
                  </label>
                  <button
                    className="button primary"
                    disabled={state !== "idle" || !consented}
                    onClick={() => void generate()}
                  >
                    {busy ? <LoaderCircle className="spin" size={16} /> : <Sparkles size={16} />}
                    {busy
                      ? "Preparing..."
                      : state === "requested" ? "Generation requested"
                        : retryAfterFunding ? "Retry with Bluey" : "Generate with Bluey"}
                  </button>
                </div>
              )}

              {response && (
                <section className="prep-brief">
                  <div className="prep-section-heading"><p>BLUEY COACHING BRIEF</p><h3>Draft for review</h3><span>{response.cost_cents > 0 ? `${response.cost_cents} cents used` : "Included in your current access"}</span></div>
                  <article>{response.content}</article>
                  <fieldset className="prep-review">
                    <legend>Check this draft before rehearsal</legend>
                    <p>Only check a box after reviewing this version. Bluey does not verify these checks for you.</p>
                    <label className="prep-consent">
                      <input
                        type="checkbox"
                        checked={factsChecked}
                        onChange={(event) => setFactsChecked(event.target.checked)}
                      />
                      <span>I checked that personal claims match my experience and that assumptions are labeled.</span>
                    </label>
                    <label className="prep-consent">
                      <input
                        type="checkbox"
                        checked={reasoningChecked}
                        onChange={(event) => setReasoningChecked(event.target.checked)}
                      />
                      <span>I can explain the decisions, tradeoffs, and follow-up answers in my own words.</span>
                    </label>
                  </fieldset>
                  <button className="button secondary" type="button" onClick={() => downloadDraft("text")}>
                    Download practice draft (.txt)
                  </button>
                  <button className="button secondary" type="button" onClick={() => downloadDraft("json")}>
                    Download structured draft (.json)
                  </button>
                </section>
              )}
            </>
          )}

          {(snapshotError || error) && <div className="prep-error" role="alert"><AlertCircle size={18} /><span><b>Prep is not ready</b><p>{snapshotError || error}</p></span></div>}
          <footer className="prep-privacy"><CheckCircle2 size={14} />Contact details, demographic answers, and calendar attendees are excluded from coaching context.</footer>
        </div>
    </Dialog>
  );
}

function interviewPrepScopeKey(target: InterviewPrepTarget, workspace: JobsWorkspace): string {
  const evidence = workspace.application_evidence
    .filter((item) => item.application_id === target.application.id)
    .map((item) => {
      if (item.kind === "interview_event" || item.kind === "status_email") {
        return [item.id, item.kind, item.label, item.provider, item.occurred_at_ms];
      }
      if (item.kind === "resume") {
        return [
          item.id,
          item.kind,
          item.resume_version_id ?? null,
          item.sha256,
          item.storage_key,
        ];
      }
      if (item.kind === "submission_confirmation") {
        return [item.id, item.kind];
      }
      return [item.id, item.kind];
    })
    .sort((left, right) => JSON.stringify(left).localeCompare(JSON.stringify(right)));
  return JSON.stringify({
    application_id: target.application.id,
    receipt: target.application.receipt,
    receipt_fingerprint: workspace.interview_prep_receipts?.[target.application.id],
    resume_id: target.resume.id,
    resume_checksum: target.resume.checksum,
    evidence,
  });
}

function submittedApplicationSnapshot(
  target: InterviewPrepTarget,
  workspace: JobsWorkspace,
): SubmittedApplicationSnapshot {
  if (target.application.state !== "submitted") {
    throw new Error("Interview preparation starts after a confirmed submission.");
  }
  if (target.application.job_id !== target.resume.job_id || target.application.resume_version_id !== target.resume.id) {
    throw new Error("The submitted job and resume do not match this application.");
  }
  const receipt = target.application.receipt;
  const packet = objectValue(receipt.packet);
  const result = objectValue(receipt.result);
  if ((receipt.schemaVersion !== 1 && receipt.schemaVersion !== 2)
    || (receipt.schemaVersion === 2
      && objectValue(receipt.receiptObject)?.schemaVersion !== 2)
    || typeof receipt.receiptId !== "string"
    || typeof receipt.applicationId !== "string"
    || receipt.applicationId !== target.application.id
    || packet?.jobId !== target.application.job_id
    || packet?.resumeVersionId !== target.resume.id
    || result?.status !== "submitted") {
    throw new Error("The final submission receipt is incomplete.");
  }
  const receiptFingerprint = workspace.interview_prep_receipts?.[target.application.id];
  if (typeof receiptFingerprint !== "string" || receiptFingerprint.length !== 64
    || !/^[a-f0-9]{64}$/.test(receiptFingerprint)) {
    throw new Error("Refresh Bluey Jobs to confirm the submitted receipt before preparing.");
  }
  const job = objectValue(receipt.job);
  const company = job?.company;
  const jobTitle = job?.title;
  if (typeof company !== "string" || !company.trim() || typeof jobTitle !== "string" || !jobTitle.trim()) {
    throw new Error("The final submission receipt is missing the submitted job snapshot.");
  }
  const claimIds = Array.isArray(packet.verifiedClaimIds) ? packet.verifiedClaimIds : [];
  const documents: unknown[] = Array.isArray(receipt.documents) ? receipt.documents : [];
  const document = documents.map(objectValue).find((item) => item?.kind === "resume");
  if (document?.versionId !== target.resume.id || typeof document.sha256 !== "string"
    || !/^[a-fA-F0-9]{64}$/.test(document.sha256) || !target.resume.checksum) {
    throw new Error("The final receipt does not contain a matching submitted resume document.");
  }
  const answerRecord = objectValue(packet.answers);
  const answerCount = answerRecord
    ? Object.keys(answerRecord).length
    : Array.isArray(packet.answers) ? packet.answers.length : 0;
  return {
    company,
    jobTitle,
    receiptFingerprint,
    applicationId: target.application.id,
    receiptId: receipt.receiptId,
    resumeVersionId: target.resume.id,
    resumeChecksum: target.resume.checksum,
    resumeDocumentSha256: document.sha256,
    submittedClaimCount: claimIds.length,
    finalAnswerCount: answerCount,
    evidenceCount: workspace.application_evidence.filter((item) => item.application_id === target.application.id).length,
  };
}

function previewResponse(snapshot: SubmittedApplicationSnapshot): InterviewPrepCompletionResponse {
  const paragraphs = [
    `Start with a concise introduction that connects the exact resume submitted for the ${snapshot.jobTitle} role at ${snapshot.company}.`,
    "Use a verified accomplishment from that submitted resume, explain your contribution, and describe the result without adding unsupported details.",
    "When the role asks for experience not supported by the submitted application, prepare the closest truthful example and label adjacent experience clearly.",
  ];
  return {
    schema_version: 1,
    id: `prep-${snapshot.receiptId}`,
    application_id: snapshot.applicationId,
    content: paragraphs.join("\n\n"),
    generated_at_ms: Date.now(),
    provider: "preview",
    model: "deterministic-fixture",
    cost_cents: 0,
    balance_cents_after: 0,
    trial_seconds_remaining: 0,
    cost_label: null,
    confidence: null,
    grounding: {
      receipt_id: snapshot.receiptId,
      receipt_fingerprint: snapshot.receiptFingerprint,
      resume_version_id: snapshot.resumeVersionId,
      resume_checksum: snapshot.resumeChecksum,
      resume_document_sha256: snapshot.resumeDocumentSha256,
      answer_keys_used: [],
      answer_keys_omitted: [],
    },
  };
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}
