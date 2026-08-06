import { useEffect, useMemo, useState } from "react";
import {
  AlertCircle,
  ArrowRight,
  BriefcaseBusiness,
  CalendarDays,
  Check,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  Clock3,
  Download,
  FileDiff,
  FileText,
  Inbox,
  Mail,
  MonitorUp,
  MoreHorizontal,
  Play,
  ReceiptText,
  Search,
  Send,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import type { ApplicationEvidence, CandidateEventInput, Intervention, JobApplication, JobEligibilityDecision, JobPosting, JobsWorkspace, ResumeVersion, RunnerAvailability } from "../types";
import { jobsApi } from "../api";
import { relativeTime, titleCase } from "../lib/format";
import { ConfirmDialog, Dialog } from "../components/Dialog";
import { AtsCertificationSummaryCard } from "../components/AtsCertificationSummary";
import { InterviewPrepDialog } from "../components/InterviewPrepDialog";
import { portalEligibilityDecision } from "../lib/ats-certification";
import { exportResumeDocx, exportResumePdf } from "../lib/documents";
import { applicationIssueReasons, applicationIssues, applicationOutcomes, eventActionLabel, latestApplicationOutcome } from "../lib/candidate-events";
import { formatResumeDiffValue, resumeDiffHasValue, resumeDiffLabel } from "../lib/resume-diff";
import { applicationAfterInterventionResolution } from "../lib/application-flow";
import { safeDownloadFileName, saveDownloadedBlob } from "../lib/download";

interface Props {
  workspace: JobsWorkspace;
  resumeVersions: Record<string, ResumeVersion>;
  onUpdate(application: JobApplication, state: string): Promise<void>;
  onReconcileSubmission(application: JobApplication): Promise<void>;
  onCommit(application: JobApplication): Promise<void>;
  onLoadResume(id: string): Promise<ResumeVersion | undefined>;
  onResolveIntervention(intervention: Intervention, action: string, resolution?: { answer?: string; remember?: boolean; scope?: string; scope_id?: string }): Promise<void>;
  onSaveCandidateEvent(event: CandidateEventInput): Promise<unknown>;
}

const SERVER_SUBMISSION_FINGERPRINT_KEY = "_bluey_server_submission_fingerprint_v1";

const stateGroups = [
  ["active", "Active"],
  ["review", "Needs review"],
  ["submitted", "Submitted"],
  ["all", "All"],
] as const;

export function ApplicationsView({ workspace, resumeVersions, onUpdate, onReconcileSubmission, onCommit, onLoadResume, onResolveIntervention, onSaveCandidateEvent }: Props) {
  const [filter, setFilter] = useState<(typeof stateGroups)[number][0]>("active");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<JobApplication | null>(null);
  const [selectedResume, setSelectedResume] = useState<ResumeVersion | undefined>();
  const [receiptOpen, setReceiptOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState("");
  const [answer, setAnswer] = useState("");
  const [rememberAnswer, setRememberAnswer] = useState(true);
  const [answerScope, setAnswerScope] = useState<"account" | "track" | "company">("account");
  const [prepTarget, setPrepTarget] = useState<{ application: JobApplication; job: JobPosting; resume: ResumeVersion } | null>(null);
  const [feedbackApplication, setFeedbackApplication] = useState<JobApplication | null>(null);
  const [feedbackMode, setFeedbackMode] = useState<"outcome" | "issue" | null>(null);
  const [feedbackAction, setFeedbackAction] = useState("");
  const [feedbackNote, setFeedbackNote] = useState("");
  const [reconciliationTarget, setReconciliationTarget] = useState<JobApplication | null>(null);
  const openInterventions = workspace.interventions.filter((item) => item.status === "open");

  const jobs = useMemo(() => new Map(workspace.matches.map((job) => [job.id, job])), [workspace.matches]);
  const selectedSession = selected
    ? workspace.browser_sessions.find((session) => session.application_id === selected.id)
    : undefined;
  const selectedEvidence = selected
    ? workspace.application_evidence.filter((evidence) => evidence.application_id === selected.id)
    : [];
  const selectedIntervention = selected
    ? workspace.interventions.find((item) => item.application_id === selected.id && item.status === "open")
    : undefined;
  const selectedJob = selected ? jobs.get(selected.job_id) : undefined;
  const selectedEligibility = selected ? applicationEligibility(selected, selectedJob) : undefined;
  const selectedRunnerAvailable = selectedEligibility
    ? hasAvailableRunner(selectedEligibility, workspace.runner_availability)
    : false;
  const selectedRunnerReason = selectedEligibility
    ? runnerUnavailableReason(selectedEligibility, workspace.runner_availability)
    : "";
  const selectedCanHandoff = Boolean(
    selectedJob?.canonical_url
    && selectedEligibility
    && selectedEligibility.capability !== "blocked",
  );
  const canAnswerIntervention = selectedIntervention?.resolution_kind === "answer"
    && ["unknown_question", "missing_fact", "sensitive_question"].includes(selectedIntervention.kind);
  const filtered = workspace.applications.filter((application) => {
    const job = jobs.get(application.job_id);
    const textMatch = !query || `${job?.company || ""} ${job?.title || ""}`.toLowerCase().includes(query.toLowerCase());
    if (!textMatch) return false;
    if (filter === "submitted") return application.state === "submitted";
    if (filter === "review") return applicationNeedsReview(application);
    if (filter === "active") return !["submitted", "failed"].includes(application.state);
    return true;
  });

  useEffect(() => {
    let cancelled = false;
    setLocalError("");
    if (!selected?.resume_version_id) {
      setSelectedResume(undefined);
      return () => {
        cancelled = true;
      };
    }
    const cached = resumeVersions[selected.resume_version_id];
    if (cached) {
      setSelectedResume(cached);
      return () => {
        cancelled = true;
      };
    }
    setSelectedResume(undefined);
    void onLoadResume(selected.resume_version_id)
      .then((resume) => {
        if (cancelled) return;
        if (resume) setSelectedResume(resume);
        else setLocalError("The job-specific resume is unavailable. Close this application and try again.");
      })
      .catch((cause) => {
        if (!cancelled) setLocalError(errorMessage(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [selected, resumeVersions, onLoadResume]);

  useEffect(() => {
    setAnswer("");
    setRememberAnswer(selectedIntervention?.kind !== "sensitive_question");
    setAnswerScope(selectedJob?.track_id ? "track" : "account");
  }, [selectedIntervention?.id, selectedIntervention?.kind, selectedJob?.track_id]);

  const update = async (state: string) => {
    if (!selected) return;
    setBusy(true);
    setLocalError("");
    try {
      await onUpdate(selected, state);
      setSelected((current) => current ? { ...current, state: state as JobApplication["state"] } : current);
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const reconcileNotSubmitted = async () => {
    if (!reconciliationTarget || busy) return;
    setBusy(true);
    setLocalError("");
    try {
      await onReconcileSubmission(reconciliationTarget);
      setReconciliationTarget(null);
      setSelected(null);
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const download = async (format: "pdf" | "docx") => {
    if (!selected || !selectedResume) return;
    setBusy(true);
    setLocalError("");
    try {
      await onCommit(selected);
      const filename = `bluey-${jobs.get(selected.job_id)?.company || "resume"}`;
      if (format === "pdf") await exportResumePdf(selectedResume.content, filename);
      else await exportResumeDocx(selectedResume.content, filename);
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const openHandoff = async () => {
    if (!selected || !selectedJob?.canonical_url) return;
    const target = window.open("about:blank", "_blank", "noopener,noreferrer");
    setBusy(true);
    setLocalError("");
    try {
      await onCommit(selected);
      if (target) target.location.href = selectedJob.canonical_url;
      else window.location.assign(selectedJob.canonical_url);
    } catch (cause) {
      target?.close();
      setLocalError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const submitInterventionAnswer = async () => {
    if (!selected || !selectedIntervention || !answer.trim()) return;
    setBusy(true);
    setLocalError("");
    try {
      const scopeId = answerScope === "track"
        ? selectedJob?.track_id
        : answerScope === "company"
          ? normalizeCompanyKey(selectedJob?.company || "")
          : undefined;
      await onResolveIntervention(selectedIntervention, "answer", {
        answer: answer.trim(),
        remember: rememberAnswer,
        scope: answerScope,
        scope_id: scopeId,
      });
      setSelected((current) => current
        ? applicationAfterInterventionResolution(current, undefined, "answer", Date.now())
        : current);
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const openFeedback = (application: JobApplication, nextMode: "outcome" | "issue") => {
    setFeedbackApplication(application);
    setFeedbackMode(nextMode);
    setFeedbackAction("");
    setFeedbackNote("");
    setSelected(null);
  };

  const saveFeedback = async () => {
    if (!feedbackApplication || !feedbackMode || !feedbackAction) return;
    setBusy(true);
    setLocalError("");
    try {
      await onSaveCandidateEvent({
        event_type: feedbackMode === "outcome" ? "application_outcome" : "application_issue",
        job_id: feedbackApplication.job_id,
        application_id: feedbackApplication.id,
        action: feedbackAction,
        note: feedbackNote,
      });
      setFeedbackMode(null);
      setFeedbackApplication(null);
      setFeedbackAction("");
      setFeedbackNote("");
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="view-shell applications-view">
      <section className="view-heading">
        <div><p className="eyebrow">APPLICATION CONTROL</p><h1>Applications</h1><span>Every tailored application, browser run, intervention, and receipt in one timeline.</span></div>
        <div className="heading-stat"><b>{workspace.applications.filter((item) => item.state === "submitted").length}</b><span>submitted this month</span></div>
      </section>

      {localError && <div className="inline-error" role="alert">{localError}</div>}

      {openInterventions.length > 0 && (
        <section className="intervention-banner">
          <span className="intervention-icon"><Inbox size={20} /></span>
          <div><p>INTERVENTION INBOX</p><h2>{openInterventions.length} application{openInterventions.length === 1 ? "" : "s"} waiting on you</h2><span>Bluey paused instead of guessing.</span></div>
          <div className="intervention-items">
            {openInterventions.slice(0, 2).map((item) => {
              const application = workspace.applications.find((candidate) => candidate.id === item.application_id);
              const job = application ? jobs.get(application.job_id) : undefined;
              return <button key={item.id} onClick={() => application && setSelected(application)}><AlertCircle size={15} /><span><b>{job?.company || item.title}</b><small>{item.detail}</small></span><ChevronRight size={16} /></button>;
            })}
          </div>
        </section>
      )}

      <section className="application-toolbar">
        <div className="tab-control">{stateGroups.map(([value, label]) => <button key={value} className={filter === value ? "active" : ""} onClick={() => setFilter(value)}>{label}<span>{applicationCountFor(value, workspace.applications)}</span></button>)}</div>
        <label className="search-field small"><Search size={16} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search applications" /></label>
      </section>

      <section className="application-list">
        {filtered.map((application) => {
          const job = jobs.get(application.job_id);
          const rowResume = application.resume_version_id ? resumeVersions[application.resume_version_id] : undefined;
          const outcome = latestApplicationOutcome(workspace.candidate_events, application.id);
          return (
            <button key={application.id} className="application-row" onClick={() => setSelected(application)}>
              <div className="company-mark">{(job?.company || "BJ").slice(0, 2).toUpperCase()}</div>
              <div className="application-main"><strong>{job?.title || "Application"}</strong><span>{job?.company || "Unknown company"} · {job?.location || "Location not listed"}</span></div>
              <div className="application-stage">{stateIcon(application.state)}<span><b>{titleCase(application.state)}</b><small>{relativeTime(application.updated_at_ms)}{outcome ? ` · ${eventActionLabel(outcome.action)}` : ""}</small></span></div>
              <div className="application-packet"><FileText size={15} /><span>Job-specific resume<small>{rowResume ? `v${rowResume.version_no}` : "Prepared"} · {titleCase(application.submission_mode)}</small></span></div>
              <span className="icon-button" aria-hidden="true"><MoreHorizontal size={18} /></span>
            </button>
          );
        })}
        {filtered.length === 0 && <div className="empty-state"><div className="empty-icon"><BriefcaseBusiness /></div><h3>No applications here</h3><p>Prepare an application from Matches and it will appear in this timeline.</p></div>}
      </section>

      <Dialog open={Boolean(selected) && !receiptOpen} title={selected ? `${jobs.get(selected.job_id)?.title || "Application"}` : "Application"} description={selected ? `${jobs.get(selected.job_id)?.company || ""} · ${titleCase(selected.state)}` : ""} onClose={() => setSelected(null)} size="large">
        {selected && (
          <div className="application-detail">
            <div className="application-steps">
              {[
                ["Materials", true],
                ["Review", !["matched", "preparing"].includes(selected.state)],
                ["Apply", ["running", "needs_input", "submitted"].includes(selected.state)],
                ["Receipt", selected.state === "submitted"],
              ].map(([label, complete], index) => <div key={String(label)} className={complete ? "complete" : ""}><span>{complete ? <Check size={13} /> : index + 1}</span><b>{label}</b></div>)}
            </div>
            {(latestApplicationOutcome(workspace.candidate_events, selected.id) || applicationIssues(workspace.candidate_events, selected.id).length > 0) && <div className="candidate-event-summary">
              {latestApplicationOutcome(workspace.candidate_events, selected.id) && <span className="status-chip success">Outcome: {eventActionLabel(latestApplicationOutcome(workspace.candidate_events, selected.id)?.action || "")}</span>}
              {applicationIssues(workspace.candidate_events, selected.id).length > 0 && <span className="status-chip warning">{applicationIssues(workspace.candidate_events, selected.id).length} open report{applicationIssues(workspace.candidate_events, selected.id).length === 1 ? "" : "s"}</span>}
            </div>}
            {selected.state === "side_effect_unknown" && (
              <div className="input-needed" role="alert">
                <AlertCircle size={17} />
                <div>
                  <b>Submission outcome needs reconciliation</b>
                  <p>Bluey stopped because the final employer action may have happened. Do not submit again. Check the employer confirmation page or email, then reconcile this run from its preserved evidence.</p>
                </div>
              </div>
            )}
            <div className="application-detail-grid">
              <section className="resume-sheet compact-sheet">
                {selectedResume ? <ResumePreview resume={selectedResume} /> : <div className="resume-loading">Loading job-specific resume...</div>}
              </section>
              <section className="review-panel">
                <div className="review-title"><div><p>PACKET REVIEW</p><h3>Application kit</h3></div><FileDiff size={20} /></div>
                <ApplicationKitSummary application={selected} job={selectedJob} resume={selectedResume} />
                <DiffList resume={selectedResume} />
                <CoverLetterPreview coverLetter={selected.cover_letter} />
                <FinalAnswers answers={selected.answers} />
                <PauseReasons application={selected} job={selectedJob} intervention={selectedIntervention} />
                <div className="claim-note"><CheckCircle2 size={17} /><span><b>No unsupported claims</b><small>{selectedResume?.claim_ids.length || 0} profile facts carry provenance into this version.</small></span></div>
                {selected.state === "needs_input" && canAnswerIntervention && selectedIntervention
                  ? <div className="answer-intervention">
                      <div className="answer-intervention-heading"><AlertCircle size={17} /><div><b>{selectedIntervention.title}</b><p>{selectedIntervention.detail}</p></div></div>
                      <textarea aria-label="Application answer" value={answer} onChange={(event) => setAnswer(event.target.value)} placeholder="Type the answer Bluey should use" rows={3} />
                      <div className="answer-memory-options">
                        <label><input type="checkbox" checked={rememberAnswer} onChange={(event) => setRememberAnswer(event.target.checked)} /><span><b>Remember this answer</b><small>Reuse it when the same question appears.</small></span></label>
                        {rememberAnswer && <label className="answer-scope"><span>Use for</span><select value={answerScope} onChange={(event) => setAnswerScope(event.target.value as typeof answerScope)}><option value="account">All applications</option>{selectedJob?.track_id && <option value="track">This Career Track</option>}<option value="company">{selectedJob?.company || "This company"} only</option></select></label>}
                      </div>
                      <button className="button primary compact" disabled={busy || !answer.trim()} onClick={() => void submitInterventionAnswer()}>{answerInterventionActionLabel(busy)}<ArrowRight size={15} /></button>
                    </div>
                  : selected.state === "needs_input" && <div className="input-needed"><AlertCircle size={17} /><div><b>Bluey needs you</b><p>{selectedIntervention?.detail || "Open the browser takeover to continue."}</p></div></div>}
                {selected.state === "awaiting_review" && !selectedRunnerAvailable && (
                  <div className="input-needed" role="status">
                    <AlertCircle size={17} />
                    <div>
                      <b>Auto-submit is unavailable</b>
                      <p>{selectedRunnerReason}</p>
                    </div>
                  </div>
                )}
                <div className="download-row"><button disabled={busy || !selectedResume} onClick={() => void download("pdf")}><Download size={15} />PDF</button><button disabled={busy || !selectedResume} onClick={() => void download("docx")}><Download size={15} />DOCX</button></div>
              </section>
            </div>
            <div className="dialog-actions spread"><p>{selected.state === "submitted" ? "Receipt locked to this exact resume and answer set." : selected.state === "side_effect_unknown" ? "Automatic retry is disabled until the employer-facing outcome is reconciled." : selected.state === "awaiting_review" && !selectedRunnerAvailable ? "Your tailored kit is ready. Download it or continue on the original job site." : "Approving counts this tailored application once. Retries do not double-charge."}</p><div><button className="button secondary" onClick={() => openFeedback(selected, "issue")}><TriangleAlert size={16} />Report problem</button>{selected.state === "side_effect_unknown" && <button className="button danger subtle" disabled={busy} onClick={() => setReconciliationTarget(selected)}><AlertCircle size={16} />I checked: not submitted</button>}{selected.state === "submitted" && <button className="button secondary" onClick={() => openFeedback(selected, "outcome")}><CalendarDays size={16} />Update outcome</button>}{selected.state === "needs_input" && !canAnswerIntervention && selectedSession?.takeover_url && <a className="button secondary" href={selectedSession.takeover_url}><MonitorUp size={16} />Take over browser</a>}{selected.state === "needs_input" && !canAnswerIntervention && !selectedSession?.takeover_url && <button className="button secondary" disabled title="A scoped resume link is not available for this run"><MonitorUp size={16} />Takeover unavailable</button>}{selected.state === "awaiting_review" && selectedRunnerAvailable && <button className="button primary" disabled={busy} onClick={() => void update("queued")}><Play size={16} />Approve application</button>}{selected.state === "awaiting_review" && !selectedRunnerAvailable && selectedCanHandoff && <button className="button primary" disabled={busy} onClick={() => void openHandoff()}><Send size={16} />Open job site</button>}{selected.state === "queued" && <a className="button primary" href="/jobs/browser"><Send size={16} />Choose runner</a>}{selected.state === "submitted" && selectedJob && selectedResume && <button className="button primary" onClick={() => { setPrepTarget({ application: selected, job: selectedJob, resume: selectedResume }); setSelected(null); }}><Sparkles size={16} />Prepare interview</button>}{selected.state === "submitted" && <button className="button secondary" onClick={() => setReceiptOpen(true)}><CheckCircle2 size={16} />View receipt</button>}</div></div>
          </div>
        )}
      </Dialog>
      <ConfirmDialog
        open={Boolean(reconciliationTarget)}
        title="Confirm application was not submitted"
        description="Check the employer confirmation page and your application email first. Bluey will close this uncertain run and allow a new reviewed attempt."
        confirmLabel={busy ? "Closing run..." : "Confirm not submitted"}
        tone="danger"
        onConfirm={() => void reconcileNotSubmitted()}
        onClose={() => !busy && setReconciliationTarget(null)}
      />
      <Dialog open={receiptOpen} title="Submission receipt" description={selected ? `${jobs.get(selected.job_id)?.company || "Application"} · ${jobs.get(selected.job_id)?.title || ""}` : ""} onClose={() => setReceiptOpen(false)}>
        {selected && <ReceiptView application={selected} resume={selectedResume} evidence={selectedEvidence} />}
      </Dialog>
      <Dialog open={feedbackMode === "outcome"} title="Update application outcome" description={feedbackApplication ? `${jobs.get(feedbackApplication.job_id)?.title || "Application"} at ${jobs.get(feedbackApplication.job_id)?.company || ""}` : ""} onClose={() => setFeedbackMode(null)}>
        <div className="feedback-dialog">
          <p>Record what the employer told you. Bluey keeps this separate from the locked submission receipt.</p>
          <div className="choice-chips" role="group" aria-label="Application outcome">
            {applicationOutcomes.map(([value, label]) => <button key={value} type="button" className={feedbackAction === value ? "active" : ""} onClick={() => setFeedbackAction(value)}>{label}</button>)}
          </div>
          <label><span>Note <small>optional</small></span><textarea value={feedbackNote} maxLength={1000} rows={3} onChange={(event) => setFeedbackNote(event.target.value)} placeholder="Interview date, recruiter note, or next step" /></label>
        </div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setFeedbackMode(null)}>Cancel</button><button className="button primary" disabled={busy || !feedbackAction} onClick={() => void saveFeedback()}>{busy ? "Saving..." : "Save outcome"}</button></div>
      </Dialog>
      <Dialog open={feedbackMode === "issue"} title="Report an application problem" description={feedbackApplication ? `${jobs.get(feedbackApplication.job_id)?.title || "Application"} at ${jobs.get(feedbackApplication.job_id)?.company || ""}` : ""} onClose={() => setFeedbackMode(null)}>
        <div className="feedback-dialog">
          <p>Tell Bluey what went wrong. The report stays attached to this application for support and review.</p>
          <div className="choice-chips" role="group" aria-label="Problem category">
            {applicationIssueReasons.map(([value, label]) => <button key={value} type="button" className={feedbackAction === value ? "active" : ""} onClick={() => setFeedbackAction(value)}>{label}</button>)}
          </div>
          <label><span>What happened?</span><textarea value={feedbackNote} maxLength={1000} rows={4} onChange={(event) => setFeedbackNote(event.target.value)} placeholder="Include the page or step where Bluey stopped" /></label>
        </div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setFeedbackMode(null)}>Cancel</button><button className="button primary" disabled={busy || !feedbackAction} onClick={() => void saveFeedback()}>{busy ? "Saving..." : "Send report"}</button></div>
      </Dialog>
      <InterviewPrepDialog target={prepTarget} workspace={workspace} onClose={() => setPrepTarget(null)} />
    </div>
  );
}

function normalizeCompanyKey(company: string): string {
  return company.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error && cause.message.trim()
    ? cause.message
    : "Bluey could not finish that application action. Please try again.";
}

function ApplicationKitSummary({ application, job, resume }: { application: JobApplication; job?: JobPosting; resume?: ResumeVersion }) {
  const identity = application.receipt.application_identity && typeof application.receipt.application_identity === "object"
    ? application.receipt.application_identity as Record<string, unknown>
    : {};
  const email = String(identity.email || "Not selected");
  const eligibility = applicationEligibility(application, job);
  const capability = capabilityLabel(eligibility.capability);
  const metering = application.receipt.metering && typeof application.receipt.metering === "object"
    ? application.receipt.metering as Record<string, unknown>
    : {};
  const meteringStatus = String(metering.status || "");
  return (
    <>
      <div className="kit-summary">
        <div><b>Resume</b><span>{resume ? `v${resume.version_no} · ${titleCase(resume.mode)}` : "Loading version"}</span></div>
        <div><b>Email</b><span>{email}</span></div>
        <div><b>Answers</b><span>{application.answers.length ? `${application.answers.length} final answer${application.answers.length === 1 ? "" : "s"}` : "No answers required yet"}</span></div>
        <div><b>Cover letter</b><span>{application.cover_letter?.trim() ? "Included" : "Not included"}</span></div>
        <div><b>Site</b><span>{capability}</span></div>
        <div><b>Metering</b><span>{meteringStatus === "counts_when_approved_or_downloaded" || application.state === "awaiting_review" ? "Counts when approved or downloaded" : application.state === "submitted" ? "Counted once" : "Counted once for this job"}</span></div>
      </div>
      <AtsCertificationSummaryCard eligibility={applicationEligibilityAuthority(application, job)} />
    </>
  );
}

function FinalAnswers({ answers }: { answers: Array<Record<string, unknown>> }) {
  if (answers.length === 0) {
    return <div className="kit-section-empty"><b>Final answers</b><span>No reusable application answers are needed yet.</span></div>;
  }
  return (
    <section className="kit-detail-section">
      <div><p>FINAL ANSWERS</p><h4>What Bluey will use</h4></div>
      <dl>{answers.map((answer, index) => {
        const question = String(answer.question || answer.label || answer.key || `Answer ${index + 1}`);
        const value = String(answer.value || answer.answer || "");
        const scope = answer.scope ? ` · ${titleCase(String(answer.scope))}` : "";
        return <div key={`${question}-${index}`}><dt>{question}</dt><dd>{value || "Awaiting your answer"}{scope}</dd></div>;
      })}</dl>
    </section>
  );
}

function CoverLetterPreview({ coverLetter }: { coverLetter: string }) {
  const content = coverLetter.trim();
  if (!content) {
    return (
      <div className="kit-section-empty">
        <b>Cover letter</b>
        <span>This application does not include a cover letter.</span>
      </div>
    );
  }
  return (
    <section className="kit-detail-section cover-letter-preview">
      <div><p>COVER LETTER</p><h4>What Bluey will submit</h4></div>
      <p>{content}</p>
    </section>
  );
}

function PauseReasons({ application, job, intervention }: { application: JobApplication; job?: JobPosting; intervention?: Intervention }) {
  const eligibility = applicationEligibility(application, job);
  const reasons = [...eligibility.hard_failures, ...eligibility.review_reasons]
    .map((reason) => reason.message);
  if (intervention?.detail) reasons.unshift(intervention.detail);
  const uniqueReasons = [...new Set(reasons)];
  return (
    <section className="kit-detail-section pause-section">
      <div><p>PAUSE CONDITIONS</p><h4>{uniqueReasons.length ? "Bluey will stop for these checks" : "No unresolved checks"}</h4></div>
      {uniqueReasons.length
        ? <ul>{uniqueReasons.map((reason) => <li key={reason}>{reason}</li>)}</ul>
        : <p>The server-side rules and required facts currently pass.</p>}
    </section>
  );
}

function DiffList({ resume }: { resume?: ResumeVersion }) {
  const entries = resume ? Object.entries(resume.diff).filter(([, value]) => resumeDiffHasValue(value)) : [];
  if (!resume) return <div className="diff-empty">Loading visible diff...</div>;
  if (entries.length === 0) {
    return <div className="diff-empty">No visible resume changes were recorded for this version.</div>;
  }
  return (
    <ul className="diff-list">
      {entries.map(([key, value]) => <li key={key}><span>{resumeDiffLabel(key)}</span><p>{formatResumeDiffValue(value)}</p></li>)}
    </ul>
  );
}

export function applicationEligibility(
  application: JobApplication,
  job?: JobPosting,
): JobEligibilityDecision {
  return portalEligibilityDecision(applicationEligibilityAuthority(application, job), true);
}

function applicationEligibilityAuthority(application: JobApplication, job?: JobPosting): unknown {
  const stored = application.receipt.eligibility;
  if (stored && typeof stored === "object") return stored;
  if (job?.eligibility) return job.eligibility;
  return undefined;
}

export function hasAvailableRunner(
  eligibility: JobEligibilityDecision,
  runners: RunnerAvailability,
): boolean {
  const safeEligibility = portalEligibilityDecision(eligibility, true);
  return (safeEligibility.can_queue_local && runners.local.available)
    || (safeEligibility.can_queue_cloud && runners.cloud.available);
}

export function runnerUnavailableReason(
  eligibility: JobEligibilityDecision,
  runners: RunnerAvailability,
): string {
  const safeEligibility = portalEligibilityDecision(eligibility, true);
  if (safeEligibility.hard_failures.length > 0) {
    return safeEligibility.hard_failures[0].message;
  }
  if (safeEligibility.capability === "beta_review") {
    return "This application system is still in beta. Review the kit and continue on the job site.";
  }
  if (safeEligibility.capability === "handoff") {
    return "This site requires a user-controlled handoff after Bluey prepares the application kit.";
  }
  if (safeEligibility.capability === "unknown_review") {
    return safeEligibility.review_reasons[0]?.message
      || "This application system is not certified for a Bluey runner. Review the kit and continue on the job site.";
  }
  if (safeEligibility.capability === "blocked") {
    return "This listing cannot use a Bluey runner.";
  }
  if (!safeEligibility.can_queue_local && !safeEligibility.can_queue_cloud) {
    return safeEligibility.review_reasons[0]?.message
      || "This application must stay in review until the current eligibility checks pass.";
  }
  return runners.auto_submit_reason;
}

function capabilityLabel(capability: JobEligibilityDecision["capability"]): string {
  if (capability === "certified") return "Certified";
  if (capability === "beta_review") return "Beta · Review first";
  if (capability === "handoff") return "Handoff";
  if (capability === "blocked") return "Blocked";
  return "Review only";
}

export function hasVerifiedSubmissionEvidence(
  application: Pick<
    JobApplication,
    "id" | "state" | "resume_version_id" | "receipt" | "submitted_at_ms"
  >,
  evidence: ApplicationEvidence[],
): boolean {
  if (application.state !== "submitted"
    || !application.resume_version_id
    || positiveInteger(application.submitted_at_ms) === undefined) {
    return false;
  }
  const applicationEvidence = evidence.filter((item) => item.application_id === application.id);
  const resumeEvidence = applicationEvidence.filter((item) => item.kind === "resume");
  const receiptEvidence = applicationEvidence.filter((item) => item.kind === "application_receipt");
  const confirmationEvidence = applicationEvidence.filter(
    (item) => item.kind === "submission_confirmation",
  );

  if (resumeEvidence.length !== 1
    || receiptEvidence.length !== 1
    || confirmationEvidence.length < 1
    || confirmationEvidence.length > 4) {
    return false;
  }

  const resume = resumeEvidence[0];
  const receipt = receiptEvidence[0];
  const resumeVersionId = application.resume_version_id;
  if ([resume, receipt, ...confirmationEvidence]
    .some((item) => item.resume_version_id !== resumeVersionId)) {
    return false;
  }

  const resumeMetadata = objectValue(resume.metadata);
  const receiptMetadata = objectValue(receipt.metadata);
  const confirmationMetadata = confirmationEvidence.map((item) => objectValue(item.metadata));
  if (!resumeMetadata || !receiptMetadata || confirmationMetadata.some((item) => !item)) return false;
  const confirmationRecords = confirmationEvidence.map((item, index) => ({
    evidence: item,
    metadata: confirmationMetadata[index] as Record<string, unknown>,
  }));
  const receiptId = recordString(receiptMetadata, "receipt_id");
  if (!receiptId
    || recordString(resumeMetadata, "receipt_id") !== receiptId
    || confirmationRecords.some(({ metadata }) => recordString(metadata, "receipt_id") !== receiptId)) {
    return false;
  }

  if (resume.media_type !== "application/pdf"
    || receipt.media_type !== "application/json"
    || resumeMetadata.attached_to_submission !== true
    || receiptMetadata.immutable !== true
    || receiptMetadata.schema_version !== 1
    || confirmationRecords.some(({ evidence: confirmation, metadata }) => (
      confirmation.media_type !== "image/png"
      || !confirmation.file_name.toLowerCase().endsWith(".png")
      || metadata.evidence_strength !== "browser_confirmed"
      || !recordString(metadata, "confirmation")
    ))) {
    return false;
  }

  if (!validSha256(resume.sha256)
    || !validSha256(receipt.sha256)
    || positiveInteger(resumeMetadata.size_bytes) === undefined
    || positiveInteger(receiptMetadata.size_bytes) === undefined
    || confirmationRecords.some(({ evidence: confirmation, metadata }) => (
      !validSha256(confirmation.sha256)
      || positiveInteger(metadata.size_bytes) === undefined
    ))) {
    return false;
  }

  const scopes = [resume, receipt, ...confirmationEvidence]
    .map((item) => accountStorageScope(item.storage_key));
  if (scopes.some((scope) => !scope) || new Set(scopes).size !== 1) return false;
  const accountScope = scopes[0];
  if (!accountScope) return false;

  const storedReceipt = objectValue(application.receipt);
  if (!storedReceipt) return false;
  if (recordString(storedReceipt, "receiptId") !== receiptId
    || recordString(storedReceipt, "applicationId") !== application.id
    || recordString(storedReceipt, "accountId") !== accountScope.split("/").at(-1)
    || !validSha256(recordString(storedReceipt, SERVER_SUBMISSION_FINGERPRINT_KEY))) {
    return false;
  }
  const packet = objectValue(storedReceipt.packet);
  if (!packet || recordString(packet, "resumeVersionId") !== resumeVersionId) return false;
  const receiptObject = objectValue(storedReceipt.receiptObject);
  if (!receiptObject
    || recordString(receiptObject, "storageKey") !== receipt.storage_key
    || !sameSha256(recordString(receiptObject, "sha256"), receipt.sha256)
    || recordString(receiptObject, "mediaType") !== receipt.media_type
    || positiveInteger(receiptObject.sizeBytes) !== positiveInteger(receiptMetadata.size_bytes)
    || receiptObject.schemaVersion !== 1) {
    return false;
  }

  const documents = arrayOfRecords(storedReceipt.documents);
  if (!documents) return false;
  const resumeDocuments = documents.filter((document) => recordString(document, "kind") === "resume");
  if (resumeDocuments.length !== 1
    || new Set(documents.map((document) => recordString(document, "storageKey"))).size
      !== documents.length
    || documents.some((document) => !validReceiptDocument(document, accountScope))) {
    return false;
  }
  const resumeDocument = resumeDocuments[0];
  if (recordString(resumeDocument, "versionId") !== resumeVersionId
    || recordString(resumeDocument, "storageKey") !== resume.storage_key
    || !sameSha256(recordString(resumeDocument, "sha256"), resume.sha256)
    || recordString(resumeDocument, "mediaType") !== resume.media_type) {
    return false;
  }

  const screenshotKeys = stringArray(storedReceipt.screenshotKeys);
  if (!screenshotKeys
    || screenshotKeys.length === 0
    || screenshotKeys.length > 4
    || screenshotKeys.length !== confirmationRecords.length
    || new Set(screenshotKeys).size !== screenshotKeys.length
    || screenshotKeys.some((key) => accountStorageScope(key) !== accountScope)) {
    return false;
  }
  const indexedConfirmations = confirmationRecords.map(({ evidence: confirmation, metadata }) => {
    const metadataKeys = stringArray(metadata.screenshot_keys);
    const screenshotIndex = positiveInteger(metadata.screenshot_index);
    const screenshotCount = positiveInteger(metadata.screenshot_count);
    const legacySingle = screenshotKeys.length === 1
      && !Object.prototype.hasOwnProperty.call(metadata, "screenshot_index")
      && !Object.prototype.hasOwnProperty.call(metadata, "screenshot_count")
      && !Object.prototype.hasOwnProperty.call(metadata, "immutable");
    if (!metadataKeys
      || !sameStringArray(screenshotKeys, metadataKeys)
      || (!legacySingle && (
        metadata.immutable !== true
        || screenshotCount !== screenshotKeys.length
        || screenshotIndex === undefined
        || screenshotIndex > screenshotKeys.length
      ))) {
      return undefined;
    }
    const index = legacySingle ? 0 : (screenshotIndex as number) - 1;
    return confirmation.storage_key === screenshotKeys[index]
      ? { confirmation, metadata, index }
      : undefined;
  });
  if (indexedConfirmations.some((item) => !item)
    || new Set(indexedConfirmations.map((item) => item?.index)).size !== screenshotKeys.length
    || new Set(confirmationEvidence.map((item) => item.storage_key)).size !== screenshotKeys.length
    || new Set(confirmationEvidence.map((item) => item.file_name)).size !== screenshotKeys.length) {
    return false;
  }

  const manifest = arrayOfRecords(storedReceipt.evidenceObjects);
  if (!manifest
    || manifest.length === 0
    || manifest.length !== documents.length + screenshotKeys.length
    || new Set(manifest.map((item) => recordString(item, "storageKey"))).size !== manifest.length
    || manifest.some((item) => !validManifestObject(item, accountScope))
    || manifest.filter((item) => recordString(item, "kind") === "resume").length !== 1
    || documents.some((document) => !receiptDocumentMatchesManifest(document, manifest))
    || screenshotKeys.some((key) => !manifest.some((item) => (
      recordString(item, "storageKey") === key && recordString(item, "kind") === "screenshot"
    )))
    || !manifestObjectMatches(
      manifest,
      "resume",
      resume.storage_key,
      resume.sha256,
      resume.media_type,
      positiveInteger(resumeMetadata.size_bytes),
    )
    || indexedConfirmations.some((item) => !item || !manifestObjectMatches(
      manifest,
      "screenshot",
      item.confirmation.storage_key,
      item.confirmation.sha256,
      item.confirmation.media_type,
      positiveInteger(item.metadata.size_bytes),
    ))) {
    return false;
  }

  const documentEvidence = applicationEvidence.filter((item) => (
    ["resume", "cover_letter", "attachment"].includes(item.kind)
  ));
  if (documentEvidence.length !== documents.length
    || documents.some((document) => {
      const storageKey = recordString(document, "storageKey");
      const matchingEvidence = documentEvidence.filter((item) => item.storage_key === storageKey);
      return matchingEvidence.length !== 1
        || !evidenceRecordMatchesReceiptDocument(
          matchingEvidence[0],
          document,
          manifest,
          receiptId,
          accountScope,
          resumeVersionId,
        );
    })) {
    return false;
  }

  return true;
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function arrayOfRecords(value: unknown): Record<string, unknown>[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const records = value.map(objectValue);
  return records.some((item) => !item)
    ? undefined
    : records as Record<string, unknown>[];
}

function recordString(value: Record<string, unknown>, key: string): string {
  const item = value[key];
  return typeof item === "string" ? item.trim() : "";
}

function positiveInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

function validSha256(value: string): boolean {
  return /^[a-f\d]{64}$/i.test(value);
}

function sameSha256(left: string, right: string): boolean {
  return validSha256(left) && validSha256(right) && left.toLowerCase() === right.toLowerCase();
}

function accountStorageScope(storageKey: string): string | undefined {
  if (!storageKey || storageKey !== storageKey.trim() || storageKey.startsWith("/")) return undefined;
  const parts = storageKey.split("/");
  if (parts.some((part) => !part || part === "." || part === ".." || part.includes("\\"))) {
    return undefined;
  }
  const accountsIndex = parts.indexOf("accounts");
  if (accountsIndex < 0
    || accountsIndex !== parts.lastIndexOf("accounts")
    || accountsIndex + 2 >= parts.length
    || !/^[a-zA-Z0-9_-]+$/.test(parts[accountsIndex + 1])) {
    return undefined;
  }
  return parts.slice(0, accountsIndex + 2).join("/");
}

function stringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)
    || value.some((item) => typeof item !== "string" || !item.trim() || item !== item.trim())) {
    return undefined;
  }
  return value as string[];
}

function sameStringArray(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((item, index) => item === right[index]);
}

function validManifestObject(item: Record<string, unknown>, accountScope: string): boolean {
  const kind = recordString(item, "kind");
  const mediaType = recordString(item, "mediaType");
  const expectedMediaType = kind === "screenshot" ? "image/png" : "application/pdf";
  return ["resume", "cover_letter", "attachment", "screenshot"].includes(kind)
    && mediaType === expectedMediaType
    && accountStorageScope(recordString(item, "storageKey")) === accountScope
    && validSha256(recordString(item, "sha256"))
    && positiveInteger(item.sizeBytes) !== undefined;
}

function validReceiptDocument(item: Record<string, unknown>, accountScope: string): boolean {
  const kind = recordString(item, "kind");
  return ["resume", "cover_letter", "attachment"].includes(kind)
    && recordString(item, "mediaType") === "application/pdf"
    && accountStorageScope(recordString(item, "storageKey")) === accountScope
    && validSha256(recordString(item, "sha256"));
}

function receiptDocumentMatchesManifest(
  document: Record<string, unknown>,
  manifest: Record<string, unknown>[],
): boolean {
  const storageKey = recordString(document, "storageKey");
  const match = manifest.filter((item) => recordString(item, "storageKey") === storageKey);
  return match.length === 1
    && recordString(match[0], "kind") === recordString(document, "kind")
    && sameSha256(recordString(match[0], "sha256"), recordString(document, "sha256"))
    && recordString(match[0], "mediaType") === recordString(document, "mediaType");
}

function evidenceRecordMatchesReceiptDocument(
  evidence: ApplicationEvidence,
  document: Record<string, unknown>,
  manifest: Record<string, unknown>[],
  receiptId: string,
  accountScope: string,
  resumeVersionId: string,
): boolean {
  const kind = recordString(document, "kind");
  const storageKey = recordString(document, "storageKey");
  const metadata = objectValue(evidence.metadata);
  const sizeBytes = metadata && positiveInteger(metadata.size_bytes);
  return Boolean(metadata)
    && evidence.kind === kind
    && evidence.storage_key === storageKey
    && accountStorageScope(evidence.storage_key) === accountScope
    && sameSha256(evidence.sha256, recordString(document, "sha256"))
    && evidence.media_type === recordString(document, "mediaType")
    && metadata?.attached_to_submission === true
    && recordString(metadata as Record<string, unknown>, "receipt_id") === receiptId
    && (kind !== "resume" || evidence.resume_version_id === resumeVersionId)
    && manifestObjectMatches(
      manifest,
      kind,
      evidence.storage_key,
      evidence.sha256,
      evidence.media_type,
      sizeBytes,
    );
}

function manifestObjectMatches(
  manifest: Record<string, unknown>[],
  kind: string,
  storageKey: string,
  sha256: string,
  mediaType: string,
  sizeBytes: number | undefined,
): boolean {
  if (sizeBytes === undefined) return false;
  const matchingKey = manifest.filter((item) => recordString(item, "storageKey") === storageKey);
  return matchingKey.length === 1
    && recordString(matchingKey[0], "kind") === kind
    && sameSha256(recordString(matchingKey[0], "sha256"), sha256)
    && recordString(matchingKey[0], "mediaType") === mediaType
    && positiveInteger(matchingKey[0].sizeBytes) === sizeBytes;
}

export function ReceiptView({ application, resume, evidence }: {
  application: JobApplication;
  resume?: ResumeVersion;
  evidence: ApplicationEvidence[];
}) {
  const [downloadingEvidenceId, setDownloadingEvidenceId] = useState("");
  const [downloadError, setDownloadError] = useState("");
  const orderedEvidence = [...evidence].sort((left, right) => right.occurred_at_ms - left.occurred_at_ms);
  const resumeEvidence = orderedEvidence.find((item) => item.kind === "resume" && item.resume_version_id === application.resume_version_id);
  const submissionVerified = hasVerifiedSubmissionEvidence(application, orderedEvidence);
  const storedReceipt = objectValue(application.receipt) || {};
  const applicationIdentity = objectValue(storedReceipt.application_identity);
  const packet = objectValue(storedReceipt.packet);
  const applicationEmail = applicationIdentity
    ? recordString(applicationIdentity, "email")
    : packet
      ? recordString(packet, "applicationEmail")
      : "";

  const downloadEvidence = async (item: ApplicationEvidence) => {
    setDownloadingEvidenceId(item.id);
    setDownloadError("");
    try {
      const fallbackName = evidenceDownloadFileName(item);
      const downloaded = await jobsApi.downloadApplicationEvidence(application.id, item.id, fallbackName);
      saveDownloadedBlob(downloaded.blob, downloaded.fileName);
    } catch (cause) {
      setDownloadError(cause instanceof Error && cause.message.trim()
        ? cause.message
        : "Bluey could not download that evidence. Please try again.");
    } finally {
      setDownloadingEvidenceId("");
    }
  };

  return (
    <div className="receipt-view">
      <div
        className={`receipt-check ${submissionVerified ? "verified" : "warning"}`}
        role="status"
        aria-live="polite"
      >
        {submissionVerified ? <CheckCircle2 size={22} /> : <AlertCircle size={22} />}
        <span>
          <b>{submissionVerified ? "Submission verified" : "Evidence not verified"}</b>
          <small>
            {application.submitted_at_ms
              ? new Date(application.submitted_at_ms).toLocaleString()
              : "Submission time pending"}
          </small>
        </span>
      </div>
      <dl>
        <div><dt>Status</dt><dd>{titleCase(application.state)}</dd></div>
        <div><dt>Exact resume</dt><dd>{resumeEvidence?.file_name || (resume ? `Version ${resume.version_no} · ${titleCase(resume.mode)}` : "Evidence missing")}</dd></div>
        {applicationEmail && <div><dt>Application email</dt><dd>{applicationEmail}</dd></div>}
        <div><dt>Application ID</dt><dd>{application.id}</dd></div>
      </dl>
      <section className="receipt-evidence" aria-busy={Boolean(downloadingEvidenceId)}>
        <div className="receipt-section-heading"><div><p>EVIDENCE TRAIL</p><h3>What was sent and what happened next</h3></div><span>{orderedEvidence.length} record{orderedEvidence.length === 1 ? "" : "s"}</span></div>
        {downloadError && <div className="evidence-download-error" role="alert">
          <AlertCircle size={15} />
          <span>{downloadError}</span>
        </div>}
        <div className="evidence-list">
          {orderedEvidence.map((item) => <div className="evidence-row" key={item.id}>
            <span className={`evidence-icon ${item.kind}`}>{evidenceIcon(item.kind)}</span>
            <div><b>{item.label || evidenceTitle(item.kind)}</b><p>{evidenceDetail(item, resume)}</p></div>
            <aside className="evidence-row-actions">
              <time>{new Date(item.occurred_at_ms).toLocaleString([], {
                dateStyle: "medium",
                timeStyle: "short",
              })}</time>
              {evidenceDownloadAction(item.kind) && <button
                type="button"
                disabled={Boolean(downloadingEvidenceId)}
                aria-label={`${evidenceDownloadAction(item.kind)} for application ${application.id}`}
                onClick={() => void downloadEvidence(item)}
              >
                <Download size={13} />
                {downloadingEvidenceId === item.id ? "Downloading..." : evidenceDownloadAction(item.kind)}
              </button>}
            </aside>
          </div>)}
          {orderedEvidence.length === 0 && <div className="evidence-empty"><AlertCircle size={18} /><span><b>No evidence attached</b><p>Bluey will not treat future applications as submitted until the exact resume and confirmation are recorded.</p></span></div>}
        </div>
      </section>
    </div>
  );
}

function evidenceDownloadAction(kind: string): string {
  if (kind === "resume") return "Download submitted resume";
  if (kind === "cover_letter") return "Download submitted cover letter";
  if (kind === "attachment") return "Download submitted attachment";
  if (kind === "application_receipt") return "Download receipt JSON";
  if (kind === "submission_confirmation") return "Download confirmation screenshot";
  return "";
}

function evidenceDownloadFileName(item: ApplicationEvidence): string {
  const fallback = item.kind === "resume"
    ? "bluey-submitted-resume.pdf"
    : item.kind === "cover_letter"
      ? "bluey-submitted-cover-letter.pdf"
      : item.kind === "attachment"
        ? "bluey-submitted-attachment.pdf"
        : item.kind === "application_receipt"
          ? "bluey-application-receipt.json"
          : "bluey-submission-confirmation.png";
  return safeDownloadFileName(typeof item.file_name === "string" ? item.file_name : "", fallback);
}

function evidenceIcon(kind: string) {
  if (kind === "status_email") return <Mail size={16} />;
  if (kind === "interview_event") return <CalendarDays size={16} />;
  if (kind === "application_receipt") return <ReceiptText size={16} />;
  if (kind === "submission_confirmation") return <CheckCircle2 size={16} />;
  return <FileText size={16} />;
}

function evidenceTitle(kind: string): string {
  if (kind === "resume") return "Resume attached";
  if (kind === "application_receipt") return "Immutable application receipt";
  if (kind === "status_email") return "Inbox update";
  if (kind === "interview_event") return "Interview scheduled";
  if (kind === "submission_confirmation") return "Application submitted";
  return titleCase(kind);
}

function evidenceDetail(item: ApplicationEvidence, resume?: ResumeVersion): string {
  const metadata = objectValue(item.metadata) || {};
  if (item.kind === "resume") {
    const version = resume && item.resume_version_id === resume.id ? `Resume v${resume.version_no}` : "Job-specific resume";
    const checksum = item.sha256 ? ` · SHA-256 ${item.sha256.slice(0, 10)}…` : "";
    return `${version}${checksum}`;
  }
  if (item.kind === "application_receipt") {
    const receiptId = recordString(metadata, "receipt_id");
    const reference = receiptId ? `Receipt ${receiptId}` : item.file_name || "Application receipt";
    const checksum = item.sha256 ? ` · SHA-256 ${item.sha256.slice(0, 10)}…` : "";
    return `${reference} · Immutable JSON${checksum}`;
  }
  if (item.kind === "status_email") {
    return `${recordString(metadata, "subject") || "Application status message"} · ${providerName(item.provider)}`;
  }
  if (item.kind === "interview_event") {
    return providerName(item.provider);
  }
  if (item.kind === "submission_confirmation") {
    return `${recordString(metadata, "confirmation") || "Application received"} · ${providerName(item.provider)}`;
  }
  return item.file_name || providerName(item.provider);
}

function providerName(provider: string): string {
  return provider ? titleCase(provider.replaceAll("_", " ")) : "Bluey";
}

function ResumePreview({ resume }: { resume: ResumeVersion }) {
  const content = resume.content;
  return <div className="resume-paper"><header><h2>{content.contact?.name || "Candidate"}</h2><p>{[content.contact?.email, content.contact?.phone, content.contact?.location].filter(Boolean).join(" · ")}</p></header><h3>{content.headline || "Professional Summary"}</h3><p>{content.summary}</p><h4>SKILLS</h4><p className="skill-line">{content.skills?.join(" · ")}</p><h4>EXPERIENCE</h4>{content.employment?.map((role) => <div className="resume-role" key={role.id}><div><b>{role.title}</b><span>{role.company}</span></div><small>{role.start_date} - {role.current ? "Present" : role.end_date}</small>{role.highlights.map((highlight) => <p key={highlight}>• {highlight}</p>)}</div>)}</div>;
}

export function applicationNeedsReview(application: JobApplication): boolean {
  return ["awaiting_review", "needs_confirmation", "needs_input", "side_effect_unknown"].includes(
    application.state,
  );
}

export function answerInterventionActionLabel(busy: boolean): string {
  return busy ? "Saving..." : "Save answer for review";
}

export function applicationCountFor(filter: string, applications: JobApplication[]): number {
  if (filter === "submitted") return applications.filter((item) => item.state === "submitted").length;
  if (filter === "review") return applications.filter(applicationNeedsReview).length;
  if (filter === "active") return applications.filter((item) => !["submitted", "failed"].includes(item.state)).length;
  return applications.length;
}

function stateIcon(state: string) {
  if (state === "submitted") return <CheckCircle2 className="success" size={18} />;
  if (["needs_input", "needs_confirmation", "side_effect_unknown"].includes(state)) {
    return <AlertCircle className="warning" size={18} />;
  }
  if (state === "running") return <CircleDot className="accent" size={18} />;
  return <Clock3 size={18} />;
}
