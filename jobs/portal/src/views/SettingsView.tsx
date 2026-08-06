import { useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Bot,
  BrainCircuit,
  Check,
  ChevronRight,
  CirclePlus,
  ClipboardCheck,
  CreditCard,
  FileText,
  MailCheck,
  MapPin,
  MessageCircleQuestion,
  MonitorUp,
  Pencil,
  Plus,
  ShieldCheck,
  Smartphone,
  Trash2,
  UserRound,
} from "lucide-react";
import type {
  AnswerMemory,
  ApplicationIdentity,
  CareerProfile,
  CareerTrack,
  JobPreferences,
  JobsWorkspace,
  MailboxConnection,
  MailboxMessage,
  MailboxProviderAvailability,
  MailboxSyncState,
} from "../types";
import { Dialog, ConfirmDialog } from "../components/Dialog";
import { ApplicationInboxSettings } from "../components/ApplicationInboxSettings";
import { CareerField, CareerTagField } from "../components/CareerFields";
import {
  canonicalTargetRoles,
  canonicalizeTargetRole,
  mergeCareerSuggestions,
  ROLE_SUGGESTIONS,
  TARGET_ROLE_SUGGESTIONS,
  targetRoleSuggestions as filterTargetRoleSuggestions,
} from "../data/career-suggestions";
import { useLocationSuggestions } from "../data/use-location-suggestions";
import { SearchPolicySummary } from "../components/SearchPolicySummary";
import {
  ENGAGEMENT_TYPE_OPTIONS,
  EMPLOYMENT_TYPE_OPTIONS,
  JobCategoryChoices,
} from "../components/JobCategoryChoices";
import {
  BLUEY_AUTO_SUBMIT_THRESHOLD,
  BLUEY_DAILY_APPLICATION_LIMIT,
  BLUEY_MAX_POSTING_AGE_DAYS,
} from "../lib/search-policy";
import { money, titleCase } from "../lib/format";

interface Props {
  workspace: JobsWorkspace;
  onSaveProfile(profile: CareerProfile): Promise<void>;
  onSavePreferences(preferences: JobPreferences): Promise<void>;
  onSaveTrack(track: CareerTrack): Promise<void>;
  onDeleteTrack(track: CareerTrack): Promise<void>;
  onAuthorizeTrackAutoSubmit(track: CareerTrack): Promise<void>;
  onRevokeTrackAutoSubmit(track: CareerTrack): Promise<void>;
  onCreateIdentity(identity: ApplicationIdentity): Promise<ApplicationIdentity>;
  onUpdateIdentity(identity: ApplicationIdentity): Promise<ApplicationIdentity>;
  onVerifyIdentity(identity: ApplicationIdentity, code: string): Promise<ApplicationIdentity>;
  onResendIdentity(identity: ApplicationIdentity): Promise<void>;
  onDeleteIdentity(identity: ApplicationIdentity): Promise<void>;
  onMailboxProviders(): Promise<MailboxProviderAvailability[]>;
  onConnectMailbox(provider: MailboxConnection["provider"]): Promise<void>;
  onAuthorizeMailboxCommunication(connection: MailboxConnection): Promise<void>;
  onMailboxSyncState(connection: MailboxConnection): Promise<MailboxSyncState>;
  onMailboxMessages(connectionId?: string): Promise<MailboxMessage[]>;
  onSyncMailbox(connection: MailboxConnection): Promise<MailboxSyncState>;
  onDeleteMailbox(connection: MailboxConnection): Promise<void>;
  onSaveAnswerMemory(answer: AnswerMemory): Promise<AnswerMemory>;
  onDeleteAnswerMemory(answer: AnswerMemory): Promise<void>;
}

