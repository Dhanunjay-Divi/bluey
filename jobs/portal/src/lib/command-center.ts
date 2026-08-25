import type {
  CandidateEvent,
  DiscoverySource,
  Intervention,
  JobApplication,
  JobPosting,
  JobsIntegration,
  JobsWorkspace,
  MailboxConnection,
} from "../types";
import { isJobPassed } from "./candidate-events";
import { discoverySourceState } from "./discovery-source";

const ONE_DAY_MS = 24 * 60 * 60 * 1_000;

export type CommandCenterTone = "ready" | "attention" | "muted";
export type ReadinessState = "ready" | "needs_action" | "reported" | "optional";

export interface ReadinessItem {
  id: string;
  label: string;
  detail: string;
  state: ReadinessState;
  href: string;
}

export interface CommandCenterIntervention {
  intervention: Intervention;
  application?: JobApplication;
  job?: JobPosting;
}

export interface ApplicationOutcomeSummary {
  prepared: number;
  in_flight: number;
  submitted: number;
  interviews: number;
  offers: number;
  needs_reconciliation: number;
  recent: CandidateEvent[];
}

export interface SourceHealthSummary {
  healthy: number;
  attention: number;
  waiting: number;
  sources: CommandCenterDiscoverySource[];
}

export interface CommandCenterDiscoverySource {
  source: DiscoverySource;
  effective_health: ReturnType<typeof discoverySourceState>;
}

export interface InboxTruthSummary {
  connected: MailboxConnection[];
  needs_attention: MailboxConnection[];
  calendars: JobsIntegration[];
  linkedin_supported: false;
}

export interface NextBestAction {
  id: string;
  title: string;
  detail: string;
  href: string;
  tone: CommandCenterTone;
}

export interface CommandCenterSummary {
  readiness: ReadinessItem[];
  readiness_ready: number;
  readiness_required: number;
  today_matches: JobPosting[];
  open_interventions: CommandCenterIntervention[];
  source_health: SourceHealthSummary;
  outcomes: ApplicationOutcomeSummary;
  inbox: InboxTruthSummary;
  next_actions: NextBestAction[];
}

export function commandCenterHref(path: string, previewSearch = ""): string {
  return `${path}${previewSearch}`;
}

