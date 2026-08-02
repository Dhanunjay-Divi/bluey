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
  Search,
  Send,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import type { ApplicationEvidence, CandidateEventInput, Intervention, JobApplication, JobEligibilityDecision, JobPosting, JobsWorkspace, ResumeVersion, RunnerAvailability } from "../types";
import { relativeTime, titleCase } from "../lib/format";
import { Dialog } from "../components/Dialog";
import { InterviewPrepDialog } from "../components/InterviewPrepDialog";
import { exportResumeDocx, exportResumePdf } from "../lib/documents";
import { applicationIssueReasons, applicationIssues, applicationOutcomes, eventActionLabel, latestApplicationOutcome } from "../lib/candidate-events";
import { formatResumeDiffValue, resumeDiffHasValue, resumeDiffLabel } from "../lib/resume-diff";

interface Props {
  workspace: JobsWorkspace;
  resumeVersions: Record<string, ResumeVersion>;
  onUpdate(application: JobApplication, state: string): Promise<void>;
  onCommit(application: JobApplication): Promise<void>;
  onLoadResume(id: string): Promise<ResumeVersion | undefined>;
  onResolveIntervention(intervention: Intervention, action: string, resolution?: { answer?: string; remember?: boolean; scope?: string; scope_id?: string }): Promise<void>;
  onSaveCandidateEvent(event: CandidateEventInput): Promise<unknown>;
}

const stateGroups = [
  ["active", "Active"],
  ["review", "Needs review"],
  ["submitted", "Submitted"],
  ["all", "All"],
] as const;

export function ApplicationsView({ workspace, resumeVersions, onUpdate, onCommit, onLoadResume, onResolveIntervention, onSaveCandidateEvent }: Props) {
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
    if (filter === "review") return ["awaiting_review", "needs_confirmation", "needs_input"].includes(application.state);
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
      setSelected((current) => current ? { ...current, state: "queued", updated_at_ms: Date.now() } : current);
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
        <div className="tab-control">{stateGroups.map(([value, label]) => <button key={value} className={filter === value ? "active" : ""} onClick={() => setFilter(value)}>{label}<span>{countFor(value, workspace.applications)}</span></button>)}</div>
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
                      <button className="button primary compact" disabled={busy || !answer.trim()} onClick={() => void submitInterventionAnswer()}>{busy ? "Saving..." : "Use answer & resume"}<ArrowRight size={15} /></button>
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
            <div className="dialog-actions spread"><p>{selected.state === "submitted" ? "Receipt locked to this exact resume and answer set." : selected.state === "side_effect_unknown" ? "Automatic retry is disabled until the employer-facing outcome is reconciled." : selected.state === "awaiting_review" && !selectedRunnerAvailable ? "Your tailored kit is ready. Download it or continue on the original job site." : "Approving counts this tailored application once. Retries do not double-charge."}</p><div><button className="button secondary" onClick={() => openFeedback(selected, "issue")}><TriangleAlert size={16} />Report problem</button>{selected.state === "submitted" && <button className="button secondary" onClick={() => openFeedback(selected, "outcome")}><CalendarDays size={16} />Update outcome</button>}{selected.state === "needs_input" && !canAnswerIntervention && selectedSession?.takeover_url && <a className="button secondary" href={selectedSession.takeover_url}><MonitorUp size={16} />Take over browser</a>}{selected.state === "needs_input" && !canAnswerIntervention && !selectedSession?.takeover_url && <button className="button secondary" disabled title="A scoped resume link is not available for this run"><MonitorUp size={16} />Takeover unavailable</button>}{selected.state === "awaiting_review" && selectedRunnerAvailable && <button className="button primary" disabled={busy} onClick={() => void update("queued")}><Play size={16} />Approve application</button>}{selected.state === "awaiting_review" && !selectedRunnerAvailable && selectedCanHandoff && <button className="button primary" disabled={busy} onClick={() => void openHandoff()}><Send size={16} />Open job site</button>}{selected.state === "queued" && <a className="button primary" href="/jobs/browser"><Send size={16} />Choose runner</a>}{selected.state === "submitted" && selectedJob && selectedResume && <button className="button primary" onClick={() => { setPrepTarget({ application: selected, job: selectedJob, resume: selectedResume }); setSelected(null); }}><Sparkles size={16} />Prepare interview</button>}{selected.state === "submitted" && <button className="button secondary" onClick={() => setReceiptOpen(true)}><CheckCircle2 size={16} />View receipt</button>}</div></div>
          </div>
        )}
      </Dialog>
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
    <div className="kit-summary">
      <div><b>Resume</b><span>{resume ? `v${resume.version_no} · ${titleCase(resume.mode)}` : "Loading version"}</span></div>
      <div><b>Email</b><span>{email}</span></div>
      <div><b>Answers</b><span>{application.answers.length ? `${application.answers.length} final answer${application.answers.length === 1 ? "" : "s"}` : "No answers required yet"}</span></div>
      <div><b>Cover letter</b><span>{application.cover_letter?.trim() ? "Included" : "Not included"}</span></div>
      <div><b>Site</b><span>{capability}</span></div>
      <div><b>Metering</b><span>{meteringStatus === "counts_when_approved_or_downloaded" || application.state === "awaiting_review" ? "Counts when approved or downloaded" : application.state === "submitted" ? "Counted once" : "Counted once for this job"}</span></div>
    </div>
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

