import { useEffect, useState } from "react";
import {
  ArrowRight,
  Bot,
  BrainCircuit,
  CalendarDays,
  Check,
  ChevronRight,
  CirclePlus,
  ClipboardCheck,
  CreditCard,
  Mail,
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
  JobsIntegration,
  JobsWorkspace,
  MailboxConnection,
} from "../types";
import { Dialog, ConfirmDialog } from "../components/Dialog";
import { money, titleCase } from "../lib/format";

interface Props {
  workspace: JobsWorkspace;
  onSaveProfile(profile: CareerProfile): Promise<void>;
  onSavePreferences(preferences: JobPreferences): Promise<void>;
  onSaveTrack(track: CareerTrack): Promise<void>;
  onDeleteTrack(track: CareerTrack): Promise<void>;
  onSaveIntegration(integration: JobsIntegration): Promise<void>;
  onCreateIdentity(identity: ApplicationIdentity): Promise<ApplicationIdentity>;
  onUpdateIdentity(identity: ApplicationIdentity): Promise<ApplicationIdentity>;
  onVerifyIdentity(identity: ApplicationIdentity, code: string): Promise<ApplicationIdentity>;
  onResendIdentity(identity: ApplicationIdentity): Promise<void>;
  onDeleteIdentity(identity: ApplicationIdentity): Promise<void>;
  onRequestMailbox(connection: MailboxConnection): Promise<MailboxConnection>;
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
  onSaveIntegration,
  onCreateIdentity,
  onUpdateIdentity,
  onVerifyIdentity,
  onResendIdentity,
  onDeleteIdentity,
  onRequestMailbox,
  onDeleteMailbox,
  onSaveAnswerMemory,
  onDeleteAnswerMemory,
}: Props) {
  const [preferences, setPreferences] = useState(workspace.preferences);
  const [profile, setProfile] = useState(workspace.profile);
  const [trackOpen, setTrackOpen] = useState(false);
  const [editingTrack, setEditingTrack] = useState<CareerTrack | null>(null);
  const [disconnecting, setDisconnecting] = useState<JobsIntegration | null>(null);
  const [connecting, setConnecting] = useState<JobsIntegration | null>(null);
  const [deletingTrack, setDeletingTrack] = useState<CareerTrack | null>(null);
  const [identityOpen, setIdentityOpen] = useState(false);
  const [verifyingIdentity, setVerifyingIdentity] = useState<ApplicationIdentity | null>(null);
  const [deletingIdentity, setDeletingIdentity] = useState<ApplicationIdentity | null>(null);
  const [mailboxOpen, setMailboxOpen] = useState(false);
  const [disconnectingMailbox, setDisconnectingMailbox] = useState<MailboxConnection | null>(null);
  const [answerOpen, setAnswerOpen] = useState(false);
  const [editingAnswer, setEditingAnswer] = useState<AnswerMemory | null>(null);
  const [deletingAnswer, setDeletingAnswer] = useState<AnswerMemory | null>(null);
  const [saved, setSaved] = useState("");
  const [localError, setLocalError] = useState("");
  const [savingSearch, setSavingSearch] = useState(false);

  const saveSearchSettings = async () => {
    if (savingSearch) return;
    setSavingSearch(true);
    setLocalError("");
    setSaved("");
    try {
      await Promise.all([onSavePreferences(preferences), onSaveProfile(profile)]);
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
          {workspace.tracks.map((track) => <button key={track.id} onClick={() => { setEditingTrack(track); setTrackOpen(true); }}><span className="agent-orbit"><Bot size={18} /></span><div><b>{track.name}</b><p>{track.role}</p><small><MapPin size={12} />{track.locations.join(" · ") || "No locations"}</small></div><span className={track.active ? "agent-state active" : "agent-state"}>{track.active ? "Active" : "Paused"}</span><ChevronRight size={17} /></button>)}
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
            <TagInput label="Target roles" values={preferences.desired_roles} onChange={(values) => setPreferences({ ...preferences, desired_roles: values })} />
            <TagInput label="Target locations" values={preferences.desired_locations} onChange={(values) => setPreferences({ ...preferences, desired_locations: values })} />
            <label><span>When a job uses another location</span><select value={preferences.location_policy} onChange={(event) => setPreferences({ ...preferences, location_policy: event.target.value as JobPreferences["location_policy"] })}><option value="ask">Ask me what to say</option><option value="local">Use my current location only</option><option value="willing_to_relocate">Say I am willing to relocate</option><option value="remote_only">Skip unless remote</option></select></label>
            <div className="form-grid two"><label><span>Daily application limit</span><input type="number" min="1" max="50" value={preferences.daily_limit} onChange={(event) => setPreferences({ ...preferences, daily_limit: Number(event.target.value) })} /></label><label><span>Maximum job age</span><select value={preferences.max_posting_age_days} onChange={(event) => setPreferences({ ...preferences, max_posting_age_days: Number(event.target.value) })}><option value={7}>7 days</option><option value={14}>14 days</option><option value={21}>21 days</option><option value={30}>30 days</option></select></label></div>
            <p className="field-note">Bluey skips older listings and confirms a job is still open before applying.</p>
            <label><span>Minimum salary</span><input type="number" value={preferences.minimum_compensation || ""} onChange={(event) => setPreferences({ ...preferences, minimum_compensation: Number(event.target.value) || undefined })} /></label>
            <TagInput label="Excluded companies" values={preferences.excluded_companies} onChange={(values) => setPreferences({ ...preferences, excluded_companies: values })} />
            <div className="setting-line simple"><div><b>One application per company</b><span>Always enforced across Career Tracks, resume versions, and application emails.</span></div><span className="status-chip success">Locked</span></div>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-title compact"><span><ShieldCheck /></span><div><p>APPLICATION DEFAULTS</p><h2>Control before speed</h2></div></div>
          <div className="settings-form">
            <label><span>Resume mode</span><div className="segmented"><button className={profile.resume_mode === "factual" ? "active" : ""} onClick={() => setProfile({ ...profile, resume_mode: "factual" })}>Factual</button><button className={profile.resume_mode === "enhance" ? "active" : ""} onClick={() => setProfile({ ...profile, resume_mode: "enhance" })}>Enhance</button></div></label>
            <label><span>Submission mode</span><div className="segmented"><button className={profile.default_submission_mode === "review_first" ? "active" : ""} onClick={() => setProfile({ ...profile, default_submission_mode: "review_first" })}>Review first</button><button className={profile.default_submission_mode === "auto_submit" ? "active" : ""} onClick={() => setProfile({ ...profile, default_submission_mode: "auto_submit" })}>Auto-submit</button></div></label>
            <label className="setting-line simple"><div><b>Review new claims</b><span>Pause before a newly proposed factual claim can enter an application.</span></div><Toggle checked={profile.review_new_claims} onChange={(checked) => setProfile({ ...profile, review_new_claims: checked })} /></label>
            <label><span>Auto-submit match threshold</span><div className="range-field"><input type="range" min="60" max="100" step="5" value={profile.auto_submit_threshold} onChange={(event) => setProfile({ ...profile, auto_submit_threshold: Number(event.target.value) })} /><b>{profile.auto_submit_threshold}%</b></div></label>
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

      <section className="settings-section">
        <div className="settings-section-title"><span><Mail /></span><div><p>INBOXES & CALENDARS</p><h2>Keep every application timeline current</h2><small>One inbox connection includes its aliases. Connect another slot only for a separate Gmail or Outlook mailbox.</small></div><button className="button secondary compact" onClick={() => setMailboxOpen(true)} disabled={workspace.mailbox_connections.filter((item) => item.status !== "disconnected").length >= workspace.entitlement.connected_inbox_limit}><Plus size={15} />Connect inbox</button></div>
        <div className="connection-usage"><span><b>{workspace.mailbox_connections.filter((item) => item.status !== "disconnected").length}</b> of {workspace.entitlement.connected_inbox_limit} inbox connections</span><span>Extra inbox slot: {money(workspace.entitlement.additional_inbox_cents)}/month</span></div>
        <div className="integration-list">
          {workspace.mailbox_connections.map((connection) => <div key={connection.id}>
            <span className="integration-icon"><Mail /></span>
            <div><b>{connection.account_label}</b><p>{connectionName(connection.provider)}{connection.aliases.length ? ` · ${connection.aliases.length} alias${connection.aliases.length === 1 ? "" : "es"}` : ""}</p></div>
            <span className={`integration-state ${connection.status}`}>{titleCase(connection.status)}</span>
            <button className="button secondary compact" onClick={() => setDisconnectingMailbox(connection)}>{connection.status === "pending" ? "Remove" : "Disconnect"}</button>
          </div>)}
          {workspace.integrations.map((integration) => <div key={integration.provider}><span className="integration-icon"><CalendarDays /></span><div><b>{integrationName(integration.provider)}</b><p>{integration.status === "connected" ? integration.account_label : integration.capabilities.map(titleCase).join(" · ")}</p></div><span className={`integration-state ${integration.status}`}>{titleCase(integration.status)}</span>{integration.status === "connected" ? <button className="button secondary compact" onClick={() => setDisconnecting(integration)}>Disconnect</button> : <button className="button secondary compact" onClick={() => setConnecting(integration)}>Connect</button>}</div>)}
        </div>
      </section>

      <section className="settings-section" id="plans">
        <div className="settings-section-title"><span><CreditCard /></span><div><p>PLAN</p><h2>{titleCase(workspace.entitlement.plan)} Jobs</h2><small>{workspace.entitlement.used_packets} of {workspace.entitlement.monthly_packet_limit} included applications used this month.</small></div><a className="button secondary compact" href="/account#billing">Shared balance<ArrowRight size={15} /></a></div>
        <div className="plan-grid">
          <Plan name="Free" price="$0" details="1 agent · 5 reviewed applications · 2 application emails · 1 inbox" active={workspace.entitlement.plan === "free"} />
          <Plan name="Pro" price="$29" details="3 agents · 50 applications · 10 application emails · 2 inboxes · invited local runner beta" active={workspace.entitlement.plan === "pro"} />
          <Plan name="Cloud" price="$49" details="5 agents · 100 applications · 25 application emails · 5 inboxes · invited cloud runner beta" active={workspace.entitlement.plan === "cloud"} />
        </div>
        <p className="plan-footnote">Application emails and aliases are included. Separate inboxes use connection slots; additional slots are {money(workspace.entitlement.additional_inbox_cents)}/month. After the included applications, each additional completed application is {money(workspace.entitlement.overage_cents)} from your shared Bluey balance. Retries and browser handoffs do not count again.</p>
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
      <MailboxDialog
        open={mailboxOpen}
        onClose={() => setMailboxOpen(false)}
        onSave={async (connection) => {
          setLocalError("");
          await onRequestMailbox(connection);
          setMailboxOpen(false);
        }}
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
        onClose={() => setTrackOpen(false)}
        onSave={async (track) => { await onSaveTrack(track); setTrackOpen(false); }}
        onDelete={(track) => { setTrackOpen(false); setDeletingTrack(track); }}
      />
      <Dialog open={Boolean(connecting)} title={`Connect ${connecting ? integrationName(connecting.provider) : "account"}`} description="Email and calendar connections are being enabled for invited Jobs beta accounts." onClose={() => setConnecting(null)}>
        <div className="integration-connect-copy">
          <span className="integration-icon large">{connecting?.provider.includes("calendar") ? <CalendarDays /> : <Mail />}</span>
          <div><h3>Integration beta access</h3><p>Email and calendar sync is not active for this account yet. Request access to help test provider authorization; Bluey will never ask for your email password.</p></div>
        </div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setConnecting(null)}>Not now</button><a className="button primary" href={`mailto:hello@bluey.sh?subject=${encodeURIComponent(`Bluey Jobs ${connecting ? integrationName(connecting.provider) : "integration"} beta`)}`}>Request beta access<ArrowRight size={15} /></a></div>
      </Dialog>
      <ConfirmDialog open={Boolean(disconnecting)} title={`Disconnect ${disconnecting ? integrationName(disconnecting.provider) : "integration"}?`} description="Bluey will stop syncing new status updates from this account. Existing application history stays in Jobs." confirmLabel="Disconnect" tone="danger" onClose={() => setDisconnecting(null)} onConfirm={() => { const integration = disconnecting; setDisconnecting(null); if (integration) void onSaveIntegration({ ...integration, status: "disconnected", account_label: "" }).catch(showError(setLocalError)); }} />
      <ConfirmDialog open={Boolean(deletingIdentity)} title={`Remove ${deletingIdentity?.email || "application email"}?`} description="Bluey will keep existing application receipts, but this address will no longer be available for new Career Tracks." confirmLabel="Remove email" tone="danger" onClose={() => setDeletingIdentity(null)} onConfirm={() => { const identity = deletingIdentity; setDeletingIdentity(null); if (identity) void onDeleteIdentity(identity).catch(showError(setLocalError)); }} />
      <ConfirmDialog open={Boolean(disconnectingMailbox)} title={`${disconnectingMailbox?.status === "pending" ? "Remove" : "Disconnect"} ${disconnectingMailbox?.account_label || "inbox"}?`} description="Bluey will stop reading new application updates from this inbox. Existing application history stays in Jobs." confirmLabel={disconnectingMailbox?.status === "pending" ? "Remove" : "Disconnect"} tone="danger" onClose={() => setDisconnectingMailbox(null)} onConfirm={() => { const connection = disconnectingMailbox; setDisconnectingMailbox(null); if (connection) void onDeleteMailbox(connection).catch(showError(setLocalError)); }} />
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

function MailboxDialog({ open, onClose, onSave }: { open: boolean; onClose(): void; onSave(connection: MailboxConnection): Promise<void> }) {
  const [provider, setProvider] = useState<MailboxConnection["provider"]>("gmail");
  const [email, setEmail] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (open) { setProvider("gmail"); setEmail(""); setError(""); }
  }, [open]);
  const submit = async () => {
    setSaving(true);
    setError("");
    try {
      await onSave({ id: "", provider, status: "pending", account_label: email, aliases: [], capabilities: ["status_sync", "follow_ups"], created_at_ms: 0, updated_at_ms: 0 });
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSaving(false);
    }
  };
  return <Dialog open={open} title="Connect an inbox" description="Connect each independent mailbox once. Addresses that deliver into the same inbox count as aliases." onClose={onClose}>
    <div className="dialog-form">
      <label><span>Provider</span><div className="segmented"><button className={provider === "gmail" ? "active" : ""} onClick={() => setProvider("gmail")}>Gmail</button><button className={provider === "outlook" ? "active" : ""} onClick={() => setProvider("outlook")}>Outlook</button></div></label>
      <label><span>Inbox email</span><input type="email" value={email} onChange={(event) => setEmail(event.target.value)} placeholder="you@company.com" /></label>
      <p className="field-note">Bluey uses provider authorization. Your mailbox password is never requested or stored.</p>
      {error && <div className="inline-error">{error}</div>}
    </div>
    <div className="dialog-actions"><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !email.includes("@")} onClick={() => void submit()}>{saving ? "Preparing..." : `Continue with ${provider === "gmail" ? "Gmail" : "Outlook"}`}</button></div>
  </Dialog>;
}