export function commandCenterSummary(
  workspace: JobsWorkspace,
  nowMs = Date.now(),
  previewSearch = "",
): CommandCenterSummary {
  const verifiedIdentityIds = new Set(
    workspace.application_identities
      .filter((identity) => identity.verification_status === "verified")
      .map((identity) => identity.id),
  );
  const activeTracks = workspace.tracks.filter((track) => track.active);
  const activeTrackIds = new Set(activeTracks.map((track) => track.id));
  const sourceResumeMetadataComplete = Boolean(
    workspace.profile.source_resume_asset_id.trim()
    && workspace.profile.source_resume_sha256.trim(),
  );
  const sourceResumeMetadataPartial = Boolean(
    workspace.profile.source_resume_asset_id.trim()
    || workspace.profile.source_resume_sha256.trim(),
  );
  const identityBoundTracks = activeTracks.filter((track) =>
    Boolean(track.application_identity_id)
      && verifiedIdentityIds.has(track.application_identity_id ?? ""),
  );
  const unconfirmedFacts = workspace.facts.filter(
    (fact) => fact.verification_status !== "confirmed",
  );
  const connectedMailboxes = workspace.mailbox_connections.filter(
    (connection) => connection.status === "connected",
  );
  const sourceRows: CommandCenterDiscoverySource[] = workspace.discovery_sources.map(
    (source) => ({ source, effective_health: discoverySourceState(source, nowMs) }),
  );
  const healthySources = sourceRows.filter((row) => row.effective_health === "healthy");

  const readiness: ReadinessItem[] = [
    {
      id: "resume",
      label: "Source resume",
      detail: sourceResumeMetadataComplete
        ? "The workspace reports a resume reference, but this view has no authoritative asset read-back."
        : sourceResumeMetadataPartial
          ? "The source resume binding is incomplete; re-import it before preparation."
        : "Import and review the resume Bluey must bind to each Career Track.",
      state: sourceResumeMetadataComplete ? "reported" : "needs_action",
      href: commandCenterHref("/resume", previewSearch),
    },
    {
      id: "identity",
      label: "Verified identity",
      detail: verifiedIdentityIds.size > 0
        ? `${verifiedIdentityIds.size} verified application ${
          verifiedIdentityIds.size === 1 ? "identity" : "identities"
        }.`
        : "Verify an email identity before preparing employer-facing work.",
      state: verifiedIdentityIds.size > 0 ? "ready" : "needs_action",
      href: commandCenterHref("/settings", previewSearch),
    },
    {
      id: "tracks",
      label: "Career Track setup",
      detail: identityBoundTracks.length > 0
        ? `${identityBoundTracks.length} active ${
          identityBoundTracks.length === 1 ? "Track has" : "Tracks have"
        } a verified identity binding. Canonical role, location, policy, and resume binding are not proven here.`
        : "Create an active Track and bind it to a verified application identity.",
      state: identityBoundTracks.length > 0 ? "reported" : "needs_action",
      href: commandCenterHref("/settings", previewSearch),
    },
    {
      id: "facts",
      label: "Candidate facts",
      detail: unconfirmedFacts.length === 0
        ? `${workspace.facts.length} candidate ${workspace.facts.length === 1 ? "fact is" : "facts are"} confirmed.`
        : `${unconfirmedFacts.length} ${
          unconfirmedFacts.length === 1 ? "fact needs" : "facts need"
        } review before reuse.`,
      state: workspace.facts.length > 0 && unconfirmedFacts.length === 0
        ? "ready"
        : "needs_action",
      href: commandCenterHref("/settings", previewSearch),
    },
    {
      id: "sources",
      label: "Discovery sources",
      detail: healthySources.length > 0
        ? `${healthySources.length} active ${healthySources.length === 1 ? "source is" : "sources are"} healthy.`
        : "No active source is currently reporting healthy.",
      state: healthySources.length > 0 ? "ready" : "needs_action",
      href: commandCenterHref("/matches", previewSearch),
    },
    {
      id: "inbox",
      label: "Application inbox (optional)",
      detail: connectedMailboxes.length > 0
        ? `${connectedMailboxes.length} ${
          connectedMailboxes.length === 1 ? "inbox is" : "inboxes are"
        } connected for status correlation.`
        : "No inbox is connected; mailbox correlation is optional.",
      state: "optional",
      href: commandCenterHref("/settings", previewSearch),
    },
  ];

  const todayMatches = workspace.matches
    .filter((job) => job.availability_status === "active"
      && activeTrackIds.has(job.track_id)
      && !isJobPassed(workspace.candidate_events, job.id)
      && job.posted_at_ms !== undefined
      && job.posted_at_ms <= nowMs
      && nowMs - job.posted_at_ms <= ONE_DAY_MS)
    .sort((left, right) => right.match_score - left.match_score
      || (right.posted_at_ms ?? 0) - (left.posted_at_ms ?? 0)
      || left.id.localeCompare(right.id));

  const applicationById = new Map(
    workspace.applications.map((application) => [application.id, application]),
  );
  const jobById = new Map(workspace.matches.map((job) => [job.id, job]));
  const openInterventions = workspace.interventions
    .filter((intervention) => intervention.status === "open")
    .sort((left, right) => right.created_at_ms - left.created_at_ms)
    .map((intervention) => {
      const application = intervention.application_id
        ? applicationById.get(intervention.application_id)
        : undefined;
      return {
        intervention,
        application,
        job: application ? jobById.get(application.job_id) : undefined,
      };
    });

  const outcomeEvents = workspace.candidate_events
    .filter((event) => event.event_type === "application_outcome")
    .sort((left, right) => right.created_at_ms - left.created_at_ms);
  const interviewApplications = new Set<string>();
  const offerApplications = new Set<string>();
  for (const event of outcomeEvents) {
    const action = event.action.trim().toLowerCase();
    if (action === "interview" && event.application_id) {
      interviewApplications.add(event.application_id);
    }
    if (action === "offer" && event.application_id) {
      offerApplications.add(event.application_id);
    }
  }
  for (const evidence of workspace.application_evidence) {
    if (evidence.kind === "interview_event") interviewApplications.add(evidence.application_id);
  }

  const sourceHealth: SourceHealthSummary = {
    healthy: healthySources.length,
    attention: sourceRows.filter(
      (row) => row.effective_health === "degraded" || row.effective_health === "paused",
    ).length,
    waiting: sourceRows.filter((row) => row.effective_health === "waiting").length,
    sources: sourceRows.sort((left, right) => {
      const rank = (row: CommandCenterDiscoverySource) => {
        if (row.effective_health === "degraded" || row.effective_health === "paused") return 0;
        if (row.effective_health === "waiting") return 1;
        return 2;
      };
      return rank(left) - rank(right)
        || left.source.config.company.localeCompare(right.source.config.company);
    }),
  };

  const calendars = workspace.integrations
    .filter((integration) =>
      integration.provider === "google_calendar" || integration.provider === "outlook_calendar",
    )
    .sort((left, right) => {
      const connectedRank = (integration: JobsIntegration) =>
        integration.status === "connected" ? 0 : 1;
      return connectedRank(left) - connectedRank(right)
        || left.provider.localeCompare(right.provider);
    });
  const inbox: InboxTruthSummary = {
    connected: connectedMailboxes,
    needs_attention: workspace.mailbox_connections.filter(
      (connection) => connection.status !== "connected",
    ),
    calendars,
    // JobsWorkspace has no typed LinkedIn connector capability. Keep this fail-closed until one exists.
    linkedin_supported: false,
  };

  const outcomes: ApplicationOutcomeSummary = {
    prepared: workspace.applications.filter(
      (application) => application.state === "awaiting_review",
    ).length,
    in_flight: workspace.applications.filter(
      (application) => ["queued", "running", "needs_input"].includes(application.state),
    ).length,
    submitted: workspace.applications.filter((application) => application.state === "submitted").length,
    interviews: interviewApplications.size,
    offers: offerApplications.size,
    needs_reconciliation: workspace.applications.filter(
      (application) => application.state === "side_effect_unknown",
    ).length,
    recent: outcomeEvents.slice(0, 4),
  };

  return {
    readiness,
    readiness_ready: readiness.filter((item) => item.state === "ready").length,
    readiness_required: readiness.filter((item) => item.state !== "optional").length,
    today_matches: todayMatches,
    open_interventions: openInterventions,
    source_health: sourceHealth,
    outcomes,
    inbox,
    next_actions: nextBestActions(
      workspace,
      readiness,
      todayMatches,
      openInterventions,
      sourceHealth,
      previewSearch,
    ),
  };
}

