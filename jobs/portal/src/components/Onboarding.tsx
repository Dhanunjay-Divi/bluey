import { useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  BriefcaseBusiness,
  Check,
  FileUp,
  GraduationCap,
  LoaderCircle,
  MapPin,
  Mic,
  Plus,
  Settings2,
  Sparkles,
  Trash2,
  UserRound,
} from "lucide-react";
import type {
  CareerProfile,
  CareerTrack,
  EducationEntry,
  EmploymentEntry,
  JobPreferences,
  JobsWorkspace,
} from "../types";
import {
  importResume,
  inferProfileFromResume,
  summarizeResumeImport,
  type ResumeImportSummary,
} from "../lib/documents";
import blueyIcon from "../../../../web/assets/bluey-logo.svg";
import blueyWordmark from "../../../../web/assets/bluey-wordmark.svg";

interface Props {
  workspace: JobsWorkspace;
  error: string;
  onProgress(profile: CareerProfile, preferences: JobPreferences): Promise<void>;
  onComplete(profile: CareerProfile, preferences: JobPreferences, track: CareerTrack): Promise<void>;
}

const steps = [
  { label: "You", icon: UserRound },
  { label: "Experience", icon: BriefcaseBusiness },
  { label: "Education", icon: GraduationCap },
  { label: "Goals", icon: MapPin },
  { label: "Defaults", icon: Settings2 },
  { label: "Ready", icon: Check },
];

