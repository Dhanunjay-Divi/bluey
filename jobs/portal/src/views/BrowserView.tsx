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
  MailCheck,
  MonitorUp,
  PauseCircle,
  Play,
  ShieldCheck,
  WifiOff,
} from "lucide-react";
import type { Intervention, JobApplication, JobsWorkspace } from "../types";
import { isFinalSubmissionReview, runnerEligibleApplications } from "../lib/application-flow";
import { relativeTime, titleCase } from "../lib/format";
import { Dialog } from "../components/Dialog";

export function BrowserView({ workspace, onQueueLocal, onQueueCloud, onUpdateSession, onResolveIntervention }: { workspace: JobsWorkspace; onQueueLocal(application: JobApplication): Promise<void>; onQueueCloud(application: JobApplication): Promise<void>; onUpdateSession(session: JobsWorkspace["browser_sessions"][number], status: string): Promise<void>; onResolveIntervention(intervention: Intervention, action: string): Promise<void> }) {
  const [installOpen, setInstallOpen] = useState(false);
  const [localOpen, setLocalOpen] = useState(false);
  const [cloudOpen, setCloudOpen] = useState(false);
  const [queueing, setQueueing] = useState("");
  const [resolving, setResolving] = useState(false);
  const [approvalOpen, setApprovalOpen] = useState(false);
  const [reviewConfirmed, setReviewConfirmed] = useState(false);
  const active = workspace.browser_sessions.find((session) => !["complete", "failed"].includes(session.status));
  const intervention = active ? workspace.interventions.find((item) => item.application_id === active.application_id && item.status === "open") : undefined;
  const emailCodeReady = intervention?.resolution_kind === "email_otp_approval"
    && (!intervention.expires_at_ms || intervention.expires_at_ms > Date.now());
  const finalSubmissionReview = isFinalSubmissionReview(intervention);
  const takeoverUrl = active?.takeover_url || (active ? `bluey-jobs://takeover?session_id=${encodeURIComponent(active.id)}` : "#");
  const queuedApplications = runnerEligibleApplications(workspace.applications);

  return (
    <div className="view-shell browser-view">
      <section className="view-heading">
        <div><p className="eyebrow">APPLICATION RUNNERS</p><h1>Browser</h1><span>Use the same tailored application locally or in Bluey's isolated cloud runner.</span></div>
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
            <div className="run-actions">
              {finalSubmissionReview && intervention ? <>
                <a className="button secondary compact" href={takeoverUrl}><MonitorUp size={15} />Review form</a>
                <button className="button primary" disabled={resolving} onClick={() => { setReviewConfirmed(false); setApprovalOpen(true); }}><ShieldCheck size={16} />Approve submission</button>
              </> : emailCodeReady && intervention
                ? <><button className="button primary" disabled={resolving} onClick={() => { setResolving(true); void onResolveIntervention(intervention, "approve_email_otp").finally(() => setResolving(false)); }}><MailCheck size={16} />{resolving ? "Approving..." : "Use email code"}</button><a className="button secondary compact" href={takeoverUrl}><MonitorUp size={15} />Take over</a></>
                : <a className="button primary" href={takeoverUrl}><MonitorUp size={16} />Take over</a>}
              <button className="icon-button" title={active.status === "paused" ? "Resume run" : "Pause run"} onClick={() => void onUpdateSession(active, active.status === "paused" ? "queued" : "paused")}>{active.status === "paused" ? <Play size={18} /> : <PauseCircle size={18} />}</button>
            </div>
          </div>
        </section>
      )}

      <section className="runner-grid">
        <article className={`runner-option ${workspace.entitlement.local_browser ? "enabled" : "locked"}`}>
          <div className="runner-icon"><Laptop /></div>
          <div className="runner-copy"><p>LOCAL</p><h2>Bluey Browser</h2><span>Applications run on your computer in a separate Jobs browser. Take over at any moment.</span><ul><li><Check size={14} />Keeps your job-site sign-ins ready</li><li><Check size={14} />Uses the same application flow as cloud</li><li><Check size={14} />Stops when your computer is off</li></ul></div>
          <div className="runner-action"><b>{workspace.entitlement.local_browser ? "Included" : "Pro"}</b>{workspace.entitlement.local_browser ? <><button className="button primary" onClick={() => setLocalOpen(true)}>Run locally<ArrowRight size={16} /></button><button className="button secondary compact" onClick={() => setInstallOpen(true)}>Set up browser</button></> : <a className="button secondary" href="/jobs/settings#plans">See Pro<ArrowRight size={16} /></a>}</div>
        </article>
        <article className={`runner-option ${workspace.entitlement.cloud_browser ? "enabled" : "locked"}`}>
          <div className="runner-icon cloud"><Cloud /></div>
          <div className="runner-copy"><p>CLOUD</p><h2>Background runner</h2><span>Bluey continues from an encrypted, isolated browser profile while your computer is off.</span><ul><li><Check size={14} />Offers one-click email-code approval</li><li><Check size={14} />Pauses for security checks and user handoff</li><li><Check size={14} />Stores an evidence-backed submission receipt</li></ul></div>
          <div className="runner-action"><b>{workspace.entitlement.cloud_browser ? "Included" : "Cloud"}</b>{workspace.entitlement.cloud_browser ? <button className="button primary" onClick={() => setCloudOpen(true)}>Queue a run<ArrowRight size={16} /></button> : <a className="button secondary" href="/jobs/settings#plans">See Cloud<ArrowRight size={16} /></a>}</div>
        </article>
      </section>

      <section className="adapter-section">
        <div className="section-heading"><div><p>SUPPORTED APPLICATIONS</p><h2>Built for employer application systems</h2><span>Bluey recognizes the application flow and carries the right resume and answers through every step.</span></div></div>
        <div className="adapter-list">{["Workday", "Greenhouse", "Lever", "Ashby", "SmartRecruiters"].map((name) => <div key={name}><span>{name.slice(0, 1)}</span><b>{name}</b><small>Beta · review required</small></div>)}</div>
        <div className="handoff-policy"><WifiOff size={18} /><div><b>LinkedIn and Indeed use handoff</b><p>Bluey prepares the tailored resume and answers, opens the listing, and lets you complete submission there.</p></div></div>
      </section>

      <Dialog open={installOpen} title="Open Bluey Browser" description="A separate application profile keeps job-site sessions away from your everyday browser." onClose={() => setInstallOpen(false)}>
        <div className="launch-steps"><div><span>1</span><p><b>Install Bluey Browser</b><small>Available for macOS and Windows during Jobs beta.</small></p><a className="button secondary compact" href="/download">Download<ExternalLink size={14} /></a></div><div><span>2</span><p><b>Sign in with Bluey</b><small>The browser links to this Jobs workspace and stores its profile locally.</small></p></div><div><span>3</span><p><b>Start from Applications</b><small>Review an application, then choose the local runner.</small></p></div></div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setInstallOpen(false)}>Close</button><a className="button primary" href="bluey-jobs://open"><Chrome size={16} />Open Bluey Browser</a></div>
      </Dialog>

      <Dialog open={cloudOpen} title="Queue a cloud application" description="Cloud runs start only after a packet is approved or marked Auto-submit eligible." onClose={() => setCloudOpen(false)}>
        <div className="cloud-queue-list">{queuedApplications.map((application) => { const job = workspace.matches.find((item) => item.id === application.job_id); return <button key={application.id} disabled={Boolean(queueing)} onClick={() => { setQueueing(application.id); void onQueueCloud(application).then(() => setCloudOpen(false)).finally(() => setQueueing("")); }}><div className="company-mark">{job?.company.slice(0, 2).toUpperCase()}</div><span><b>{job?.title}</b><small>{job?.company} · {application.match_score}% match</small></span>{queueing === application.id ? <small>Queuing...</small> : <Play size={17} />}</button>; })}{queuedApplications.length === 0 && <div className="empty-state small"><KeyRound /><h3>No queued applications</h3><p>Approve a packet in Applications first.</p></div>}</div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setCloudOpen(false)}>Close</button><a className="button primary" href="/jobs/applications">Go to Applications<ArrowRight size={16} /></a></div>
      </Dialog>

      <Dialog open={localOpen} title="Open a local application" description="Choose a reviewed application. Bluey opens the frozen resume and answers in your isolated browser profile." onClose={() => setLocalOpen(false)}>
        <div className="cloud-queue-list">{queuedApplications.map((application) => { const job = workspace.matches.find((item) => item.id === application.job_id); return <button key={application.id} disabled={Boolean(queueing)} onClick={() => { setQueueing(application.id); void onQueueLocal(application).then(() => setLocalOpen(false)).finally(() => setQueueing("")); }}><div className="company-mark">{job?.company.slice(0, 2).toUpperCase()}</div><span><b>{job?.title}</b><small>{job?.company} · {application.match_score}% match</small></span>{queueing === application.id ? <small>Opening...</small> : <Play size={17} />}</button>; })}{queuedApplications.length === 0 && <div className="empty-state small"><KeyRound /><h3>No queued applications</h3><p>Approve a packet in Applications first.</p></div>}</div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setLocalOpen(false)}>Close</button><button className="button secondary" onClick={() => { setLocalOpen(false); setInstallOpen(true); }}>Set up Bluey Browser</button></div>
      </Dialog>

      <Dialog open={approvalOpen} title="Approve this submission?" description="This is the final checkpoint before Bluey clicks the employer's Submit button." onClose={() => { if (!resolving) setApprovalOpen(false); }}>
        <div className="submission-approval">
          <ShieldCheck size={22} />
          <div><h3>Confirm the preserved form</h3><p>Check every employer-facing answer, attachment, contact detail, and consent on the live form before continuing.</p></div>
          <label><input type="checkbox" checked={reviewConfirmed} onChange={(event) => setReviewConfirmed(event.target.checked)} /><span>I reviewed the final application and want Bluey to submit it.</span></label>
        </div>
        <div className="dialog-actions"><button className="button secondary" disabled={resolving} onClick={() => setApprovalOpen(false)}>Cancel</button><button className="button primary" disabled={!reviewConfirmed || resolving || !intervention} onClick={() => { if (!intervention) return; setResolving(true); void onResolveIntervention(intervention, "approve_submission").then(() => setApprovalOpen(false)).finally(() => setResolving(false)); }}><ShieldCheck size={16} />{resolving ? "Submitting..." : "Approve and submit"}</button></div>
      </Dialog>
    </div>
  );
}
