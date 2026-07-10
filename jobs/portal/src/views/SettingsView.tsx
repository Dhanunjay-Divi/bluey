import { useEffect, useState } from "react";
import {
  ArrowRight,
  Bot,
  CalendarDays,
  Check,
  ChevronRight,
  CirclePlus,
  CreditCard,
  Mail,
  MapPin,
  Pencil,
  Plus,
  ShieldCheck,
  Trash2,
  UserRound,
} from "lucide-react";
import type { CareerProfile, CareerTrack, JobPreferences, JobsIntegration, JobsWorkspace } from "../types";
import { Dialog, ConfirmDialog } from "../components/Dialog";
import { money, titleCase } from "../lib/format";

interface Props {
  workspace: JobsWorkspace;
  onSaveProfile(profile: CareerProfile): Promise<void>;
  onSavePreferences(preferences: JobPreferences): Promise<void>;
  onSaveTrack(track: CareerTrack): Promise<void>;
  onDeleteTrack(track: CareerTrack): Promise<void>;
  onSaveIntegration(integration: JobsIntegration): Promise<void>;
}

export function SettingsView({ workspace, onSaveProfile, onSavePreferences, onSaveTrack, onDeleteTrack, onSaveIntegration }: Props) {
  const [preferences, setPreferences] = useState(workspace.preferences);
  const [profile, setProfile] = useState(workspace.profile);
  const [trackOpen, setTrackOpen] = useState(false);
  const [editingTrack, setEditingTrack] = useState<CareerTrack | null>(null);
  const [disconnecting, setDisconnecting] = useState<JobsIntegration | null>(null);
  const [connecting, setConnecting] = useState<JobsIntegration | null>(null);
  const [deletingTrack, setDeletingTrack] = useState<CareerTrack | null>(null);
  const [saved, setSaved] = useState("");

  const saveSearchSettings = async () => {
    await Promise.all([onSavePreferences(preferences), onSaveProfile(profile)]);
    setSaved("Search and automation defaults saved.");
    window.setTimeout(() => setSaved(""), 2600);
  };

  return (
    <div className="view-shell settings-view">
      <section className="view-heading">
        <div><p className="eyebrow">JOBS PREFERENCES</p><h1>Settings</h1><span>Career Tracks, application rules, integrations, and plan controls.</span></div>
        <button className="button primary" onClick={() => void saveSearchSettings()}>Save changes</button>
      </section>
      {saved && <div className="global-message success"><Check size={16} />{saved}</div>}

      <section className="settings-section" id="tracks">
        <div className="settings-section-title"><span><Bot /></span><div><p>CAREER TRACK AGENTS</p><h2>Separate searches for separate goals</h2><small>Each agent has its own role, location, and match stream.</small></div><button className="button secondary compact" onClick={() => { setEditingTrack(null); setTrackOpen(true); }} disabled={workspace.tracks.length >= workspace.entitlement.track_limit}><Plus size={15} />New track</button></div>
        <div className="track-settings-list">
          {workspace.tracks.map((track) => <button key={track.id} onClick={() => { setEditingTrack(track); setTrackOpen(true); }}><span className="agent-orbit"><Bot size={18} /></span><div><b>{track.name}</b><p>{track.role}</p><small><MapPin size={12} />{track.locations.join(" · ") || "No locations"}</small></div><span className={track.active ? "agent-state active" : "agent-state"}>{track.active ? "Active" : "Paused"}</span><ChevronRight size={17} /></button>)}
          <div className="track-limit"><span>{workspace.tracks.length} of {workspace.entitlement.track_limit} agents</span><div><i style={{ width: `${Math.min(100, workspace.tracks.length / workspace.entitlement.track_limit * 100)}%` }} /></div></div>
        </div>
      </section>

      <div className="settings-columns">
        <section className="settings-section">
          <div className="settings-section-title compact"><span><MapPin /></span><div><p>SEARCH RULES</p><h2>Where and what to apply for</h2></div></div>
          <div className="settings-form">
            <TagInput label="Target roles" values={preferences.desired_roles} onChange={(values) => setPreferences({ ...preferences, desired_roles: values })} />
            <TagInput label="Target locations" values={preferences.desired_locations} onChange={(values) => setPreferences({ ...preferences, desired_locations: values })} />
            <label><span>When a job uses another location</span><select value={preferences.location_policy} onChange={(event) => setPreferences({ ...preferences, location_policy: event.target.value as JobPreferences["location_policy"] })}><option value="ask">Ask me what to say</option><option value="local">Use my current location only</option><option value="willing_to_relocate">Say I am willing to relocate</option><option value="remote_only">Skip unless remote</option></select></label>
            <div className="form-grid two"><label><span>Daily application limit</span><input type="number" min="1" max="50" value={preferences.daily_limit} onChange={(event) => setPreferences({ ...preferences, daily_limit: Number(event.target.value) })} /></label><label><span>Minimum salary</span><input type="number" value={preferences.minimum_compensation || ""} onChange={(event) => setPreferences({ ...preferences, minimum_compensation: Number(event.target.value) || undefined })} /></label></div>
            <TagInput label="Excluded companies" values={preferences.excluded_companies} onChange={(values) => setPreferences({ ...preferences, excluded_companies: values })} />
            <label className="setting-line simple"><div><b>One active application per company</b><span>Protects against duplicate-company submissions.</span></div><Toggle checked={preferences.apply_once_per_company} onChange={(checked) => setPreferences({ ...preferences, apply_once_per_company: checked })} /></label>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-title compact"><span><ShieldCheck /></span><div><p>APPLICATION DEFAULTS</p><h2>Control before speed</h2></div></div>
          <div className="settings-form">
            <label><span>Resume mode</span><div className="segmented"><button className={profile.resume_mode === "factual" ? "active" : ""} onClick={() => setProfile({ ...profile, resume_mode: "factual" })}>Factual</button><button className={profile.resume_mode === "enhance" ? "active" : ""} onClick={() => setProfile({ ...profile, resume_mode: "enhance" })}>Enhance</button></div></label>
            <label><span>Submission mode</span><div className="segmented"><button className={profile.default_submission_mode === "review_first" ? "active" : ""} onClick={() => setProfile({ ...profile, default_submission_mode: "review_first" })}>Review first</button><button className={profile.default_submission_mode === "auto_submit" ? "active" : ""} onClick={() => setProfile({ ...profile, default_submission_mode: "auto_submit" })}>Auto-submit</button></div></label>
            <label className="setting-line simple"><div><b>Review new claims</b><span>Pause before a newly proposed factual claim can enter a packet.</span></div><Toggle checked={profile.review_new_claims} onChange={(checked) => setProfile({ ...profile, review_new_claims: checked })} /></label>
            <label><span>Auto-submit match threshold</span><div className="range-field"><input type="range" min="60" max="100" step="5" value={profile.auto_submit_threshold} onChange={(event) => setProfile({ ...profile, auto_submit_threshold: Number(event.target.value) })} /><b>{profile.auto_submit_threshold}%</b></div></label>
            <div className="pause-rules"><p>BLUEY ALWAYS PAUSES FOR</p><div>{["CAPTCHA", "2FA", "Assessments", "Unknown legal questions", "Missing required facts"].map((rule) => <span key={rule}><Check size={12} />{rule}</span>)}</div></div>
          </div>
        </section>
      </div>

      <section className="settings-section">
        <div className="settings-section-title"><span><Mail /></span><div><p>INTEGRATIONS</p><h2>Application updates and interview calendars</h2><small>Bluey reads status signals and creates reminders after you connect an account.</small></div></div>
        <div className="integration-list">{workspace.integrations.map((integration) => <div key={integration.provider}><span className="integration-icon">{integration.provider.includes("calendar") ? <CalendarDays /> : <Mail />}</span><div><b>{integrationName(integration.provider)}</b><p>{integration.status === "connected" ? integration.account_label : integration.capabilities.map(titleCase).join(" · ")}</p></div><span className={`integration-state ${integration.status}`}>{titleCase(integration.status)}</span>{integration.status === "connected" ? <button className="button secondary compact" onClick={() => setDisconnecting(integration)}>Disconnect</button> : <button className="button secondary compact" onClick={() => setConnecting(integration)}>Connect</button>}</div>)}</div>
      </section>

      <section className="settings-section" id="plans">
        <div className="settings-section-title"><span><CreditCard /></span><div><p>PLAN</p><h2>{titleCase(workspace.entitlement.plan)} Jobs</h2><small>{workspace.entitlement.used_packets} of {workspace.entitlement.monthly_packet_limit} application packets used this month.</small></div><a className="button secondary compact" href="/account#billing">Shared balance<ArrowRight size={15} /></a></div>
        <div className="plan-grid">
          <Plan name="Free" price="$0" details="1 agent · 5 reviewed packets" active={workspace.entitlement.plan === "free"} />
          <Plan name="Pro" price="$29" details="3 agents · Local Browser · 50 packets" active={workspace.entitlement.plan === "pro"} />
          <Plan name="Cloud" price="$49" details="5 agents · Local + cloud · 100 packets" active={workspace.entitlement.plan === "cloud"} />
        </div>
        <p className="plan-footnote">After the monthly allowance, each additional completed packet is {money(workspace.entitlement.overage_cents)} from your shared Bluey balance. Retries and browser handoffs do not count again.</p>
      </section>

      <TrackDialog
        open={trackOpen}
        track={editingTrack}
        onClose={() => setTrackOpen(false)}
        onSave={async (track) => { await onSaveTrack(track); setTrackOpen(false); }}
        onDelete={(track) => { setTrackOpen(false); setDeletingTrack(track); }}
      />
      <Dialog open={Boolean(connecting)} title={`Connect ${connecting ? integrationName(connecting.provider) : "account"}`} description="Email and calendar connections are being enabled for invited Jobs beta accounts." onClose={() => setConnecting(null)}>
        <div className="integration-connect-copy">
          <span className="integration-icon large">{connecting?.provider.includes("calendar") ? <CalendarDays /> : <Mail />}</span>
          <div><h3>Keep your application timeline current</h3><p>Bluey will use provider authorization to recognize application updates, interviews, and reminders. It will never ask for your email password.</p></div>
        </div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setConnecting(null)}>Not now</button><a className="button primary" href={`mailto:hello@bluey.sh?subject=${encodeURIComponent(`Bluey Jobs ${connecting ? integrationName(connecting.provider) : "integration"} beta`)}`}>Request beta access<ArrowRight size={15} /></a></div>
      </Dialog>
      <ConfirmDialog open={Boolean(disconnecting)} title={`Disconnect ${disconnecting ? integrationName(disconnecting.provider) : "integration"}?`} description="Bluey will stop syncing new status updates from this account. Existing application history stays in Jobs." confirmLabel="Disconnect" tone="danger" onClose={() => setDisconnecting(null)} onConfirm={() => { if (disconnecting) void onSaveIntegration({ ...disconnecting, status: "disconnected", account_label: "" }); setDisconnecting(null); }} />
      <ConfirmDialog open={Boolean(deletingTrack)} title={`Delete ${deletingTrack?.name || "Career Track"}?`} description="This stops discovery for the track. Existing matches and applications stay in your history." confirmLabel="Delete track" tone="danger" onClose={() => setDeletingTrack(null)} onConfirm={() => { const track = deletingTrack; setDeletingTrack(null); if (track) void onDeleteTrack(track); }} />
    </div>
  );
}

