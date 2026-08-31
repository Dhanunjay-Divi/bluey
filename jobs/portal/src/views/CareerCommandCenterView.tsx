import {
  Activity,
  AlertTriangle,
  ArrowRight,
  BriefcaseBusiness,
  CalendarDays,
  CheckCircle2,
  CircleDashed,
  Cloud,
  FileCheck2,
  Gauge,
  Inbox,
  Link2Off,
  MailCheck,
  MapPin,
  MonitorUp,
  RefreshCcw,
  SearchCheck,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { Link } from "react-router-dom";
import type { JobsWorkspace } from "../types";
import {
  commandCenterHref,
  commandCenterRecency,
  commandCenterSummary,
} from "../lib/command-center";
import { titleCase } from "../lib/format";
import "./CareerCommandCenterView.css";

export interface CareerCommandCenterViewProps {
  workspace: JobsWorkspace;
  previewSearch: string;
  nowMs?: number;
}

export function CareerCommandCenterView({
  workspace,
  previewSearch,
  nowMs,
}: CareerCommandCenterViewProps) {
  const now = nowMs ?? Date.now();
  const summary = commandCenterSummary(workspace, now, previewSearch);
  const firstName = workspace.profile.full_name.trim().split(/\s+/)[0] || "there";
  const packetLimit = workspace.entitlement.monthly_packet_limit;
  const packetRemaining = Math.max(0, packetLimit - workspace.entitlement.used_packets);

  return (
    <div className="view-shell career-command-center">
      <section className="view-heading career-command-heading">
        <div>
          <p className="eyebrow">CAREER OPERATIONS</p>
          <h1>Good to see you, {firstName}.</h1>
          <span>
            One truthful view of discovery, preparation, review, execution, and outcomes.
          </span>
        </div>
        <Link className="button primary" to={commandCenterHref("/matches", previewSearch)}>
          <SearchCheck size={16} />Review fresh matches
        </Link>
      </section>

      <section className="career-command-metrics" aria-label="Career operations summary">
        <CommandMetric
          icon={<Sparkles size={18} />}
          value={summary.today_matches.length}
          label="Fresh in 24 hours"
          href={commandCenterHref("/matches", previewSearch)}
        />
        <CommandMetric
          icon={<AlertTriangle size={18} />}
          value={summary.open_interventions.length}
          label="Open interventions"
          href={commandCenterHref("/applications", previewSearch)}
          attention={summary.open_interventions.length > 0}
        />
        <CommandMetric
          icon={<FileCheck2 size={18} />}
          value={summary.outcomes.submitted}
          label="Submitted state"
          href={commandCenterHref("/applications", previewSearch)}
        />
        <CommandMetric
          icon={<Activity size={18} />}
          value={`${summary.source_health.healthy}/${summary.source_health.sources.length}`}
          label="Healthy sources"
          href={commandCenterHref("/matches", previewSearch)}
          attention={summary.source_health.attention > 0}
        />
      </section>

      <div className="career-command-primary-grid">
        <section className="career-command-panel readiness-panel">
          <CommandSectionHeading
            eyebrow="SETUP READINESS"
            title={`${summary.readiness_ready} of ${summary.readiness_required} required checks ready`}
            detail="Setup status does not replace per-job eligibility or submission authority."
            icon={<Gauge size={18} />}
          />
          <ul className="career-readiness-list">
            {summary.readiness.map((item) => (
              <li key={item.id} className={item.state}>
                {item.state === "ready"
                  ? <CheckCircle2 aria-hidden="true" size={18} />
                  : <CircleDashed aria-hidden="true" size={18} />}
                <span>
                  <strong>{item.label}</strong>
                  <small>{item.detail}</small>
                </span>
                <Link to={item.href} aria-label={`Open ${item.label}`}>
                  <ArrowRight size={16} />
                </Link>
              </li>
            ))}
          </ul>
        </section>

        <section className="career-command-panel actions-panel">
          <CommandSectionHeading
            eyebrow="NEXT BEST ACTIONS"
            title="What deserves attention now"
            detail="Ordered from blocking work to useful review. Nothing runs from this page."
            icon={<Sparkles size={18} />}
          />
          <ol className="career-next-actions">
            {summary.next_actions.map((action, index) => (
              <li key={action.id} className={action.tone}>
                <span className="action-order">{index + 1}</span>
                <span>
                  <strong>{action.title}</strong>
                  <small>{action.detail}</small>
                </span>
                <Link className="button secondary compact" to={action.href}>
                  Open <ArrowRight size={14} />
                </Link>
              </li>
            ))}
          </ol>
        </section>
      </div>

      <div className="career-command-secondary-grid">
        <section className="career-command-panel today-panel">
          <CommandSectionHeading
            eyebrow="TODAY"
            title="Fresh, active matches"
            detail={(
              "Posted in the last 24 hours on an active Track and not passed. "
              + "Freshness does not establish eligibility."
            )}
            icon={<BriefcaseBusiness size={18} />}
            action={(
              <Link to={commandCenterHref("/matches", previewSearch)}>
                All matches <ArrowRight size={14} />
              </Link>
            )}
          />
          {summary.today_matches.length > 0 ? (
            <ul className="career-match-list">
              {summary.today_matches.slice(0, 4).map((job) => (
                <li key={job.id}>
                  <div className="career-company-mark" aria-hidden="true">
                    {companyInitials(job.company)}
                  </div>
                  <span>
                    <strong>{job.title}</strong>
                    <small>{job.company}</small>
                    <small className="career-location">
                      <MapPin size={11} />{job.location} · {job.workplace}
                    </small>
                  </span>
                  <div className="career-match-meta">
                    <b>{job.match_score}%</b>
                    <small>{titleCase(job.source)} source</small>
                    <small>
                      {job.last_verified_at_ms == null || !Number.isFinite(job.last_verified_at_ms)
                        ? "Verification unavailable"
                        : `verified ${commandCenterRecency(job.last_verified_at_ms, now)}`}
                    </small>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <CommandEmpty
              icon={<SearchCheck size={20} />}
              title="No active match arrived in the last 24 hours"
              detail="Older active matches remain available in Matches."
            />
          )}
        </section>

        <section className="career-command-panel interventions-panel">
          <CommandSectionHeading
            eyebrow="REVIEW INBOX"
            title="Human decisions"
            detail="Bluey pauses instead of guessing employer-facing information."
            icon={<ShieldCheck size={18} />}
            action={(
              <Link to={commandCenterHref("/applications", previewSearch)}>
                Applications <ArrowRight size={14} />
              </Link>
            )}
          />
          {summary.open_interventions.length > 0 ? (
            <ul className="career-intervention-list">
              {summary.open_interventions.slice(0, 4).map(({ intervention, job }) => (
                <li key={intervention.id}>
                  <AlertTriangle size={17} aria-hidden="true" />
                  <span>
                    <strong>{intervention.title}</strong>
                    <small>{job ? `${job.company} · ${job.title}` : "Application review"}</small>
                    <small>{intervention.detail}</small>
                  </span>
                  <Link
                    className="button secondary compact"
                    to={commandCenterHref("/applications", previewSearch)}
                  >
                    Review
                  </Link>
                </li>
              ))}
            </ul>
          ) : (
            <CommandEmpty
              icon={<CheckCircle2 size={20} />}
              title="No open interventions"
              detail="No active workflow is waiting for an answer or approval."
            />
          )}
        </section>
      </div>

      <div className="career-command-secondary-grid">
        <section className="career-command-panel source-panel">
          <CommandSectionHeading
            eyebrow="DISCOVERY CONTROL PLANE"
            title="Source health"
            detail="Provider health is shown independently from match score and eligibility."
            icon={<RefreshCcw size={18} />}
            action={(
              <Link to={commandCenterHref("/matches", previewSearch)}>
                Manage sources <ArrowRight size={14} />
              </Link>
            )}
          />
          {summary.source_health.sources.length > 0 ? (
            <ul className="career-source-list">
              {summary.source_health.sources.slice(0, 5).map(({ source, effective_health }) => {
                const visibleHealth = effective_health;
                return (
                  <li key={source.id}>
                    <span className={`source-dot ${visibleHealth}`} aria-hidden="true" />
                    <span>
                      <strong>{source.config.company}</strong>
                      <small>{titleCase(source.provider)}</small>
                    </span>
                    <span className={`source-state ${visibleHealth}`}>
                      {titleCase(visibleHealth)}
                    </span>
                    <small>{commandCenterRecency(source.last_success_at_ms, now)}</small>
                  </li>
                );
              })}
            </ul>
          ) : (
            <CommandEmpty
              icon={<RefreshCcw size={20} />}
              title="No discovery source is enrolled"
              detail="Connect an employer source from Matches."
            />
          )}
        </section>

        <section className="career-command-panel outcomes-panel">
          <CommandSectionHeading
            eyebrow="APPLICATION OUTCOMES"
            title="Evidence-backed progress"
            detail="Counts come from application state, stored evidence, and recorded outcomes."
            icon={<Activity size={18} />}
            action={(
              <Link to={commandCenterHref("/applications", previewSearch)}>
                Full tracker <ArrowRight size={14} />
              </Link>
            )}
          />
          <div className="career-outcome-metrics">
            <OutcomeMetric value={summary.outcomes.prepared} label="Prepared for review" />
            <OutcomeMetric value={summary.outcomes.in_flight} label="In flight" />
            <OutcomeMetric value={summary.outcomes.submitted} label="Submitted" />
            <OutcomeMetric value={summary.outcomes.interviews} label="Interviews" />
            <OutcomeMetric value={summary.outcomes.offers} label="Offers" />
            <OutcomeMetric
              value={summary.outcomes.needs_reconciliation}
              label="Needs reconciliation"
              attention={summary.outcomes.needs_reconciliation > 0}
            />
          </div>
          <div className="career-outcome-authority">
            <ShieldCheck size={16} aria-hidden="true" />
            <span>
              <strong>Eligibility, approval, and communication sent are not inferred</strong>
              <small>
                This workspace projection has no policy, exact-approval, or communication receipts;
                inspect Applications for authoritative evidence.
              </small>
            </span>
          </div>
          {summary.outcomes.recent.length > 0 ? (
            <ul className="career-outcome-list">
              {summary.outcomes.recent.map((event) => (
                <li key={event.id}>
                  <CheckCircle2 size={15} aria-hidden="true" />
                  <span>
                    <strong>{titleCase(event.action)}</strong>
                    <small>{event.note || "Outcome recorded by the candidate."}</small>
                    <small>Candidate event · {titleCase(event.status)}</small>
                  </span>
                  <small>{commandCenterRecency(event.created_at_ms, now)}</small>
                </li>
              ))}
            </ul>
          ) : (
            <CommandEmpty
              icon={<CircleDashed size={20} />}
              title="No recent candidate outcomes"
              detail={
                "This panel receives candidate outcome events only. "
                + "Preparation, provider, ambiguity, and correction evidence remain in Applications."
              }
            />
          )}
        </section>
      </div>

      <div className="career-command-secondary-grid integration-grid">
        <section className="career-command-panel integration-panel">
          <CommandSectionHeading
            eyebrow="CONNECTED SERVICES"
            title="Integration truth"
            detail="Only workspace-reported connections appear as connected."
            icon={<Inbox size={18} />}
            action={(
              <Link to={commandCenterHref("/settings", previewSearch)}>
                Connection settings <ArrowRight size={14} />
              </Link>
            )}
          />
          <ul className="career-integration-list">
            {summary.inbox.connected.map((connection) => (
              <li key={connection.id}>
                <MailCheck size={18} aria-hidden="true" />
                <span>
                  <strong>{titleCase(connection.provider)} inbox</strong>
                  <small>{connection.account_label}</small>
                  <small>{connection.capabilities.map(titleCase).join(" · ")}</small>
                </span>
                <b className="integration-truth connected">Connected</b>
              </li>
            ))}
            {summary.inbox.needs_attention.map((connection) => (
              <li key={connection.id}>
                <AlertTriangle size={18} aria-hidden="true" />
                <span>
                  <strong>{titleCase(connection.provider)} inbox</strong>
                  <small>{connection.account_label || "No active account"}</small>
                </span>
                <b className="integration-truth attention">{titleCase(connection.status)}</b>
              </li>
            ))}
            {workspace.mailbox_connections.length === 0 && (
              <li>
                <Inbox size={18} aria-hidden="true" />
                <span>
                  <strong>Application inbox</strong>
                  <small>No Gmail or Outlook inbox is connected.</small>
                </span>
                <b className="integration-truth muted">Not connected</b>
              </li>
            )}
            {summary.inbox.calendars.map((calendar) => (
              <li key={calendar.id}>
                <CalendarDays size={18} aria-hidden="true" />
                <span>
                  <strong>{titleCase(calendar.provider)}</strong>
                  <small>{calendar.account_label || "No connected calendar account"}</small>
                </span>
                <b className={`integration-truth ${
                  calendar.status === "connected"
                    ? "connected"
                    : calendar.status === "disconnected" ? "muted" : "attention"
                }`}>
                  {titleCase(calendar.status)}
                </b>
              </li>
            ))}
            {summary.inbox.calendars.length === 0 && (
              <li>
                <CalendarDays size={18} aria-hidden="true" />
                <span>
                  <strong>Interview calendar</strong>
                  <small>No connected calendar account</small>
                </span>
                <b className="integration-truth muted">Unavailable</b>
              </li>
            )}
            <li>
              <Link2Off size={18} aria-hidden="true" />
              <span>
                <strong>LinkedIn</strong>
                <small>Bluey Jobs has no official LinkedIn connector.</small>
              </span>
              <b className="integration-truth muted">Unavailable</b>
            </li>
          </ul>
        </section>

        <section className="career-command-panel plan-panel">
          <CommandSectionHeading
            eyebrow="PLAN & EXECUTION"
            title={`${titleCase(workspace.entitlement.plan)} plan`}
            detail="Entitlements and execution availability come from the server workspace."
            icon={<Cloud size={18} />}
            action={(
              <Link to={commandCenterHref("/automation", previewSearch)}>
                Automation <ArrowRight size={14} />
              </Link>
            )}
          />
          <div className="career-plan-usage">
            <div>
              <span>
                <b>{workspace.entitlement.used_packets}</b>
                <small>of {packetLimit} packets used</small>
              </span>
              <strong>{packetRemaining} remaining</strong>
            </div>
            <div
              className="career-plan-progress"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={packetLimit}
              aria-valuenow={Math.min(workspace.entitlement.used_packets, packetLimit)}
              aria-label="Monthly application packet usage"
            >
              <span style={{ width: packetLimit > 0
                ? `${Math.min(100, (workspace.entitlement.used_packets / packetLimit) * 100)}%`
                : "0%" }} />
            </div>
          </div>
          <div className="career-runner-list">
            <RunnerState
              label="Managed cloud runner"
              availability={workspace.runner_availability.cloud}
              icon={<Cloud size={20} aria-hidden="true" />}
            />
            <RunnerState
              label="Local browser runner"
              availability={workspace.runner_availability.local}
              icon={<MonitorUp size={20} aria-hidden="true" />}
            />
          </div>
          <div className="career-authority-note">
            <ShieldCheck size={17} aria-hidden="true" />
            <span>
              <strong>Per-application authority still applies</strong>
              <small>{workspace.runner_availability.auto_submit_reason}</small>
            </span>
          </div>
        </section>
      </div>
    </div>
  );
}

interface CommandMetricProps {
  icon: React.ReactNode;
  value: number | string;
  label: string;
  href: string;
  attention?: boolean;
}

function CommandMetric({ icon, value, label, href, attention = false }: CommandMetricProps) {
  return (
    <Link className={attention ? "attention" : ""} to={href}>
      <span className="career-metric-icon">{icon}</span>
      <span>
        <b>{value}</b>
        <small>{label}</small>
      </span>
      <ArrowRight size={14} aria-hidden="true" />
    </Link>
  );
}

interface CommandSectionHeadingProps {
  eyebrow: string;
  title: string;
  detail: string;
  icon: React.ReactNode;
  action?: React.ReactNode;
}

function CommandSectionHeading({
  eyebrow,
  title,
  detail,
  icon,
  action,
}: CommandSectionHeadingProps) {
  return (
    <header className="career-command-panel-heading">
      <span className="panel-heading-icon">{icon}</span>
      <span>
        <p>{eyebrow}</p>
        <h2>{title}</h2>
        <small>{detail}</small>
      </span>
      {action && <div className="panel-heading-action">{action}</div>}
    </header>
  );
}

function OutcomeMetric({
  value,
  label,
  attention = false,
}: {
  value: number;
  label: string;
  attention?: boolean;
}) {
  return (
    <span className={attention ? "attention" : ""}>
      <b>{value}</b>
      <small>{label}</small>
    </span>
  );
}

function RunnerState({
  label,
  availability,
  icon,
}: {
  label: string;
  availability: JobsWorkspace["runner_availability"]["cloud"];
  icon: React.ReactNode;
}) {
  return (
    <div className={`career-runner-state ${availability.available ? "available" : "unavailable"}`}>
      {availability.available ? icon : <AlertTriangle size={20} aria-hidden="true" />}
      <span>
        <strong>{label}</strong>
        <small>{availability.reason}</small>
        <small>{availability.next_action}</small>
      </span>
      <b>{titleCase(availability.status)}</b>
    </div>
  );
}

function CommandEmpty({
  icon,
  title,
  detail,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
}) {
  return (
    <div className="career-command-empty">
      {icon}
      <span>
        <strong>{title}</strong>
        <small>{detail}</small>
      </span>
    </div>
  );
}

function companyInitials(company: string): string {
  return company
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}