function TrackDialog({ open, track, identities, onClose, onSave, onDelete }: { open: boolean; track: CareerTrack | null; identities: ApplicationIdentity[]; onClose(): void; onSave(track: CareerTrack): Promise<void>; onDelete(track: CareerTrack): void }) {
  const defaultIdentityId = identities.find((identity) => identity.is_default && identity.verification_status === "verified")?.id;
  const [draft, setDraft] = useState<CareerTrack>(track || emptyTrack(defaultIdentityId));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (open) {
      setDraft(track || emptyTrack(defaultIdentityId));
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
        application_identity_id: current.application_identity_id || defaultIdentityId,
      });
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSaving(false);
    }
  };
  return <Dialog open={open} title={track ? "Edit Career Track" : "New Career Track"} description="Give this agent one role, location policy, and application email." onClose={onClose}><div className="dialog-form"><label><span>Track name</span><input value={current.name} onChange={(event) => update({ ...current, name: event.target.value })} placeholder="Product engineering" /></label><label><span>Target role</span><input value={current.role} onChange={(event) => update({ ...current, role: event.target.value })} placeholder="Senior Product Engineer" /></label><TagInput label="Locations" values={current.locations} onChange={(values) => update({ ...current, locations: values })} /><label><span>Workplace preference</span><select value={current.remote_preference} onChange={(event) => update({ ...current, remote_preference: event.target.value })}><option value="remote_or_hybrid">Remote or hybrid</option><option value="remote_only">Remote only</option><option value="hybrid_ok">Hybrid is fine</option><option value="onsite_ok">On-site is fine</option></select></label><label><span>Application email</span><select value={current.application_identity_id || defaultIdentityId || ""} onChange={(event) => update({ ...current, application_identity_id: event.target.value || undefined })}>{identities.filter((identity) => identity.verification_status === "verified").map((identity) => <option key={identity.id} value={identity.id}>{identity.email}{identity.is_default ? " (default)" : ""}</option>)}</select></label><label className="setting-line simple"><div><b>Agent active</b><span>Paused agents keep history but stop discovery.</span></div><Toggle checked={current.active} onChange={(checked) => update({ ...current, active: checked })} /></label>{error && <div className="inline-error" role="alert">{error}</div>}</div><div className="dialog-actions">{track && <button className="button danger subtle" onClick={() => onDelete(track)}><Trash2 size={15} />Delete</button>}<span /><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !current.name || !current.role || !(current.application_identity_id || defaultIdentityId)} onClick={() => void save()}>{saving ? "Saving..." : "Save track"}</button></div></Dialog>;
}

