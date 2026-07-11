import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import {
  ArrowRight,
  BriefcaseBusiness,
  Check,
  ChevronRight,
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
} from "lucide-react";
import type { JobPosting, JobsWorkspace } from "../types";
import { relativeTime } from "../lib/format";
import { Dialog } from "../components/Dialog";

interface Props {
  workspace: JobsWorkspace;
  onAddJob(job: JobPosting): Promise<JobPosting>;
  onPrepare(job: JobPosting, mode: string, submissionMode: string): Promise<void>;
}

export function MatchesView({ workspace, onAddJob, onPrepare }: Props) {
  const [query, setQuery] = useState("");
  const [activeTrack, setActiveTrack] = useState("all");
  const [selected, setSelected] = useState<JobPosting | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [mode, setMode] = useState(workspace.profile.resume_mode);
  const [submissionMode, setSubmissionMode] = useState(workspace.profile.default_submission_mode);
  const [filterOpen, setFilterOpen] = useState(false);
  const [minimumScore, setMinimumScore] = useState(0);
  const [workplace, setWorkplace] = useState("all");
  const [onlyUnprepared, setOnlyUnprepared] = useState(false);
  const [density, setDensity] = useState<"comfortable" | "compact">("comfortable");

  const preparedJobIds = useMemo(
    () => new Set(workspace.applications.map((application) => application.job_id)),
    [workspace.applications],
  );

  const filtered = useMemo(() => {
    const needle = query.toLowerCase();
    return workspace.matches.filter((job) => {
      const matchesTrack = activeTrack === "all" || job.track_id === activeTrack;
      const matchesQuery = !needle || `${job.company} ${job.title} ${job.location}`.toLowerCase().includes(needle);
      const matchesScore = job.match_score >= minimumScore;
      const matchesWorkplace = workplace === "all" || job.workplace.toLowerCase().includes(workplace);
      const matchesPacket = !onlyUnprepared || !preparedJobIds.has(job.id);
      const isRecent = isRecentPosting(job, workspace.preferences.max_posting_age_days);
      return matchesTrack && matchesQuery && matchesScore && matchesWorkplace && matchesPacket
        && job.status !== "skipped" && job.availability_status === "active" && isRecent;
    });
  }, [workspace.matches, workspace.preferences.max_posting_age_days, activeTrack, query, minimumScore, workplace, onlyUnprepared, preparedJobIds]);

  const averageScore = filtered.length ? Math.round(filtered.reduce((sum, item) => sum + item.match_score, 0) / filtered.length) : 0;
  const activeFilterCount = Number(minimumScore > 0) + Number(workplace !== "all") + Number(onlyUnprepared);
  const activeTracks = workspace.tracks.filter((track) => track.active);
  const selectedTrack = activeTrack === "all"
    ? activeTracks[0]
    : workspace.tracks.find((track) => track.id === activeTrack);
  const searchTitle = activeTrack === "all" && activeTracks.length > 1
    ? `${activeTracks.length} Career Tracks active`
    : selectedTrack?.name || "Career Track paused";
  const searchDetail = selectedTrack
    ? `${selectedTrack.role} · ${selectedTrack.locations.join(" · ") || "Location not set"}`
    : "Activate a Career Track to discover new matches.";

  const prepare = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await onPrepare(selected, mode, submissionMode);
      setSelected(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="view-shell matches-view">
      <section className="view-heading">
        <div><p className="eyebrow">ACTIVE SEARCH</p><h1>Matches</h1><span>Recent openings ranked for your profile, locations, and Career Tracks.</span></div>
        <button className="button primary" onClick={() => setAddOpen(true)}><Link2 size={17} />Add a job link</button>
      </section>

      <section className="metric-band">
        <div><Target /><span><b>{filtered.length}</b><small>ready matches</small></span></div>
        <div><Sparkles /><span><b>{averageScore || "-"}%</b><small>average fit</small></span></div>
        <div><BriefcaseBusiness /><span><b>{workspace.applications.filter((item) => item.state === "submitted").length}</b><small>submitted</small></span></div>
        <div className="metric-action"><span><b>{Math.max(0, workspace.entitlement.monthly_packet_limit - workspace.entitlement.used_packets)}</b><small>applications left this month</small></span><Link to={`../settings${window.location.search}#plans`}>Plan details<ChevronRight size={14} /></Link></div>
      </section>

      <section className={`search-status-band ${activeTracks.length ? "active" : "paused"}`} aria-label="Active search settings">
        <span className="search-status-icon"><Radar size={20} /></span>
        <div className="search-status-copy"><p><i />{activeTracks.length ? "SEARCH ACTIVE" : "SEARCH PAUSED"}</p><b>{searchTitle}</b><small>{searchDetail}</small></div>
        <dl>
          <div><dt>Freshness</dt><dd>{workspace.preferences.max_posting_age_days} days</dd></div>
          <div><dt>Mode</dt><dd>{workspace.profile.default_submission_mode === "auto_submit" ? `Auto at ${workspace.profile.auto_submit_threshold}%` : "Review first"}</dd></div>
          <div><dt>Pace</dt><dd>Up to {workspace.preferences.daily_limit}/day</dd></div>
        </dl>
        <Link className="button secondary compact" to={`../settings${window.location.search}#tracks`}>Adjust search<ChevronRight size={14} /></Link>
      </section>

      <section className="track-strip" aria-label="Career Tracks">
        <button className={activeTrack === "all" ? "active" : ""} onClick={() => setActiveTrack("all")}><span>All matches</span><b>{workspace.matches.length}</b></button>
        {workspace.tracks.map((track) => <button key={track.id} className={activeTrack === track.id ? "active" : ""} onClick={() => setActiveTrack(track.id)}><span>{track.name}</span><b>{workspace.matches.filter((job) => job.track_id === track.id).length || track.match_count}</b></button>)}
        <Link className="add-track" title="Add Career Track" to={`../settings${window.location.search}#tracks`}><Plus size={15} /></Link>
      </section>

      <section className="toolbar">
        <label className="search-field"><Search size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search company, role, or location" /></label>
        <button className={`button secondary compact ${activeFilterCount ? "active-filter" : ""}`} onClick={() => setFilterOpen(true)}><Filter size={15} />Filters{activeFilterCount ? <span className="filter-count">{activeFilterCount}</span> : null}</button>
        <button className="icon-button" title={`Use ${density === "comfortable" ? "compact" : "comfortable"} rows`} onClick={() => setDensity((current) => current === "comfortable" ? "compact" : "comfortable")}><SlidersHorizontal size={17} /></button>
      </section>

      <section className={`job-list ${density}`} aria-label="Job matches">
        <div className="job-list-head"><span>ROLE</span><span>FIT</span><span>LOCATION</span><span>STATUS</span><span /></div>
        {filtered.map((job) => (
          <button className="job-row" key={job.id} onClick={() => setSelected(job)}>
            <div className="company-mark">{job.company.slice(0, 2).toUpperCase()}</div>
            <div className="job-main"><strong>{job.title}</strong><span>{job.company} · {postingAgeLabel(job)}</span></div>
            <div className={`score score-${Math.floor(job.match_score / 10)}`}><b>{job.match_score}</b><span>%</span></div>
            <div className="job-location"><MapPin size={14} /><span>{job.location}<small>{job.workplace}</small></span></div>
            <div className={`status-pill ${preparedJobIds.has(job.id) ? "prepared" : "new"}`}>{preparedJobIds.has(job.id) ? "Application ready" : "Fresh"}</div>
            <ChevronRight size={18} />
          </button>
        ))}
        {filtered.length === 0 && <div className="empty-state"><Search /><h3>No matches in this view</h3><p>Try another Career Track or add a job link you already found.</p><button className="button primary" onClick={() => setAddOpen(true)}>Add job link</button></div>}
      </section>

      <Dialog open={Boolean(selected)} title={selected ? `${selected.title} at ${selected.company}` : "Job match"} description={selected?.location} onClose={() => setSelected(null)} size="large">
        {selected && (
          <div className="job-detail">
            <div className="job-detail-summary">
              <div className="large-score"><b>{selected.match_score}</b><span>% match</span></div>
              <div><span>{selected.workplace}</span><span>{selected.compensation || "Compensation not listed"}</span><span className="freshness-note"><Clock3 size={14} />{postingAgeLabel(selected)}</span>{selected.last_verified_at_ms && <span>Checked {relativeTime(selected.last_verified_at_ms)}</span>}<a href={selected.canonical_url} target="_blank" rel="noreferrer">Original job<ExternalLink size={14} /></a></div>
            </div>
            <div className="detail-columns">
              <section><h3>Why it matched</h3><ul className="check-list">{selected.matched_reasons.map((reason) => <li key={reason}><Check size={15} />{reason}</li>)}</ul>{selected.missing_requirements.length > 0 && <><h3>Check before applying</h3><ul className="watch-list">{selected.missing_requirements.map((reason) => <li key={reason}>{reason}</li>)}</ul></>}</section>
              <section><h3>Tailored application</h3><p>Bluey creates a new resume version for this job. It will never reuse this version for another role.</p><label>Resume mode</label><div className="segmented"><button className={mode === "factual" ? "active" : ""} onClick={() => setMode("factual")}>Factual</button><button className={mode === "enhance" ? "active" : ""} onClick={() => setMode("enhance")}>Enhance</button></div><label>After preparation</label><div className="segmented"><button className={submissionMode === "review_first" ? "active" : ""} onClick={() => setSubmissionMode("review_first")}>Review first</button><button className={submissionMode === "auto_submit" ? "active" : ""} onClick={() => setSubmissionMode("auto_submit")}>Auto-submit</button></div></section>
            </div>
            <div className="dialog-actions spread"><p>{selected.source.includes("handoff") ? "Bluey prepares everything; you finish on this site." : "This application uses one monthly allowance when completed."}</p><button className="button primary" disabled={busy} onClick={() => void prepare()}>{busy ? "Preparing..." : "Prepare application"}<ArrowRight size={17} /></button></div>
          </div>
        )}
      </Dialog>

      <AddJobDialog open={addOpen} onClose={() => setAddOpen(false)} onSave={async (job) => { const saved = await onAddJob(job); setAddOpen(false); setSelected(saved); }} />
      <Dialog open={filterOpen} title="Filter matches" description="Narrow this view without changing your Career Track." onClose={() => setFilterOpen(false)}>
        <div className="dialog-form"><label><span>Minimum match score</span><div className="range-field"><input type="range" min="0" max="95" step="5" value={minimumScore} onChange={(event) => setMinimumScore(Number(event.target.value))} /><b>{minimumScore || "Any"}{minimumScore ? "%" : ""}</b></div></label><label><span>Workplace</span><select value={workplace} onChange={(event) => setWorkplace(event.target.value)}><option value="all">Any workplace</option><option value="remote">Remote</option><option value="hybrid">Hybrid</option><option value="on-site">On-site</option></select></label><label className="setting-line simple"><div><b>Only jobs not prepared</b><span>Hide applications you already prepared.</span></div><button type="button" className={`toggle ${onlyUnprepared ? "on" : ""}`} role="switch" aria-checked={onlyUnprepared} onClick={() => setOnlyUnprepared((current) => !current)}><span /></button></label></div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => { setMinimumScore(0); setWorkplace("all"); setOnlyUnprepared(false); }}>Reset</button><button className="button primary" onClick={() => setFilterOpen(false)}>Show {filtered.length} match{filtered.length === 1 ? "" : "es"}</button></div>
      </Dialog>
    </div>
  );
}

