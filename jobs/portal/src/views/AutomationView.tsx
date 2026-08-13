import { useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  Check,
  Cloud,
  KeyRound,
  LockKeyhole,
  MailCheck,
  MonitorUp,
  Play,
  ShieldCheck,
  WifiOff,
} from "lucide-react";
import type {
  Intervention,
  JobApplication,
  JobsWorkspace,
  RunnerChannelAvailability,
} from "../types";
import {
  cloudAutomationEligibleApplications,
  isFinalSubmissionReview,
} from "../lib/application-flow";
import { titleCase } from "../lib/format";
import { cloudRunnerAccessCopy } from "../lib/runner-access";
import { Dialog } from "../components/Dialog";

interface AutomationViewProps {
  workspace: JobsWorkspace;
  previewSearch: string;
  onQueueCloud(application: JobApplication): Promise<void>;
  onResolveIntervention(intervention: Intervention, action: string): Promise<void>;
}

export type AutomationAction =
  | { kind: "queue" }
  | { kind: "review"; href: string };

export interface SubmissionApprovalTarget {
  interventionId: string;
  takeoverUrl: string;
}

export function automationAction(
  access: RunnerChannelAvailability,
  previewSearch = "",
): AutomationAction {
  return access.available
    ? { kind: "queue" }
    : { kind: "review", href: `/jobs/applications${previewSearch}` };
}

export async function queueCloudApplication(
  access: RunnerChannelAvailability,
  application: JobApplication,
  onQueueCloud: (application: JobApplication) => Promise<void>,
): Promise<void> {
  if (!access.available) {
    throw new Error(access.reason || "Background automation is not available right now.");
  }
  await onQueueCloud(application);
}

export function submissionApprovalEnabled(
  reviewConfirmed: boolean,
  resolving: boolean,
  targetMatches: boolean,
): boolean {
  return reviewConfirmed && !resolving && targetMatches;
}

export function submissionApprovalTargetMatches(
  target: SubmissionApprovalTarget | undefined,
  intervention: Intervention | undefined,
  takeoverUrl: string | undefined,
): boolean {
  return Boolean(
    target
      && intervention
      && target.interventionId === intervention.id
      && target.takeoverUrl === takeoverUrl
      && isFinalSubmissionReview(intervention, takeoverUrl),
  );
}

