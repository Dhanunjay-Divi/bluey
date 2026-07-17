import { AlertTriangle, BriefcaseBusiness, Check, FileSearch, GraduationCap } from "lucide-react";
import type { ResumeImportMode, ResumeImportPreview } from "../lib/documents";
import { Dialog } from "./Dialog";

interface Props {
  preview?: ResumeImportPreview;
  onApply(mode: ResumeImportMode): void;
  onClose(): void;
}

export function ResumeImportReview({ preview, onApply, onClose }: Props) {
  const profile = preview?.replacement;
  return (
    <Dialog
      open={Boolean(preview)}
      title="Review imported resume"
      description="Bluey has not changed your Career Profile yet. Check the extracted person and history first."
      onClose={onClose}
      size="large"
    >
      {preview && profile && (
        <>
          <div className="resume-import-review">
            {preview.likely_different_person && (
              <div className="import-person-warning" role="alert">
                <AlertTriangle size={18} />
                <div>
                  <b>This appears to be a different person</b>
                  <p>
                    This file has a different name. Replacing clears the existing candidate-specific application facts so two people cannot be mixed.
                  </p>
                </div>
              </div>
            )}

            <section className="import-person-summary">
              <FileSearch size={20} />
              <div>
                <p className="eyebrow">EXTRACTED FROM {preview.imported.name}</p>
                <h3>{profile.full_name || "Name not found"}</h3>
                <span>{[profile.headline, profile.current_location, profile.email].filter(Boolean).join(" · ") || "Review the source file and add missing contact details."}</span>
              </div>
            </section>

            <div className="import-review-grid">
              <section>
                <div className="import-review-heading"><BriefcaseBusiness size={16} /><b>Experience</b><span>{profile.employment.length}</span></div>
                {profile.employment.slice(0, 6).map((entry) => (
                  <div className="import-review-row" key={entry.id}>
                    <b>{entry.title || "Title not found"}</b>
                    <span>{[entry.company, entry.location].filter(Boolean).join(" · ")}</span>
                  </div>
                ))}
                {profile.employment.length === 0 && <p className="muted-copy">No work history was found.</p>}
              </section>
              <section>
                <div className="import-review-heading"><GraduationCap size={16} /><b>Education</b><span>{profile.education.length}</span></div>
                {profile.education.slice(0, 4).map((entry) => (
                  <div className="import-review-row" key={entry.id}>
                    <b>{entry.degree || entry.field || "Degree not found"}</b>
                    <span>{entry.school || "School not found"}</span>
                  </div>
                ))}
                {profile.education.length === 0 && <p className="muted-copy">No education history was found.</p>}
              </section>
            </div>

            <div className="import-counts" aria-label="Imported resume summary">
              <span><b>{preview.summary.skills}</b> skills</span>
              <span><b>{preview.summary.certifications}</b> certifications</span>
              <span><b>{preview.summary.projects}</b> projects</span>
              <span><Check size={13} /> {preview.changed_sections.join(", ")}</span>
            </div>

            <p className="import-review-note">
              {preview.likely_different_person
                ? "Replacing starts a clean candidate baseline while keeping account-level automation preferences. Review legal and application answers again before applying."
                : "Replacing keeps your work authorization, application answers, salary settings, submission preferences, and Career Track settings."}
            </p>
          </div>
          <div className="dialog-actions spread import-review-actions">
            <p>{preview.likely_different_person ? "Confirm this is the correct candidate before replacing the current profile." : "Choose Replace for a corrected resume. Use Fill blanks to retain confirmed fields while adding new history."}</p>
            <div>
              <button type="button" className="button secondary" onClick={onClose}>Cancel</button>
              {!preview.likely_different_person && <button type="button" className="button secondary" onClick={() => onApply("merge")}>Fill blanks only</button>}
              <button type="button" className="button primary" onClick={() => onApply("replace")}>Replace resume facts</button>
            </div>
          </div>
        </>
      )}
    </Dialog>
  );
}