function applicationEligibility(application: JobApplication, job?: JobPosting): JobEligibilityDecision {
  const stored = application.receipt.eligibility;
  if (stored && typeof stored === "object") return stored as unknown as JobEligibilityDecision;
  if (job?.eligibility) return job.eligibility;
  return {
    capability: "unknown_review",
    can_prepare: true,
    can_auto_submit: false,
    can_queue_local: false,
    can_queue_cloud: false,
    hard_failures: [],
    review_reasons: [{ code: "eligibility_pending", message: "Bluey will verify this site and your rules before queueing." }],
    passed_checks: [],
    evaluated_at_ms: 0,
  };
}

export function hasAvailableRunner(
  eligibility: JobEligibilityDecision,
  runners: RunnerAvailability,
): boolean {
  return (eligibility.can_queue_local && runners.local.available)
    || (eligibility.can_queue_cloud && runners.cloud.available);
}

export function runnerUnavailableReason(
  eligibility: JobEligibilityDecision,
  runners: RunnerAvailability,
): string {
  if (eligibility.hard_failures.length > 0) {
    return eligibility.hard_failures[0].message;
  }
  if (eligibility.capability === "beta_review") {
    return "This application system is still in beta. Review the kit and continue on the job site.";
  }
  if (eligibility.capability === "handoff") {
    return "This site requires a user-controlled handoff after Bluey prepares the application kit.";
  }
  if (eligibility.capability === "unknown_review") {
    return "This application system is not certified for a Bluey runner. Review the kit and continue on the job site.";
  }
  if (eligibility.capability === "blocked") {
    return "This listing cannot use a Bluey runner.";
  }
  if (!eligibility.can_queue_local && !eligibility.can_queue_cloud) {
    return eligibility.review_reasons[0]?.message
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

function ReceiptView({ application, resume, evidence }: { application: JobApplication; resume?: ResumeVersion; evidence: ApplicationEvidence[] }) {
  const orderedEvidence = [...evidence].sort((left, right) => right.occurred_at_ms - left.occurred_at_ms);
  const resumeEvidence = orderedEvidence.find((item) => item.kind === "resume" && item.resume_version_id === application.resume_version_id);
  const confirmation = orderedEvidence.find((item) => item.kind === "submission_confirmation");
  const applicationEmail = application.receipt.application_identity && typeof application.receipt.application_identity === "object"
    ? String((application.receipt.application_identity as Record<string, unknown>).email || "")
    : application.receipt.packet && typeof application.receipt.packet === "object"
      ? String((application.receipt.packet as Record<string, unknown>).applicationEmail || "")
      : "";
  return (
    <div className="receipt-view">
      <div className={`receipt-check ${resumeEvidence && confirmation ? "verified" : "warning"}`}>
        {resumeEvidence && confirmation ? <CheckCircle2 size={22} /> : <AlertCircle size={22} />}
        <span><b>{resumeEvidence && confirmation ? "Submission verified" : "Evidence incomplete"}</b><small>{application.submitted_at_ms ? new Date(application.submitted_at_ms).toLocaleString() : "Submission time pending"}</small></span>
      </div>
      <dl>
        <div><dt>Status</dt><dd>{titleCase(application.state)}</dd></div>
        <div><dt>Exact resume</dt><dd>{resumeEvidence?.file_name || (resume ? `Version ${resume.version_no} · ${titleCase(resume.mode)}` : "Evidence missing")}</dd></div>
        {applicationEmail && <div><dt>Application email</dt><dd>{applicationEmail}</dd></div>}
        <div><dt>Application ID</dt><dd>{application.id}</dd></div>
      </dl>
      <section className="receipt-evidence">
        <div className="receipt-section-heading"><div><p>EVIDENCE TRAIL</p><h3>What was sent and what happened next</h3></div><span>{orderedEvidence.length} record{orderedEvidence.length === 1 ? "" : "s"}</span></div>
        <div className="evidence-list">
          {orderedEvidence.map((item) => <div className="evidence-row" key={item.id}>
            <span className={`evidence-icon ${item.kind}`}>{evidenceIcon(item.kind)}</span>
            <div><b>{item.label || evidenceTitle(item.kind)}</b><p>{evidenceDetail(item, resume)}</p></div>
            <time>{new Date(item.occurred_at_ms).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })}</time>
          </div>)}
          {orderedEvidence.length === 0 && <div className="evidence-empty"><AlertCircle size={18} /><span><b>No evidence attached</b><p>Bluey will not treat future applications as submitted until the exact resume and confirmation are recorded.</p></span></div>}
        </div>
      </section>
    </div>
  );
}