export function SettingsView({
  workspace,
  onSaveProfile,
  onSavePreferences,
  onSaveTrack,
  onDeleteTrack,
  onAuthorizeTrackAutoSubmit,
  onRevokeTrackAutoSubmit,
  onCreateIdentity,
  onUpdateIdentity,
  onVerifyIdentity,
  onResendIdentity,
  onDeleteIdentity,
  onMailboxProviders,
  onConnectMailbox,
  onAuthorizeMailboxCommunication,
  onMailboxSyncState,
  onMailboxMessages,
  onSyncMailbox,
  onDeleteMailbox,
  onSaveAnswerMemory,
  onDeleteAnswerMemory,
}: Props) {
  const [preferences, setPreferences] = useState({
    ...workspace.preferences,
    desired_roles: canonicalTargetRoles(workspace.preferences.desired_roles),
    engagement_types: workspace.preferences.engagement_types || [],
  });
  const [profile, setProfile] = useState(workspace.profile);
  const [trackOpen, setTrackOpen] = useState(false);
  const [editingTrack, setEditingTrack] = useState<CareerTrack | null>(null);
  const [deletingTrack, setDeletingTrack] = useState<CareerTrack | null>(null);
  const [identityOpen, setIdentityOpen] = useState(false);
  const [verifyingIdentity, setVerifyingIdentity] = useState<ApplicationIdentity | null>(null);
  const [deletingIdentity, setDeletingIdentity] = useState<ApplicationIdentity | null>(null);
  const [answerOpen, setAnswerOpen] = useState(false);
  const [editingAnswer, setEditingAnswer] = useState<AnswerMemory | null>(null);
  const [deletingAnswer, setDeletingAnswer] = useState<AnswerMemory | null>(null);
  const [saved, setSaved] = useState("");
  const [localError, setLocalError] = useState("");
  const [savingSearch, setSavingSearch] = useState(false);
  const [autoSubmitBusyTrackId, setAutoSubmitBusyTrackId] = useState("");
  const [searchDirty, setSearchDirty] = useState(false);
  const roleSuggestions = useMemo(
    () => mergeCareerSuggestions(preferences.desired_roles, [profile.headline], profile.employment.map((entry) => entry.title), ROLE_SUGGESTIONS),
    [preferences.desired_roles, profile.employment, profile.headline],
  );
  const targetRoleSuggestionValues = useMemo(
    () => mergeCareerSuggestions(canonicalTargetRoles(preferences.desired_roles), canonicalTargetRoles([profile.headline]), canonicalTargetRoles(profile.employment.map((entry) => entry.title)), TARGET_ROLE_SUGGESTIONS),
    [preferences.desired_roles, profile.employment, profile.headline],
  );
  const locationSeeds = useMemo(
    () => mergeCareerSuggestions(preferences.desired_locations, [profile.current_location], profile.employment.map((entry) => entry.location)),
    [preferences.desired_locations, profile.current_location, profile.employment],
  );
  const locationSuggestions = useLocationSuggestions(locationSeeds);
  const companySuggestions = useMemo(
    () => mergeCareerSuggestions(profile.employment.map((entry) => entry.company)),
    [profile.employment],
  );

  useEffect(() => {
    if (searchDirty) return;
    setPreferences({
      ...workspace.preferences,
      desired_roles: canonicalTargetRoles(workspace.preferences.desired_roles),
      engagement_types: workspace.preferences.engagement_types || [],
    });
    setProfile(workspace.profile);
  }, [searchDirty, workspace.preferences, workspace.profile]);

  const updatePreferences = (next: JobPreferences) => {
    setPreferences(next);
    setSearchDirty(true);
  };
  const updateProfile = (next: CareerProfile) => {
    setProfile(next);
    setSearchDirty(true);
  };

  const saveSearchSettings = async () => {
    if (savingSearch) return;
    setSavingSearch(true);
    setLocalError("");
    setSaved("");
    try {
      await Promise.all([
        onSavePreferences({
          ...preferences,
          desired_roles: canonicalTargetRoles(preferences.desired_roles),
          daily_limit: BLUEY_DAILY_APPLICATION_LIMIT,
          max_posting_age_days: BLUEY_MAX_POSTING_AGE_DAYS,
        }),
        onSaveProfile({
          ...profile,
          auto_submit_threshold: BLUEY_AUTO_SUBMIT_THRESHOLD,
          daily_limit: BLUEY_DAILY_APPLICATION_LIMIT,
        }),
      ]);
      setSearchDirty(false);
      setSaved("Search and automation defaults saved.");
      window.setTimeout(() => setSaved(""), 2600);
    } catch (requestError) {
      setLocalError(errorMessage(requestError));
    } finally {
      setSavingSearch(false);
    }
  };

  return (
    <div className="view-shell settings-view">
      <section className="view-heading">
        <div><p className="eyebrow">JOBS PREFERENCES</p><h1>Settings</h1><span>Career Tracks, application emails, inboxes, and plan controls.</span></div>
        <button className="button primary" disabled={savingSearch} onClick={() => void saveSearchSettings()}>{savingSearch ? "Saving..." : "Save changes"}</button>
      </section>
      {saved && <div className="global-message success"><Check size={16} />{saved}</div>}
      {localError && <div className="global-message error">{localError}</div>}

      <section className="settings-section" id="tracks">
        <div className="settings-section-title"><span><Bot /></span><div><p>CAREER TRACK AGENTS</p><h2>Separate searches for separate goals</h2><small>Each agent has its own role, location, and match stream.</small></div><button className="button secondary compact" onClick={() => { setEditingTrack(null); setTrackOpen(true); }} disabled={workspace.tracks.length >= workspace.entitlement.track_limit}><Plus size={15} />New track</button></div>
        <div className="track-settings-list">
          {workspace.tracks.map((track) => {
            const identity = workspace.application_identities.find(
              (item) => item.id === track.application_identity_id,
            );
            const autoSubmit = workspace.auto_submit_authorizations.find(
              (authorization) => authorization.career_track_id === track.id,
            );
            const autoSubmitActive = autoSubmit?.status === "active";
            const canAuthorize = track.active
              && identity?.verification_status === "verified"
              && Boolean(workspace.profile.source_resume_asset_id);
            const autoSubmitDetail = autoSubmitActive
              ? "Eligible certified jobs may queue automatically after every server check passes."
              : autoSubmit?.status === "needs_review"
                ? "This Track, resume, or application email changed. Review it before enabling again."
                : "Review first stays on until you authorize this exact Track, resume, and application email.";
            return (
              <article className="track-settings-card" key={track.id}>
                <button
                  className="track-settings-main"
                  onClick={() => { setEditingTrack(track); setTrackOpen(true); }}
                >
                  <span className="agent-orbit"><Bot size={18} /></span>
                  <div>
                    <b>{track.name}</b>
                    <p>{track.role}</p>
                    <small><MapPin size={12} />{track.locations.join(" · ") || "No locations"}</small>
                    <small><MailCheck size={12} />{identity?.email || "Application email required"}</small>
                    <small><FileText size={12} />{workspace.profile.source_resume_name || "Source resume required"}</small>
                  </div>
                  <span className={track.active ? "agent-state active" : "agent-state"}>{track.active ? "Active" : "Paused"}</span>
                  <ChevronRight size={17} />
                </button>
                <div className="track-auto-submit">
                  <span className={autoSubmitActive ? "track-auto-submit-icon active" : "track-auto-submit-icon"}>
                    <ShieldCheck size={16} />
                  </span>
                  <div>
                    <b>{autoSubmitActive ? "Auto-submit enabled" : autoSubmit?.status === "needs_review" ? "Auto-submit needs review" : "Review first"}</b>
                    <small>{autoSubmitDetail}</small>
                  </div>
                  <button
                    className={autoSubmitActive ? "button secondary compact" : "button primary compact"}
                    disabled={autoSubmitBusyTrackId === track.id || (!autoSubmitActive && !canAuthorize)}
                    title={!canAuthorize && !autoSubmitActive
                      ? "Activate the Track, verify its application email, and review the current resume first."
                      : undefined}
                    onClick={() => {
                      setLocalError("");
                      setAutoSubmitBusyTrackId(track.id);
                      const action = autoSubmitActive
                        ? onRevokeTrackAutoSubmit(track)
                        : onAuthorizeTrackAutoSubmit(track);
                      void action
                        .catch(showError(setLocalError))
                        .finally(() => setAutoSubmitBusyTrackId(""));
                    }}
                  >
                    {autoSubmitBusyTrackId === track.id
                      ? "Saving..."
                      : autoSubmitActive
                        ? "Turn off"
                        : autoSubmit?.status === "needs_review"
                          ? "Enable again"
                          : "Enable Auto-submit"}
                  </button>
                </div>
              </article>
            );
          })}
          <div className="track-limit"><span>{workspace.tracks.length} of {workspace.entitlement.track_limit} agents</span><div><i style={{ width: `${Math.min(100, workspace.tracks.length / workspace.entitlement.track_limit * 100)}%` }} /></div></div>
        </div>
      </section>

      <section className="settings-section" id="application-emails">
        <div className="settings-section-title">
          <span><UserRound /></span>
          <div>
            <p>APPLICATION EMAILS</p>
            <h2>Choose which address employers see</h2>
            <small>Every Career Track can use a different verified address. Your Bluey login does not change.</small>
          </div>
          <button
            className="button secondary compact"
            onClick={() => setIdentityOpen(true)}
            disabled={workspace.application_identities.length >= workspace.entitlement.application_identity_limit}
          ><Plus size={15} />Add email</button>
        </div>
        <div className="identity-summary">
          <span><b>{workspace.application_identities.length}</b> of {workspace.entitlement.application_identity_limit} application emails</span>
          <span>Aliases in one inbox share a single connection slot.</span>
        </div>
        <div className="identity-list">
          {workspace.application_identities.map((identity) => {
            const trackCount = workspace.tracks.filter((track) => track.application_identity_id === identity.id).length;
            return <div key={identity.id}>
              <span className="identity-avatar">{identity.email.slice(0, 1).toUpperCase()}</span>
              <div>
                <b>{identity.email}</b>
                <p>{identity.label || "Application email"}{trackCount ? ` · ${trackCount} Career Track${trackCount === 1 ? "" : "s"}` : ""}</p>
              </div>
              <div className="identity-badges">
                {identity.is_default && <span className="status-chip accent">Default</span>}
                <span className={`status-chip ${identity.verification_status === "verified" ? "success" : "warning"}`}>{identity.verification_status === "verified" ? "Verified" : "Verify"}</span>
              </div>
              <div className="identity-actions">
                {identity.verification_status === "pending" && <button className="button secondary compact" onClick={() => setVerifyingIdentity(identity)}>Enter code</button>}
                {!identity.is_default && identity.verification_status === "verified" && <button className="icon-button" title="Make default" aria-label={`Make ${identity.email} the default`} onClick={() => void onUpdateIdentity({ ...identity, is_default: true }).catch(showError(setLocalError))}><Check size={15} /></button>}
                {!identity.is_default && <button className="icon-button danger" title="Remove email" aria-label={`Remove ${identity.email}`} onClick={() => setDeletingIdentity(identity)}><Trash2 size={15} /></button>}
              </div>
            </div>;
          })}
        </div>
        <p className="field-note">Application emails isolate inboxes and site sessions. They do not create separate candidate profiles or bypass company limits.</p>
      </section>

      <div className="settings-columns">
        <section className="settings-section">
          <div className="settings-section-title compact"><span><MapPin /></span><div><p>SEARCH RULES</p><h2>Where and what to apply for</h2></div></div>
          <div className="settings-form">
            <CareerTagField label="Target roles" values={preferences.desired_roles} onChange={(values) => updatePreferences({ ...preferences, desired_roles: canonicalTargetRoles(values) })} placeholder="Software Engineer" suggestions={targetRoleSuggestionValues} normalizeValue={canonicalizeTargetRole} filterSuggestions={(query, _suggestions, selected, limit) => filterTargetRoleSuggestions(query, selected, limit)} customHint="Choose the full role name. Custom roles are saved exactly as entered for this search." />
            <CareerTagField label="Target locations" values={preferences.desired_locations} onChange={(values) => updatePreferences({ ...preferences, desired_locations: values })} placeholder="Add a city, region, or remote" suggestions={locationSuggestions} />
            <label><span>When a job uses another location</span><select value={preferences.location_policy} onChange={(event) => updatePreferences({ ...preferences, location_policy: event.target.value as JobPreferences["location_policy"] })}><option value="ask">Ask me what to say</option><option value="local">Use my current location only</option><option value="willing_to_relocate">Say I am willing to relocate</option><option value="remote_only">Skip unless remote</option></select></label>
            <JobCategoryChoices
              label="Employment types"
              description="Choose the job arrangements Bluey may match."
              values={preferences.employment_types}
              options={EMPLOYMENT_TYPE_OPTIONS}
              onChange={(values) => updatePreferences({ ...preferences, employment_types: values })}
            />
            <JobCategoryChoices
              label="Contract engagement"
              description="Optional. Use this only when W-2, C2C, 1099, or direct-hire terms matter."
              values={preferences.engagement_types}
              options={ENGAGEMENT_TYPE_OPTIONS}
              onChange={(values) => updatePreferences({ ...preferences, engagement_types: values })}
            />
            <label><span>Sponsorship filter</span><select value={preferences.sponsorship} onChange={(event) => updatePreferences({ ...preferences, sponsorship: event.target.value })}><option value="ask">Ask when unclear</option><option value="required">Only roles offering sponsorship</option><option value="not_required">Sponsorship not required</option><option value="any">Do not filter</option></select></label>
            <SearchPolicySummary profile={profile} role={preferences.desired_roles[0]} compact />
            <label><span>Minimum salary</span><input type="number" value={preferences.minimum_compensation || ""} onChange={(event) => updatePreferences({ ...preferences, minimum_compensation: Number(event.target.value) || undefined })} /></label>
            <CareerTagField label="Excluded companies" values={preferences.excluded_companies} onChange={(values) => updatePreferences({ ...preferences, excluded_companies: values })} placeholder="Add a company" suggestions={companySuggestions} />
            <CareerTagField label="Excluded titles" values={preferences.excluded_titles} onChange={(values) => updatePreferences({ ...preferences, excluded_titles: values })} placeholder="Add a title" suggestions={roleSuggestions} />
            <div className="setting-line simple"><div><b>One application per company</b><span>Always enforced across Career Tracks, resume versions, and application emails.</span></div><span className="status-chip success">Locked</span></div>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-title compact"><span><ShieldCheck /></span><div><p>APPLICATION DEFAULTS</p><h2>Control before speed</h2></div></div>
          <div className="settings-form">
            <label><span>Resume mode</span><div className="segmented"><button className={profile.resume_mode === "factual" ? "active" : ""} onClick={() => updateProfile({ ...profile, resume_mode: "factual" })}>Factual</button><button className={profile.resume_mode === "enhance" ? "active" : ""} onClick={() => updateProfile({ ...profile, resume_mode: "enhance" })}>Enhance</button></div></label>
            <label><span>Submission mode</span><div className="segmented"><button className={profile.default_submission_mode === "review_first" ? "active" : ""} onClick={() => updateProfile({ ...profile, default_submission_mode: "review_first" })}>Review first</button><button className={profile.default_submission_mode === "auto_submit" ? "active" : ""} onClick={() => updateProfile({ ...profile, default_submission_mode: "auto_submit" })}>Auto-submit</button></div></label>
            <label className="setting-line simple"><div><b>Review new claims</b><span>Pause before a newly proposed factual claim can enter an application.</span></div><Toggle checked={profile.review_new_claims} onChange={(checked) => updateProfile({ ...profile, review_new_claims: checked })} /></label>
            <div className="setting-line simple"><div><b>Bluey eligibility gate</b><span>Freshness, experience, location, job status, company safety, and site capability must all pass before Auto-submit can run.</span></div><span className="status-chip success">Managed</span></div>
            <div className="challenge-rules">
              <p>CHALLENGE HANDLING</p>
              <div>
                <span><MonitorUp size={15} /><b>CAPTCHA</b><small>Take over, then resume</small></span>
                <span><MailCheck size={15} /><b>Email code</b><small>One-click approval</small></span>
                <span><Smartphone size={15} /><b>Phone or app 2FA</b><small>Take over, then resume</small></span>
                <span><ClipboardCheck size={15} /><b>Assessment</b><small>Take over without losing progress</small></span>
                <span><MessageCircleQuestion size={15} /><b>Required question</b><small>Answer once and remember</small></span>
              </div>
            </div>
          </div>
        </section>
      </div>

      <section className="settings-section" id="answer-memory">
        <div className="settings-section-title">
          <span><BrainCircuit /></span>
          <div>
            <p>ANSWER MEMORY</p>
            <h2>Answer once, reuse it carefully</h2>
            <small>Company answers win first, then Career Track answers, then account answers.</small>
          </div>
          <button className="button secondary compact" onClick={() => { setEditingAnswer(null); setAnswerOpen(true); }}><Plus size={15} />Add answer</button>
        </div>
        <div className="answer-memory-list">
          {workspace.answer_memory.map((item) => <div key={item.id}>
            <span className="answer-memory-mark"><MessageCircleQuestion size={17} /></span>
            <div><b>{item.question}</b><p>{item.value}</p><small>{answerScopeLabel(item, workspace)}{item.use_count > 0 ? ` · Used ${item.use_count} time${item.use_count === 1 ? "" : "s"}` : ""}</small></div>
            <button className="icon-button" title="Edit saved answer" aria-label={`Edit ${item.question}`} onClick={() => { setEditingAnswer(item); setAnswerOpen(true); }}><Pencil size={15} /></button>
            <button className="icon-button danger" title="Remove saved answer" aria-label={`Remove ${item.question}`} onClick={() => setDeletingAnswer(item)}><Trash2 size={15} /></button>
          </div>)}
          {workspace.answer_memory.length === 0 && <div className="answer-memory-empty"><BrainCircuit size={20} /><span><b>No saved answers yet</b><p>When an application pauses on a question, choose Remember this answer to add it here.</p></span></div>}
        </div>
      </section>

      <ApplicationInboxSettings
        workspace={workspace}
        onMailboxProviders={onMailboxProviders}
        onConnectMailbox={onConnectMailbox}
        onAuthorizeMailboxCommunication={onAuthorizeMailboxCommunication}
        onMailboxSyncState={onMailboxSyncState}
        onMailboxMessages={onMailboxMessages}
        onSyncMailbox={onSyncMailbox}
        onDeleteMailbox={onDeleteMailbox}
        onError={setLocalError}
      />

      <section className="settings-section" id="plans">
        <div className="settings-section-title"><span><CreditCard /></span><div><p>PLAN</p><h2>{titleCase(workspace.entitlement.plan)} Jobs</h2><small>{workspace.entitlement.used_packets} of {workspace.entitlement.monthly_packet_limit} included applications used this month.</small></div><a className="button secondary compact" href="/account#billing">Shared balance<ArrowRight size={15} /></a></div>
        <div className="plan-grid">
          <Plan name="Free" price="$0" details="1 agent · 5 reviewed applications · 2 application emails" active={workspace.entitlement.plan === "free"} />
          <Plan name="Pro" price="$29" details="3 agents · 50 applications · 10 application emails · local runner beta waitlist" active={workspace.entitlement.plan === "pro"} />
          <Plan name="Cloud" price="$49" details="5 agents · 100 applications · 25 application emails · invited cloud runner beta" active={workspace.entitlement.plan === "cloud"} />
        </div>
        <p className="plan-footnote">
          Application emails are included. Connected inboxes start with read-only employer-update
          access. Where provider authorization is offered, you can separately add send and
          calendar-write scopes for reviewed replies and events; every exact draft still requires
          approval. After the included applications, each additional completed application is
          {` ${money(workspace.entitlement.overage_cents)} `}
          from your shared Bluey balance. Retries and browser handoffs do not count again.
        </p>
      </section>

      <ApplicationEmailDialog
        open={identityOpen}
        onClose={() => setIdentityOpen(false)}
        onSave={async (identity) => {
          setLocalError("");
          const savedIdentity = await onCreateIdentity(identity);
          setIdentityOpen(false);
          if (savedIdentity.verification_status === "pending") setVerifyingIdentity(savedIdentity);
        }}
      />
      <VerifyIdentityDialog
        identity={verifyingIdentity}
        onClose={() => setVerifyingIdentity(null)}
        onVerify={async (identity, code) => {
          setLocalError("");
          await onVerifyIdentity(identity, code);
          setVerifyingIdentity(null);
        }}
        onResend={onResendIdentity}
      />
      <AnswerMemoryDialog
        open={answerOpen}
        answer={editingAnswer}
        tracks={workspace.tracks}
        onClose={() => setAnswerOpen(false)}
        onSave={async (answer) => {
          setLocalError("");
          await onSaveAnswerMemory(answer);
          setAnswerOpen(false);
        }}
      />
      <TrackDialog
        open={trackOpen}
        track={editingTrack}
        identities={workspace.application_identities}
        roleSuggestions={targetRoleSuggestionValues}
        locationSuggestions={locationSuggestions}
        onClose={() => setTrackOpen(false)}
        onSave={async (track) => { await onSaveTrack(track); setTrackOpen(false); }}
        onDelete={(track) => { setTrackOpen(false); setDeletingTrack(track); }}
      />
      <ConfirmDialog open={Boolean(deletingIdentity)} title={`Remove ${deletingIdentity?.email || "application email"}?`} description="Bluey will keep existing application receipts, but this address will no longer be available for new Career Tracks." confirmLabel="Remove email" tone="danger" onClose={() => setDeletingIdentity(null)} onConfirm={() => { const identity = deletingIdentity; setDeletingIdentity(null); if (identity) void onDeleteIdentity(identity).catch(showError(setLocalError)); }} />
      <ConfirmDialog open={Boolean(deletingTrack)} title={`Delete ${deletingTrack?.name || "Career Track"}?`} description="This stops discovery for the track. Existing matches and applications stay in your history." confirmLabel="Delete track" tone="danger" onClose={() => setDeletingTrack(null)} onConfirm={() => { const track = deletingTrack; setDeletingTrack(null); if (track) void onDeleteTrack(track).catch(showError(setLocalError)); }} />
      <ConfirmDialog open={Boolean(deletingAnswer)} title="Remove this saved answer?" description="Bluey will ask again the next time this question appears. Existing application receipts stay unchanged." confirmLabel="Remove answer" tone="danger" onClose={() => setDeletingAnswer(null)} onConfirm={() => { const answer = deletingAnswer; setDeletingAnswer(null); if (answer) void onDeleteAnswerMemory(answer).catch(showError(setLocalError)); }} />
    </div>
  );
}

