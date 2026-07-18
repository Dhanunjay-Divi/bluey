import { useEffect, useMemo, useRef, useState } from "react";
import {
  Check,
  ChevronRight,
  Download,
  FileCheck2,
  FileDiff,
  FileUp,
  History,
  Pencil,
  Plus,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import type { CareerProfile, JobsWorkspace, ResumeContent, ResumeVersion } from "../types";
import {
  applyResumeImport,
  exportResumeDocx,
  exportResumePdf,
  importResume,
  prepareResumeImport,
  type ResumeImportMode,
  type ResumeImportPreview,
} from "../lib/documents";
import { Dialog } from "../components/Dialog";
import {
  CareerEducationEditor,
  CareerEmploymentEditor,
  CareerField,
  CareerProjectEditor,
  CareerTagField,
  emptyCareerEducation,
  emptyCareerEmployment,
  emptyCareerProject,
} from "../components/CareerFields";
import { ResumeImportReview } from "../components/ResumeImportReview";
import {
  CERTIFICATION_SUGGESTIONS,
  mergeCareerSuggestions,
  ROLE_SUGGESTIONS,
  SKILL_SUGGESTIONS,
} from "../data/career-suggestions";
import { useLocationSuggestions } from "../data/use-location-suggestions";
import { relativeTime } from "../lib/format";
import { validateCareerProfile } from "../lib/profile-validation";

interface Props {
  workspace: JobsWorkspace;
  resumeVersions: Record<string, ResumeVersion>;
  onSave(profile: CareerProfile): Promise<void>;
  onCommit(application: JobsWorkspace["applications"][number]): Promise<void>;
  onLoadResume(id: string): Promise<ResumeVersion | undefined>;
}

export function ResumeView({ workspace, resumeVersions, onSave, onCommit, onLoadResume }: Props) {
  const [profile, setProfile] = useState(workspace.profile);
  const [editOpen, setEditOpen] = useState(false);
  const [selectedResume, setSelectedResume] = useState<ResumeVersion | undefined>();
  const [saving, setSaving] = useState(false);
  const [importPreview, setImportPreview] = useState<ResumeImportPreview>();
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);
  const applicationsWithResume = workspace.applications.filter((item) => item.resume_version_id);
  const selectedApplication = selectedResume
    ? workspace.applications.find((item) => item.resume_version_id === selectedResume.id)
    : undefined;
  const roleSuggestions = useMemo(
    () => mergeCareerSuggestions([profile.headline], profile.employment.map((entry) => entry.title), ROLE_SUGGESTIONS),
    [profile.employment, profile.headline],
  );
  const locationSeeds = useMemo(
    () => mergeCareerSuggestions(
      [profile.current_location],
      profile.employment.map((entry) => entry.location),
      profile.education.map((entry) => entry.location),
    ),
    [profile.current_location, profile.education, profile.employment],
  );
  const locationSuggestions = useLocationSuggestions(locationSeeds);
  const companySuggestions = useMemo(
    () => mergeCareerSuggestions(profile.employment.map((entry) => entry.company)),
    [profile.employment],
  );
  const skillSuggestions = useMemo(
    () => mergeCareerSuggestions(profile.skills, profile.projects.flatMap((entry) => entry.technologies), SKILL_SUGGESTIONS),
    [profile.projects, profile.skills],
  );

  const baseContent = useMemo<ResumeContent>(() => ({
    contact: {
      name: profile.full_name,
      email: profile.email,
      phone: profile.phone,
      location: profile.current_location,
    },
    headline: profile.headline,
    summary: profile.summary,
    skills: profile.skills,
    employment: profile.employment,
    education: profile.education,
    projects: profile.projects,
    certifications: profile.certifications,
  }), [profile]);

  useEffect(() => {
    if (!editOpen) setProfile(workspace.profile);
  }, [editOpen, workspace.profile]);

  const upload = async (file?: File) => {
    if (!file) return;
    setMessage("");
    setError("");
    try {
      const imported = await importResume(file);
      setImportPreview(prepareResumeImport(profile, imported));
    } catch (requestError) {
      setError(resumeErrorMessage(requestError, "Resume import failed."));
    }
  };

  const applyImport = (mode: ResumeImportMode) => {
    if (!importPreview) return;
    setProfile(applyResumeImport(importPreview, mode));
    setImportPreview(undefined);
    setEditOpen(true);
    setMessage("Import staged. Review the extracted Career Profile, then save or cancel.");
  };

  const cancelEdit = () => {
    setProfile(workspace.profile);
    setEditOpen(false);
    setMessage("");
  };

  const save = async () => {
    const validationError = validateCareerProfile(profile);
    if (validationError) {
      setError(validationError);
      return;
    }
    setSaving(true);
    setError("");
    try {
      await onSave(profile);
      setEditOpen(false);
      setMessage("Career Profile saved.");
    } catch (requestError) {
      setError(resumeErrorMessage(requestError, "Career Profile could not be saved."));
    } finally {
      setSaving(false);
    }
  };

  const openVersion = async (resumeId?: string) => {
    if (!resumeId) return;
    setError("");
    try {
      const version = resumeVersions[resumeId] || (await onLoadResume(resumeId));
      if (!version) throw new Error("That resume version is no longer available.");
      setSelectedResume(version);
    } catch (requestError) {
      setError(resumeErrorMessage(requestError, "Resume version could not be loaded."));
    }
  };

  const exportSelected = async (format: "pdf" | "docx") => {
    if (!selectedResume || !selectedApplication) return;
    setError("");
    try {
      await onCommit(selectedApplication);
      if (format === "pdf") await exportResumePdf(selectedResume.content, "bluey-tailored-resume");
      else await exportResumeDocx(selectedResume.content, "bluey-tailored-resume");
    } catch (requestError) {
      setError(resumeErrorMessage(requestError, "Resume export could not be completed."));
    }
  };

  const updateResumeMode = async (mode: CareerProfile["resume_mode"]) => {
    const previous = profile;
    const next = { ...profile, resume_mode: mode };
    setProfile(next);
    setError("");
    try {
      await onSave(next);
    } catch (requestError) {
      setProfile(previous);
      setError(resumeErrorMessage(requestError, "Default resume mode could not be saved."));
    }
  };

  const exportBaseResume = async () => {
    setError("");
    try {
      await exportResumePdf(baseContent, "bluey-career-profile");
    } catch (requestError) {
      setError(resumeErrorMessage(requestError, "Career Profile export could not be completed."));
    }
  };

  return (
    <div className="view-shell resume-view">
      <section className="view-heading">
        <div><p className="eyebrow">SOURCE OF TRUTH</p><h1>Resume</h1><span>One verified Career Profile, then a different resume version for every job.</span></div>
        <div className="heading-actions"><input ref={fileRef} hidden type="file" accept=".pdf,.docx,.txt" onChange={(event) => { void upload(event.target.files?.[0]); event.target.value = ""; }} /><button className="button secondary" onClick={() => fileRef.current?.click()}><FileUp size={16} />Import resume</button><button className="button primary" onClick={() => { setProfile(workspace.profile); setEditOpen(true); }}><Pencil size={16} />Edit profile</button></div>
      </section>

      {message && <div className="global-message success"><Check size={16} />{message}</div>}
      {error && <div className="global-message error" role="alert">{error}</div>}

      <section className="profile-band">
        <div className="profile-identity"><div>{profile.full_name.split(" ").map((part) => part[0]).join("").slice(0, 2)}</div><span><h2>{profile.full_name}</h2><p>{profile.headline}</p><small>{profile.current_location} · {profile.email}</small></span></div>
        <div className="profile-health"><span><b>{profile.employment.length}</b><small>roles</small></span><span><b>{profile.skills.length}</b><small>skills</small></span><span><b>{workspace.facts.filter((fact) => fact.verification_status === "confirmed").length}</b><small>confirmed facts</small></span></div>
        <div className="mode-control"><label>DEFAULT MODE</label><div className="segmented"><button className={profile.resume_mode === "factual" ? "active" : ""} onClick={() => void updateResumeMode("factual")}>Factual</button><button className={profile.resume_mode === "enhance" ? "active" : ""} onClick={() => void updateResumeMode("enhance")}>Enhance</button></div></div>
      </section>

      <div className="resume-layout">
        <section className="resume-sheet">
          <div className="section-heading compact"><div><p>BASE PROFILE</p><h2>{profile.source_resume_name || "Bluey Career Profile"}</h2></div><div><button className="icon-button" title="Download PDF" onClick={() => void exportBaseResume()}><Download size={16} /></button></div></div>
          <BaseResume content={baseContent} />
        </section>

        <aside className="resume-sidebar">
          <section>
            <div className="section-heading compact"><div><p>PROVENANCE</p><h2>Career facts</h2></div><ShieldCheck size={19} /></div>
            <div className="fact-list">
              {workspace.facts.map((fact) => <div key={fact.id}><span className={`fact-status ${fact.verification_status}`}>{fact.verification_status === "confirmed" ? <Check size={12} /> : <Sparkles size={12} />}</span><div><b>{fact.label}</b><p>{String(fact.value)}</p><small>{fact.source.replaceAll("_", " ")}</small></div></div>)}
              {workspace.facts.length === 0 && <p className="muted-copy">Imported and confirmed facts appear here with their source.</p>}
            </div>
          </section>
          <section>
            <div className="section-heading compact"><div><p>JOB-SPECIFIC</p><h2>Version history</h2></div><History size={19} /></div>
            <div className="version-list">
              {applicationsWithResume.map((application) => {
                const job = workspace.matches.find((item) => item.id === application.job_id);
                return <button key={application.id} onClick={() => void openVersion(application.resume_version_id)}><FileCheck2 size={17} /><span><b>{job?.title || "Tailored resume"}</b><small>{job?.company} · {relativeTime(application.updated_at_ms)}</small></span><ChevronRight size={16} /></button>;
              })}
              {applicationsWithResume.length === 0 && <p className="muted-copy">Prepare your first application from Matches to create a tailored version.</p>}
            </div>
          </section>
        </aside>
      </div>

      <ResumeImportReview preview={importPreview} onApply={applyImport} onClose={() => setImportPreview(undefined)} />

      <Dialog open={editOpen} title="Edit Career Profile" description="These facts become reusable source material for job-specific resumes." onClose={cancelEdit} size="large">
        <div className="dialog-form profile-edit-form full-profile-editor">
          <section className="profile-editor-section">
            <div className="profile-editor-heading"><span>IDENTITY</span><h3>Contact and professional summary</h3></div>
            <div className="form-grid two">
              <CareerField label="Full name" value={profile.full_name} onChange={(value) => setProfile({ ...profile, full_name: value })} autoFocus />
              <div className="profile-email-field">
                <CareerField label="Resume contact email" value={profile.email} onChange={(value) => setProfile({ ...profile, email: value })} inputMode="email" />
                <p className="field-note profile-email-note">This address appears on generated resumes. Bluey submits with a separately verified application email from Settings.</p>
              </div>
              <CareerField label="Phone" value={profile.phone} onChange={(value) => setProfile({ ...profile, phone: value })} inputMode="tel" />
              <CareerField label="Current location" value={profile.current_location} onChange={(value) => setProfile({ ...profile, current_location: value })} suggestions={locationSuggestions} />
              <CareerField label="Professional headline" value={profile.headline} onChange={(value) => setProfile({ ...profile, headline: value })} suggestions={roleSuggestions} />
              <CareerField label="Street address" value={profile.street_address} onChange={(value) => setProfile({ ...profile, street_address: value })} />
              <CareerField label="LinkedIn" value={profile.linkedin_url} onChange={(value) => setProfile({ ...profile, linkedin_url: value })} inputMode="url" placeholder="https://linkedin.com/in/..." />
              <CareerField label="Portfolio" value={profile.portfolio_url} onChange={(value) => setProfile({ ...profile, portfolio_url: value })} inputMode="url" placeholder="https://..." />
            </div>
            <label className="field"><span>Professional summary</span><textarea rows={5} value={profile.summary} onChange={(event) => setProfile({ ...profile, summary: event.target.value })} /></label>
          </section>

          <section className="profile-editor-section">
            <div className="profile-editor-heading"><span>EXPERIENCE</span><h3>Employment history</h3><button type="button" className="button secondary compact" onClick={() => setProfile({ ...profile, employment: [...profile.employment, emptyCareerEmployment()] })}><Plus size={15} />Add role</button></div>
            <div className="entry-list">
              {profile.employment.map((entry, index) => <CareerEmploymentEditor
                key={entry.id}
                entry={entry}
                companySuggestions={companySuggestions}
                roleSuggestions={roleSuggestions}
                locationSuggestions={locationSuggestions}
                onChange={(next) => setProfile({ ...profile, employment: profile.employment.map((item, itemIndex) => itemIndex === index ? next : item) })}
                onRemove={() => setProfile({ ...profile, employment: profile.employment.filter((_, itemIndex) => itemIndex !== index) })}
              />)}
              {profile.employment.length === 0 && <p className="editor-empty">No roles yet. Add the jobs you want Bluey to use as factual source material.</p>}
            </div>
          </section>

          <section className="profile-editor-section">
            <div className="profile-editor-heading"><span>EDUCATION</span><h3>Schools and degrees</h3><button type="button" className="button secondary compact" onClick={() => setProfile({ ...profile, education: [...profile.education, emptyCareerEducation()] })}><Plus size={15} />Add education</button></div>
            <div className="entry-list">
              {profile.education.map((entry, index) => <CareerEducationEditor
                key={entry.id}
                entry={entry}
                locationSuggestions={locationSuggestions}
                onChange={(next) => setProfile({ ...profile, education: profile.education.map((item, itemIndex) => itemIndex === index ? next : item) })}
                onRemove={() => setProfile({ ...profile, education: profile.education.filter((_, itemIndex) => itemIndex !== index) })}
              />)}
              {profile.education.length === 0 && <p className="editor-empty">No education entries yet.</p>}
            </div>
          </section>

          <section className="profile-editor-section">
            <div className="profile-editor-heading"><span>PROJECTS</span><h3>Projects and portfolio work</h3><button type="button" className="button secondary compact" onClick={() => setProfile({ ...profile, projects: [...profile.projects, emptyCareerProject()] })}><Plus size={15} />Add project</button></div>
            <div className="entry-list">
              {profile.projects.map((entry, index) => <CareerProjectEditor
                key={entry.id}
                entry={entry}
                skillSuggestions={skillSuggestions}
                onChange={(next) => setProfile({ ...profile, projects: profile.projects.map((item, itemIndex) => itemIndex === index ? next : item) })}
                onRemove={() => setProfile({ ...profile, projects: profile.projects.filter((_, itemIndex) => itemIndex !== index) })}
              />)}
              {profile.projects.length === 0 && <p className="editor-empty">No projects yet.</p>}
            </div>
          </section>

          <section className="profile-editor-section">
            <div className="profile-editor-heading"><span>QUALIFICATIONS</span><h3>Skills, certifications, and application facts</h3></div>
            <div className="form-grid two qualification-grid">
              <CareerTagField variant="skills" label="Skills" values={profile.skills} onChange={(values) => setProfile({ ...profile, skills: values })} placeholder="Add a skill" suggestions={skillSuggestions} />
              <CareerTagField variant="certifications" label="Certifications" values={profile.certifications} onChange={(values) => setProfile({ ...profile, certifications: values })} placeholder="Add a certification" suggestions={mergeCareerSuggestions(profile.certifications, CERTIFICATION_SUGGESTIONS)} />
              <CareerField label="Work authorization" value={profile.work_authorization} onChange={(value) => setProfile({ ...profile, work_authorization: value })} />
              <CareerField label="Salary expectation" value={profile.salary_expectation} onChange={(value) => setProfile({ ...profile, salary_expectation: value })} />
              <CareerField label="Notice period" value={profile.notice_period} onChange={(value) => setProfile({ ...profile, notice_period: value })} />
              <label className="field"><span>Sponsorship</span><select value={profile.sponsorship_required === null ? "unknown" : profile.sponsorship_required ? "required" : "not_required"} onChange={(event) => setProfile({ ...profile, sponsorship_required: event.target.value === "unknown" ? null : event.target.value === "required" })}><option value="unknown">Ask before answering</option><option value="not_required">Not required</option><option value="required">Required</option></select></label>
            </div>
          </section>
        </div>
        <div className="dialog-actions"><button className="button secondary" onClick={cancelEdit}>Cancel</button><button className="button primary" disabled={saving} onClick={() => void save()}>{saving ? "Saving..." : "Save profile"}</button></div>
      </Dialog>

      <Dialog open={Boolean(selectedResume)} title={selectedResume?.content.target ? `${selectedResume.content.target.title} at ${selectedResume.content.target.company}` : "Tailored resume"} description={`Version ${selectedResume?.version_no || 1} · ${selectedResume?.mode || "factual"}`} onClose={() => setSelectedResume(undefined)} size="large">
        {selectedResume && <div className="resume-version-dialog"><section className="resume-sheet compact-sheet"><BaseResume content={selectedResume.content} /></section><aside><div className="section-heading compact"><div><p>VISIBLE DIFF</p><h2>Why this version changed</h2></div><FileDiff /></div>{Object.entries(selectedResume.diff).map(([key, value]) => <div className="diff-item" key={key}><b>{key.replaceAll("_", " ")}</b><p>{Array.isArray(value) ? value.join(", ") || "None" : String(value)}</p></div>)}<div className="download-row"><button disabled={!selectedApplication} onClick={() => void exportSelected("pdf")}><Download size={15} />PDF</button><button disabled={!selectedApplication} onClick={() => void exportSelected("docx")}><Download size={15} />DOCX</button></div></aside></div>}
      </Dialog>
    </div>
  );
}

function resumeErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.trim() ? error.message : fallback;
}

function BaseResume({ content }: { content: ResumeContent }) {
  return <div className="resume-paper"><header><h2>{content.contact?.name}</h2><p>{[content.contact?.email, content.contact?.phone, content.contact?.location].filter(Boolean).join(" · ")}</p></header><h3>{content.headline}</h3><p>{content.summary}</p><h4>SKILLS</h4><p className="skill-line">{content.skills?.join(" · ")}</p><h4>EXPERIENCE</h4>{content.employment?.map((role) => <div className="resume-role" key={role.id}><div><b>{role.title}</b><span>{role.company}</span></div><small>{role.start_date} - {role.current ? "Present" : role.end_date}</small>{role.highlights.map((highlight) => <p key={highlight}>• {highlight}</p>)}</div>)}{content.education && content.education.length > 0 && <><h4>EDUCATION</h4>{content.education.map((entry) => <div className="resume-role" key={entry.id}><div><b>{entry.degree} {entry.field}</b><span>{entry.school}</span></div><small>{entry.end_date}</small></div>)}</>}</div>;
}
