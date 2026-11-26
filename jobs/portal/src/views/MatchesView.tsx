import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  ArrowRight,
  BriefcaseBusiness,
  Building2,
  Check,
  ChevronRight,
  CircleCheck,
  CirclePause,
  Clock3,
  ExternalLink,
  Filter,
  Link2,
  MapPin,
  Plus,
  Radar,
  Search,
  SlidersHorizontal,
  Sparkles,
  Target,
  Undo2,
  TriangleAlert,
} from "lucide-react";
import type {
  AutoSubmitAuthorization,
  CandidateEventInput,
  CareerTrack,
  DiscoverySource,
  DiscoverySourceCatalogEntry,
  DiscoverySourceCatalogResponse,
  DiscoverySourceHealth,
  JobEligibilityDecision,
  JobPosting,
  JobsWorkspace,
  RunnerAvailability,
  UserJobInput,
} from "../types";
import { relativeTime, titleCase } from "../lib/format";
import { isNonActionableRequirementsLoadResult } from "../api";
import { isMatchVisibleByState, postingAgeLabel } from "../lib/posting-date";
import { Dialog } from "../components/Dialog";
import { DiscoverySourceDialog } from "../components/DiscoverySourceDialog";
import { AtsCertificationSummaryCard } from "../components/AtsCertificationSummary";
import { effectiveSubmissionMode } from "../lib/application-flow";
import { portalEligibilityDecision } from "../lib/ats-certification";
import { PORTAL_AUTHORITY_REFRESH_REASON } from "../lib/portal-authority-status";
import { matchPassReasons, passedMatchJobIds } from "../lib/candidate-events";
import {
  discoverySourceAction,
  discoverySourceState,
} from "../lib/discovery-source";
import {
  clearMatchViewFilters,
  filterMatches,
  hasMatchViewFilters,
  readMatchFilters,
  writeMatchFilters,
  type MatchFilterState,
} from "../lib/match-filters";

interface Props {
  workspace: JobsWorkspace;
  previewSearch: string;
  onAddJob(job: UserJobInput): Promise<JobPosting>;
  onVerifyOriginal(jobId: string): Promise<JobPosting>;
  onLoadRequirements(jobId: string, trackId: string): Promise<JobPosting>;
  onPrepare(job: JobPosting, mode: string, submissionMode: string): Promise<void>;
  onSaveCandidateEvent(event: CandidateEventInput): Promise<unknown>;
  onSearchDiscoverySources(query: string, trackId: string, provider: string): Promise<DiscoverySourceCatalogResponse>;
  onConnectDiscoverySource(trackId: string, entry: DiscoverySourceCatalogEntry): Promise<DiscoverySource>;
  onLoadMore(): Promise<void>;
}

export const MATCH_PAGE_SIZE = 50;

export function visibleMatches<T>(matches: T[], count: number): T[] {
  return matches.slice(0, Math.max(0, count));
}

export function loadedMatchScopeNotice(loadedCount: number, hasMore: boolean): string | null {
  if (!hasMore) return null;
  return [
    "Search, sorting, filters, and Career Track counts currently cover",
    `the ${loadedCount} loaded jobs. Load more to expand this view.`,
  ].join(" ");
}