function nextBestActions(
  workspace: JobsWorkspace,
  readiness: ReadinessItem[],
  todayMatches: JobPosting[],
  openInterventions: CommandCenterIntervention[],
  sourceHealth: SourceHealthSummary,
  previewSearch: string,
): NextBestAction[] {
  const actions: NextBestAction[] = [];
  const applicationHref = commandCenterHref("/applications", previewSearch);

  if (openInterventions.length > 0) {
    actions.push({
      id: "interventions",
      title: `Resolve ${openInterventions.length} open ${
        openInterventions.length === 1 ? "intervention" : "interventions"
      }`,
      detail: "Affected work remains gated; Bluey will not guess the requested employer-facing value.",
      href: applicationHref,
      tone: "attention",
    });
  }

  const unknownSideEffects = workspace.applications.filter(
    (application) => application.state === "side_effect_unknown",
  ).length;
  if (unknownSideEffects > 0) {
    actions.push({
      id: "reconcile",
      title: `Reconcile ${unknownSideEffects} uncertain ${unknownSideEffects === 1 ? "submission" : "submissions"}`,
      detail: "Bluey will not retry an employer submit while the side effect is unknown.",
      href: applicationHref,
      tone: "attention",
    });
  }

  const awaitingReview = workspace.applications.filter(
    (application) => application.state === "awaiting_review",
  ).length;
  if (awaitingReview > 0) {
    actions.push({
      id: "reviews",
      title: `Review ${awaitingReview} prepared ${awaitingReview === 1 ? "application" : "applications"}`,
      detail: "Inspect the exact resume, answers, claims, and eligibility before approval.",
      href: applicationHref,
      tone: "ready",
    });
  }

  const firstMissingReadiness = readiness.find((item) => item.state === "needs_action");
  if (firstMissingReadiness) {
    actions.push({
      id: `readiness-${firstMissingReadiness.id}`,
      title: `Review ${firstMissingReadiness.label.toLowerCase()}`,
      detail: firstMissingReadiness.detail,
      href: firstMissingReadiness.href,
      tone: "attention",
    });
  }

  if (sourceHealth.attention > 0) {
    actions.push({
      id: "source-health",
      title: `Review ${sourceHealth.attention} ${
        sourceHealth.attention === 1 ? "source" : "sources"
      } needing attention`,
      detail: "Source health is visible separately from match quality and execution eligibility.",
      href: commandCenterHref("/matches", previewSearch),
      tone: "attention",
    });
  }

  if (todayMatches.length > 0) {
    actions.push({
      id: "fresh-matches",
      title: `Review ${todayMatches.length} fresh ${todayMatches.length === 1 ? "match" : "matches"}`,
      detail: "Open the source details and verification evidence before preparing an application.",
      href: commandCenterHref("/matches", previewSearch),
      tone: "ready",
    });
  }

  if (actions.length === 0) {
    actions.push({
      id: "matches",
      title: "Review your latest matches",
      detail: "Bluey keeps discovery, preparation, approval, and execution authority separate.",
      href: commandCenterHref("/matches", previewSearch),
      tone: "muted",
    });
  }

  return actions.slice(0, 5);
}

export function commandCenterRecency(
  timestampMs: number | null | undefined,
  nowMs = Date.now(),
): string {
  if (timestampMs == null || !Number.isFinite(timestampMs)) return "No successful sync yet";
  const elapsed = Math.max(0, nowMs - timestampMs);
  if (elapsed < 60_000) return "just now";
  if (elapsed < 60 * 60_000) return `${Math.floor(elapsed / 60_000)}m ago`;
  if (elapsed < ONE_DAY_MS) return `${Math.floor(elapsed / (60 * 60_000))}h ago`;
  return `${Math.floor(elapsed / ONE_DAY_MS)}d ago`;
}
