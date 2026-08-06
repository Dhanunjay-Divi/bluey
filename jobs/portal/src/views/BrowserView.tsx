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
import type {
  Intervention,
  JobApplication,
  JobsWorkspace,
  LocalBrowserReleaseArchitecture,
  RunnerChannelAvailability,
} from "../types";
import { isFinalSubmissionReview, runnerEligibleApplications } from "../lib/application-flow";
import {
  detectLocalBrowserTarget,
  exactByteSize,
  localBrowserReleaseAuthorized,
  localBrowserReleasePresentation,
  selectMacBrowserArchitecture,
  targetLabel,
  type LocalBrowserClientTarget,
} from "../lib/browser-release";
import { titleCase } from "../lib/format";
import { cloudRunnerAccessCopy, localRunnerAccessCopy } from "../lib/runner-access";
import { Dialog } from "../components/Dialog";

interface BrowserViewProps {
  workspace: JobsWorkspace;
  browserTarget?: LocalBrowserClientTarget;
  onQueueLocal(application: JobApplication): Promise<void>;
  onQueueCloud(application: JobApplication): Promise<void>;
  onUpdateSession(
    session: JobsWorkspace["browser_sessions"][number],
    status: string,
  ): Promise<void>;
  onResolveIntervention(intervention: Intervention, action: string): Promise<void>;
}