export function MatchesView({
  workspace,
  previewSearch,
  onAddJob,
  onVerifyOriginal,
  onLoadRequirements,
  onPrepare,
  onSaveCandidateEvent,
  onSearchDiscoverySources,
  onConnectDiscoverySource,
  onLoadMore,
}: Props) {
  const [searchParams, setSearchParams] = useSearchParams();
  const [selectedSnapshot, setSelected] = useState<JobPosting | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [mode, setMode] = useState(workspace.profile.resume_mode);
  const [submissionMode, setSubmissionMode] = useState(workspace.profile.default_submission_mode);
  const [filterOpen, setFilterOpen] = useState(false);
  const [passTarget, setPassTarget] = useState<JobPosting | null>(null);
  const [passReasons, setPassReasons] = useState<string[]>([]);
  const [passNote, setPassNote] = useState("");
  const [feedbackBusy, setFeedbackBusy] = useState(false);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [visibleCount, setVisibleCount] = useState(MATCH_PAGE_SIZE);
  const [actionError, setActionError] = useState("");
  const [restoreErrorJobId, setRestoreErrorJobId] = useState("");
  const [pageBusy, setPageBusy] = useState(false);
  const [pageError, setPageError] = useState("");
  const [verifyingJobId, setVerifyingJobId] = useState<string | null>(null);
  const verificationInFlight = useRef<string | null>(null);
  const [verificationResult, setVerificationResult] = useState<{ posting: JobPosting; error: string } | null>(null);
  const [requirementsPending, setRequirementsPending] = useState<{ jobId: string; trackId: string } | null>(null);
  const requirementsInFlight = useRef<{ jobId: string; trackId: string } | null>(null);
  const [requirementsResult, setRequirementsResult] = useState<{
    jobId: string;
    trackId: string;
    error: string;
    loaded: boolean;
  } | null>(null);
  const committedDetail = useRef<JobPosting | null>(null);
  const detailActionEpoch = useRef(0);
  const committedTrackBlock = useRef<MatchDetailActionBlock | null>(null);
  const matchStatusHeading = useRef<HTMLHeadingElement | null>(null);

  const selected = matchDetailPosting(selectedSnapshot, workspace.matches);
  const selectedTrackBlock = matchDetailActionBlockReason(selected, workspace.tracks, workspace.matches);
  const selectedIsHeld = selected !== null && isPostingIdentityHeld(selected);
  const selectedNeedsRequirements = selected !== null && postingRequirementsNotLoaded(selected.eligibility);
  useLayoutEffect(() => {
    // Async verification must see a committed refresh before it can change the panel.
    if (selectedTrackBlock !== committedTrackBlock.current) {
      detailActionEpoch.current += 1;
      if (selectedTrackBlock) {
        setActionError("");
        setVerificationResult(null);
        setRequirementsResult(null);
        setRestoreErrorJobId("");
      }
    }
    committedTrackBlock.current = selectedTrackBlock;
    committedDetail.current = selectedTrackBlock ? null : selected;
    // A later page can omit this row. Retain the last accepted selection so
    // absence does not resurrect the earlier detail decision.
    if (selected !== selectedSnapshot) {
      setSelected((current) => current === selectedSnapshot ? selected : current);
    }
  }, [selected, selectedSnapshot, selectedTrackBlock]);

  const selectMatch = (job: JobPosting | null) => {
    detailActionEpoch.current += 1;
    setActionError("");
    setRestoreErrorJobId("");
    setVerificationResult(null);
    setRequirementsResult(null);
    setSelected(job);
  };

  const activeTracks = useMemo(
    () => workspace.tracks.filter((track) => track.active),
    [workspace.tracks],
  );
  const activeTrackIds = useMemo(
    () => new Set(activeTracks.map((track) => track.id)),
    [activeTracks],
  );
  const filters = useMemo(
    () => readMatchFilters(searchParams, activeTrackIds),
    [searchParams, activeTrackIds],
  );
  const {
    query,
    activeTrack,
    minimumScore,
    workplace,
    onlyUnprepared,
    showOutsideTrack,
    density,
    showPassed,
  } = filters;
  const updateFilters = (patch: Partial<MatchFilterState>) => {
    setSearchParams(
      writeMatchFilters(searchParams, { ...filters, ...patch }),
      { replace: true },
    );
  };

  const preparedJobIds = useMemo(
    () => new Set(workspace.applications.map((application) => application.job_id)),
    [workspace.applications],
  );
  const passedJobIds = useMemo(
    () => passedMatchJobIds(workspace.candidate_events),
    [workspace.candidate_events],
  );

  const activeTrackMatches = useMemo(
    () => workspace.matches.filter((job) => activeTrackIds.has(job.track_id)),
    [workspace.matches, activeTrackIds],
  );
  const stateMatches = useMemo(
    () => activeTrackMatches.filter((job) => isMatchVisibleByState(
      job,
      workspace.preferences.max_posting_age_days,
      passedJobIds.has(job.id),
      showPassed,
    )),
    [
      activeTrackMatches,
      workspace.preferences.max_posting_age_days,
      passedJobIds,
      showPassed,
    ],
  );
  const passedVisibleCount = useMemo(
    () => activeTrackMatches.filter((job) => isMatchVisibleByState(
      job,
      workspace.preferences.max_posting_age_days,
      passedJobIds.has(job.id),
      true,
    )).length,
    [activeTrackMatches, workspace.preferences.max_posting_age_days, passedJobIds],
  );
  const outsideCurrentTrackCount = useMemo(
    () => stateMatches.filter((job) => (
      (activeTrack === "all" || job.track_id === activeTrack)
      && !isMatchVisibleInDefaultView(job)
    )).length,
    [activeTrack, stateMatches],
  );
  const activeMatches = useMemo(
    () => showPassed || showOutsideTrack ? stateMatches : stateMatches.filter(isMatchVisibleInDefaultView),
    [showPassed, showOutsideTrack, stateMatches],
  );

  const filtered = useMemo(
    () => filterMatches(activeMatches, filters, preparedJobIds),
    [activeMatches, filters, preparedJobIds],
  );
  const visibleJobs = visibleMatches(filtered, visibleCount);
  const hasOnlyExcludedMatches = !showPassed && !showOutsideTrack
    && outsideCurrentTrackCount > 0
    && filtered.length === 0
    && !hasMatchViewFilters(filters);
  const hasViewFilters = hasMatchViewFilters(filters);

  useEffect(() => {
    setVisibleCount(MATCH_PAGE_SIZE);
  }, [activeTrack, query, minimumScore, workplace, onlyUnprepared, showOutsideTrack, showPassed]);

  const averageScore = filtered.length ? Math.round(filtered.reduce((sum, item) => sum + item.match_score, 0) / filtered.length) : 0;
  const activeFilterCount = Number(minimumScore > 0)
    + Number(workplace !== "all")
    + Number(onlyUnprepared)
    + Number(showOutsideTrack);
  const selectedTrack = activeTrack === "all"
    ? activeTracks[0]
    : activeTracks.find((track) => track.id === activeTrack);
  const healthySourceCount = workspace.discovery_sources.filter((source) => discoverySourceState(source) === "healthy").length;
  const configuredSourceCount = workspace.discovery_sources.length;
  const discoveryReady = healthySourceCount > 0;
  const searchState = !selectedTrack ? "paused" : discoveryReady ? "active" : "manual";
  const searchStateLabel = !selectedTrack
    ? "TRACK PAUSED"
    : discoveryReady
      ? "DISCOVERY ACTIVE"
      : configuredSourceCount > 0
        ? "UPDATES DELAYED"
        : "READY FOR A JOB LINK";
  const searchTitle = activeTrack === "all" && activeTracks.length > 1
    ? `${activeTracks.length} Career Tracks configured`
    : selectedTrack?.name || "Career Track paused";
  const trackScope = selectedTrack
    ? `${selectedTrack.role} · ${selectedTrack.locations.join(" · ") || "Location not set"}`
    : "Configure a Career Track before adding or importing matches.";
  const searchDetail = selectedTrack && !discoveryReady
    ? configuredSourceCount > 0
      ? `${trackScope}. Managed sources are behind schedule. Bluey is retrying; you can still add an urgent job link.`
      : `${trackScope}. Add a job link now; automatic discovery begins when a source is connected.`
    : trackScope;
  const emptyFilteredTitle = showPassed
    ? "No passed jobs in this view"
    : activeTrack !== "all"
      ? "No loaded jobs in this Career Track"
      : workspace.matches_page.has_more
        ? "No loaded jobs match these filters"
        : "No jobs match these filters";
  const emptyFilteredDetail = showPassed
    ? "Return to active matches or choose another Career Track."
    : workspace.matches_page.has_more
      ? "Load more to search the next results, or broaden this view."
      : "Try a broader search, lower the minimum fit, or view every active Career Track.";

  const prepare = async () => {
    if (!selected || selectedTrackBlock || busy || verificationInFlight.current
      || requirementsInFlight.current || isCandidateLead(selected)
      || passedJobIds.has(selected.id) || !jobEligibility(selected).can_prepare) return;
    const epoch = detailActionEpoch.current;
    setActionError("");
    setBusy(true);
    try {
      const job = selected;
      await onPrepare(
        job,
        mode,
        effectiveSubmissionMode(
          job,
          submissionMode,
          workspace.runner_availability,
          hasActiveAutoSubmitAuthorization(job, workspace.auto_submit_authorizations),
        ),
      );
      if (epoch === detailActionEpoch.current) selectMatch(null);
    } catch (cause) {
      if (epoch === detailActionEpoch.current) {
        setActionError(cause instanceof Error ? cause.message : "Bluey could not prepare this application.");
      }
    } finally {
      setBusy(false);
    }
  };

  const verifyOriginal = async () => {
    if (!selected || selectedTrackBlock || busy || passedJobIds.has(selected.id) || previewSearch
      || verificationInFlight.current
      || requirementsInFlight.current
      || !matchOriginalCheckMode(selected, workspace.selected_curated_verification_available)) return;
    const jobId = selected.id;
    const epoch = detailActionEpoch.current;
    verificationInFlight.current = jobId;
    setVerifyingJobId(jobId);
    setVerificationResult(null);
    try {
      const verified = await checkMatchOriginal(
        selected, workspace.selected_curated_verification_available, onVerifyOriginal, onAddJob,
      );
      if (epoch !== detailActionEpoch.current) return;
      const latest = committedDetail.current;
      const replacement = matchDetailAfterVerification(selected, latest, verified);
      if (replacement) {
        setSelected((current) => current === selected || current === latest ? replacement : current);
        setVerificationResult({ posting: replacement, error: "" });
      }
    } catch (cause) {
      const latest = committedDetail.current;
      if (epoch === detailActionEpoch.current && latest === selected) {
        setVerificationResult({
          posting: latest,
          error: cause instanceof Error ? cause.message : "Bluey could not check the original posting. Refresh Matches.",
        });
      }
    } finally {
      verificationInFlight.current = null;
      setVerifyingJobId(null);
    }
  };

  const loadRequirements = async () => {
    if (!selected || selectedTrackBlock || busy || verificationInFlight.current || requirementsInFlight.current
      || passedJobIds.has(selected.id)
      || !canLoadPostingRequirements(
        selected,
        workspace.smartrecruiters_requirements_load_available,
        previewSearch,
      )) return;
    const started = selected;
    const epoch = detailActionEpoch.current;
    const request = { jobId: started.id, trackId: started.track_id };
    requirementsInFlight.current = request;
    setRequirementsPending(request);
    setRequirementsResult(null);
    try {
      const loaded = await onLoadRequirements(started.id, started.track_id);
      if (epoch !== detailActionEpoch.current) return;
      const latest = committedDetail.current;
      const replacement = matchDetailAfterRequirementsLoad(started, latest, loaded);
      if (replacement) {
        setSelected((current) => current === started || current === latest ? replacement : current);
        setRequirementsResult({ ...request, error: "", loaded: true });
      } else if (samePostingSelection(latest, started)) {
        setRequirementsResult({
          ...request,
          error: "Bluey could not load these requirements. Try again or open the original job.",
          loaded: false,
        });
      }
    } catch {
      const latest = committedDetail.current;
      if (epoch === detailActionEpoch.current && samePostingSelection(latest, started)) {
        setRequirementsResult({
          ...request,
          error: "Bluey could not load these requirements. Try again or open the original job.",
          loaded: false,
        });
      }
    } finally {
      requirementsInFlight.current = null;
      setRequirementsPending(null);
    }
  };

  const selectedVerification = !selectedTrackBlock && verificationResult?.posting === selected
    ? verificationResult : null;
  const selectedVerificationSucceeded = Boolean(selectedVerification && !selectedVerification.error
    && selected?.availability_status === "active" && !selectedIsHeld && !isCandidateLead(selected));
  const selectedRequirementsResult = !selectedTrackBlock && selected && requirementsResult
    && requirementsResult.jobId === selected.id && requirementsResult.trackId === selected.track_id
    ? requirementsResult : null;
  const selectedRequirementsSucceeded = Boolean(selectedRequirementsResult?.loaded
    && !selectedRequirementsResult.error && !selectedNeedsRequirements);
  useEffect(() => {
    if (selectedVerificationSucceeded || selectedRequirementsSucceeded) matchStatusHeading.current?.focus();
  }, [selectedRequirementsSucceeded, selectedVerificationSucceeded]);

  const savePass = async () => {
    if (!passTarget) return;
    setFeedbackBusy(true);
    try {
      await onSaveCandidateEvent({
        event_type: "match_feedback",
        job_id: passTarget.id,
        action: "pass",
        reasons: passReasons,
        note: passNote,
      });
      setPassTarget(null);
      setPassReasons([]);
      setPassNote("");
    } finally {
      setFeedbackBusy(false);
    }
  };

  const restoreMatch = async (job: JobPosting) => {
    if (selectedTrackBlock || feedbackBusy || !passedJobIds.has(job.id)) return;
    const epoch = detailActionEpoch.current;
    setRestoreErrorJobId("");
    setFeedbackBusy(true);
    try {
      await onSaveCandidateEvent({ event_type: "match_feedback", job_id: job.id, action: "restore" });
      if (epoch === detailActionEpoch.current && samePostingSelection(committedDetail.current, job)) {
        selectMatch(null);
      }
    } catch {
      if (epoch === detailActionEpoch.current) setRestoreErrorJobId(job.id);
    } finally {
      setFeedbackBusy(false);
    }
  };

  const loadMore = async () => {
    setPageBusy(true);
    setPageError("");
    try {
      await onLoadMore();
      setVisibleCount((current) => current + MATCH_PAGE_SIZE);
    } catch (cause) {
      setPageError(
        cause instanceof Error
          ? cause.message
          : "Bluey could not load more matches right now.",
      );
    } finally {
      setPageBusy(false);
    }
  };

  const selectedCanAutoSubmit = selected
    ? canAutoSubmit(
        selected,
        workspace.runner_availability,
        workspace.auto_submit_authorizations,
      )
    : false;
  const selectedIsPassed = selected !== null && passedJobIds.has(selected.id);
  const selectedIsLead = selected !== null && isCandidateLead(selected);
  const selectedCanCheck = selected !== null && !previewSearch
    && matchOriginalCheckMode(selected, workspace.selected_curated_verification_available) !== null;
  const selectedCanLoadRequirements = selected !== null && canLoadPostingRequirements(
    selected,
    workspace.smartrecruiters_requirements_load_available,
    previewSearch,
  );
  const selectedRequirementsPending = Boolean(selected && requirementsPending
    && requirementsPending.jobId === selected.id && requirementsPending.trackId === selected.track_id);

  return (
    <div className="view-shell matches-view">
      <section className="view-heading">
        <div><p className="eyebrow">CAREER TRACKS</p><h1>Matches</h1><span>Relevant jobs from Bluey's managed feeds and employer career pages, ranked for each Career Track.</span></div>
        <button className="button primary" onClick={() => setAddOpen(true)}><Link2 size={17} />Add a job link</button>
      </section>

      <section className="metric-band">
        <div><Target /><span><b>{filtered.length}</b><small>relevant loaded</small></span></div>
        <div><Sparkles /><span><b>{averageScore ? `${averageScore}%` : "—"}</b><small>average fit</small></span></div>
        <div><BriefcaseBusiness /><span><b>{workspace.applications.filter((item) => item.state === "submitted").length}</b><small>submitted</small></span></div>
        <div className="metric-action"><span><b>{Math.max(0, workspace.entitlement.monthly_packet_limit - workspace.entitlement.used_packets)}</b><small>applications left this month</small></span><Link to={`../settings${previewSearch}#plans`}>Plan details<ChevronRight size={14} /></Link></div>
      </section>

      <section className={`search-status-band ${searchState}`} aria-label="Active search settings">
        <span className="search-status-icon"><Radar size={20} /></span>
        <div className="search-status-copy"><p><i />{searchStateLabel}</p><b>{searchTitle}</b><small>{searchDetail}</small></div>
        <dl>
          <div><dt>Postings</dt><dd>Recent + open</dd></div>
          <div><dt>Fit</dt><dd>Experience + location</dd></div>
          <div><dt>Submission</dt><dd>Review first</dd></div>
        </dl>
        <Link className="button secondary compact" to={`../settings${previewSearch}#tracks`}>Adjust search<ChevronRight size={14} /></Link>
      </section>

      <DiscoverySourceHealthList
        sources={workspace.discovery_sources}
        onAddJob={() => setAddOpen(true)}
        onWatchCompanies={() => setSourceOpen(true)}
      />

      <section className="track-strip" aria-label="Career Tracks">
        <button className={activeTrack === "all" ? "active" : ""} onClick={() => updateFilters({ activeTrack: "all" })}><span>All matches</span><b>{activeMatches.length}</b></button>
        {activeTracks.map((track) => <button key={track.id} className={activeTrack === track.id ? "active" : ""} onClick={() => updateFilters({ activeTrack: track.id })}><span>{track.name}</span><b>{activeMatches.filter((job) => job.track_id === track.id).length}</b></button>)}
        <Link className="add-track" title="Add Career Track" to={`../settings${previewSearch}#tracks`}><Plus size={15} /></Link>
      </section>

      <section className="toolbar">
        <label className="search-field"><Search size={17} /><input value={query} onChange={(event) => updateFilters({ query: event.target.value })} placeholder="Search company, role, or location" /></label>
        {(showPassed || passedVisibleCount > 0) && <button className={`button secondary compact ${showPassed ? "active-filter" : ""}`} onClick={() => updateFilters({ showPassed: !showPassed })}>{showPassed ? <Undo2 size={15} /> : null}{showPassed ? "Back to matches" : `Passed ${passedVisibleCount}`}</button>}
        <button className={`button secondary compact ${activeFilterCount ? "active-filter" : ""}`} onClick={() => setFilterOpen(true)}><Filter size={15} />Filters{activeFilterCount ? <span className="filter-count">{activeFilterCount}</span> : null}</button>
        <button className="icon-button" title={`Use ${density === "comfortable" ? "compact" : "comfortable"} rows`} onClick={() => updateFilters({ density: density === "comfortable" ? "compact" : "comfortable" })}><SlidersHorizontal size={17} /></button>
      </section>
      {workspace.matches_page.has_more && (
        <p className="pagination-scope-note" role="status">
          {loadedMatchScopeNotice(workspace.matches.length, workspace.matches_page.has_more)}
        </p>
      )}

      <section className={`job-list ${density}`} aria-label="Job matches">
        <div className="job-list-head"><span>ROLE</span><span>FIT</span><span>LOCATION</span><span>STATUS</span><span /></div>
        {visibleJobs.map((job) => (
          <button className={`job-row ${passedJobIds.has(job.id) ? "passed" : ""}`} key={job.id} onClick={() => selectMatch(job)}>
            <div className="company-mark">{job.company.slice(0, 2).toUpperCase()}</div>
            <div className="job-main"><strong>{job.title}</strong><span>{job.company} · {postingAgeLabel(job)}</span></div>
            <div className={`score score-${Math.floor(job.match_score / 10)}`}><b>{job.match_score}</b><span>%</span></div>
            <div className="job-location"><MapPin size={14} /><span>{job.location}<small>{job.workplace}</small></span></div>
            <div className={`status-pill ${matchStatusClass(job)}`}>{matchStatusLabel(job)}</div>
            <ChevronRight size={18} />
          </button>
        ))}
        {visibleJobs.length < filtered.length ? (
          <div className="job-list-more">
            <span>Showing {visibleJobs.length} of {filtered.length} loaded jobs</span>
            <button
              className="button secondary compact"
              onClick={() => setVisibleCount((current) => current + MATCH_PAGE_SIZE)}
            >
              Show {Math.min(MATCH_PAGE_SIZE, filtered.length - visibleJobs.length)} more
            </button>
          </div>
        ) : workspace.matches_page.has_more ? (
          <div className="job-list-more">
            <span>
              Showing {visibleJobs.length} relevant loaded jobs · load more to check later rows
            </span>
            <button
              className="button secondary compact"
              disabled={pageBusy}
              onClick={() => void loadMore()}
            >
              {pageBusy ? "Loading…" : `Load ${workspace.matches_page.page_size} more`}
            </button>
          </div>
        ) : null}
        {pageError && <p className="inline-error" role="alert">{pageError}</p>}
        {filtered.length === 0 && (workspace.matches_page.has_more ? (
          <div className="empty-state match-empty-state">
            <Search />
            <h3>No relevant jobs in the loaded rows yet</h3>
            <p>Later match rows are still available. Load the next bounded page before drawing an account-wide conclusion.</p>
          </div>
        ) : hasOnlyExcludedMatches ? (
          <div className="empty-state match-empty-state">
            <Filter />
            <h3>{outsideCurrentTrackCount} job{outsideCurrentTrackCount === 1 ? "" : "s"} outside this Career Track</h3>
            <p>Bluey excluded these jobs using your location, job type, experience, authorization, and other saved rules.</p>
            <div className="empty-actions">
              <button className="button primary" onClick={() => updateFilters({ showOutsideTrack: true })}>Review excluded jobs</button>
              <Link className="button secondary" to={`../settings${previewSearch}#tracks`}>Adjust Career Track</Link>
            </div>
          </div>
        ) : hasViewFilters || activeTrack !== "all" || showPassed ? (
          <div className="empty-state match-empty-state">
            <Search />
            <h3>{emptyFilteredTitle}</h3>
            <p>{emptyFilteredDetail}</p>
            <div className="empty-actions">
              <button
                className="button primary"
                onClick={() => updateFilters({
                  ...clearMatchViewFilters(filters),
                  activeTrack: "all",
                  showOutsideTrack: false,
                  showPassed: false,
                })}
              >
                Clear filters
              </button>
              <Link className="button secondary" to={`../settings${previewSearch}#tracks`}>Adjust Career Track</Link>
            </div>
          </div>
        ) : (
          <div className="empty-state match-empty-state"><Search /><h3>{selectedTrack ? "Start with a job link" : "Create a Career Track first"}</h3><p>{selectedTrack ? "Paste a recent opening. Bluey verifies it, checks your hard filters, ranks the fit, and builds the application kit for review." : "A Career Track keeps each role, location, resume, and application stream separate."}</p><div className="empty-actions">{selectedTrack && <button className="button primary" onClick={() => setAddOpen(true)}><Link2 size={16} />Add job link</button>}<Link className="button secondary" to={`../settings${previewSearch}#tracks`}>{selectedTrack ? "Adjust Career Track" : "Create Career Track"}</Link></div><ol className="match-activation-flow"><li><b>1</b><span>Verify posting</span></li><li><b>2</b><span>Check hard filters</span></li><li><b>3</b><span>Rank the fit</span></li><li><b>4</b><span>Review application kit</span></li></ol></div>
        ))}
      </section>

      <Dialog open={Boolean(selected)} title={selected ? `${selected.title} at ${selected.company}` : "Job match"} description={selected?.location} onClose={() => selectMatch(null)} size="large">
        {selected && (
          <div className="job-detail">
            <div className="job-detail-summary">
              <div className="large-score"><b>{selected.match_score}</b><span>% match</span></div>
              <div><span>{selected.workplace}</span><span>{selected.compensation || "Compensation not listed"}</span><span className="freshness-note"><Clock3 size={14} />{postingAgeLabel(selected)}</span>{!selectedNeedsRequirements && selected.last_verified_at_ms && <span>Checked {relativeTime(selected.last_verified_at_ms)}</span>}{!selectedIsHeld && <a href={selected.canonical_url} target="_blank" rel="noreferrer">Original job<ExternalLink size={14} /></a>}</div>
            </div>
            {selectedTrackBlock ? <MatchDetailReadOnlyNotice reason={selectedTrackBlock} /> : (
            <section className={`eligibility-panel ${selectedIsLead && !selectedIsHeld
              ? "candidate-lead" : `capability-${jobEligibility(selected).capability}`}`}>
              <div>
                <p>APPLICATION STATUS</p>
                <h3 ref={matchStatusHeading} tabIndex={-1}>{matchStatusLabel(selected)}</h3>
                <span>{selectedIsHeld
                  ? "Bluey won’t prepare or submit an application while the posting’s identity is unresolved."
                  : selectedNeedsRequirements
                    ? "This listing only contains a summary. Bluey needs the full requirements to prepare an application."
                    : selectedIsLead
                      ? "Bluey needs to verify the original employer posting before you can prepare an application."
                      : capabilityDescription(jobEligibility(selected).capability)}</span>
              </div>
              {jobEligibility(selected).hard_failures.length > 0 && <ul className="eligibility-reasons blocked">{jobEligibility(selected).hard_failures.map((reason) => <li key={reason.code}>{reason.message}</li>)}</ul>}
              {jobEligibility(selected).review_reasons.length > 0 && <ul className="eligibility-reasons review">{jobEligibility(selected).review_reasons.map((reason) => <li key={reason.code}>{reason.message}</li>)}</ul>}
            </section>
            )}
            {!selectedTrackBlock && !selectedIsHeld && !selectedNeedsRequirements && <AtsCertificationSummaryCard eligibility={selected.eligibility} />}
            <div className="detail-columns">
              <section><h3>Why it matched</h3><ul className="check-list">{selected.matched_reasons.map((reason) => <li key={reason}><Check size={15} />{reason}</li>)}</ul>{selected.missing_requirements.length > 0 && <><h3>Check before applying</h3><ul className="watch-list">{selected.missing_requirements.map((reason) => <li key={reason}>{reason}</li>)}</ul></>}</section>
              {!selectedTrackBlock && !selectedIsPassed && !selectedIsLead && !selectedIsHeld && jobEligibility(selected).can_prepare && <section>
                <h3>Tailored application</h3>
                <p>Prepare automatically builds a new resume for this job from your verified experience, then shows every change before anything is submitted.</p>
                <label>Resume mode</label>
                <div className="segmented"><button className={mode === "factual" ? "active" : ""} onClick={() => setMode("factual")}>Factual</button><button className={mode === "enhance" ? "active" : ""} onClick={() => setMode("enhance")}>Enhance</button></div>
                <p className="selection-help">{mode === "enhance" ? "Enhance strengthens wording and emphasizes JD-relevant skills already supported by your profile. It never invents employers, dates, credentials, or experience." : "Factual keeps your verified wording and moves the strongest relevant evidence first."}</p>
                <label>After preparation</label>
                <div className="segmented"><button className={submissionMode === "review_first" || !selectedCanAutoSubmit ? "active" : ""} onClick={() => setSubmissionMode("review_first")}>Review first</button><button disabled={!selectedCanAutoSubmit} title={autoSubmitUnavailableReason(selected, workspace.runner_availability, workspace.auto_submit_authorizations)} className={submissionMode === "auto_submit" && selectedCanAutoSubmit ? "active" : ""} onClick={() => setSubmissionMode("auto_submit")}>Auto-submit</button></div>
                {!selectedCanAutoSubmit && <p className="selection-help warning-copy">{autoSubmitUnavailableReason(selected, workspace.runner_availability, workspace.auto_submit_authorizations)}</p>}
              </section>}
            </div>
            {!selectedTrackBlock && actionError && <div className="inline-error" role="alert">{actionError}</div>}
            {selectedVerification?.error && <div className="inline-error" role="alert">{selectedVerification.error}</div>}
            {selectedRequirementsResult?.error && (
              <div className="inline-error" role="alert">{selectedRequirementsResult.error}</div>
            )}
            {selectedVerificationSucceeded && <p role="status">Original posting checked. No application was prepared.</p>}
            {selectedRequirementsSucceeded && (
              <p role="status">Requirements loaded. Review the current application status before preparing.</p>
            )}
            {!selectedTrackBlock && selectedIsPassed && restoreErrorJobId === selected.id && (
              <div className="inline-error" role="alert">Bluey could not restore this match. Try again.</div>
            )}
            {selectedTrackBlock ? null : selectedIsPassed ? (
              <PassedMatchActions busy={feedbackBusy} onRestore={() => void restoreMatch(selected)} />
            ) : selectedIsHeld ? null : selectedIsLead ? (
              <div className="dialog-actions spread">
                <p>{selectedNeedsRequirements
                  ? selectedCanLoadRequirements
                    ? "Load the full posting requirements into this match. This does not prepare an application or use your monthly application allowance."
                    : "Bluey can’t load these requirements here right now. Open the original job to review them."
                  : selectedCanCheck
                    ? "This checks the original posting and updates the match. It doesn’t prepare an application or use your monthly application allowance."
                    : "Bluey can’t verify this listing here right now. You can open the original job to review it."}</p>
                <div>
                  <button type="button" className="button secondary"
                    disabled={verifyingJobId === selected.id || selectedRequirementsPending}
                    onClick={() => { setPassTarget(selected); selectMatch(null); }}>Pass</button>
                  {selectedNeedsRequirements ? (
                    <RequirementsLoadAction
                      available={selectedCanLoadRequirements}
                      pending={selectedRequirementsPending}
                      blocked={busy || verifyingJobId !== null || requirementsPending !== null}
                      onLoad={() => void loadRequirements()}
                    />
                  ) : (
                    <OriginalPostingVerificationAction
                      available={selectedCanCheck}
                      pending={verifyingJobId === selected.id}
                      blocked={busy || verifyingJobId !== null || requirementsPending !== null}
                      onVerify={() => void verifyOriginal()}
                    />
                  )}
                </div>
              </div>
            ) : (
              <div className="dialog-actions spread"><p>{!jobEligibility(selected).can_prepare
                ? "Review the current application status above. No application can be prepared from this match right now."
                : jobEligibility(selected).capability === "handoff" || jobEligibility(selected).capability === "unknown_review"
                  ? "Bluey prepares the application kit for your review; this site stays user-controlled."
                  : "This application uses one monthly allowance when approved, downloaded, or queued."}</p><div><button className="button secondary" disabled={selectedRequirementsPending} onClick={() => { setPassTarget(selected); selectMatch(null); }}>Pass</button><button type="button" className="button primary" aria-busy={busy || selectedRequirementsPending} disabled={busy || selectedRequirementsPending || !jobEligibility(selected).can_prepare} onClick={() => void prepare()}>{selectedRequirementsPending ? "Loading requirements…" : busy ? "Preparing…" : jobEligibility(selected).can_prepare ? "Prepare application" : "Preparation unavailable"}<ArrowRight size={17} aria-hidden="true" /></button></div></div>
            )}
          </div>
        )}
      </Dialog>

      <Dialog open={Boolean(passTarget)} title="Pass on this match?" description={passTarget ? `${passTarget.title} at ${passTarget.company}` : ""} onClose={() => setPassTarget(null)}>
        <div className="feedback-dialog">
          <p>Tell Bluey why so future matches get better. This does not change your Career Track automatically.</p>
          <div className="choice-chips" role="group" aria-label="Reasons for passing">
            {matchPassReasons.map(([value, label]) => <button key={value} type="button" className={passReasons.includes(value) ? "active" : ""} onClick={() => setPassReasons((current) => current.includes(value) ? current.filter((item) => item !== value) : [...current, value])}>{label}</button>)}
          </div>
          <label><span>Note <small>optional</small></span><textarea value={passNote} maxLength={1000} rows={3} onChange={(event) => setPassNote(event.target.value)} placeholder="Anything Bluey should remember about this match" /></label>
        </div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setPassTarget(null)}>Keep match</button><button className="button primary" disabled={feedbackBusy} onClick={() => void savePass()}>{feedbackBusy ? "Saving..." : "Pass on match"}</button></div>
      </Dialog>

      <AddJobDialog open={addOpen} onClose={() => setAddOpen(false)} trackId={selectedTrack?.id || ""} onSave={async (job) => { const saved = await onAddJob(job); setAddOpen(false); selectMatch(saved); }} />
      <DiscoverySourceDialog
        open={sourceOpen}
        tracks={activeTracks}
        initialTrackId={selectedTrack?.id || ""}
        onClose={() => setSourceOpen(false)}
        onSearch={onSearchDiscoverySources}
        onConnect={onConnectDiscoverySource}
      />
      <Dialog open={filterOpen} title="Filter matches" description="Narrow this view without changing your Career Track." onClose={() => setFilterOpen(false)}>
        <div className="dialog-form"><label><span>Minimum match score</span><div className="range-field"><input type="range" min="0" max="100" step="5" value={minimumScore} onChange={(event) => updateFilters({ minimumScore: Number(event.target.value) })} /><b>{minimumScore || "Any"}{minimumScore ? "%" : ""}</b></div></label><label><span>Workplace</span><select value={workplace} onChange={(event) => updateFilters({ workplace: event.target.value as MatchFilterState["workplace"] })}><option value="all">Any workplace</option><option value="remote">Remote</option><option value="hybrid">Hybrid</option><option value="on-site">On-site</option></select></label><label className="setting-line simple"><div><b>Only jobs not prepared</b><span>Hide applications you already prepared.</span></div><button type="button" className={`toggle ${onlyUnprepared ? "on" : ""}`} role="switch" aria-checked={onlyUnprepared} onClick={() => updateFilters({ onlyUnprepared: !onlyUnprepared })}><span /></button></label><label className="setting-line simple"><div><b>Show jobs outside my rules</b><span>{outsideCurrentTrackCount ? `Review ${outsideCurrentTrackCount} job${outsideCurrentTrackCount === 1 ? "" : "s"} excluded by location, job type, experience, authorization, or other Career Track rules.` : "No jobs are currently excluded by your Career Track rules."}</span></div><button type="button" className={`toggle ${showOutsideTrack ? "on" : ""}`} role="switch" aria-checked={showOutsideTrack} onClick={() => updateFilters({ showOutsideTrack: !showOutsideTrack })}><span /></button></label></div>
        <div className="dialog-actions">
          <button
            className="button secondary"
            onClick={() => updateFilters({
              ...clearMatchViewFilters(filters),
              showOutsideTrack: false,
            })}
          >
            Reset
          </button>
          <button className="button primary" onClick={() => setFilterOpen(false)}>
            Show {filtered.length} loaded match{filtered.length === 1 ? "" : "es"}
          </button>
        </div>
      </Dialog>
    </div>
  );
}