function AnswerMemoryDialog({ open, answer, tracks, onClose, onSave }: { open: boolean; answer: AnswerMemory | null; tracks: CareerTrack[]; onClose(): void; onSave(answer: AnswerMemory): Promise<void> }) {
  const [value, setValue] = useState<AnswerMemory>(() => emptyAnswerMemory());
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) {
      setValue(answer ? { ...answer } : emptyAnswerMemory());
      setError("");
    }
  }, [open, answer]);

  const scopeId = value.scope === "track" ? value.scope_id || tracks[0]?.id || "" : value.scope_id || "";
  const save = async () => {
    if (!value.question.trim() || !value.value.trim()) return;
    setSaving(true);
    setError("");
    try {
      await onSave({
        ...value,
        key: normalizeAnswerKey(value.question),
        scope_id: value.scope === "account" ? undefined : value.scope === "company" ? normalizeCompanyKey(scopeId) : scopeId,
        confirmed: true,
      });
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSaving(false);
    }
  };

  return <Dialog open={open} title={answer ? "Edit saved answer" : "Add saved answer"} description="Bluey reuses the closest confirmed answer when the same application question appears." onClose={onClose}>
    <div className="answer-memory-form">
      <label><span>Application question</span><input value={value.question} onChange={(event) => setValue({ ...value, question: event.target.value })} placeholder="Why are you interested in this role?" /></label>
      <label><span>Answer</span><textarea rows={4} value={value.value} onChange={(event) => setValue({ ...value, value: event.target.value })} placeholder="Enter the answer Bluey should use" /></label>
      <label><span>Reuse for</span><select value={value.scope} onChange={(event) => setValue({ ...value, scope: event.target.value as AnswerMemory["scope"], scope_id: undefined })}><option value="account">All applications</option>{tracks.length > 0 && <option value="track">One Career Track</option>}<option value="company">One company</option></select></label>
      {value.scope === "track" && <label><span>Career Track</span><select value={scopeId} onChange={(event) => setValue({ ...value, scope_id: event.target.value })}>{tracks.map((track) => <option value={track.id} key={track.id}>{track.name}</option>)}</select></label>}
      {value.scope === "company" && <label><span>Company</span><input value={scopeId.replace(/-/g, " ")} onChange={(event) => setValue({ ...value, scope_id: event.target.value })} placeholder="Company name" /></label>}
      {error && <div className="inline-error" role="alert">{error}</div>}
    </div>
    <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !value.question.trim() || !value.value.trim() || (value.scope !== "account" && !scopeId)} onClick={() => void save()}>{saving ? "Saving..." : "Save answer"}</button></div>
  </Dialog>;
}

