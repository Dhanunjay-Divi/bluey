import type { AtsCertificationStatus, SubmissionCapability } from "../types";
import { atsCertificationPresentation } from "../lib/ats-certification";

interface Props {
  eligibility: unknown;
}

export function AtsCertificationSummaryCard({ eligibility }: Props) {
  const summary = atsCertificationPresentation(eligibility);
  const cloudCertified = summary.certified_runner_kinds.includes("cloud");
  const cloudActive = cloudCertified
    && summary.capability === "certified"
    && summary.status === "active";
  const reviewedBeta = summary.capability === "beta_review"
    && summary.status === "review_only"
    && summary.can_queue_cloud;
  const guidance = reviewedBeta
    ? {
        reason: "This reviewed provider flow can use enabled cloud automation.",
        nextAction: "Review the exact kit first; Bluey pauses again for final form approval.",
      }
    : cloudCertified && !mentionsParkedLocalRunner(summary.reason, summary.next_action)
    ? { reason: summary.reason, nextAction: summary.next_action }
    : {
        reason: "This application system is not currently certified for cloud automation.",
        nextAction: "Review the application kit and continue on the original job site.",
      };
  return (
    <section className={`ats-certification-summary status-${summary.status}`}>
      <div className="ats-certification-heading">
        <div>
          <p>CLOUD AUTOMATION</p>
          <h4>{summary.provider_label}</h4>
        </div>
        <span className="ats-certification-status">
          {reviewedBeta ? "Reviewed beta" : cloudCertified ? statusLabel(summary.status) : "Review only"}
        </span>
      </div>
      <dl className="ats-certification-grid">
        <div>
          <dt>Capability</dt>
          <dd>{reviewedBeta
            ? "Beta · Final review"
            : cloudActive ? capabilityLabel(summary.capability) : "Review only"}</dd>
        </div>
        <div><dt>Adapter</dt><dd>{summary.adapter_version}</dd></div>
        <div>
          <dt>Automation scope</dt>
          <dd>{reviewedBeta
            ? "Cloud · final review"
            : cloudCertified ? "Cloud runner" : "Not certified"}</dd>
        </div>
        <div><dt>Last verified</dt><dd>{formatAuthorityTime(summary.last_verified_at_ms)}</dd></div>
        <div><dt>Expires</dt><dd>{formatAuthorityTime(summary.expires_at_ms)}</dd></div>
        <div>
          <dt>Canary</dt>
          <dd>{cloudCertified && summary.canary_available ? "Available" : "Not available"}</dd>
        </div>
      </dl>
      <div className="ats-certification-guidance">
        <p>{guidance.reason}</p>
        <b>{guidance.nextAction}</b>
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

function mentionsParkedLocalRunner(reason: string, nextAction: string): boolean {
  return /\b(?:local (?:browser|runner)|bluey browser)\b/i.test(`${reason} ${nextAction}`);
}

function formatAuthorityTime(timestamp?: number): string {
  if (!timestamp) return "Not available";
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}