export function Onboarding({ workspace, error, onProgress, onComplete }: Props) {
  const [step, setStep] = useState(Math.min(workspace.profile.onboarding_step || 0, steps.length - 1));
  const [profile, setProfile] = useState<CareerProfile>(workspace.profile);
  const [preferences, setPreferences] = useState<JobPreferences>(workspace.preferences);
  const [notes, setNotes] = useState("");
  const [importing, setImporting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [validation, setValidation] = useState("");
  const [importSummary, setImportSummary] = useState<ResumeImportSummary | null>(
    workspace.profile.source_resume_name ? summarizeResumeImport(workspace.profile) : null,
  );
  const [resumeStart, setResumeStart] = useState<"import" | "build">("import");
  const fileRef = useRef<HTMLInputElement>(null);
  const trackId = useRef(workspace.tracks[0]?.id || "onboarding-primary-track");

  const progress = ((step + 1) / steps.length) * 100;
  const track = useMemo<CareerTrack>(
    () => ({
      id: trackId.current,
      name: preferences.desired_roles[0] || "Primary search",
      role: preferences.desired_roles[0] || profile.headline,
      locations: preferences.desired_locations,
      remote_preference: preferences.remote_preference,
      active: true,
      match_count: 0,
      created_at_ms: workspace.tracks[0]?.created_at_ms || 0,
      updated_at_ms: 0,
    }),
    [preferences, profile.headline, workspace.tracks],
  );

  const update = <K extends keyof CareerProfile>(key: K, value: CareerProfile[K]) =>
    setProfile((current) => ({ ...current, [key]: value }));
  const updatePreferences = <K extends keyof JobPreferences>(key: K, value: JobPreferences[K]) =>
    setPreferences((current) => ({ ...current, [key]: value }));

  const handleFile = async (file?: File) => {
    if (!file) return;
    setImporting(true);
    setValidation("");
    try {
      const imported = await importResume(file);
      setProfile((current) => {
        const inferred = inferProfileFromResume(current, imported);
        setImportSummary(summarizeResumeImport(inferred));
        return inferred;
      });
    } catch (fileError) {
      setValidation(fileError instanceof Error ? fileError.message : "That resume could not be read.");
    } finally {
      setImporting(false);
    }
  };

  const moveToStep = async (nextStep: number) => {
    const boundedStep = Math.max(0, Math.min(nextStep, steps.length - 1));
    const nextProfile = {
      ...profile,
      onboarding_step: boundedStep,
      onboarding_complete: false,
    };
    setSaving(true);
    setValidation("");
    try {
      await onProgress(nextProfile, preferences);
      setProfile(nextProfile);
      setStep(boundedStep);
      window.scrollTo({ top: 0, behavior: "smooth" });
    } catch {
      // App owns the request error shown below the active step.
    } finally {
      setSaving(false);
    }
  };

  const next = async () => {
    const message = validateStep(step, profile, preferences);
    if (message) {
      setValidation(message);
      return;
    }
    await moveToStep(step + 1);
  };

  const finish = async () => {
    if (profile.employment.length === 0 && profile.education.length === 0) {
      setValidation("Bluey could not confirm a role or education entry. Review either section before launching your first Career Track.");
      return;
    }
    const message = validateStep(3, profile, preferences);
    if (message) {
      setValidation(message);
      return;
    }
    setSaving(true);
    const finishedProfile = {
      ...profile,
      onboarding_step: steps.length,
      onboarding_complete: true,
    };
    try {
      await onComplete(finishedProfile, preferences, track);
    } finally {
      setSaving(false);
    }
  };

  return (
    <main className="onboarding-shell">
      <header className="onboarding-header">
        <a className="brand-lockup small" href="/" aria-label="Bluey home">
          <img className="brand-icon" src={blueyIcon} alt="" />
          <img className="brand-wordmark" src={blueyWordmark} alt="" />
          <b>jobs</b>
        </a>
        <span>Career Profile setup · about 5 minutes</span>
        <a href="/account">Exit</a>
      </header>

      <div className="onboarding-progress" aria-hidden="true"><span style={{ width: `${progress}%` }} /></div>

      <div className="onboarding-layout">
        <aside className="setup-steps" aria-label="Setup progress">
          <p className="eyebrow">YOUR BASELINE</p>
          <h1>Set it up once. Bluey handles the repetition.</h1>
          <p>Your baseline becomes a separate, job-specific resume and answer set for every application.</p>
          <ol>
            {steps.map(({ label, icon: Icon }, index) => (
              <li key={label} className={index === step ? "active" : index < step ? "done" : ""}>
                <span>{index < step ? <Check size={15} /> : <Icon size={15} />}</span>
                <div><b>{label}</b><small>{stepDescription(index)}</small></div>
              </li>
            ))}
          </ol>
        </aside>

        <section className="setup-panel">
          {step === 0 && (
            <>
              <div className="setup-heading"><p>STEP 1 OF 6</p><h2>Import once, then tailor every job</h2><span>Bluey extracts a baseline profile. You review it before the first Career Track starts.</span></div>
              <input
                ref={fileRef}
                hidden
                type="file"
                accept=".pdf,.docx,.txt"
                onChange={(event) => void handleFile(event.target.files?.[0])}
              />
              <div className="onboarding-start-choice" role="group" aria-label="Career Profile starting point">
                <button className={resumeStart === "import" ? "active" : ""} onClick={() => setResumeStart("import")}><FileUp size={18} /><span><b>Import my resume</b><small>Start from PDF, DOCX, or TXT.</small></span></button>
                <button className={resumeStart === "build" ? "active" : ""} onClick={() => setResumeStart("build")}><UserRound size={18} /><span><b>I do not have a resume</b><small>Build the same Career Profile from your history.</small></span></button>
              </div>
              {resumeStart === "import" ? <button className="resume-dropzone" onClick={() => fileRef.current?.click()}>
                {importing ? <LoaderCircle className="spin" /> : <FileUp />}
                <strong>{profile.source_resume_name || "Choose PDF, DOCX, or TXT"}</strong>
                <span>{profile.source_resume_name ? "Resume imported. Existing edits stay in place if you choose another file." : "Bluey extracts a baseline you can edit before anything is prepared."}</span>
              </button> : <div className="no-resume-note"><Sparkles size={18} /><span><b>Start with the facts you know</b><small>Add roles, education, and skills in the next steps. Bluey builds the first resume from that profile.</small></span></div>}
              {importSummary && (
                <div className="resume-import-summary" aria-live="polite">
                  <Check size={17} />
                  <div>
                    <b>Baseline extracted</b>
                    <span>
                      {importSummary.employment} role{importSummary.employment === 1 ? "" : "s"} ·{" "}
                      {importSummary.education} school{importSummary.education === 1 ? "" : "s"} ·{" "}
                      {importSummary.skills} skill{importSummary.skills === 1 ? "" : "s"} ·{" "}
                      {importSummary.projects} project{importSummary.projects === 1 ? "" : "s"}
                    </span>
                    <small>Review the next sections. Bluey never submits the imported draft by itself.</small>
                  </div>
                </div>
              )}
              <div className="form-grid two">
                <Field label="Full name" value={profile.full_name} onChange={(value) => update("full_name", value)} autoFocus />
                <Field label="Phone" value={profile.phone} onChange={(value) => update("phone", value)} />
                <Field label="Current location" value={profile.current_location} onChange={(value) => update("current_location", value)} placeholder="City, state" />
                <Field label="Professional headline" value={profile.headline} onChange={(value) => update("headline", value)} placeholder="Senior Product Engineer" />
                <Field label="LinkedIn" value={profile.linkedin_url} onChange={(value) => update("linkedin_url", value)} placeholder="https://linkedin.com/in/..." />
                <Field label="Portfolio" value={profile.portfolio_url} onChange={(value) => update("portfolio_url", value)} placeholder="https://..." />
              </div>
            </>
          )}

          {step === 1 && (
            <>
              <div className="setup-heading"><p>STEP 2 OF 6</p><h2>Work history</h2><span>Company, title, dates, and truthful outcomes give Bluey the raw material to tailor.</span></div>
              <div className="entry-list">
                {profile.employment.map((entry, index) => (
                  <EmploymentEditor
                    key={entry.id || index}
                    entry={entry}
                    onChange={(next) => update("employment", profile.employment.map((item, itemIndex) => itemIndex === index ? next : item))}
                    onRemove={() => update("employment", profile.employment.filter((_, itemIndex) => itemIndex !== index))}
                  />
                ))}
              </div>
              <button className="button secondary compact" onClick={() => update("employment", [...profile.employment, emptyEmployment()])}><Plus size={16} />Add role</button>
              <QuickCapture notes={notes} setNotes={setNotes} onUse={() => { update("summary", [profile.summary, notes].filter(Boolean).join(" ")); setNotes(""); }} />
            </>
          )}

          {step === 2 && (
            <>
              <div className="setup-heading"><p>STEP 3 OF 6</p><h2>Education and skills</h2><span>Add what recruiters need to verify. Bluey can reorder it later without changing the facts.</span></div>
              {profile.education.map((entry, index) => (
                <EducationEditor
                  key={entry.id || index}
                  entry={entry}
                  onChange={(next) => update("education", profile.education.map((item, itemIndex) => itemIndex === index ? next : item))}
                  onRemove={() => update("education", profile.education.filter((_, itemIndex) => itemIndex !== index))}
                />
              ))}
              <button className="button secondary compact" onClick={() => update("education", [...profile.education, emptyEducation()])}><Plus size={16} />Add education</button>
              <div className="form-grid two roomy-top">
                <TagField label="Skills" values={profile.skills} onChange={(values) => update("skills", values)} placeholder="Type a skill and press Enter" />
                <TagField label="Certifications" values={profile.certifications} onChange={(values) => update("certifications", values)} placeholder="Type a certification and press Enter" />
              </div>
            </>
          )}

          {step === 3 && (
            <>
              <div className="setup-heading"><p>STEP 4 OF 6</p><h2>Where should Bluey look?</h2><span>Location is a hard filter. Tell Bluey what to say instead of letting an application guess.</span></div>
              <div className="form-grid two">
                <TagField label="Target roles" values={preferences.desired_roles} onChange={(values) => updatePreferences("desired_roles", values)} placeholder="Senior Product Engineer" />
                <TagField label="Target locations" values={preferences.desired_locations} onChange={(values) => updatePreferences("desired_locations", values)} placeholder="New York, NY" />
                <SelectField label="Location answer" value={preferences.location_policy} onChange={(value) => updatePreferences("location_policy", value as JobPreferences["location_policy"])} options={[
                  ["ask", "Ask before using another location"],
                  ["local", "Use my current location only"],
                  ["willing_to_relocate", "I am willing to relocate"],
                  ["remote_only", "Remote roles only"],
                ]} />
                <SelectField label="Workplace" value={preferences.remote_preference} onChange={(value) => updatePreferences("remote_preference", value)} options={[
                  ["remote_or_hybrid", "Remote or hybrid"],
                  ["remote_only", "Remote only"],
                  ["hybrid_ok", "Hybrid is fine"],
                  ["onsite_ok", "On-site is fine"],
                ]} />
                <Field label="Minimum salary" value={preferences.minimum_compensation ? String(preferences.minimum_compensation) : ""} onChange={(value) => updatePreferences("minimum_compensation", Number(value) || undefined)} placeholder="165000" inputMode="numeric" />
                <SelectField label="Sponsorship" value={preferences.sponsorship} onChange={(value) => updatePreferences("sponsorship", value)} options={[
                  ["ask", "Ask me before answering"],
                  ["not_required", "I do not require sponsorship"],
                  ["required", "I require sponsorship"],
                ]} />
              </div>
            </>
          )}

          {step === 4 && (
            <>
              <div className="setup-heading"><p>STEP 5 OF 6</p><h2>How should Bluey work?</h2><span>Start carefully. You can loosen review rules per Career Track later.</span></div>
              <div className="onboarding-kit-preview">
                <div><p>YOUR FIRST APPLICATION KIT</p><h3>{track.role || profile.headline || "Target role"}</h3><span>A separate resume version, exact final answers, application email, site status, pause checks, and one metering decision.</span></div>
                <ul><li><Check size={14} />Real before/after resume diff</li><li><Check size={14} />One candidate truth across every Track and email</li><li><Check size={14} />Review before any runner starts</li></ul>
              </div>
              <div className="choice-section">
                <label>Resume mode</label>
                <div className="segmented large">
                  <button className={profile.resume_mode === "factual" ? "active" : ""} onClick={() => update("resume_mode", "factual")}><b>Factual</b><span>Rewrite and emphasize only what your profile supports.</span></button>
                  <button className={profile.resume_mode === "enhance" ? "active" : ""} onClick={() => update("resume_mode", "enhance")}><b>Enhance</b><span>Stronger JD-aligned framing, with provenance retained.</span></button>
                </div>
              </div>
              <div className="choice-section">
                <label>Submission default</label>
                <div className="segmented large">
                  <button className={profile.default_submission_mode === "review_first" ? "active" : ""} onClick={() => update("default_submission_mode", "review_first")}><b>Review first · Recommended</b><span>Inspect your first five application kits and receipts before enabling automation.</span></button>
                  <button className={profile.default_submission_mode === "auto_submit" ? "active" : ""} onClick={() => update("default_submission_mode", "auto_submit")}><b>Auto-submit later</b><span>Available per Career Track only when server rules and site certification pass.</span></button>
                </div>
              </div>
              <div className="setting-line">
                <div><b>Review new claims</b><span>Pause when Bluey proposes a factual claim not already in your profile.</span></div>
                <Toggle checked={profile.review_new_claims} onChange={(checked) => update("review_new_claims", checked)} />
              </div>
              <div className="form-grid two roomy-top">
                <Field label="Applications per day" value={String(profile.daily_limit)} onChange={(value) => update("daily_limit", Math.max(1, Math.min(50, Number(value) || 10)))} inputMode="numeric" />
                <Field label="Auto-submit threshold" value={String(profile.auto_submit_threshold)} onChange={(value) => update("auto_submit_threshold", Math.max(60, Math.min(100, Number(value) || 80)))} inputMode="numeric" suffix="%" />
              </div>
            </>
          )}

          {step === 5 && (
            <>
              <div className="setup-heading"><p>STEP 6 OF 6</p><h2>Your first Career Track is ready</h2><span>Bluey will rank roles for this exact combination of role, location, and application rules.</span></div>
              <div className="track-ready">
                <span><Sparkles size={20} /></span>
                <div><p>CAREER TRACK AGENT</p><h3>{track.role || "Add a target role"}</h3><strong>{track.locations.join(" + ") || "Add a target location"}</strong></div>
                <ul>
                  <li><Check size={15} />{profile.resume_mode === "factual" ? "Factual" : "Enhanced"} resume per job</li>
                  <li><Check size={15} />{profile.default_submission_mode === "review_first" ? "Review before submit" : `${profile.auto_submit_threshold}% auto-submit threshold`}</li>
                  <li><Check size={15} />Up to {profile.daily_limit} applications each day</li>
                </ul>
              </div>
              <div className="setup-review">
                <div><span>PROFILE</span><b>{profile.full_name}</b><small>{profile.employment.length} roles · {profile.skills.length} skills</small></div>
                <div><span>LOCATION</span><b>{preferences.desired_locations.join(", ")}</b><small>{preferences.location_policy.replaceAll("_", " ")}</small></div>
                <div><span>STARTING PLAN</span><b>Free</b><small>5 reviewed applications each month</small></div>
              </div>
            </>
          )}

          {(validation || error) && <div className="inline-error">{validation || error}</div>}
          {step === steps.length - 1 && profile.employment.length === 0 && profile.education.length === 0 && validation && (
            <div className="setup-repair-actions">
              <button className="button secondary compact" disabled={saving} onClick={() => void moveToStep(1)}>Review experience</button>
              <button className="button secondary compact" disabled={saving} onClick={() => void moveToStep(2)}>Review education</button>
            </div>
          )}
          <div className="setup-actions">
            <button className="button ghost" disabled={step === 0 || saving} onClick={() => void moveToStep(step - 1)}><ArrowLeft size={16} />Back</button>
            {step < steps.length - 1 ? (
              <button className="button primary" disabled={saving} onClick={() => void next()}>{saving ? <LoaderCircle className="spin" size={16} /> : null}Continue<ArrowRight size={16} /></button>
            ) : (
              <button className="button primary" disabled={saving} onClick={() => void finish()}>{saving ? <LoaderCircle className="spin" size={17} /> : <Sparkles size={17} />}Find my matches</button>
            )}
          </div>
        </section>
      </div>
    </main>
  );
}

function Field({ label, value, onChange, placeholder, autoFocus, inputMode, suffix }: {
  label: string; value: string; onChange(value: string): void; placeholder?: string; autoFocus?: boolean; inputMode?: "numeric"; suffix?: string;
}) {
  return <label className="field"><span>{label}</span><div>{suffix && <i>{suffix}</i>}<input value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} autoFocus={autoFocus} inputMode={inputMode} /></div></label>;
}

