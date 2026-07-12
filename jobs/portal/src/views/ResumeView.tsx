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
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import type { CareerProfile, JobsWorkspace, ResumeContent, ResumeVersion } from "../types";
import { importResume, inferProfileFromResume, exportResumeDocx, exportResumePdf } from "../lib/documents";
import { Dialog } from "../components/Dialog";
import { relativeTime } from "../lib/format";

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
  const [message, setMessage] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);
  const applicationsWithResume = workspace.applications.filter((item) => item.resume_version_id);
  const selectedApplication = selectedResume
    ? workspace.applications.find((item) => item.resume_version_id === selectedResume.id)
    : undefined;

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

  useEffect(() => setProfile(workspace.profile), [workspace.profile]);

  const upload = async (file?: File) => {
    if (!file) return;
    setMessage("");
    try {
      const imported = await importResume(file);
      const next = inferProfileFromResume(profile, imported);
      setProfile(next);
      await onSave(next);
      setMessage("Resume imported. Review the profile facts Bluey extracted before your next application.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Resume import failed.");
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      await onSave(profile);
      setEditOpen(false);
    } finally {
      setSaving(false);
    }
  };

  const openVersion = async (resumeId?: string) => {
    if (!resumeId) return;
    const version = resumeVersions[resumeId] || (await onLoadResume(resumeId));
    setSelectedResume(version);
  };

  const exportSelected = async (format: "pdf" | "docx") => {
    if (!selectedResume || !selectedApplication) return;
    await onCommit(selectedApplication);
    if (format === "pdf") await exportResumePdf(selectedResume.content, "bluey-tailored-resume");
    else await exportResumeDocx(selectedResume.content, "bluey-tailored-resume");
  };

  return (
    <div className="view-shell resume-view">
      <section className="view-heading">
        <div><p className="eyebrow">SOURCE OF TRUTH</p><h1>Resume</h1><span>One verified Career Profile, then a different resume version for every job.</span></div>
        <div className="heading-actions"><input ref={fileRef} hidden type="file" accept=".pdf,.docx,.txt" onChange={(event) => void upload(event.target.files?.[0])} /><button className="button secondary" onClick={() => fileRef.current?.click()}><FileUp size={16} />Import resume</button><button className="button primary" onClick={() => setEditOpen(true)}><Pencil size={16} />Edit profile</button></div>
      </section>

      {message && <div className="global-message success"><Check size={16} />{message}</div>}

      <section className="profile-band">
        <div className="profile-identity"><div>{profile.full_name.split(" ").map((part) => part[0]).join("").slice(0, 2)}</div><span><h2>{profile.full_name}</h2><p>{profile.headline}</p><small>{profile.current_location} · {profile.email}</small></span></div>
        <div className="profile-health"><span><b>{profile.employment.length}</b><small>roles</small></span><span><b>{profile.skills.length}</b><small>skills</small></span><span><b>{workspace.facts.filter((fact) => fact.verification_status === "confirmed").length}</b><small>confirmed facts</small></span></div>
        <div className="mode-control"><label>DEFAULT MODE</label><div className="segmented"><button className={profile.resume_mode === "factual" ? "active" : ""} onClick={() => { const next = { ...profile, resume_mode: "factual" as const }; setProfile(next); void onSave(next); }}>Factual</button><button className={profile.resume_mode === "enhance" ? "active" : ""} onClick={() => { const next = { ...profile, resume_mode: "enhance" as const }; setProfile(next); void onSave(next); }}>Enhance</button></div></div>
      </section>

      <div className="resume-layout">
        <section className="resume-sheet">
          <div className="section-heading compact"><div><p>BASE PROFILE</p><h2>{profile.source_resume_name || "Bluey Career Profile"}</h2></div><div><button className="icon-button" title="Download PDF" onClick={() => void exportResumePdf(baseContent, "bluey-career-profile")}><Download size={16} /></button></div></div>
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

      <Dialog open={editOpen} title="Edit Career Profile" description="These facts become reusable source material for job-specific resumes." onClose={() => setEditOpen(false)} size="large">
        <div className="dialog-form profile-edit-form"><div className="form-grid two"><label><span>Full name</span><input value={profile.full_name} onChange={(event) => setProfile({ ...profile, full_name: event.target.value })} /></label><label><span>Headline</span><input value={profile.headline} onChange={(event) => setProfile({ ...profile, headline: event.target.value })} /></label><label><span>Current location</span><input value={profile.current_location} onChange={(event) => setProfile({ ...profile, current_location: event.target.value })} /></label><label><span>Phone</span><input value={profile.phone} onChange={(event) => setProfile({ ...profile, phone: event.target.value })} /></label></div><label><span>Professional summary</span><textarea rows={5} value={profile.summary} onChange={(event) => setProfile({ ...profile, summary: event.target.value })} /></label><label><span>Skills (comma separated)</span><input value={profile.skills.join(", ")} onChange={(event) => setProfile({ ...profile, skills: event.target.value.split(",").map((item) => item.trim()).filter(Boolean) })} /></label><div className="form-grid two"><label><span>Work authorization</span><input value={profile.work_authorization} onChange={(event) => setProfile({ ...profile, work_authorization: event.target.value })} /></label><label><span>Salary expectation</span><input value={profile.salary_expectation} onChange={(event) => setProfile({ ...profile, salary_expectation: event.target.value })} /></label></div></div>
        <div className="dialog-actions"><button className="button secondary" onClick={() => setEditOpen(false)}>Cancel</button><button className="button primary" disabled={saving} onClick={() => void save()}>{saving ? "Saving..." : "Save profile"}</button></div>
      </Dialog>

      <Dialog open={Boolean(selectedResume)} title={selectedResume?.content.target ? `${selectedResume.content.target.title} at ${selectedResume.content.target.company}` : "Tailored resume"} description={`Version ${selectedResume?.version_no || 1} · ${selectedResume?.mode || "factual"}`} onClose={() => setSelectedResume(undefined)} size="large">
        {selectedResume && <div className="resume-version-dialog"><section className="resume-sheet compact-sheet"><BaseResume content={selectedResume.content} /></section><aside><div className="section-heading compact"><div><p>VISIBLE DIFF</p><h2>Why this version changed</h2></div><FileDiff /></div>{Object.entries(selectedResume.diff).map(([key, value]) => <div className="diff-item" key={key}><b>{key.replaceAll("_", " ")}</b><p>{Array.isArray(value) ? value.join(", ") || "None" : String(value)}</p></div>)}<div className="download-row"><button disabled={!selectedApplication} onClick={() => void exportSelected("pdf")}><Download size={15} />PDF</button><button disabled={!selectedApplication} onClick={() => void exportSelected("docx")}><Download size={15} />DOCX</button></div></aside></div>}
      </Dialog>
    </div>
  );
}

function BaseResume({ content }: { content: ResumeContent }) {
  return <div className="resume-paper"><header><h2>{content.contact?.name}</h2><p>{[content.contact?.email, content.contact?.phone, content.contact?.location].filter(Boolean).join(" · ")}</p></header><h3>{content.headline}</h3><p>{content.summary}</p><h4>SKILLS</h4><p className="skill-line">{content.skills?.join(" · ")}</p><h4>EXPERIENCE</h4>{content.employment?.map((role) => <div className="resume-role" key={role.id}><div><b>{role.title}</b><span>{role.company}</span></div><small>{role.start_date} - {role.current ? "Present" : role.end_date}</small>{role.highlights.map((highlight) => <p key={highlight}>• {highlight}</p>)}</div>)}{content.education && content.education.length > 0 && <><h4>EDUCATION</h4>{content.education.map((entry) => <div className="resume-role" key={entry.id}><div><b>{entry.degree} {entry.field}</b><span>{entry.school}</span></div><small>{entry.end_date}</small></div>)}</>}</div>;
}