export function BrowserView({
  workspace,
  browserTarget,
  onQueueLocal,
  onQueueCloud,
  onUpdateSession,
  onResolveIntervention,
}: BrowserViewProps) {
  const [installOpen, setInstallOpen] = useState(false);
  const [localOpen, setLocalOpen] = useState(false);
  const [cloudOpen, setCloudOpen] = useState(false);
  const [queueing, setQueueing] = useState("");
  const [resolving, setResolving] = useState(false);
  const [sessionBusy, setSessionBusy] = useState(false);
  const [localError, setLocalError] = useState("");
  const [approvalOpen, setApprovalOpen] = useState(false);
  const [reviewConfirmed, setReviewConfirmed] = useState(false);
  const active = workspace.browser_sessions.find((session) => !["complete", "failed"].includes(session.status));
  const intervention = active ? workspace.interventions.find((item) => item.application_id === active.application_id && item.status === "open") : undefined;
  const emailCodeReady = intervention?.resolution_kind === "email_otp_approval"
    && (!intervention.expires_at_ms || intervention.expires_at_ms > Date.now());
  const finalSubmissionReview = isFinalSubmissionReview(intervention);
  const takeoverUrl = active?.takeover_url;
  const queuedApplications = runnerEligibleApplications(workspace.applications);
  const localAccess = workspace.runner_availability.local;
  const cloudAccess = workspace.runner_availability.cloud;
  const localTarget = browserTarget ?? detectLocalBrowserTarget();
  const localRelease = localBrowserReleasePresentation(localAccess, localTarget);
  const localReleaseAvailable = localBrowserReleaseAuthorized(localAccess, localTarget);
  const localCopy = localRunnerAccessCopy(localAccess, {
    available: localReleaseAvailable,
    reason: localReleaseAvailable || localRelease.status === "available"
      ? localAccess.reason
      : localRelease.message,
  });
  const cloudCopy = cloudRunnerAccessCopy(cloudAccess);

  const updateActiveSession = async () => {
    if (!active || sessionBusy) return;
    setSessionBusy(true);
    setLocalError("");
    try {
      await onUpdateSession(active, active.status === "paused" ? "queued" : "paused");
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setSessionBusy(false);
    }
  };

  const resolve = async (item: Intervention, action: string, closeApproval = false) => {
    setResolving(true);
    setLocalError("");
    try {
      await onResolveIntervention(item, action);
      if (closeApproval) setApprovalOpen(false);
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setResolving(false);
    }
  };

  const queue = async (application: JobApplication, runner: "local" | "cloud") => {
    if (runner === "local" && !localReleaseAvailable) {
      setLocalError(
        localRelease.status === "hidden"
          ? localRelease.message
          : "Bluey Browser is not available for this computer.",
      );
      return;
    }
    setQueueing(application.id);
    setLocalError("");
    try {
      if (runner === "local") {
        await onQueueLocal(application);
        setLocalOpen(false);
      } else {
        await onQueueCloud(application);
        setCloudOpen(false);
      }
    } catch (cause) {
      setLocalError(errorMessage(cause));
    } finally {
      setQueueing("");
    }
  };

  return (
    <div className="view-shell browser-view">
      <section className="view-heading">
        <div><p className="eyebrow">APPLICATION RUNNERS</p><h1>Browser</h1><span>Use the same tailored application locally or in Bluey's isolated cloud runner.</span></div>
        <div className="security-note"><ShieldCheck size={17} /><span><b>Separate and private</b><small>Your job-site sign-ins stay inside the Jobs browser.</small></span></div>
      </section>

      {localError && <div className="inline-error" role="alert">{localError}</div>}

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
                <TakeoverAction url={takeoverUrl} label="Review form" compact />
                <button className="button primary" disabled={resolving} onClick={() => { setReviewConfirmed(false); setApprovalOpen(true); }}><ShieldCheck size={16} />Approve submission</button>
              </> : emailCodeReady && intervention
                ? <><button className="button primary" disabled={resolving} onClick={() => void resolve(intervention, "approve_email_otp")}><MailCheck size={16} />{resolving ? "Approving..." : "Use email code"}</button><TakeoverAction url={takeoverUrl} label="Take over" compact /></>
                : <TakeoverAction url={takeoverUrl} label="Take over" primary />}
              <button className="icon-button" disabled={sessionBusy} title={active.status === "paused" ? "Resume run" : "Pause run"} onClick={() => void updateActiveSession()}>{active.status === "paused" ? <Play size={18} /> : <PauseCircle size={18} />}</button>
            </div>
          </div>
        </section>
      )}

      <section className="runner-grid">
        <article className={`runner-option ${localReleaseAvailable ? "enabled" : "locked"}`}>
          <div className="runner-icon"><Laptop /></div>
          <div className="runner-copy">
            <p>LOCAL</p>
            <h2>Bluey Browser</h2>
            <span>{localCopy.description}</span>
            <ul>
              {localCopy.points.map((point) => (
                <li key={point}><Check size={14} />{point}</li>
              ))}
            </ul>
          </div>
          <div className="runner-action">
            <b>{localCopy.badge}</b>
            {localReleaseAvailable ? (
              <>
                <button className="button primary" onClick={() => setLocalOpen(true)}>
                  {localCopy.action}<ArrowRight size={16} />
                </button>
                <button
                  className="button secondary compact"
                  onClick={() => setInstallOpen(true)}
                >
                  Set up browser
                </button>
              </>
            ) : (
              <a
                className="button secondary"
                href={
                  localAccess.status === "upgrade_required"
                    ? "/jobs/settings#plans"
                    : "/jobs/applications"
                }
              >
                {localCopy.action}<ArrowRight size={16} />
              </a>
            )}
          </div>
        </article>
        <article className={`runner-option ${cloudAccess.available ? "enabled" : "locked"}`}>
          <div className="runner-icon cloud"><Cloud /></div>
          <div className="runner-copy"><p>CLOUD</p><h2>Background runner</h2><span>{cloudCopy.description}</span><ul>{cloudCopy.points.map((point) => <li key={point}><Check size={14} />{point}</li>)}</ul></div>
          <div className="runner-action"><b>{cloudCopy.badge}</b>{cloudAccess.available ? <button className="button primary" onClick={() => setCloudOpen(true)}>{cloudCopy.action}<ArrowRight size={16} /></button> : <a className="button secondary" href={cloudAccess.status === "upgrade_required" ? "/jobs/settings#plans" : "/jobs/applications"}>{cloudCopy.action}<ArrowRight size={16} /></a>}</div>
        </article>
      </section>

      <section className="adapter-section">
        <div className="section-heading"><div><p>SUPPORTED APPLICATIONS</p><h2>Built for employer application systems</h2><span>Bluey recognizes the application flow and carries the right resume and answers through every step.</span></div></div>
        <div className="adapter-list">{["Workday", "Greenhouse", "Lever", "Ashby", "SmartRecruiters"].map((name) => <div key={name}><span>{name.slice(0, 1)}</span><b>{name}</b><small>Beta · review required</small></div>)}</div>
        <div className="handoff-policy"><WifiOff size={18} /><div><b>LinkedIn and Indeed use handoff</b><p>Bluey prepares the tailored resume and answers, opens the listing, and lets you complete submission there.</p></div></div>
      </section>

      <Dialog open={installOpen} title="Open Bluey Browser" description="A separate application profile keeps job-site sessions away from your everyday browser." onClose={() => setInstallOpen(false)}>
        <BrowserInstallContent
          access={localAccess}
          target={localTarget}
          onClose={() => setInstallOpen(false)}
        />
      </Dialog>

      <Dialog open={cloudOpen} title="Queue a cloud application" description="Cloud runs start only after a packet is approved or marked Auto-submit eligible." onClose={() => setCloudOpen(false)}>
        <div className="cloud-queue-list">{queuedApplications.map((application) => { const job = workspace.matches.find((item) => item.id === application.job_id); return <button key={application.id} disabled={Boolean(queueing)} onClick={() => void queue(application, "cloud")}><div className="company-mark">{job?.company.slice(0, 2).toUpperCase()}</div><span><b>{job?.title}</b><small>{job?.company} · {application.match_score}% match</small></span>{queueing === application.id ? <small>Queuing...</small> : <Play size={17} />}</button>; })}{queuedApplications.length === 0 && <div className="empty-state small"><KeyRound /><h3>No queued applications</h3><p>Approve a packet in Applications first.</p></div>}</div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setCloudOpen(false)}>Close</button><a className="button primary" href="/jobs/applications">Go to Applications<ArrowRight size={16} /></a></div>
      </Dialog>

      <Dialog open={localOpen} title="Open a local application" description="Choose a reviewed application. Bluey opens the frozen resume and answers in your isolated browser profile." onClose={() => setLocalOpen(false)}>
        <div className="cloud-queue-list">{queuedApplications.map((application) => { const job = workspace.matches.find((item) => item.id === application.job_id); return <button key={application.id} disabled={Boolean(queueing)} onClick={() => void queue(application, "local")}><div className="company-mark">{job?.company.slice(0, 2).toUpperCase()}</div><span><b>{job?.title}</b><small>{job?.company} · {application.match_score}% match</small></span>{queueing === application.id ? <small>Opening...</small> : <Play size={17} />}</button>; })}{queuedApplications.length === 0 && <div className="empty-state small"><KeyRound /><h3>No queued applications</h3><p>Approve a packet in Applications first.</p></div>}</div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setLocalOpen(false)}>Close</button><button className="button secondary" onClick={() => { setLocalOpen(false); setInstallOpen(true); }}>Set up Bluey Browser</button></div>
      </Dialog>

      <Dialog open={approvalOpen} title="Approve this submission?" description="This is the final checkpoint before Bluey clicks the employer's Submit button." onClose={() => { if (!resolving) setApprovalOpen(false); }}>
        <div className="submission-approval">
          <ShieldCheck size={22} />
          <div><h3>Confirm the preserved form</h3><p>Check every employer-facing answer, attachment, contact detail, and consent on the live form before continuing.</p></div>
          <label><input type="checkbox" checked={reviewConfirmed} onChange={(event) => setReviewConfirmed(event.target.checked)} /><span>I reviewed the final application and want Bluey to submit it.</span></label>
        </div>
        <div className="dialog-actions"><button className="button secondary" disabled={resolving} onClick={() => setApprovalOpen(false)}>Cancel</button><button className="button primary" disabled={!reviewConfirmed || resolving || !intervention} onClick={() => { if (intervention) void resolve(intervention, "approve_submission", true); }}><ShieldCheck size={16} />{resolving ? "Submitting..." : "Approve and submit"}</button></div>
      </Dialog>
    </div>
  );
}