function SelectField({ label, value, onChange, options }: { label: string; value: string; onChange(value: string): void; options: Array<[string, string]> }) {
  return <label className="field"><span>{label}</span><div><select value={value} onChange={(event) => onChange(event.target.value)}>{options.map(([optionValue, labelText]) => <option key={optionValue} value={optionValue}>{labelText}</option>)}</select></div></label>;
}

function TagField({ label, values, onChange, placeholder }: { label: string; values: string[]; onChange(values: string[]): void; placeholder: string }) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const next = draft.trim();
    if (!next || values.some((value) => value.toLowerCase() === next.toLowerCase())) return;
    onChange([...values, next]);
    setDraft("");
  };
  return (
    <label className="field tag-field"><span>{label}</span><div className="tag-input">
      {values.map((value) => <button key={value} onClick={() => onChange(values.filter((item) => item !== value))}>{value}<span>×</span></button>)}
      <input value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === ",") { event.preventDefault(); add(); } }} onBlur={add} placeholder={values.length ? "Add another" : placeholder} />
    </div></label>
  );
}

function EmploymentEditor({ entry, onChange, onRemove }: { entry: EmploymentEntry; onChange(entry: EmploymentEntry): void; onRemove(): void }) {
  return (
    <div className="entry-editor">
      <div className="entry-editor-title"><BriefcaseBusiness size={17} /><strong>{entry.title || "New role"}</strong><button title="Remove role" onClick={onRemove}><Trash2 size={15} /></button></div>
      <div className="form-grid two">
        <Field label="Company" value={entry.company} onChange={(value) => onChange({ ...entry, company: value })} />
        <Field label="Title" value={entry.title} onChange={(value) => onChange({ ...entry, title: value })} />
        <Field label="Location" value={entry.location} onChange={(value) => onChange({ ...entry, location: value })} />
        <div className="form-grid two dates"><Field label="Start" value={entry.start_date} onChange={(value) => onChange({ ...entry, start_date: value })} placeholder="2022-03" /><Field label="End" value={entry.end_date} onChange={(value) => onChange({ ...entry, end_date: value })} placeholder={entry.current ? "Present" : "2024-06"} /></div>
      </div>
      <label className="field"><span>Highlights (one per line)</span><textarea rows={3} value={entry.highlights.join("\n")} onChange={(event) => onChange({ ...entry, highlights: event.target.value.split("\n").filter(Boolean) })} /></label>
    </div>
  );
}