function evidenceIcon(kind: string) {
  if (kind === "status_email") return <Mail size={16} />;
  if (kind === "interview_event") return <CalendarDays size={16} />;
  if (kind === "submission_confirmation") return <CheckCircle2 size={16} />;
  return <FileText size={16} />;
}

function evidenceTitle(kind: string): string {
  if (kind === "resume") return "Resume attached";
  if (kind === "status_email") return "Inbox update";
  if (kind === "interview_event") return "Interview scheduled";
  if (kind === "submission_confirmation") return "Application submitted";
  return titleCase(kind);
}

function evidenceDetail(item: ApplicationEvidence, resume?: ResumeVersion): string {
  if (item.kind === "resume") {
    const version = resume && item.resume_version_id === resume.id ? `Resume v${resume.version_no}` : "Job-specific resume";
    const checksum = item.sha256 ? ` · SHA-256 ${item.sha256.slice(0, 10)}…` : "";
    return `${version}${checksum}`;
  }
  if (item.kind === "status_email") {
    return `${String(item.metadata.subject || "Application status message")} · ${providerName(item.provider)}`;
  }
  if (item.kind === "interview_event") {
    return providerName(item.provider);
  }
  if (item.kind === "submission_confirmation") {
    return `${String(item.metadata.confirmation || "Application received")} · ${providerName(item.provider)}`;
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

function countFor(filter: string, applications: JobApplication[]): number {
  if (filter === "submitted") return applications.filter((item) => item.state === "submitted").length;
  if (filter === "review") return applications.filter((item) => ["awaiting_review", "needs_confirmation", "needs_input"].includes(item.state)).length;
  if (filter === "active") return applications.filter((item) => !["submitted", "failed"].includes(item.state)).length;
  return applications.length;
}

function stateIcon(state: string) {
  if (state === "submitted") return <CheckCircle2 className="success" size={18} />;
  if (state === "needs_input" || state === "needs_confirmation") return <AlertCircle className="warning" size={18} />;
  if (state === "running") return <CircleDot className="accent" size={18} />;
  return <Clock3 size={18} />;
}