function emptyAnswerMemory(): AnswerMemory {
  return {
    id: "",
    key: "",
    question: "",
    value: "",
    scope: "account",
    confirmed: true,
    source: "settings",
    created_at_ms: 0,
    updated_at_ms: 0,
    use_count: 0,
  };
}

function answerScopeLabel(answer: AnswerMemory, workspace: JobsWorkspace): string {
  if (answer.scope === "company") return `Company · ${titleCase((answer.scope_id || "Company").replace(/-/g, " "))}`;
  if (answer.scope === "track") return `Career Track · ${workspace.tracks.find((track) => track.id === answer.scope_id)?.name || "Saved track"}`;
  return "All applications";
}

function normalizeAnswerKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function normalizeCompanyKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function ApplicationEmailDialog({ open, onClose, onSave }: { open: boolean; onClose(): void; onSave(identity: ApplicationIdentity): Promise<void> }) {
  const [email, setEmail] = useState("");
  const [label, setLabel] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (open) { setEmail(""); setLabel(""); setError(""); }
  }, [open]);
  const submit = async () => {
    setSaving(true);
    setError("");
    try {
      await onSave({ id: "", email, label, verification_status: "pending", is_default: false, created_at_ms: 0, updated_at_ms: 0 });
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSaving(false);
    }
  };
  return <Dialog open={open} title="Add application email" description="Use this address on resumes and application forms without changing your Bluey login." onClose={onClose}>
    <div className="dialog-form">
      <label><span>Email address</span><input type="email" value={email} onChange={(event) => setEmail(event.target.value)} placeholder="jobs@yourdomain.com" autoFocus /></label>
      <label><span>Label</span><input value={label} onChange={(event) => setLabel(event.target.value)} placeholder="Engineering applications" /></label>
      <p className="field-note">Bluey sends a 6-digit code before this address can be used.</p>
      {error && <div className="inline-error">{error}</div>}
    </div>
    <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !email.includes("@") || !label.trim()} onClick={() => void submit()}>{saving ? "Sending..." : "Send code"}</button></div>
  </Dialog>;
}

