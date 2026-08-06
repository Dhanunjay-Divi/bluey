import { useEffect, useMemo, useState } from "react";
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
  DiscoverySource,
  DiscoverySourceCatalogEntry,
  DiscoverySourceCatalogResponse,
  DiscoverySourceHealth,
  JobPosting,
  JobsWorkspace,
  RunnerAvailability,
  UserJobInput,
} from "../types";
import { relativeTime, titleCase } from "../lib/format";
import { Dialog } from "../components/Dialog";
import { DiscoverySourceDialog } from "../components/DiscoverySourceDialog";
import { AtsCertificationSummaryCard } from "../components/AtsCertificationSummary";
import { effectiveSubmissionMode } from "../lib/application-flow";
import { portalEligibilityDecision } from "../lib/ats-certification";
import { isJobPassed, matchPassReasons } from "../lib/candidate-events";
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
  onPrepare(job: JobPosting, mode: string, submissionMode: string): Promise<void>;
  onSaveCandidateEvent(event: CandidateEventInput): Promise<unknown>;
  onSearchDiscoverySources(query: string, trackId: string, provider: string): Promise<DiscoverySourceCatalogResponse>;
  onConnectDiscoverySource(trackId: string, entry: DiscoverySourceCatalogEntry): Promise<DiscoverySource>;
}

export const MATCH_PAGE_SIZE = 50;

export function visibleMatches<T>(matches: T[], count: number): T[] {
  return matches.slice(0, Math.max(0, count));
}