export function PassedMatchActions({ busy, onRestore }: { busy: boolean; onRestore: () => void }) {
  return (
    <div className="dialog-actions spread">
      <p>Restore this match to reconsider it. Your Career Track rules still apply.</p>
      <button className="button secondary" type="button" disabled={busy} onClick={onRestore}>
        <Undo2 size={16} aria-hidden="true" />{busy ? "Restoring…" : "Restore match"}
      </button>
    </div>
  );
}

export function DiscoverySourceHealthList({
  sources,
  onAddJob,
  onWatchCompanies,
}: {
  sources: DiscoverySource[];
  onAddJob(): void;
  onWatchCompanies(): void;
}) {
  const healthyCount = sources.filter((source) => discoverySourceState(source) === "healthy").length;

  return (
    <section className="discovery-health" aria-labelledby="discovery-health-title">
      <header>
        <div>
          <p>DISCOVERY SOURCES</p>
          <h2 id="discovery-health-title">Source health</h2>
        </div>
        <div className="discovery-header-actions">
          {sources.length > 0 && <span>{healthyCount} of {sources.length} healthy</span>}
          <button className="button secondary compact" onClick={onWatchCompanies}><Radar size={15} />Watch companies</button>
        </div>
      </header>
      {sources.length === 0 ? (
        <div className="discovery-source-empty">
          <Radar size={18} aria-hidden="true" />
          <div>
            <strong>Automatic discovery is not connected</strong>
            <span>Connect employer career pages for automatic checks, or add a job link you already found.</span>
          </div>
          <div className="discovery-source-empty-actions">
            <button className="button primary compact" onClick={onWatchCompanies}><Radar size={15} />Watch companies</button>
            <button className="button secondary compact" onClick={onAddJob}><Link2 size={15} />Add job link</button>
          </div>
        </div>
      ) : (
        <>
          <div className="discovery-source-columns" aria-hidden="true">
            <span>Source</span><span>State</span><span>Last successful sync</span><span>Action</span>
          </div>
          <ul className="discovery-source-list">
            {sources.map((source) => {
              const state = discoverySourceState(source);
              return (
                <li key={source.id}>
                  <div className="discovery-source-identity">
                    <Building2 size={17} aria-hidden="true" />
                    <span><strong>{source.provider === "curated_feed" ? "Career feeds" : source.config.company || "Company not provided"}</strong><small>{source.provider === "curated_feed" ? "Public, allowlisted feeds" : titleCase(source.provider) || "Provider"}</small></span>
                  </div>
                  <div className={`discovery-source-state ${state}`}>
                    <span className="sr-only">State: </span><DiscoverySourceStateIcon state={state} />{titleCase(state)}
                  </div>
                  <div className="discovery-source-sync">
                    <span className="sr-only">Last successful sync: </span>
                    {source.last_success_at_ms ? (
                      <time dateTime={new Date(source.last_success_at_ms).toISOString()} title={new Date(source.last_success_at_ms).toLocaleString()}>{relativeTime(source.last_success_at_ms)}</time>
                    ) : "Not synced yet"}
                  </div>
                  <p className={`discovery-source-action ${state}`}><span className="sr-only">Action: </span>{discoverySourceAction(state)}</p>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}

function DiscoverySourceStateIcon({ state }: { state: DiscoverySourceHealth }) {
  if (state === "healthy") return <CircleCheck size={15} aria-hidden="true" />;
  if (state === "paused") return <CirclePause size={15} aria-hidden="true" />;
  if (state === "degraded") return <TriangleAlert size={15} aria-hidden="true" />;
  return <Clock3 size={15} aria-hidden="true" />;
}

export function jobEligibility(job: JobPosting) {
  const decision = portalEligibilityDecision(job.eligibility);
  if (!hasPostingIdentityHold(decision) && !hasUnloadedRequirements(decision)) return decision;
  return {
    ...decision,
    capability: "blocked",
    can_prepare: false,
    can_auto_submit: false,
    can_queue_local: false,
    can_queue_cloud: false,
  } satisfies JobEligibilityDecision;
}

function hasUnloadedRequirements(decision: JobEligibilityDecision): boolean {
  return decision.review_reasons.some(({ code }) => code === "posting_requirements_not_loaded");
}

export function postingRequirementsNotLoaded(value: JobPosting["eligibility"]): boolean {
  return hasUnloadedRequirements(portalEligibilityDecision(value));
}

type MatchDetailTrackBlock = "inactive" | "refresh_required";
type MatchDetailActionBlock = MatchDetailTrackBlock | "match_unconfirmed";

export function matchDetailTrackBlockReason(
  job: Pick<JobPosting, "track_id"> | null,
  tracks: readonly CareerTrack[],
): MatchDetailTrackBlock | null {
  if (!job) return null;
  const track = tracks.find(({ id }) => id === job.track_id);
  if (!track || track.active !== true) return "inactive";
  const reasons = track.policy.authority?.review_reason_codes;
  return Array.isArray(reasons) && reasons.includes(PORTAL_AUTHORITY_REFRESH_REASON)
    ? "refresh_required" : null;
}

export function matchDetailActionBlockReason(
  job: JobPosting | null, tracks: readonly CareerTrack[], matches: readonly JobPosting[],
): MatchDetailActionBlock | null {
  const trackBlock = matchDetailTrackBlockReason(job, tracks);
  if (trackBlock || !job) return trackBlock;
  // Pagination can retain the last detail for display. A missing or
  // retargeted workspace row cannot still authorize that detail's actions.
  const current = matches.find((match) => samePostingSelection(match, job));
  return current?.source === job.source && current.canonical_url === job.canonical_url
    ? null : "match_unconfirmed";
}

export function MatchDetailReadOnlyNotice({ reason }: { reason: MatchDetailActionBlock }) {
  return <section className="eligibility-panel capability-blocked" role="status">
    <div>
      <p>APPLICATION STATUS</p>
      <h3>Match details are read-only</h3>
      <span>{reason === "inactive"
        ? "This Career Track is paused or no longer available."
        : reason === "refresh_required"
          ? "Refresh Matches to use the current Career Track."
          : "This posting isn’t in your current Matches. Reopen it from the refreshed list to take action."}</span>
    </div>
  </section>;
}

function samePostingSelection(left: JobPosting | null, right: JobPosting): boolean {
  return left?.id === right.id && left.track_id === right.track_id;
}

export function canLoadPostingRequirements(
  job: JobPosting,
  available: unknown,
  previewSearch: string,
): boolean {
  return available === true && previewSearch === "" && postingRequirementsNotLoaded(job.eligibility);
}

function hasPostingIdentityHold(decision: JobEligibilityDecision): boolean {
  return decision.hard_failures.some(({ code }) => code === "workday_reconciliation_required");
}

function isPostingIdentityHeld(job: JobPosting): boolean {
  return hasPostingIdentityHold(portalEligibilityDecision(job.eligibility));
}

export function matchDetailPosting(snapshot: JobPosting | null, matches: JobPosting[]): JobPosting | null {
  if (!snapshot) return null;
  const current = matches.find((job) => job.id === snapshot.id && job.track_id === snapshot.track_id);
  if (!current) return snapshot;
  // Identity holds always take precedence. For ordinary refreshes, use the
  // server's eligibility evaluation, not when the posting text last changed.
  if (isPostingIdentityHeld(current)) return current;
  if (isPostingIdentityHeld(snapshot)) return snapshot;
  const previous = jobEligibility(snapshot);
  const refreshed = jobEligibility(current);
  if (refreshed.evaluated_at_ms > previous.evaluated_at_ms) return current;
  if (refreshed.evaluated_at_ms < previous.evaluated_at_ms || refreshed.evaluated_at_ms === 0) return snapshot;
  // Same-millisecond responses cannot be ordered. A stricter read can veto
  // prior availability, but a tied read must not grant another action.
  const actions = ["can_prepare", "can_queue_cloud", "can_auto_submit"] as const;
  return actions.every((key) => !refreshed[key] || previous[key])
    && actions.some((key) => previous[key] && !refreshed[key]) ? current : snapshot;
}

export function matchDetailAfterVerification(
  started: JobPosting,
  latest: JobPosting | null,
  verified: JobPosting,
): JobPosting | null {
  if (!latest || latest.id !== started.id || latest.track_id !== started.track_id
    || verified.track_id !== started.track_id || verified.canonical_url !== started.canonical_url
    || isPostingIdentityHeld(latest)) {
    return null;
  }
  if (latest === started) return verified;
  // A supported link import may create a different normalized ID; only
  // replace the original selection while its exact starting detail is
  // current or the committed readback still names the result's exact target.
  return latest.id === verified.id && latest.source === verified.source
    && latest.canonical_url === verified.canonical_url
    && matchDetailPosting(latest, [verified]) === verified ? verified : null;
}

export function matchDetailAfterRequirementsLoad(
  started: JobPosting,
  latest: JobPosting | null,
  loaded: JobPosting,
): JobPosting | null {
  if (!latest || latest.id !== started.id || latest.track_id !== started.track_id
    || loaded.id !== started.id || loaded.track_id !== started.track_id
    || latest.source !== started.source || latest.canonical_url !== started.canonical_url
    || loaded.source !== started.source || loaded.canonical_url !== started.canonical_url
    || !isNonActionableRequirementsLoadResult(loaded)
    || isPostingIdentityHeld(latest) || isPostingIdentityHeld(loaded)) return null;
  // App keeps the endpoint object out of workspace state and resolves only
  // after its authenticated workspace readback. Prefer that committed row when
  // it has replaced the summary; the fallback is display-only and explicitly
  // non-actionable, not acknowledgement of a workspace update.
  const replacement = latest === started ? loaded : latest;
  return typeof replacement.description !== "string" || replacement.description.trim() === ""
    || postingRequirementsNotLoaded(replacement.eligibility) ? null : replacement;
}

export function isMatchVisibleInDefaultView(job: JobPosting): boolean {
  if (!job.eligibility || isCandidateLead(job)) return true;
  const decision = jobEligibility(job);
  return decision.can_prepare || hasPostingIdentityHold(decision);
}

export function isCandidateLead(
  job: Pick<JobPosting, "source" | "availability_status" | "last_verified_at_ms" | "eligibility">,
): boolean {
  return isSavedCuratedLead(job)
    || postingRequirementsNotLoaded(job.eligibility)
    || (job.availability_status === "unknown" && !job.last_verified_at_ms);
}

export function isSavedCuratedLead(job: Pick<JobPosting, "source">): boolean {
  return job.source.startsWith("curated_feed:");
}

export function canVerifySavedCuratedLead(
  job: Pick<JobPosting, "source" | "availability_status">,
  available: unknown,
): boolean {
  return available === true && isSavedCuratedLead(job) && job.availability_status === "unknown";
}

export function matchOriginalCheckMode(job: JobPosting, curatedAvailable: unknown): "curated" | "import" | null {
  // Summary hydration must use an explicitly admitted stored-ID action,
  // never the generic imported-link route.
  if (isPostingIdentityHeld(job) || postingRequirementsNotLoaded(job.eligibility)) return null;
  if (isSavedCuratedLead(job)) return canVerifySavedCuratedLead(job, curatedAvailable) ? "curated" : null;
  return isCandidateLead(job) ? "import" : null;
}

export async function checkMatchOriginal(
  job: JobPosting,
  curatedAvailable: unknown,
  verifyCurated: Props["onVerifyOriginal"],
  importSupported: Props["onAddJob"],
): Promise<JobPosting> {
  const mode = matchOriginalCheckMode(job, curatedAvailable);
  if (!mode) throw new Error("Original posting verification is not available for this match.");
  // Blank manual details make an unsupported URL a refusal instead of a pasted-link fallback.
  const verified = mode === "curated" ? await verifyCurated(job.id) : await importSupported({
    canonical_url: job.canonical_url,
    track_id: job.track_id,
    pasted_description: "",
    company: "",
    title: "",
    location: "",
    workplace: "Unknown",
    compensation: "",
  });
  if (verified && verified.canonical_url !== job.canonical_url) {
    throw new Error("Bluey returned a different posting link. Reopen the result from Matches.");
  }
  if (!verified || typeof verified.source !== "string" || verified.track_id !== job.track_id
    || (mode === "curated" && verified.id !== job.id)
    || isCandidateLead(verified) || verified.availability_status !== "active"
    || !(typeof verified.last_verified_at_ms === "number" && Number.isFinite(verified.last_verified_at_ms)
      && verified.last_verified_at_ms > 0)) {
    throw new Error("Bluey couldn’t verify the original posting. No application was prepared.");
  }
  return verified;
}

export function OriginalPostingVerificationAction({ available, pending, blocked, onVerify }: {
  available: boolean;
  pending: boolean;
  blocked: boolean;
  onVerify(): void;
}) {
  if (!available) return null;
  return <button type="button" className="button primary match-verify-button" disabled={blocked || pending}
    aria-busy={pending} onClick={onVerify}>
    {pending ? "Checking posting…" : "Verify original posting"}
  </button>;
}

export function RequirementsLoadAction({ available, pending, blocked, onLoad }: {
  available: boolean;
  pending: boolean;
  blocked: boolean;
  onLoad(): void;
}) {
  if (!available) return null;
  return <button type="button" className="button primary match-verify-button"
    disabled={blocked || pending} aria-busy={pending} onClick={onLoad}>
    {pending ? "Loading requirements…" : "Load requirements"}
  </button>;
}

function matchStatusLabel(job: JobPosting): string {
  if (isPostingIdentityHeld(job)) return "Posting needs review";
  if (postingRequirementsNotLoaded(job.eligibility)) return "Lead · Requirements not loaded";
  if (isCandidateLead(job)) return "Lead · Verify first";
  if (!jobEligibility(job).can_prepare) return "Outside your rules";
  return capabilityLabel(jobEligibility(job).capability);
}

function matchStatusClass(job: JobPosting): string {
  if (isPostingIdentityHeld(job)) return "capability-blocked";
  if (isCandidateLead(job)) return "candidate-lead";
  if (!jobEligibility(job).can_prepare) return "capability-blocked";
  return `capability-${jobEligibility(job).capability}`;
}

function capabilityLabel(capability: ReturnType<typeof jobEligibility>["capability"]): string {
  if (capability === "certified") return "Certified";
  if (capability === "beta_review") return "Beta · Review first";
  if (capability === "handoff") return "Handoff";
  if (capability === "blocked") return "Blocked";
  return "Review only";
}

function capabilityDescription(capability: ReturnType<typeof jobEligibility>["capability"]): string {
  if (capability === "certified") return "This application system passed runner certification and your server-side rules.";
  if (capability === "beta_review") return "Bluey can use a runner after you inspect and approve the application kit.";
  if (capability === "handoff") return "Bluey prepares the exact resume and answers, then you finish on the job site.";
  if (capability === "blocked") return "Bluey will not prepare or open this listing.";
  return "Bluey can prepare a kit, but this application system is not certified for runner submission.";
}

export function hasActiveAutoSubmitAuthorization(
  job: Pick<JobPosting, "track_id">,
  authorizations: AutoSubmitAuthorization[],
): boolean {
  return authorizations.some(
    (authorization) =>
      authorization.career_track_id === job.track_id
      && authorization.status === "active",
  );
}

export function canAutoSubmit(
  job: JobPosting,
  runners: RunnerAvailability,
  authorizations: AutoSubmitAuthorization[],
): boolean {
  const eligibility = jobEligibility(job);
  const certifiedRunnerAvailable = eligibility.can_queue_cloud && runners.cloud.available;
  return eligibility.can_auto_submit
    && certifiedRunnerAvailable
    && runners.auto_submit_available
    && hasActiveAutoSubmitAuthorization(job, authorizations);
}

export function autoSubmitUnavailableReason(
  job: JobPosting,
  runners?: RunnerAvailability,
  authorizations: AutoSubmitAuthorization[] = [],
): string | undefined {
  const eligibility = jobEligibility(job);
  if (hasPostingIdentityHold(eligibility)) return "The posting’s identity needs review before Bluey can apply.";
  if (eligibility.hard_failures.length > 0) return "Resolve the Career Track rules above before Auto-submit can be considered.";
  if (eligibility.capability === "beta_review") return "Review first is required while this application system is in beta.";
  if (eligibility.capability === "handoff") return "This site uses a user-controlled handoff after Bluey prepares the application kit.";
  if (eligibility.capability === "unknown_review") {
    return eligibility.review_reasons.find(
      (reason) => reason.code === "ats_certification_unavailable",
    )?.message || "Review first is required because this application system is not certified.";
  }
  if (eligibility.capability === "blocked") return "This listing cannot use a Bluey runner.";
  if (!eligibility.can_queue_cloud) {
    return "This application system is not currently certified for cloud automation.";
  }
  if (!eligibility.can_auto_submit) return "Auto-submit is available only after every server rule and application-system check passes.";
  const authorization = authorizations.find(
    (item) => item.career_track_id === job.track_id,
  );
  if (authorization?.status === "needs_review") {
    return "This Career Track changed. Review and enable Auto-submit again in Settings.";
  }
  if (!authorization || authorization.status !== "active") {
    return "Enable Auto-submit for this Career Track in Settings first.";
  }
  if (runners && !runners.cloud.available) {
    return runners.cloud.reason || runners.cloud.next_action;
  }
  if (runners && !runners.auto_submit_available) return runners.auto_submit_reason;
  if (eligibility.can_auto_submit) return undefined;
  return "Auto-submit is available only after every server rule and application-system check passes.";
}

export { isMatchVisibleByState, isRecentPosting, postingAgeLabel } from "../lib/posting-date";

export const JOB_IMPORT_DESCRIPTION = "Paste a direct employer link. Bluey imports supported ATS facts, checks freshness, and scores the role.";
export const JOB_IMPORT_FALLBACK_LABEL = "Can't import this link? Enter details manually";
export const JOB_IMPORT_ACTION_LABEL = "Import & score";

function AddJobDialog({ open, onClose, onSave, trackId }: { open: boolean; onClose(): void; onSave(job: UserJobInput): Promise<void>; trackId: string }) {
  const [url, setUrl] = useState("");
  const [company, setCompany] = useState("");
  const [title, setTitle] = useState("");
  const [location, setLocation] = useState("");
  const [description, setDescription] = useState("");
  const [manual, setManual] = useState(false);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const submit = async () => {
    if (!url.trim() || (manual && (!company.trim() || !title.trim()))) return;
    setError("");
    setSaving(true);
    try {
      await onSave({
        canonical_url: url.trim(),
        pasted_description: description,
        company: company.trim(),
        title: title.trim(),
        location: location.trim(),
        workplace: location.toLowerCase().includes("remote") ? "Remote" : "Unknown",
        compensation: "",
        track_id: trackId,
      });
      setUrl(""); setCompany(""); setTitle(""); setLocation(""); setDescription("");
      setManual(false);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not import that job.");
      setManual(true);
    } finally { setSaving(false); }
  };
  return (
    <Dialog open={open} title="Add a job link" description={JOB_IMPORT_DESCRIPTION} onClose={onClose}>
      <div className="dialog-form">
        <label><span>Job link</span><input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="https://company.com/jobs/..." autoFocus /></label>
        <button type="button" className="text-button" onClick={() => { setManual((current) => !current); setError(""); }}>{manual ? "Hide manual details" : JOB_IMPORT_FALLBACK_LABEL}</button>
        {manual && <>
          <div className="form-grid two"><label><span>Company</span><input value={company} onChange={(event) => setCompany(event.target.value)} /></label><label><span>Role</span><input value={title} onChange={(event) => setTitle(event.target.value)} /></label></div>
          <label><span>Location</span><input value={location} onChange={(event) => setLocation(event.target.value)} placeholder="New York, NY or Remote - US" /></label>
          <label><span>Job description</span><textarea rows={6} value={description} onChange={(event) => setDescription(event.target.value)} placeholder="Paste the job description so Bluey can score the fit." /></label>
          <p className="field-note">Manual jobs remain Review only until Bluey verifies the listing.</p>
        </>}
        {error && <div className="inline-error" role="alert">{error}</div>}
      </div>
      <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !url.trim() || (manual && (!company.trim() || !title.trim()))} onClick={() => void submit()}>{saving ? "Importing..." : JOB_IMPORT_ACTION_LABEL}<ArrowRight size={16} /></button></div>
    </Dialog>
  );
}