function EducationEditor({ entry, onChange, onRemove }: { entry: EducationEntry; onChange(entry: EducationEntry): void; onRemove(): void }) {
  return (
    <div className="entry-editor">
      <div className="entry-editor-title"><GraduationCap size={17} /><strong>{entry.school || "Education"}</strong><button title="Remove education" onClick={onRemove}><Trash2 size={15} /></button></div>
      <div className="form-grid two">
        <Field label="School" value={entry.school} onChange={(value) => onChange({ ...entry, school: value })} />
        <Field label="Degree" value={entry.degree} onChange={(value) => onChange({ ...entry, degree: value })} />
        <Field label="Field of study" value={entry.field} onChange={(value) => onChange({ ...entry, field: value })} />
        <Field label="Graduation" value={entry.end_date} onChange={(value) => onChange({ ...entry, end_date: value })} placeholder="2019" />
      </div>
    </div>
  );
}

function QuickCapture({ notes, setNotes, onUse }: { notes: string; setNotes(value: string): void; onUse(): void }) {
  const [listening, setListening] = useState(false);
  const startListening = () => {
    const SpeechRecognition = (window as typeof window & { webkitSpeechRecognition?: new () => SpeechRecognitionLike }).webkitSpeechRecognition;
    if (!SpeechRecognition) return;
    const recognition = new SpeechRecognition();
    recognition.continuous = false;
    recognition.interimResults = false;
    recognition.onresult = (event) => setNotes(`${notes} ${event.results[0][0].transcript}`.trim());
    recognition.onend = () => setListening(false);
    recognition.start();
    setListening(true);
  };
  return (
    <div className="quick-capture">
      <div><Sparkles size={17} /><span><b>Tell Bluey the messy version</b><small>Paste notes or speak. Bluey keeps them as draft profile context until you save.</small></span></div>
      <textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={3} placeholder="I led the launch, worked with design and sales, and adoption reached..." />
      <div><button className="button secondary compact" onClick={startListening}><Mic size={15} />{listening ? "Listening..." : "Speak"}</button><button className="button primary compact" disabled={!notes.trim()} onClick={onUse}>Use these notes</button></div>
    </div>
  );
}