function TrackDialog({ open, track, onClose, onSave, onDelete }: { open: boolean; track: CareerTrack | null; onClose(): void; onSave(track: CareerTrack): Promise<void>; onDelete(track: CareerTrack): void }) {
  const [draft, setDraft] = useState<CareerTrack>(track || emptyTrack());
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    if (open) setDraft(track || emptyTrack());
  }, [open, track]);
  const current = draft;
  const update = (next: CareerTrack) => setDraft(next);
  return <Dialog open={open} title={track ? "Edit Career Track" : "New Career Track"} description="Give this agent one role and a clear location policy." onClose={onClose}><div className="dialog-form"><label><span>Track name</span><input value={current.name} onChange={(event) => update({ ...current, name: event.target.value })} placeholder="Product engineering" /></label><label><span>Target role</span><input value={current.role} onChange={(event) => update({ ...current, role: event.target.value })} placeholder="Senior Product Engineer" /></label><TagInput label="Locations" values={current.locations} onChange={(values) => update({ ...current, locations: values })} /><label><span>Workplace preference</span><select value={current.remote_preference} onChange={(event) => update({ ...current, remote_preference: event.target.value })}><option value="remote_or_hybrid">Remote or hybrid</option><option value="remote_only">Remote only</option><option value="hybrid_ok">Hybrid is fine</option><option value="onsite_ok">On-site is fine</option></select></label><label className="setting-line simple"><div><b>Agent active</b><span>Paused agents keep history but stop discovery.</span></div><Toggle checked={current.active} onChange={(checked) => update({ ...current, active: checked })} /></label></div><div className="dialog-actions">{track && <button className="button danger subtle" onClick={() => onDelete(track)}><Trash2 size={15} />Delete</button>}<span /><button className="button secondary" onClick={onClose}>Cancel</button><button className="button primary" disabled={saving || !current.name || !current.role} onClick={() => { setSaving(true); void onSave(current).finally(() => setSaving(false)); }}>{saving ? "Saving..." : "Save track"}</button></div></Dialog>;
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
  return ({ gmail: "Gmail", outlook_email: "Outlook Email", google_calendar: "Google Calendar", outlook_calendar: "Outlook Calendar" } as Record<string, string>)[provider] || titleCase(provider);
}

function emptyTrack(): CareerTrack {
  return { id: "", name: "", role: "", locations: [], remote_preference: "remote_or_hybrid", active: true, match_count: 0, created_at_ms: 0, updated_at_ms: 0 };
}