function VerifyIdentityDialog({ identity, onClose, onVerify, onResend }: { identity: ApplicationIdentity | null; onClose(): void; onVerify(identity: ApplicationIdentity, code: string): Promise<void>; onResend(identity: ApplicationIdentity): Promise<void> }) {
  const [code, setCode] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (identity) { setCode(""); setError(""); }
  }, [identity]);
  const verify = async () => {
    if (!identity) return;
    setSaving(true);
    setError("");
    try {
      await onVerify(identity, code);
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSaving(false);
    }
  };
  return <Dialog open={Boolean(identity)} title="Verify application email" description={identity ? `Enter the code sent to ${identity.email}.` : ""} onClose={onClose}>
    <div className="dialog-form verification-form">
      <label><span>Verification code</span><input inputMode="numeric" maxLength={6} value={code} onChange={(event) => setCode(event.target.value.replace(/\D/g, "").slice(0, 6))} placeholder="000000" autoFocus /></label>
      {error && <div className="inline-error">{error}</div>}
      {identity && <button className="text-button" onClick={() => void onResend(identity).catch((requestError) => setError(errorMessage(requestError)))}>Send a new code</button>}
    </div>
    <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || code.length !== 6} onClick={() => void verify()}>{saving ? "Verifying..." : "Verify email"}</button></div>
  </Dialog>;
}

