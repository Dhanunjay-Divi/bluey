import { useEffect, useState } from "react";
import { AlertCircle, CalendarDays, CheckCircle2, FileCheck2, LoaderCircle, Sparkles } from "lucide-react";
import { jobsApi, type InterviewPrepCompletionResponse } from "../api";
import { buildPortalInterviewPrep, type PortalInterviewPrepLaunch } from "../lib/interview-prep";
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

export function InterviewPrepDialog({ target, workspace, onClose }: Props) {
  const [launch, setLaunch] = useState<PortalInterviewPrepLaunch | null>(null);
  const [response, setResponse] = useState<InterviewPrepCompletionResponse | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const submittedClaimCount = launch?.packet.sources.filter((source) => source.kind === "resume_claim").length ?? 0;

  useEffect(() => {
    setLaunch(null);
    setResponse(null);
    setError("");
    if (!target) return;
    try {
      setLaunch(buildPortalInterviewPrep({
        application: target.application,
        job: target.job,
        resume: target.resume,
        facts: workspace.facts,
        evidence: workspace.application_evidence.filter((item) => item.application_id === target.application.id),
      }));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not verify this application packet.");
    }
  }, [target, workspace.application_evidence, workspace.facts]);

  const generate = async () => {
    if (!target || !launch) return;
    setBusy(true);
    setError("");
    try {
      if (new URLSearchParams(window.location.search).get("preview") === "1") {
        setResponse(previewResponse(launch));
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
          {launch && (
            <>
              <header className="prep-summary">
                <div><FileCheck2 size={19} /><span><b>Exact application loaded</b><small>Receipt {launch.packet.receiptId} · Resume {launch.packet.resumeVersionId}</small></span></div>
                <div><CalendarDays size={19} /><span><b>{launch.packet.interviewLabel || "Proactive preparation"}</b><small>{launch.packet.interviewAt ? formatInterviewTime(launch.packet.interviewAt) : "No interview time attached yet"}</small></span></div>
                <div><CheckCircle2 size={19} /><span><b>{submittedClaimCount} submitted claim{submittedClaimCount === 1 ? "" : "s"}</b><small>{launch.packet.warnings.length ? `${launch.packet.warnings.length} truth gap${launch.packet.warnings.length === 1 ? "" : "s"} to prepare` : "Every role theme has submitted evidence"}</small></span></div>
              </header>

              <section className="prep-plan">
                <div className="prep-section-heading"><p>PRACTICE PLAN</p><h3>Questions tied to what you actually submitted</h3></div>
                <ol>
                  {launch.packet.questions.map((question) => (
                    <li key={question.id} className={question.needsCandidateInput ? "needs-input" : "grounded"}>
                      <span>{question.needsCandidateInput ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}</span>
                      <div><b>{question.question}</b><p>{question.needsCandidateInput ? "Needs your truthful example before Bluey drafts an answer." : question.claimIds.length ? `${question.claimIds.length} submitted claim${question.claimIds.length === 1 ? "" : "s"} available for grounding.` : "Grounded in the submitted role snapshot."}</p></div>
                    </li>
                  ))}
                </ol>
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

function formatInterviewTime(value: string): string {
  return new Date(value).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}

function previewResponse(launch: PortalInterviewPrepLaunch): InterviewPrepCompletionResponse {
  const supported = launch.packet.questions.find((question) => question.claimIds.length > 0);
  const gap = launch.packet.questions.find((question) => question.needsCandidateInput);
  const paragraphs = [
    `Start with a 60-second introduction that connects your submitted experience to the ${launch.packet.title} role at ${launch.packet.company}. Keep the wording close to the resume the employer already received.`,
    supported
      ? `First rehearsal: ${supported.question} Use only the submitted claims attached to that question, then explain the decision, your contribution, and what changed.`
      : "Start by choosing one verified accomplishment from the submitted resume and explaining why it is relevant to this role.",
    gap
      ? `Truth gap to resolve: ${gap.reason} Prepare the closest honest example you have, or state clearly that the experience is adjacent rather than direct.`
      : "No unsupported role theme was found in this application packet.",
  ];
  return {
    schema_version: 1,
    id: `prep-${launch.packet.receiptId}`,
    application_id: launch.packet.applicationId,
    content: paragraphs.join("\n\n"),
    generated_at_ms: Date.now(),
    provider: "preview",
    model: "deterministic-fixture",
    cost_cents: 0,
    balance_cents_after: 0,
    trial_seconds_remaining: 0,
  };
}