function TagInput({ label, values, onChange }: { label: string; values: string[]; onChange(values: string[]): void }) {
  const [draft, setDraft] = useState("");
  const add = () => { const value = draft.trim(); if (value && !values.includes(value)) onChange([...values, value]); setDraft(""); };
  return <label className="tag-setting"><span>{label}</span><div>{values.map((value) => <button key={value} onClick={() => onChange(values.filter((item) => item !== value))}>{value}<i>×</i></button>)}<input value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === ",") { event.preventDefault(); add(); } }} onBlur={add} placeholder="Add and press Enter" /></div></label>;
}

function Toggle({ checked, onChange }: { checked: boolean; onChange(checked: boolean): void }) {
  return <button type="button" className={`toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span /></button>;
}

function Plan({ name, price, details, active }: { name: string; price: string; details: string; active: boolean }) {
  return <div className={active ? "active" : ""}><span>{active ? "CURRENT" : ""}</span><h3>{name}</h3><b>{price}<small>{price !== "$0" ? "/mo" : ""}</small></b><p>{details}</p>{active ? <button className="button secondary compact" disabled><Check size={14} />Current plan</button> : <a className="button secondary compact" href={`mailto:hello@bluey.sh?subject=${encodeURIComponent(`Bluey Jobs ${name} beta`)}`}>Request {name}</a>}</div>;
}

function integrationName(provider: string): string {
  return ({ google_calendar: "Google Calendar", outlook_calendar: "Outlook Calendar" } as Record<string, string>)[provider] || titleCase(provider);
}

function connectionName(provider: MailboxConnection["provider"]): string {
  return provider === "gmail" ? "Gmail inbox" : "Outlook inbox";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Bluey Jobs could not finish that request.";
}

function showError(setError: (message: string) => void): (error: unknown) => void {
  return (error) => setError(errorMessage(error));
}

function emptyTrack(applicationIdentityId?: string): CareerTrack {
  return { id: "", name: "", role: "", locations: [], remote_preference: "remote_or_hybrid", application_identity_id: applicationIdentityId, active: true, match_count: 0, created_at_ms: 0, updated_at_ms: 0 };
}
