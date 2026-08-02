import { useEffect, useState } from "react";
import { AlertCircle, CheckCircle2, FileCheck2, LoaderCircle, Sparkles } from "lucide-react";
import { jobsApi, type InterviewPrepCompletionResponse } from "../api";
import type { JobApplication, JobPosting, JobsWorkspace, ResumeVersion } from "../types";
import { Dialog } from "./Dialog";

interface PrepTarget {
  application: JobApplication;
  job: JobPosting;
  resume: ResumeVersion;
}

interface Props {
  target: PrepTarget | null;
  workspace: JobsWorkspace;
  onClose(): void;
}

interface SubmittedApplicationSnapshot {
  receiptId: string;
  resumeVersionId: string;
  submittedClaimCount: number;
  finalAnswerCount: number;
  evidenceCount: number;
}

export function InterviewPrepDialog({ target, workspace, onClose }: Props) {
  const [snapshot, setSnapshot] = useState<SubmittedApplicationSnapshot | null>(null);
  const [response, setResponse] = useState<InterviewPrepCompletionResponse | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setSnapshot(null);
    setResponse(null);
    setError("");
    if (!target) return;
    try {
      setSnapshot(submittedApplicationSnapshot(target, workspace));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not verify this application packet.");
    }
  }, [target, workspace.application_evidence]);

  const generate = async () => {
    if (!target || !snapshot) return;
    setBusy(true);
    setError("");
    try {
      if (new URLSearchParams(window.location.search).get("preview") === "1") {
        setResponse(previewResponse(target, snapshot));
      } else {
        setResponse(await jobsApi.prepareInterview(target.application.id));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not generate the coaching brief.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={Boolean(target)}
      title={target ? `Interview prep: ${target.job.company}` : "Interview prep"}
      description={target ? `${target.job.title} · Submitted resume v${target.resume.version_no}` : ""}
      onClose={onClose}
      size="large"
    >
      {target && (
        <div className="interview-prep">
          {snapshot && (
            <>
              <header className="prep-summary">
                <div><FileCheck2 size={19} /><span><b>Exact application loaded</b><small>Receipt {snapshot.receiptId} · Resume {snapshot.resumeVersionId}</small></span></div>
                <div><CheckCircle2 size={19} /><span><b>{snapshot.submittedClaimCount} submitted claim{snapshot.submittedClaimCount === 1 ? "" : "s"}</b><small>Only facts tied to the submitted resume are used.</small></span></div>
                <div><CheckCircle2 size={19} /><span><b>{snapshot.finalAnswerCount} final answer{snapshot.finalAnswerCount === 1 ? "" : "s"}</b><small>{snapshot.evidenceCount} evidence record{snapshot.evidenceCount === 1 ? "" : "s"} attached.</small></span></div>
              </header>

              <section className="prep-plan">
                <div className="prep-section-heading"><p>GROUNDING</p><h3>Prepared from the application the employer received</h3></div>
                <p>Bluey checks the locked receipt, exact resume version, final answers, and stored submission evidence before generating your coaching brief.</p>
              </section>

              {!response && (
                <div className="prep-generate">
                  <div><Sparkles size={18} /><span><b>Build my coaching brief</b><small>Creates one Bluey answer using your existing balance.</small></span></div>
                  <button className="button primary" disabled={busy} onClick={() => void generate()}>
                    {busy ? <LoaderCircle className="spin" size={16} /> : <Sparkles size={16} />}
                    {busy ? "Preparing..." : "Generate with Bluey"}
                  </button>
                </div>
              )}

              {response && (
                <section className="prep-brief">
                  <div className="prep-section-heading"><p>BLUEY COACHING BRIEF</p><h3>Ready to rehearse</h3><span>{response.cost_cents > 0 ? `${response.cost_cents} cents used` : "Included in your current access"}</span></div>
                  <article>{response.content}</article>
                </section>
              )}
            </>
          )}

          {error && <div className="prep-error"><AlertCircle size={18} /><span><b>Prep is not ready</b><p>{error}</p></span></div>}
          <footer className="prep-privacy"><CheckCircle2 size={14} />Contact details, demographic answers, and calendar attendees are excluded from coaching context.</footer>
        </div>
      )}
    </Dialog>
  );
}

function submittedApplicationSnapshot(target: PrepTarget, workspace: JobsWorkspace): SubmittedApplicationSnapshot {
  if (target.application.state !== "submitted") {
    throw new Error("Interview preparation starts after a confirmed submission.");
  }
  if (target.application.job_id !== target.job.id || target.application.resume_version_id !== target.resume.id) {
    throw new Error("The submitted job and resume do not match this application.");
  }
  const receipt = target.application.receipt;
  const packet = objectValue(receipt.packet);
  const result = objectValue(receipt.result);
  if (receipt.schemaVersion !== 1
    || typeof receipt.receiptId !== "string"
    || typeof receipt.applicationId !== "string"
    || receipt.applicationId !== target.application.id
    || packet?.jobId !== target.job.id
    || packet?.resumeVersionId !== target.resume.id
    || result?.status !== "submitted") {
    throw new Error("The final submission receipt is incomplete.");
  }
  const claimIds = Array.isArray(packet.verifiedClaimIds) ? packet.verifiedClaimIds : [];
  const answers = Array.isArray(packet.answers) ? packet.answers : target.application.answers;
  return {
    receiptId: receipt.receiptId,
    resumeVersionId: target.resume.id,
    submittedClaimCount: claimIds.length,
    finalAnswerCount: answers.length,
    evidenceCount: workspace.application_evidence.filter((item) => item.application_id === target.application.id).length,
  };
}

function previewResponse(target: PrepTarget, snapshot: SubmittedApplicationSnapshot): InterviewPrepCompletionResponse {
  const paragraphs = [
    `Start with a concise introduction that connects the exact resume submitted for the ${target.job.title} role at ${target.job.company}.`,
    "Use a verified accomplishment from that submitted resume, explain your contribution, and describe the result without adding unsupported details.",
    "When the role asks for experience not supported by the submitted application, prepare the closest truthful example and label adjacent experience clearly.",
  ];
  return {
    schema_version: 1,
    id: `prep-${snapshot.receiptId}`,
    application_id: target.application.id,
    content: paragraphs.join("\n\n"),
    generated_at_ms: Date.now(),
    provider: "preview",
    model: "deterministic-fixture",
    cost_cents: 0,
    balance_cents_after: 0,
    trial_seconds_remaining: 0,
    grounding: {
      receipt_id: snapshot.receiptId,
      receipt_fingerprint: "preview",
      resume_version_id: snapshot.resumeVersionId,
      resume_checksum: "preview",
      resume_document_sha256: "preview",
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
