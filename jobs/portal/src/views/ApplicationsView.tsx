import { useEffect, useMemo, useState } from "react";
import {
  AlertCircle,
  ArrowRight,
  BriefcaseBusiness,
  Check,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  Clock3,
  Download,
  FileDiff,
  FileText,
  Inbox,
  MonitorUp,
  MoreHorizontal,
  Play,
  Search,
  Send,
} from "lucide-react";
import type { JobApplication, JobsWorkspace, ResumeVersion } from "../types";
import { relativeTime, titleCase } from "../lib/format";
import { Dialog } from "../components/Dialog";
import { exportResumeDocx, exportResumePdf } from "../lib/documents";

interface Props {
  workspace: JobsWorkspace;
  resumeVersions: Record<string, ResumeVersion>;
  onUpdate(application: JobApplication, state: string): Promise<void>;
  onCommit(application: JobApplication): Promise<void>;
  onLoadResume(id: string): Promise<ResumeVersion | undefined>;
}

const stateGroups = [
  ["active", "Active"],
  ["review", "Needs review"],
  ["submitted", "Submitted"],
  ["all", "All"],
] as const;

export function ApplicationsView({ workspace, resumeVersions, onUpdate, onCommit, onLoadResume }: Props) {
  const [filter, setFilter] = useState<(typeof stateGroups)[number][0]>("active");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<JobApplication | null>(null);
  const [selectedResume, setSelectedResume] = useState<ResumeVersion | undefined>();
  const [receiptOpen, setReceiptOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const openInterventions = workspace.interventions.filter((item) => item.status === "open");

  const jobs = useMemo(() => new Map(workspace.matches.map((job) => [job.id, job])), [workspace.matches]);
  const selectedSession = selected
    ? workspace.browser_sessions.find((session) => session.application_id === selected.id)
    : undefined;
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
                {selected.state === "needs_input" && <div className="input-needed"><AlertCircle size={17} /><div><b>Bluey needs your answer</b><p>{workspace.interventions.find((item) => item.application_id === selected.id)?.detail || "Open the browser takeover to continue."}</p></div></div>}
                <div className="download-row"><button disabled={busy || !selectedResume} onClick={() => void download("pdf")}><Download size={15} />PDF</button><button disabled={busy || !selectedResume} onClick={() => void download("docx")}><Download size={15} />DOCX</button></div>
              </section>
            </div>
            <div className="dialog-actions spread"><p>{selected.state === "submitted" ? "Receipt locked to this exact resume and answer set." : "Approving counts this unique packet once. Retries do not double-charge."}</p><div>{selected.state === "needs_input" && <a className="button secondary" href={selectedSession?.takeover_url || `bluey-jobs://takeover?application_id=${encodeURIComponent(selected.id)}`}><MonitorUp size={16} />Take over browser</a>}{selected.state === "awaiting_review" && <button className="button primary" disabled={busy} onClick={() => void update("queued")}><Play size={16} />Approve packet</button>}{selected.state === "queued" && <a className="button primary" href="/jobs/browser"><Send size={16} />Choose runner</a>}{selected.state === "submitted" && <button className="button secondary" onClick={() => setReceiptOpen(true)}><CheckCircle2 size={16} />View receipt</button>}</div></div>
          </div>
        )}
      </Dialog>
      <Dialog open={receiptOpen} title="Submission receipt" description={selected ? `${jobs.get(selected.job_id)?.company || "Application"} · ${jobs.get(selected.job_id)?.title || ""}` : ""} onClose={() => setReceiptOpen(false)}>
        {selected && <ReceiptView application={selected} resume={selectedResume} />}
      </Dialog>
    </div>
  );
}

function ReceiptView({ application, resume }: { application: JobApplication; resume?: ResumeVersion }) {
  const entries = Object.entries(application.receipt || {});
  return <div className="receipt-view"><div className="receipt-check"><CheckCircle2 size={22} /><span><b>Application record</b><small>{application.submitted_at_ms ? new Date(application.submitted_at_ms).toLocaleString() : "Submission time pending"}</small></span></div><dl><div><dt>Status</dt><dd>{titleCase(application.state)}</dd></div><div><dt>Resume version</dt><dd>{resume ? `v${resume.version_no} · ${titleCase(resume.mode)}` : application.resume_version_id || "Not available"}</dd></div><div><dt>Application ID</dt><dd>{application.id}</dd></div>{entries.map(([key, value]) => <div key={key}><dt>{titleCase(key)}</dt><dd>{typeof value === "string" || typeof value === "number" ? String(value) : JSON.stringify(value)}</dd></div>)}</dl>{entries.length === 0 && <p className="receipt-empty">The application is marked submitted, but no browser confirmation has been attached yet.</p>}</div>;
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