export function AutomationView({
  workspace,
  previewSearch,
  onQueueCloud,
  onResolveIntervention,
}: AutomationViewProps) {
  const [cloudOpen, setCloudOpen] = useState(false);
  const [queueing, setQueueing] = useState("");
  const [resolving, setResolving] = useState(false);
  const [actionError, setActionError] = useState("");
  const [approvalTarget, setApprovalTarget] = useState<SubmissionApprovalTarget>();
  const [reviewConfirmed, setReviewConfirmed] = useState(false);
  const active = workspace.browser_sessions.find(
    (session) => !["complete", "failed"].includes(session.status),
  );
  const intervention = active
    ? workspace.interventions.find(
      (item) => item.application_id === active.application_id && item.status === "open",
    )
    : undefined;
  const emailCodeReady = intervention?.resolution_kind === "email_otp_approval"
    && (!intervention.expires_at_ms || intervention.expires_at_ms > Date.now());
  const takeoverUrl = active?.takeover_url;
  const finalSubmissionReview = isFinalSubmissionReview(intervention, takeoverUrl);
  const approvalTargetMatches = submissionApprovalTargetMatches(
    approvalTarget,
    intervention,
    takeoverUrl,
  );
  const retainedDeviceRecovery = active?.runner === "local";
  const queuedApplications = cloudAutomationEligibleApplications(workspace);
  const cloudAccess = workspace.runner_availability.cloud;
  const cloudCopy = cloudRunnerAccessCopy(cloudAccess);
  const action = automationAction(cloudAccess, previewSearch);

  const resolve = async (item: Intervention, resolution: string, closeApproval = false) => {
    setResolving(true);
    setActionError("");
    try {
      await onResolveIntervention(item, resolution);
      if (closeApproval) setApprovalTarget(undefined);
    } catch (cause) {
      setActionError(errorMessage(cause));
    } finally {
      setResolving(false);
    }
  };

  const queue = async (application: JobApplication) => {
    setQueueing(application.id);
    setActionError("");
    try {
      await queueCloudApplication(cloudAccess, application, onQueueCloud);
      setCloudOpen(false);
    } catch (cause) {
      setActionError(errorMessage(cause));
    } finally {
      setQueueing("");
    }
  };

  return (
    <div className="view-shell browser-view">
      <section className="view-heading">
        <div>
          <p className="eyebrow">APPLICATION AUTOMATION</p>
          <h1>Automation</h1>
          <span>Queue approved applications for Bluey to complete securely in the cloud.</span>
        </div>
        <div className="security-note">
          <ShieldCheck size={17} />
          <span>
            <b>Encrypted cloud execution</b>
            <small>Bluey pauses whenever your review or a security check is required.</small>
          </span>
        </div>
      </section>

      {actionError && <div className="inline-error" role="alert">{actionError}</div>}

      {active && (
        <section
          className="active-browser-run"
          aria-label={retainedDeviceRecovery
            ? "Retained device-session recovery"
            : "Active cloud automation run"}
        >
          <div className="browser-window-preview" aria-hidden="true">
            <header>
              <span /><span /><span />
              <div><LockKeyhole size={13} />Secure employer session</div>
            </header>
            <div className="browser-window-preview-body">
              <div className="browser-company">
                {active.current_company.slice(0, 2).toUpperCase()}
              </div>
              <span>
                <p>APPLICATION IN PROGRESS</p>
                <h2>{active.current_company}</h2>
                <b>{active.current_step}</b>
              </span>
            </div>
            <footer>
              <div>
                <span style={{ width: active.status === "needs_input" ? "68%" : "42%" }} />
              </div>
              <small>{titleCase(active.status)}</small>
            </footer>
          </div>
          <div className="run-control">
            <p>{retainedDeviceRecovery
              ? "RETAINED DEVICE-SESSION RECOVERY"
              : "ACTIVE CLOUD AUTOMATION RUN"}</p>
            <h2>{active.current_company}</h2>
            <span>{active.current_step}</span>
            {retainedDeviceRecovery && (
              <div className="run-alert">
                <AlertTriangle size={17} />
                <div>
                  <b>Recovery only</b>
                  <p>
                    This previously started device session is retained for safe recovery. New runs
                    use managed cloud automation.
                  </p>
                </div>
              </div>
            )}
            {intervention && (
              <div className="run-alert">
                <AlertTriangle size={17} />
                <div><b>{intervention.title}</b><p>{intervention.detail}</p></div>
              </div>
            )}
            <div className="run-actions">
              {finalSubmissionReview && intervention ? (
                <>
                  <TakeoverAction url={takeoverUrl} label="Review form" compact />
                  <button
                    className="button primary"
                    disabled={resolving}
                    onClick={() => {
                      setReviewConfirmed(false);
                      if (takeoverUrl) {
                        setApprovalTarget({ interventionId: intervention.id, takeoverUrl });
                      }
                    }}
                  >
                    <ShieldCheck size={16} />Approve submission
                  </button>
                </>
              ) : emailCodeReady && intervention ? (
                <>
                  <button
                    className="button primary"
                    disabled={resolving}
                    onClick={() => void resolve(intervention, "approve_email_otp")}
                  >
                    <MailCheck size={16} />
                    {resolving ? "Approving..." : "Use email code"}
                  </button>
                  <TakeoverAction url={takeoverUrl} label="Take over" compact />
                </>
              ) : (
                <TakeoverAction url={takeoverUrl} label="Take over" primary />
              )}
            </div>
          </div>
        </section>
      )}

      <section className="runner-grid" aria-label="Cloud automation availability">
        <article className={`runner-option ${cloudAccess.available ? "enabled" : "locked"}`}>
          <div className="runner-icon cloud"><Cloud /></div>
          <div className="runner-copy">
            <p>CLOUD</p>
            <h2>Background automation</h2>
            <span>{cloudCopy.description}</span>
            <ul>
              {cloudCopy.points.map((point) => (
                <li key={point}><Check size={14} />{point}</li>
              ))}
            </ul>
          </div>
          <div className="runner-action">
            <b>{cloudCopy.badge}</b>
            {action.kind === "queue" ? (
              <button className="button primary" onClick={() => setCloudOpen(true)}>
                {cloudCopy.action}<ArrowRight size={16} />
              </button>
            ) : (
              <a className="button primary" href={action.href}>
                Review applications<ArrowRight size={16} />
              </a>
            )}
          </div>
        </article>
      </section>

      <section className="adapter-section">
        <div className="section-heading">
          <div>
            <p>CLOUD AUTOMATION COVERAGE</p>
            <h2>Provider authority decides every application</h2>
            <span>
              Reviewed Greenhouse and Lever packets can enter enabled cloud automation. Only an
              exact active certification can omit the final submission review; other systems stay
              in Review or job-site handoff.
            </span>
          </div>
        </div>
        <div className="adapter-list">
          {[
            { name: "Greenhouse", status: "Reviewed beta + exact certification", kind: "cloud" },
            { name: "Lever", status: "Reviewed beta + exact certification", kind: "cloud" },
            { name: "Workday", status: "Review or handoff", kind: "review" },
            { name: "Ashby", status: "Review or handoff", kind: "review" },
            { name: "SmartRecruiters", status: "Review or handoff", kind: "review" },
          ].map(({ name, status, kind }) => (
            <div key={name} className={`adapter-${kind}`}>
              <span>{name.slice(0, 1)}</span><b>{name}</b><small>{status}</small>
            </div>
          ))}
        </div>
        <div className="handoff-policy">
          <WifiOff size={18} />
          <div>
            <b>LinkedIn and Indeed use handoff</b>
            <p>
              Bluey prepares the tailored resume and answers, opens the listing, and lets you
              complete submission there.
            </p>
          </div>
        </div>
      </section>

      <Dialog
        open={cloudOpen}
        title="Queue an application"
        description="Cloud automation starts only after a packet is approved or Auto-submit eligible."
        onClose={() => setCloudOpen(false)}
      >
        <div className="cloud-queue-list">
          {queuedApplications.map((application) => {
            const job = workspace.matches.find((item) => item.id === application.job_id);
            const label = job
              ? `Queue ${job.title} at ${job.company}`
              : "Queue approved application";
            return (
              <button
                aria-label={label}
                key={application.id}
                disabled={Boolean(queueing)}
                onClick={() => void queue(application)}
              >
                <div className="company-mark">{job?.company.slice(0, 2).toUpperCase()}</div>
                <span>
                  <b>{job?.title ?? "Approved application"}</b>
                  <small>
                    {job ? `${job.company} · ` : ""}{application.match_score}% match
                  </small>
                </span>
                {queueing === application.id ? <small>Queuing...</small> : <Play size={17} />}
              </button>
            );
          })}
          {queuedApplications.length === 0 && (
            <div className="empty-state small">
              <KeyRound />
              <h3>No applications ready</h3>
              <p>Approve an application kit in Applications first.</p>
            </div>
          )}
        </div>
        <div className="dialog-actions">
          <button className="button secondary" onClick={() => setCloudOpen(false)}>Close</button>
          <a className="button primary" href={`/jobs/applications${previewSearch}`}>
            Go to Applications<ArrowRight size={16} />
          </a>
        </div>
      </Dialog>

      <Dialog
        open={approvalTargetMatches}
        title="Approve this submission?"
        description="This is the final checkpoint before Bluey activates the employer's Submit control."
        onClose={() => {
          if (!resolving) setApprovalTarget(undefined);
        }}
      >
        <div className="submission-approval">
          <ShieldCheck size={22} />
          <div>
            <h3>Confirm the preserved form</h3>
            <p>
              Check every employer-facing answer, attachment, contact detail, and consent on the
              live form before continuing.
            </p>
          </div>
          <label>
            <input
              type="checkbox"
              checked={reviewConfirmed}
              onChange={(event) => setReviewConfirmed(event.target.checked)}
            />
            <span>I reviewed the final application and want Bluey to submit it.</span>
          </label>
        </div>
        <div className="dialog-actions">
          <button
            className="button secondary"
            disabled={resolving}
            onClick={() => setApprovalTarget(undefined)}
          >
            Cancel
          </button>
          <button
            className="button primary"
            disabled={!submissionApprovalEnabled(
              reviewConfirmed,
              resolving,
              approvalTargetMatches,
            )}
            onClick={() => {
              if (intervention && approvalTargetMatches) {
                void resolve(intervention, "approve_submission", true);
              }
            }}
          >
            <ShieldCheck size={16} />
            {resolving ? "Submitting..." : "Approve and submit"}
          </button>
        </div>
      </Dialog>
    </div>
  );
}

function TakeoverAction({ url, label, primary = false, compact = false }: {
  url?: string;
  label: string;
  primary?: boolean;
  compact?: boolean;
}) {
  const className = `button ${primary ? "primary" : "secondary"}${compact ? " compact" : ""}`;
  return url ? (
    <a className={className} href={url}><MonitorUp size={compact ? 15 : 16} />{label}</a>
  ) : (
    <button
      className={className}
      disabled
      title="A scoped takeover capability is not available for this run"
    >
      <MonitorUp size={compact ? 15 : 16} />Takeover unavailable
    </button>
  );
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error && cause.message.trim()
    ? cause.message
    : "Bluey could not finish that automation action. Please try again.";
}