const DAY_MS = 86_400_000;

function isRecentPosting(job: JobPosting, maximumAgeDays: number): boolean {
  const timestamp = job.posted_at_ms || job.created_at_ms;
  if (!timestamp) return false;
  return Date.now() - timestamp <= Math.max(1, maximumAgeDays) * DAY_MS;
}

function postingAgeLabel(job: JobPosting): string {
  const timestamp = job.posted_at_ms || job.created_at_ms;
  if (!timestamp) return "Recently found";
  const ageDays = Math.max(0, Math.floor((Date.now() - timestamp) / DAY_MS));
  if (ageDays === 0) return "Posted today";
  if (ageDays === 1) return "Posted yesterday";
  return `Posted ${ageDays} days ago`;
}

function AddJobDialog({ open, onClose, onSave }: { open: boolean; onClose(): void; onSave(job: JobPosting): Promise<void> }) {
  const [url, setUrl] = useState("");
  const [company, setCompany] = useState("");
  const [title, setTitle] = useState("");
  const [location, setLocation] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const submit = async () => {
    if (!company.trim() || !title.trim()) return;
    setSaving(true);
    try {
      await onSave({
        id: "", canonical_key: "", source: "pasted_link", external_id: "", company, title,
        location, workplace: location.toLowerCase().includes("remote") ? "Remote" : "Unknown",
        canonical_url: url, description, compensation: "", track_id: "", match_score: 0,
        matched_reasons: [], missing_requirements: [], posted_at_ms: Date.now(),
        last_verified_at_ms: Date.now(), availability_status: "active", status: "matched",
        created_at_ms: 0, updated_at_ms: 0,
      });
      setUrl(""); setCompany(""); setTitle(""); setLocation(""); setDescription("");
    } finally { setSaving(false); }
  };
  return (
    <Dialog open={open} title="Add a job" description="Paste the listing and Bluey will score it against your Career Profile." onClose={onClose}>
      <div className="dialog-form"><label><span>Job link</span><input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="https://company.com/jobs/..." autoFocus /></label><div className="form-grid two"><label><span>Company</span><input value={company} onChange={(event) => setCompany(event.target.value)} /></label><label><span>Role</span><input value={title} onChange={(event) => setTitle(event.target.value)} /></label></div><label><span>Location</span><input value={location} onChange={(event) => setLocation(event.target.value)} placeholder="New York, NY or Remote - US" /></label><label><span>Job description</span><textarea rows={6} value={description} onChange={(event) => setDescription(event.target.value)} placeholder="Paste the job description for a better score and resume diff." /></label></div>
      <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !company.trim() || !title.trim()} onClick={() => void submit()}>{saving ? "Scoring..." : "Score job"}<ArrowRight size={16} /></button></div>
    </Dialog>
  );
}
