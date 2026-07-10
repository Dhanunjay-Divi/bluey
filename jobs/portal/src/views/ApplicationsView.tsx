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
} from "lucide-react";
import type { ApplicationEvidence, Intervention, JobApplication, JobsWorkspace, ResumeVersion } from "../types";
import { relativeTime, titleCase } from "../lib/format";
import { Dialog } from "../components/Dialog";
import { exportResumeDocx, exportResumePdf } from "../lib/documents";

interface Props {
  workspace: JobsWorkspace;
  resumeVersions: Record<string, ResumeVersion>;
  onUpdate(application: JobApplication, state: string): Promise<void>;
  onCommit(application: JobApplication): Promise<void>;
  onLoadResume(id: string): Promise<ResumeVersion | undefined>;
  onResolveIntervention(intervention: Intervention, action: string, resolution?: { answer?: string; remember?: boolean; scope?: string; scope_id?: string }): Promise<void>;
}

const stateGroups = [
  ["active", "Active"],
  ["review", "Needs review"],
  ["submitted", "Submitted"],
  ["all", "All"],
] as const;

export function ApplicationsView({ workspace, resumeVersions, onUpdate, onCommit, onLoadResume, onResolveIntervention }: Props) {
  const [filter, setFilter] = useState<(typeof stateGroups)[number][0]>("active");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<JobApplication | null>(null);
  const [selectedResume, setSelectedResume] = useState<ResumeVersion | undefined>();
  const [receiptOpen, setReceiptOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [answer, setAnswer] = useState("");
  const [rememberAnswer, setRememberAnswer] = useState(true);
  const [answerScope, setAnswerScope] = useState<"account" | "track" | "company">("account");
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
    if (!selected?.resume_version_id) {
      setSelectedResume(undefined);
      return;
    }
    const cached = resumeVersions[selected.resume_version_id];
    if (cached) {
      setSelectedResume(cached);
      return;
    }
    void onLoadResume(selected.resume_version_id).then(setSelectedResume);
  }, [selected, resumeVersions, onLoadResume]);

  useEffect(() => {
    setAnswer("");
    setRememberAnswer(selectedIntervention?.kind !== "sensitive_question");
    setAnswerScope(selectedJob?.track_id ? "track" : "account");
  }, [selectedIntervention?.id, selectedIntervention?.kind, selectedJob?.track_id]);

  const update = async (state: string) => {
    if (!selected) return;
    setBusy(true);
    try {
      await onUpdate(selected, state);
      setSelected((current) => current ? { ...current, state: state as JobApplication["state"] } : current);
    } finally {
      setBusy(false);
    }
  };

  const download = async (format: "pdf" | "docx") => {
    if (!selected || !selectedResume) return;
    setBusy(true);
    try {
      await onCommit(selected);
      const filename = `bluey-${jobs.get(selected.job_id)?.company || "resume"}`;
      if (format === "pdf") await exportResumePdf(selectedResume.content, filename);
      else await exportResumeDocx(selectedResume.content, filename);
    } finally {
      setBusy(false);
    }
  };

  const submitInterventionAnswer = async () => {
    if (!selected || !selectedIntervention || !answer.trim()) return;
    setBusy(true);
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
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="view-shell applications-view">
      <section className="view-heading">
        <div><p className="eyebrow">APPLICATION CONTROL</p><h1>Applications</h1><span>Every packet, browser run, intervention, and receipt in one timeline.</span></div>
        <div className="heading-stat"><b>{workspace.applications.filter((item) => item.state === "submitted").length}</b><span>submitted this month</span></div>
      </section>

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
          return (
            <button key={application.id} className="application-row" onClick={() => setSelected(application)}>
              <div className="company-mark">{(job?.company || "BJ").slice(0, 2).toUpperCase()}</div>
              <div className="application-main"><strong>{job?.title || "Application"}</strong><span>{job?.company || "Unknown company"} · {job?.location || "Location not listed"}</span></div>
              <div className="application-stage">{stateIcon(application.state)}<span><b>{titleCase(application.state)}</b><small>{relativeTime(application.updated_at_ms)}</small></span></div>
              <div className="application-packet"><FileText size={15} /><span>Job-specific resume<small>v1 · {titleCase(application.submission_mode)}</small></span></div>
              <span className="icon-button" aria-hidden="true"><MoreHorizontal size={18} /></span>
            </button>
          );
        })}
        {filtered.length === 0 && <div className="empty-state"><div className="empty-icon"><BriefcaseBusiness /></div><h3>No applications here</h3><p>Build a packet from Matches and it will appear in this timeline.</p></div>}
      </section>

      <Dialog open={Boolean(selected)} title={selected ? `${jobs.get(selected.job_id)?.title || "Application"}` : "Application"} description={selected ? `${jobs.get(selected.job_id)?.company || ""} · ${titleCase(selected.state)}` : ""} onClose={() => setSelected(null)} size="large">
        {selected && (
          <div className="application-detail">
            <div className="application-steps">
              {[
                ["Packet", true],
                ["Review", !["matched", "preparing"].includes(selected.state)],
                ["Apply", ["running", "needs_input", "submitted"].includes(selected.state)],
                ["Receipt", selected.state === "submitted"],
              ].map(([label, complete], index) => <div key={String(label)} className={complete ? "complete" : ""}><span>{complete ? <Check size={13} /> : index + 1}</span><b>{label}</b></div>)}
            </div>
            <div className="application-detail-grid">
              <section className="resume-sheet compact-sheet">
                {selectedResume ? <ResumePreview resume={selectedResume} /> : <div className="resume-loading">Loading job-specific resume...</div>}
              </section>
              <section className="review-panel">
                <div className="review-title"><div><p>PACKET REVIEW</p><h3>What Bluey changed</h3></div><FileDiff size={20} /></div>
                <ul className="diff-list">
                  <li><span>Summary</span><p>Focused the opening on the role's product and engineering scope.</p></li>
                  <li><span>Skills</span><p>Moved job-description matches earlier without adding unsupported skills.</p></li>
                  <li><span>Experience</span><p>Prioritized outcomes closest to this company's requirements.</p></li>
                </ul>
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
                <div className="download-row"><button disabled={busy || !selectedResume} onClick={() => void download("pdf")}><Download size={15} />PDF</button><button disabled={busy || !selectedResume} onClick={() => void download("docx")}><Download size={15} />DOCX</button></div>
              </section>
            </div>
            <div className="dialog-actions spread"><p>{selected.state === "submitted" ? "Receipt locked to this exact resume and answer set." : "Approving counts this unique packet once. Retries do not double-charge."}</p><div>{selected.state === "needs_input" && !canAnswerIntervention && <a className="button secondary" href={selectedSession?.takeover_url || `bluey-jobs://takeover?application_id=${encodeURIComponent(selected.id)}`}><MonitorUp size={16} />Take over browser</a>}{selected.state === "awaiting_review" && <button className="button primary" disabled={busy} onClick={() => void update("queued")}><Play size={16} />Approve packet</button>}{selected.state === "queued" && <a className="button primary" href="/jobs/browser"><Send size={16} />Choose runner</a>}{selected.state === "submitted" && <button className="button secondary" onClick={() => setReceiptOpen(true)}><CheckCircle2 size={16} />View receipt</button>}</div></div>
          </div>
        )}
      </Dialog>
      <Dialog open={receiptOpen} title="Submission receipt" description={selected ? `${jobs.get(selected.job_id)?.company || "Application"} · ${jobs.get(selected.job_id)?.title || ""}` : ""} onClose={() => setReceiptOpen(false)}>
        {selected && <ReceiptView application={selected} resume={selectedResume} evidence={selectedEvidence} />}
      </Dialog>
    </div>
  );
}

function normalizeCompanyKey(company: string): string {
  return company.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function ReceiptView({ application, resume, evidence }: { application: JobApplication; resume?: ResumeVersion; evidence: ApplicationEvidence[] }) {
  const orderedEvidence = [...evidence].sort((left, right) => right.occurred_at_ms - left.occurred_at_ms);
  const resumeEvidence = orderedEvidence.find((item) => item.kind === "resume" && item.resume_version_id === application.resume_version_id);
  const confirmation = orderedEvidence.find((item) => item.kind === "submission_confirmation");
  const applicationEmail = application.receipt.application_identity && typeof application.receipt.application_identity === "object"
    ? String((application.receipt.application_identity as Record<string, unknown>).email || "")
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