function TrackDialog({ open, track, identities, roleSuggestions, locationSuggestions, onClose, onSave, onDelete }: { open: boolean; track: CareerTrack | null; identities: ApplicationIdentity[]; roleSuggestions: string[]; locationSuggestions: string[]; onClose(): void; onSave(track: CareerTrack): Promise<void>; onDelete(track: CareerTrack): void }) {
  const defaultIdentityId = identities.find((identity) => identity.is_default && identity.verification_status === "verified")?.id;
  const [draft, setDraft] = useState<CareerTrack>(normalizeTrack(track, defaultIdentityId));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (open) {
      setDraft(normalizeTrack(track, defaultIdentityId));
      setError("");
    }
  }, [open, track, defaultIdentityId]);
  const current = draft;
  const update = (next: CareerTrack) => setDraft(next);
  const save = async () => {
    setSaving(true);
    setError("");
    try {
      await onSave({
        ...current,
        role: canonicalizeTargetRole(current.role),
        application_identity_id: current.application_identity_id || defaultIdentityId,
      });
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSaving(false);
    }
  };
  return <Dialog open={open} title={track ? "Edit Career Track" : "New Career Track"} description="Give this agent one role, location policy, job category, and application email." onClose={onClose}>
    <div className="dialog-form">
      <CareerField label="Track name" value={current.name} onChange={(value) => update({ ...current, name: value })} placeholder="Product engineering" />
      <CareerField label="Target role" value={current.role} onChange={(value) => update({ ...current, role: value })} placeholder="Software Engineer" suggestions={roleSuggestions} />
      <p className="field-note">Choose the full role name. Bluey maps common abbreviations such as SWE and SDE to Software Engineer.</p>
      <CareerTagField label="Locations" values={current.locations} onChange={(values) => update({ ...current, locations: values })} placeholder="Add a location" suggestions={locationSuggestions} />
      <label><span>Workplace preference</span><select value={current.remote_preference} onChange={(event) => update({ ...current, remote_preference: event.target.value })}><option value="remote_or_hybrid">Remote or hybrid</option><option value="remote_only">Remote only</option><option value="hybrid_ok">Hybrid is fine</option><option value="onsite_ok">On-site is fine</option></select></label>
      <JobCategoryChoices label="Employment types" description="Only match these job arrangements for this Career Track." values={current.policy.employment_types} options={EMPLOYMENT_TYPE_OPTIONS} onChange={(values) => update({ ...current, policy: { ...current.policy, employment_types: values } })} />
      <JobCategoryChoices label="Contract engagement" description="Optional. Restrict contract work to the selected engagement types." values={current.policy.engagement_types} options={ENGAGEMENT_TYPE_OPTIONS} onChange={(values) => update({ ...current, policy: { ...current.policy, engagement_types: values } })} />
      <label><span>Application email</span><select value={current.application_identity_id || defaultIdentityId || ""} onChange={(event) => update({ ...current, application_identity_id: event.target.value || undefined })}>{identities.filter((identity) => identity.verification_status === "verified").map((identity) => <option key={identity.id} value={identity.id}>{identity.email}{identity.is_default ? " (default)" : ""}</option>)}</select></label>
      <label className="setting-line simple"><div><b>Agent active</b><span>Paused agents keep history but stop discovery.</span></div><Toggle checked={current.active} onChange={(checked) => update({ ...current, active: checked })} /></label>
      {error && <div className="inline-error" role="alert">{error}</div>}
    </div>
    <div className="dialog-actions">{track && <button className="button danger subtle" onClick={() => onDelete(track)}><Trash2 size={15} />Delete</button>}<span /><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !current.name || !current.role || !(current.application_identity_id || defaultIdentityId)} onClick={() => void save()}>{saving ? "Saving..." : "Save track"}</button></div>
  </Dialog>;
}