interface BrowserInstallContentProps {
  access: RunnerChannelAvailability;
  target: LocalBrowserClientTarget;
  onClose(): void;
}

export function BrowserInstallContent({
  access,
  target,
  onClose,
}: BrowserInstallContentProps) {
  const [selectedMacArchitecture, setSelectedMacArchitecture] =
    useState<LocalBrowserReleaseArchitecture | null>(null);
  const needsMacArchitectureChoice =
    target.platform === "macos" && target.architecture === "unknown";
  const selectedTarget: LocalBrowserClientTarget =
    needsMacArchitectureChoice && selectedMacArchitecture
      ? selectMacBrowserArchitecture(target, selectedMacArchitecture)
      : target;
  const presentation = localBrowserReleasePresentation(access, selectedTarget);

  return (
    <>
      <div className="launch-steps" data-release-status={presentation.status}>
        <div>
          <span>1</span>
          <p>
            <b>Install Bluey Browser</b>
            {presentation.status === "available" ? (
              <>
                <small>
                  Version: {presentation.release.app_version} · Channel: {presentation.release.channel}
                </small>
                <small>
                  {presentation.artifact.file_name} · {exactByteSize(presentation.artifact.size_bytes)}
                </small>
                <small>SHA-256: {presentation.artifact.sha256}</small>
              </>
            ) : (
              <small role="status">{presentation.message}</small>
            )}
            {needsMacArchitectureChoice && (
              <small>
                Check About This Mac: Apple silicon says M1 or newer; Intel models say Intel.
              </small>
            )}
          </p>
          {needsMacArchitectureChoice && (
            <div
              aria-label="Choose your Mac processor"
              className="browser-architecture-choice"
              role="group"
            >
              <button
                aria-pressed={selectedMacArchitecture === "arm64"}
                className="button secondary compact"
                data-browser-architecture="arm64"
                type="button"
                onClick={() => setSelectedMacArchitecture("arm64")}
              >
                Apple silicon
              </button>
              <button
                aria-pressed={selectedMacArchitecture === "x64"}
                className="button secondary compact"
                data-browser-architecture="x64"
                type="button"
                onClick={() => setSelectedMacArchitecture("x64")}
              >
                Intel Mac
              </button>
            </div>
          )}
          {presentation.status === "available" && (
            <a
              className="button secondary compact"
              data-browser-download="exact-release-artifact"
              download={presentation.artifact.file_name}
              href={presentation.artifact.url}
            >
              Download for {targetLabel(selectedTarget)}
              <ExternalLink size={14} />
            </a>
          )}
        </div>
        <div>
          <span>2</span>
          <p>
            <b>Sign in with Bluey</b>
            <small>The browser links to this Jobs workspace and stores its profile locally.</small>
          </p>
        </div>
        <div>
          <span>3</span>
          <p>
            <b>Start from Applications</b>
            <small>Review an application, then choose the local runner.</small>
          </p>
        </div>
      </div>
      <div className="dialog-actions">
        <button className="button secondary" onClick={onClose}>Close</button>
        {presentation.status === "available" && (
          <a className="button primary" href="bluey-jobs://open">
            <Chrome size={16} />
            Open Bluey Browser
          </a>
        )}
      </div>
    </>
  );
}

function TakeoverAction({ url, label, primary = false, compact = false }: {
  url?: string;
  label: string;
  primary?: boolean;
  compact?: boolean;
}) {
  const className = `button ${primary ? "primary" : "secondary"}${compact ? " compact" : ""}`;
  return url
    ? <a className={className} href={url}><MonitorUp size={compact ? 15 : 16} />{label}</a>
    : <button className={className} disabled title="A scoped takeover capability is not available for this run"><MonitorUp size={compact ? 15 : 16} />Takeover unavailable</button>;
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error && cause.message.trim()
    ? cause.message
    : "Bluey could not finish that browser action. Please try again.";
}