export function MatchesView({
  workspace,
  previewSearch,
  onAddJob,
  onPrepare,
  onSaveCandidateEvent,
  onSearchDiscoverySources,
  onConnectDiscoverySource,
}: Props) {
  const [searchParams, setSearchParams] = useSearchParams();
  const [selected, setSelected] = useState<JobPosting | null>(null);
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
    () => new Set(workspace.matches.filter((job) => isJobPassed(workspace.candidate_events, job.id)).map((job) => job.id)),
    [workspace.matches, workspace.candidate_events],
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
      && !isMatchEligibleForDefaultView(job)
    )).length,
    [activeTrack, stateMatches],
  );
  const activeMatches = useMemo(
    () => showOutsideTrack ? stateMatches : stateMatches.filter(isMatchEligibleForDefaultView),
    [showOutsideTrack, stateMatches],
  );

  const filtered = useMemo(
    () => filterMatches(activeMatches, filters, preparedJobIds),
    [activeMatches, filters, preparedJobIds],
  );
  const visibleJobs = visibleMatches(filtered, visibleCount);
  const hasOnlyExcludedMatches = !showOutsideTrack
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

  const prepare = async () => {
    if (!selected) return;
    setActionError("");
    setBusy(true);
    try {
      let job = selected;
      if (isCandidateLead(job)) {
        job = await onAddJob({
          canonical_url: job.canonical_url,
          pasted_description: "",
          company: "",
          title: "",
          location: "",
          workplace: "Unknown",
          compensation: "",
          track_id: job.track_id,
        });
        setSelected(job);
      }
      const eligibility = jobEligibility(job);
      if (!eligibility.can_prepare) {
        setActionError("Bluey checked the original job, but it still needs attention before an application can be prepared.");
        return;
      }
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
      setSelected(null);
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Bluey could not verify this job right now.");
    } finally {
      setBusy(false);
    }
  };

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
    setFeedbackBusy(true);
    try {
      await onSaveCandidateEvent({ event_type: "match_feedback", job_id: job.id, action: "restore" });
      setSelected(null);
    } finally {
      setFeedbackBusy(false);
    }
  };

  const selectedCanAutoSubmit = selected
    ? canAutoSubmit(
        selected,
        workspace.runner_availability,
        workspace.auto_submit_authorizations,
      )
    : false;

  return (
    <div className="view-shell matches-view">
      <section className="view-heading">
        <div><p className="eyebrow">CAREER TRACKS</p><h1>Matches</h1><span>Relevant jobs from Bluey's managed feeds and employer career pages, ranked for each Career Track.</span></div>
        <button className="button primary" onClick={() => setAddOpen(true)}><Link2 size={17} />Add a job link</button>
      </section>

      <section className="metric-band">
        <div><Target /><span><b>{filtered.length}</b><small>relevant jobs</small></span></div>
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
        {passedVisibleCount > 0 && <button className={`button secondary compact ${showPassed ? "active-filter" : ""}`} onClick={() => updateFilters({ showPassed: !showPassed })}>{showPassed ? <Undo2 size={15} /> : null}{showPassed ? "Back to matches" : `Passed ${passedVisibleCount}`}</button>}
        <button className={`button secondary compact ${activeFilterCount ? "active-filter" : ""}`} onClick={() => setFilterOpen(true)}><Filter size={15} />Filters{activeFilterCount ? <span className="filter-count">{activeFilterCount}</span> : null}</button>
        <button className="icon-button" title={`Use ${density === "comfortable" ? "compact" : "comfortable"} rows`} onClick={() => updateFilters({ density: density === "comfortable" ? "compact" : "comfortable" })}><SlidersHorizontal size={17} /></button>
      </section>

      <section className={`job-list ${density}`} aria-label="Job matches">
        <div className="job-list-head"><span>ROLE</span><span>FIT</span><span>LOCATION</span><span>STATUS</span><span /></div>
        {visibleJobs.map((job) => (
          <button className={`job-row ${passedJobIds.has(job.id) ? "passed" : ""}`} key={job.id} onClick={() => { setActionError(""); setSelected(job); }}>
            <div className="company-mark">{job.company.slice(0, 2).toUpperCase()}</div>
            <div className="job-main"><strong>{job.title}</strong><span>{job.company} · {postingAgeLabel(job)}</span></div>
            <div className={`score score-${Math.floor(job.match_score / 10)}`}><b>{job.match_score}</b><span>%</span></div>
            <div className="job-location"><MapPin size={14} /><span>{job.location}<small>{job.workplace}</small></span></div>
            <div className={`status-pill ${matchStatusClass(job)}`}>{matchStatusLabel(job)}</div>
            <ChevronRight size={18} />
          </button>
        ))}
        {visibleJobs.length < filtered.length && <div className="job-list-more"><span>Showing {visibleJobs.length} of {filtered.length} relevant jobs</span><button className="button secondary compact" onClick={() => setVisibleCount((current) => current + MATCH_PAGE_SIZE)}>Show {Math.min(MATCH_PAGE_SIZE, filtered.length - visibleJobs.length)} more</button></div>}
        {filtered.length === 0 && (hasOnlyExcludedMatches ? (
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
            <h3>{showPassed ? "No passed jobs in this view" : activeTrack !== "all" ? "No jobs in this Career Track" : "No jobs match these filters"}</h3>
            <p>{showPassed ? "Return to active matches or choose another Career Track." : "Try a broader search, lower the minimum fit, or view every active Career Track."}</p>
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

      <Dialog open={Boolean(selected)} title={selected ? `${selected.title} at ${selected.company}` : "Job match"} description={selected?.location} onClose={() => { setSelected(null); setActionError(""); }} size="large">
        {selected && (
          <div className="job-detail">
            <div className="job-detail-summary">
              <div className="large-score"><b>{selected.match_score}</b><span>% match</span></div>
              <div><span>{selected.workplace}</span><span>{selected.compensation || "Compensation not listed"}</span><span className="freshness-note"><Clock3 size={14} />{postingAgeLabel(selected)}</span>{selected.last_verified_at_ms && <span>Checked {relativeTime(selected.last_verified_at_ms)}</span>}<a href={selected.canonical_url} target="_blank" rel="noreferrer">Original job<ExternalLink size={14} /></a></div>
            </div>
            <section className={`eligibility-panel ${isCandidateLead(selected) ? "candidate-lead" : `capability-${jobEligibility(selected).capability}`}`}>
              <div><p>APPLICATION STATUS</p><h3>{matchStatusLabel(selected)}</h3><span>{isCandidateLead(selected) ? "Bluey found this role in a managed feed. The original employer page must pass a fresh check before preparation." : capabilityDescription(jobEligibility(selected).capability)}</span></div>
              {jobEligibility(selected).hard_failures.length > 0 && <ul className="eligibility-reasons blocked">{jobEligibility(selected).hard_failures.map((reason) => <li key={reason.code}>{reason.message}</li>)}</ul>}
              {jobEligibility(selected).review_reasons.length > 0 && <ul className="eligibility-reasons review">{jobEligibility(selected).review_reasons.map((reason) => <li key={reason.code}>{reason.message}</li>)}</ul>}
            </section>
            <AtsCertificationSummaryCard eligibility={selected.eligibility} />
            <div className="detail-columns">
              <section><h3>Why it matched</h3><ul className="check-list">{selected.matched_reasons.map((reason) => <li key={reason}><Check size={15} />{reason}</li>)}</ul>{selected.missing_requirements.length > 0 && <><h3>Check before applying</h3><ul className="watch-list">{selected.missing_requirements.map((reason) => <li key={reason}>{reason}</li>)}</ul></>}</section>
              <section>
                <h3>Tailored application</h3>
                <p>Prepare automatically builds a new resume for this job from your verified experience, then shows every change before anything is submitted.</p>
                <label>Resume mode</label>
                <div className="segmented"><button className={mode === "factual" ? "active" : ""} onClick={() => setMode("factual")}>Factual</button><button className={mode === "enhance" ? "active" : ""} onClick={() => setMode("enhance")}>Enhance</button></div>
                <p className="selection-help">{mode === "enhance" ? "Enhance strengthens wording and emphasizes JD-relevant skills already supported by your profile. It never invents employers, dates, credentials, or experience." : "Factual keeps your verified wording and moves the strongest relevant evidence first."}</p>
                <label>After preparation</label>
                <div className="segmented"><button className={submissionMode === "review_first" || !selectedCanAutoSubmit ? "active" : ""} onClick={() => setSubmissionMode("review_first")}>Review first</button><button disabled={!selectedCanAutoSubmit} title={autoSubmitUnavailableReason(selected, workspace.runner_availability, workspace.auto_submit_authorizations)} className={submissionMode === "auto_submit" && selectedCanAutoSubmit ? "active" : ""} onClick={() => setSubmissionMode("auto_submit")}>Auto-submit</button></div>
                {!selectedCanAutoSubmit && <p className="selection-help warning-copy">{autoSubmitUnavailableReason(selected, workspace.runner_availability, workspace.auto_submit_authorizations)}</p>}
              </section>
            </div>
            {actionError && <div className="inline-error" role="alert">{actionError}</div>}
            <div className="dialog-actions spread"><p>{isCandidateLead(selected) ? "Verification does not use an application allowance." : jobEligibility(selected).capability === "handoff" || jobEligibility(selected).capability === "unknown_review" ? "Bluey prepares the application kit for your review; this site stays user-controlled." : "This application uses one monthly allowance when approved, downloaded, or queued."}</p><div>{passedJobIds.has(selected.id) ? <button className="button secondary" disabled={feedbackBusy} onClick={() => void restoreMatch(selected)}><Undo2 size={16} />Restore</button> : <button className="button secondary" onClick={() => { setPassTarget(selected); setSelected(null); setActionError(""); }}>Pass</button>}<button className="button primary" disabled={busy || (!isCandidateLead(selected) && !jobEligibility(selected).can_prepare) || passedJobIds.has(selected.id)} onClick={() => void prepare()}>{busy ? "Checking..." : isCandidateLead(selected) ? "Verify and prepare" : jobEligibility(selected).can_prepare ? "Prepare application" : "Blocked by your rules"}<ArrowRight size={17} /></button></div></div>
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

      <AddJobDialog open={addOpen} onClose={() => setAddOpen(false)} trackId={selectedTrack?.id || ""} onSave={async (job) => { const saved = await onAddJob(job); setAddOpen(false); setSelected(saved); }} />
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
        <div className="dialog-actions"><button className="button secondary" onClick={() => updateFilters({ ...clearMatchViewFilters(filters), showOutsideTrack: false })}>Reset</button><button className="button primary" onClick={() => setFilterOpen(false)}>Show {filtered.length} match{filtered.length === 1 ? "" : "es"}</button></div>
      </Dialog>
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

export const DISCOVERY_SOURCE_STALE_AFTER_MS = 12 * 60 * 60 * 1_000;

export function discoverySourceState(
  source: Pick<DiscoverySource, "status" | "health" | "last_success_at_ms">,
  nowMs = Date.now(),
): DiscoverySourceHealth {
  if (source.status === "paused") return "paused";
  if (source.health !== "healthy") return source.health;
  if (!source.last_success_at_ms) return "waiting";
  if (source.last_success_at_ms < nowMs - DISCOVERY_SOURCE_STALE_AFTER_MS) return "degraded";
  return "healthy";
}

export function discoverySourceAction(state: DiscoverySourceHealth): string {
  if (state === "degraded") return "Updates are delayed. Bluey is retrying; add an urgent job link meanwhile.";
  if (state === "paused") return "Contact support to resume it. Paste urgent roles meanwhile.";
  if (state === "waiting") return "Waiting for the first sync.";
  return "No action needed.";
}

export function jobEligibility(job: JobPosting) {
  return portalEligibilityDecision(job.eligibility);
}

export function isMatchEligibleForDefaultView(job: JobPosting): boolean {
  if (isCandidateLead(job)) return true;
  return job.eligibility ? jobEligibility(job).can_prepare : true;
}

export function isCandidateLead(job: Pick<JobPosting, "source" | "availability_status" | "last_verified_at_ms">): boolean {
  return job.source.startsWith("curated_feed:")
    || (job.availability_status === "unknown" && !job.last_verified_at_ms);
}

function matchStatusLabel(job: JobPosting): string {
  if (isCandidateLead(job)) return "Lead · Verify first";
  if (!jobEligibility(job).can_prepare) return "Outside your rules";
  return capabilityLabel(jobEligibility(job).capability);
}

function matchStatusClass(job: JobPosting): string {
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
  const certifiedRunnerAvailable = (eligibility.can_queue_local && runners.local.available)
    || (eligibility.can_queue_cloud && runners.cloud.available);
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
  if (eligibility.hard_failures.length > 0) return "Resolve the Career Track rules above before Auto-submit can be considered.";
  if (eligibility.capability === "beta_review") return "Review first is required while this application system is in beta.";
  if (eligibility.capability === "handoff") return "This site uses a user-controlled handoff after Bluey prepares the application kit.";
  if (eligibility.capability === "unknown_review") {
    return eligibility.review_reasons.find(
      (reason) => reason.code === "ats_certification_unavailable",
    )?.message || "Review first is required because this application system is not certified.";
  }
  if (eligibility.capability === "blocked") return "This listing cannot use a Bluey runner.";
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
  if (runners && !runners.auto_submit_available) return runners.auto_submit_reason;
  const certifiedRunnerAvailable = runners
    && ((eligibility.can_queue_local && runners.local.available)
      || (eligibility.can_queue_cloud && runners.cloud.available));
  if (runners && !certifiedRunnerAvailable) {
    return "No currently available runner is included in this job's certification scope.";
  }
  if (eligibility.can_auto_submit) return undefined;
  return "Auto-submit is available only after every server rule and application-system check passes.";
}

const DAY_MS = 86_400_000;

export function isRecentPosting(
  job: Pick<JobPosting, "availability_status" | "posted_at_ms">,
  maximumAgeDays: number,
): boolean {
  const timestamp = job.posted_at_ms;
  if (!timestamp) return job.availability_status !== "expired";
  return Date.now() - timestamp <= Math.max(1, maximumAgeDays) * DAY_MS;
}

export function isMatchVisibleByState(
  job: Pick<JobPosting, "status" | "availability_status" | "posted_at_ms">,
  maximumAgeDays: number,
  passed: boolean,
  showPassed: boolean,
): boolean {
  const matchesPassedState = showPassed ? passed : !passed;
  return matchesPassedState
    && job.status !== "skipped"
    && job.availability_status !== "expired"
    && isRecentPosting(job, maximumAgeDays);
}

export function postingAgeLabel(job: JobPosting): string {
  const timestamp = job.posted_at_ms;
  if (!timestamp) return "Posting date not listed";
  const ageDays = Math.max(0, Math.floor((Date.now() - timestamp) / DAY_MS));
  if (ageDays === 0) return "Posted today";
  if (ageDays === 1) return "Posted yesterday";
  return `Posted ${ageDays} days ago`;
}

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