function Toggle({ checked, onChange }: { checked: boolean; onChange(checked: boolean): void }) {
  return <button type="button" className={`toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span /></button>;
}

function Plan({ name, price, details, active }: { name: string; price: string; details: string; active: boolean }) {
  return <div className={active ? "active" : ""}><span>{active ? "CURRENT" : ""}</span><h3>{name}</h3><b>{price}<small>{price !== "$0" ? "/mo" : ""}</small></b><p>{details}</p>{active ? <button className="button secondary compact" disabled><Check size={14} />Current plan</button> : <a className="button secondary compact" href={`mailto:hello@bluey.sh?subject=${encodeURIComponent(`Bluey Jobs ${name} beta`)}`}>Request {name}</a>}</div>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Bluey Jobs could not finish that request.";
}

function showError(setError: (message: string) => void): (error: unknown) => void {
  return (error) => setError(errorMessage(error));
}

function emptyTrack(applicationIdentityId?: string): CareerTrack {
  return {
    id: "",
    name: "",
    role: "",
    locations: [],
    remote_preference: "remote_or_hybrid",
    application_identity_id: applicationIdentityId,
    policy: {
      role_family: "",
      relevant_employment_ids: [],
      employment_types: [],
      engagement_types: [],
      work_authorizations: [],
    },
    active: true,
    match_count: 0,
    created_at_ms: 0,
    updated_at_ms: 0,
  };
}

function normalizeTrack(track: CareerTrack | null, applicationIdentityId?: string): CareerTrack {
  if (!track) return emptyTrack(applicationIdentityId);
  return {
    ...track,
    application_identity_id: track.application_identity_id || applicationIdentityId,
    policy: {
      role_family: track.policy?.role_family || "",
      relevant_employment_ids: track.policy?.relevant_employment_ids || [],
      employment_types: track.policy?.employment_types || [],
      engagement_types: track.policy?.engagement_types || [],
      work_authorizations: track.policy?.work_authorizations || [],
    },
  };
}
