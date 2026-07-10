import { useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  Check,
  Chrome,
  Cloud,
  ExternalLink,
  KeyRound,
  Laptop,
  LockKeyhole,
  MonitorUp,
  PauseCircle,
  Play,
  ShieldCheck,
  WifiOff,
} from "lucide-react";
import type { JobApplication, JobsWorkspace } from "../types";
import { relativeTime, titleCase } from "../lib/format";
import { Dialog } from "../components/Dialog";

export function BrowserView({ workspace, onQueueCloud, onUpdateSession }: { workspace: JobsWorkspace; onQueueCloud(application: JobApplication): Promise<void>; onUpdateSession(session: JobsWorkspace["browser_sessions"][number], status: string): Promise<void> }) {
  const [installOpen, setInstallOpen] = useState(false);
  const [cloudOpen, setCloudOpen] = useState(false);
  const [queueing, setQueueing] = useState("");
  const active = workspace.browser_sessions.find((session) => !["complete", "failed"].includes(session.status));
  const intervention = active ? workspace.interventions.find((item) => item.application_id === active.application_id && item.status === "open") : undefined;

  return (
    <div className="view-shell browser-view">
      <section className="view-heading">
        <div><p className="eyebrow">APPLICATION RUNNERS</p><h1>Browser</h1><span>Use the same application packet locally or in Bluey's isolated cloud runner.</span></div>
        <div className="security-note"><ShieldCheck size={17} /><span><b>Separate and private</b><small>Your job-site sign-ins stay inside the Jobs browser.</small></span></div>
      </section>

      {active && (
        <section className="active-browser-run">
          <div className="browser-window-preview">
            <header><span /><span /><span /><div><LockKeyhole size={13} />jobs.smartrecruiters.com</div></header>
            <main><div className="browser-company">{active.current_company.slice(0, 2).toUpperCase()}</div><span><p>APPLICATION IN PROGRESS</p><h2>{active.current_company}</h2><b>{active.current_step}</b></span></main>
            <footer><div><span style={{ width: active.status === "needs_input" ? "68%" : "42%" }} /></div><small>{titleCase(active.status)}</small></footer>
          </div>
          <div className="run-control">
            <p>ACTIVE {active.runner.toUpperCase()} RUN</p>
            <h2>{active.current_company}</h2>
            <span>{active.current_step}</span>
            {intervention && <div className="run-alert"><AlertTriangle size={17} /><div><b>{intervention.title}</b><p>{intervention.detail}</p></div></div>}
            <div><a className="button primary" href={active.takeover_url || `bluey-jobs://takeover?session_id=${encodeURIComponent(active.id)}`}><MonitorUp size={16} />Take over</a><button className="icon-button" title={active.status === "paused" ? "Resume run" : "Pause run"} onClick={() => void onUpdateSession(active, active.status === "paused" ? "queued" : "paused")}>{active.status === "paused" ? <Play size={18} /> : <PauseCircle size={18} />}</button></div>
          </div>
        </section>
      )}

      <section className="runner-grid">
        <article className={`runner-option ${workspace.entitlement.local_browser ? "enabled" : "locked"}`}>
          <div className="runner-icon"><Laptop /></div>
          <div className="runner-copy"><p>LOCAL</p><h2>Bluey Browser</h2><span>Applications run on your computer in a separate Jobs browser. Take over at any moment.</span><ul><li><Check size={14} />Keeps your job-site sign-ins ready</li><li><Check size={14} />Uses the same application flow as cloud</li><li><Check size={14} />Stops when your computer is off</li></ul></div>
          <div className="runner-action"><b>{workspace.entitlement.local_browser ? "Included" : "Pro"}</b>{workspace.entitlement.local_browser ? <button className="button primary" onClick={() => setInstallOpen(true)}>Open browser<ArrowRight size={16} /></button> : <a className="button secondary" href="/jobs/settings#plans">See Pro<ArrowRight size={16} /></a>}</div>
        </article>
        <article className={`runner-option ${workspace.entitlement.cloud_browser ? "enabled" : "locked"}`}>
          <div className="runner-icon cloud"><Cloud /></div>
          <div className="runner-copy"><p>CLOUD</p><h2>Background runner</h2><span>Bluey continues from an encrypted, isolated browser profile while your computer is off.</span><ul><li><Check size={14} />Runs from the application queue</li><li><Check size={14} />Pauses for CAPTCHA, 2FA, or unknown answers</li><li><Check size={14} />Stores an exact submission receipt</li></ul></div>
          <div className="runner-action"><b>{workspace.entitlement.cloud_browser ? "Included" : "Cloud"}</b>{workspace.entitlement.cloud_browser ? <button className="button primary" onClick={() => setCloudOpen(true)}>Queue a run<ArrowRight size={16} /></button> : <a className="button secondary" href="/jobs/settings#plans">See Cloud<ArrowRight size={16} /></a>}</div>
        </article>
      </section>

      <section className="adapter-section">
        <div className="section-heading"><div><p>SUPPORTED APPLICATIONS</p><h2>Built for employer application systems</h2><span>Bluey recognizes the application flow and carries the right resume and answers through every step.</span></div></div>
        <div className="adapter-list">{["Workday", "Greenhouse", "Lever", "Ashby", "SmartRecruiters"].map((name) => <div key={name}><span>{name.slice(0, 1)}</span><b>{name}</b><small>Beta</small></div>)}</div>
        <div className="handoff-policy"><WifiOff size={18} /><div><b>LinkedIn and Indeed use handoff</b><p>Bluey prepares the tailored resume and answers, opens the listing, and lets you complete submission there.</p></div></div>
      </section>

      <Dialog open={installOpen} title="Open Bluey Browser" description="A separate application profile keeps job-site sessions away from your everyday browser." onClose={() => setInstallOpen(false)}>
        <div className="launch-steps"><div><span>1</span><p><b>Install Bluey Browser</b><small>Available for macOS and Windows during Jobs beta.</small></p><a className="button secondary compact" href="/download">Download<ExternalLink size={14} /></a></div><div><span>2</span><p><b>Sign in with Bluey</b><small>The browser links to this Jobs workspace and stores its profile locally.</small></p></div><div><span>3</span><p><b>Start from Applications</b><small>Review a packet, then choose the local runner.</small></p></div></div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setInstallOpen(false)}>Close</button><a className="button primary" href="bluey-jobs://open"><Chrome size={16} />Open Bluey Browser</a></div>
      </Dialog>

      <Dialog open={cloudOpen} title="Queue a cloud application" description="Cloud runs always start from a reviewed or Auto-submit-eligible application packet." onClose={() => setCloudOpen(false)}>
        <div className="cloud-queue-list">{workspace.applications.filter((item) => ["awaiting_review", "queued"].includes(item.state)).map((application) => { const job = workspace.matches.find((item) => item.id === application.job_id); return <button key={application.id} disabled={Boolean(queueing)} onClick={() => { setQueueing(application.id); void onQueueCloud(application).then(() => setCloudOpen(false)).finally(() => setQueueing("")); }}><div className="company-mark">{job?.company.slice(0, 2).toUpperCase()}</div><span><b>{job?.title}</b><small>{job?.company} · {application.match_score}% match</small></span>{queueing === application.id ? <small>Queuing...</small> : <Play size={17} />}</button>; })}{workspace.applications.filter((item) => ["awaiting_review", "queued"].includes(item.state)).length === 0 && <div className="empty-state small"><KeyRound /><h3>No eligible packets</h3><p>Review a packet in Applications first.</p></div>}</div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setCloudOpen(false)}>Close</button><a className="button primary" href="/jobs/applications">Go to Applications<ArrowRight size={16} /></a></div>
      </Dialog>
    </div>
  );
}