interface SpeechRecognitionLike {
  continuous: boolean;
  interimResults: boolean;
  onresult: (event: { results: ArrayLike<{ [index: number]: { transcript: string } }> }) => void;
  onend: () => void;
  start(): void;
}

function Toggle({ checked, onChange }: { checked: boolean; onChange(checked: boolean): void }) {
  return <button type="button" className={`toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span /></button>;
}

function emptyEmployment(): EmploymentEntry {
  return { id: crypto.randomUUID(), company: "", title: "", location: "", start_date: "", end_date: "", current: false, highlights: [] };
}

function emptyEducation(): EducationEntry {
  return { id: crypto.randomUUID(), school: "", degree: "", field: "", start_date: "", end_date: "", location: "" };
}

function validateStep(step: number, profile: CareerProfile, preferences: JobPreferences): string {
  if (step === 0 && !profile.full_name.trim()) return "Add your full name to continue.";
  if (step === 0 && !profile.current_location.trim()) return "Add your current city and state so Bluey can answer location questions correctly.";
  if (step === 3 && preferences.desired_roles.length === 0) return "Add at least one target role.";
  if (step === 3 && preferences.desired_locations.length === 0) return "Add at least one target location.";
  return "";
}

function stepDescription(step: number): string {
  return ["Contact and resume", "Employers and outcomes", "School and skills", "Roles and locations", "Review and automation", "Launch first agent"][step];
}
