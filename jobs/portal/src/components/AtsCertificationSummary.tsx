import type { AtsCertificationStatus, SubmissionCapability } from "../types";
import { atsCertificationPresentation } from "../lib/ats-certification";

interface Props {
  eligibility: unknown;
}

export function AtsCertificationSummaryCard({ eligibility }: Props) {
  const summary = atsCertificationPresentation(eligibility);
  const runners = summary.certified_runner_kinds.length
    ? summary.certified_runner_kinds.map(runnerLabel).join(" · ")
    : "No certified runner";
  return (
    <section className={`ats-certification-summary status-${summary.status}`}>
      <div className="ats-certification-heading">
        <div>
          <p>RUNNER CERTIFICATION</p>
          <h4>{summary.provider_label}</h4>
        </div>
        <span className="ats-certification-status">{statusLabel(summary.status)}</span>
      </div>
      <dl className="ats-certification-grid">
        <div><dt>Capability</dt><dd>{capabilityLabel(summary.capability)}</dd></div>
        <div><dt>Adapter</dt><dd>{summary.adapter_version}</dd></div>
        <div><dt>Runner scope</dt><dd>{runners}</dd></div>
        <div><dt>Last verified</dt><dd>{formatAuthorityTime(summary.last_verified_at_ms)}</dd></div>
        <div><dt>Expires</dt><dd>{formatAuthorityTime(summary.expires_at_ms)}</dd></div>
        <div>
          <dt>Canary</dt>
          <dd>{summary.canary_available ? "Available" : "Not available"}</dd>
        </div>
      </dl>
      <div className="ats-certification-guidance">
        <p>{summary.reason}</p>
        <b>{summary.next_action}</b>
      </div>
      {!summary.server_authored && (
        <span className="ats-certification-fallback">Server certification summary unavailable</span>
      )}
    </section>
  );
}

function statusLabel(status: AtsCertificationStatus): string {
  switch (status) {
    case "active": return "Active";
    case "review_only": return "Review only";
    case "expired": return "Expired";
    case "suspended": return "Suspended";
    case "revoked": return "Revoked";
    case "drifted": return "Drifted";
  }
}

function capabilityLabel(capability: SubmissionCapability): string {
  switch (capability) {
    case "certified": return "Certified";
    case "beta_review": return "Beta · Review first";
    case "handoff": return "Handoff";
    case "blocked": return "Blocked";
    case "unknown_review": return "Review only";
  }
}

function runnerLabel(runner: "local" | "cloud"): string {
  return runner === "local" ? "Local Browser" : "Cloud Browser";
}

function formatAuthorityTime(timestamp?: number): string {
  if (!timestamp) return "Not available";
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}
