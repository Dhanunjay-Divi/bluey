import { useId, useMemo, useRef, useState } from "react";
import { BriefcaseBusiness, FolderKanban, GraduationCap, Trash2 } from "lucide-react";
import type { EducationEntry, EmploymentEntry, ProjectEntry } from "../types";
import { filterCareerSuggestions } from "../data/career-suggestions";

interface CareerFieldProps {
  label: string;
  value: string;
  onChange(value: string): void;
  placeholder?: string;
  autoFocus?: boolean;
  inputMode?: "numeric" | "email" | "tel" | "url";
  suffix?: string;
  suggestions?: string[];
  disabled?: boolean;
}

export function CareerField({ label, value, onChange, placeholder, autoFocus, inputMode, suffix, suggestions = [], disabled = false }: CareerFieldProps) {
  const listId = useId();
  const inputId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const options = useMemo(
    () => filterCareerSuggestions(value, suggestions, value.trim() ? [value] : []),
    [suggestions, value],
  );
  const choose = (option: string) => {
    onChange(option);
    setOpen(false);
    setActive(-1);
  };

  const type = inputMode === "email" || inputMode === "tel" || inputMode === "url" ? inputMode : "text";
  return <div className="field"><label htmlFor={inputId}>{label}</label><div className="typeahead-control">
    {suffix && <i>{suffix}</i>}
    <input
      value={value}
      id={inputId}
      type={type}
      onChange={(event) => { onChange(event.target.value); setOpen(true); setActive(-1); }}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
      onKeyDown={(event) => {
        if (!options.length) return;
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setOpen(true);
          setActive((current) => current >= options.length - 1 ? 0 : current + 1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          setOpen(true);
          setActive((current) => current <= 0 ? options.length - 1 : current - 1);
        } else if (event.key === "Enter" && open && active >= 0 && options[active]) {
          event.preventDefault();
          choose(options[active]);
        } else if (event.key === "Escape") {
          setOpen(false);
          setActive(-1);
        }
      }}
      placeholder={placeholder}
      autoFocus={autoFocus}
      inputMode={inputMode}
      disabled={disabled}
      autoComplete={suggestions.length ? "off" : undefined}
      role={suggestions.length ? "combobox" : undefined}
      aria-autocomplete={suggestions.length ? "list" : undefined}
      aria-expanded={suggestions.length ? open && options.length > 0 : undefined}
      aria-controls={suggestions.length ? listId : undefined}
      aria-activedescendant={open && active >= 0 ? `${listId}-${active}` : undefined}
    />
    <SuggestionList id={listId} open={open} options={options} active={active} onChoose={choose} onActive={setActive} />
  </div></div>;
}

export function CareerTagField({ label, values, onChange, placeholder, suggestions = [] }: {
  label: string;
  values: string[];
  onChange(values: string[]): void;
  placeholder: string;
  suggestions?: string[];
}) {
  const [draft, setDraft] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const listId = useId();
  const inputId = useId();
  const options = useMemo(
    () => filterCareerSuggestions(draft, suggestions, values),
    [draft, suggestions, values],
  );
  const add = () => {
    const next = draft.trim();
    if (!next || values.some((value) => value.toLocaleLowerCase() === next.toLocaleLowerCase())) return;
    onChange([...values, next]);
    setDraft("");
  };
  const choose = (option: string) => {
    if (!values.some((value) => value.toLocaleLowerCase() === option.toLocaleLowerCase())) onChange([...values, option]);
    setDraft("");
    setOpen(false);
    setActive(-1);
  };

  return <div className="field tag-field"><label htmlFor={inputId}>{label}</label><div className="tag-input typeahead-control">
    {values.map((value) => <button type="button" key={value} aria-label={`Remove ${value}`} onClick={() => onChange(values.filter((item) => item !== value))}>{value}<span>×</span></button>)}
    <input
      value={draft}
      id={inputId}
      onChange={(event) => { setDraft(event.target.value); setOpen(true); setActive(-1); }}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown" && options.length) {
          event.preventDefault();
          setOpen(true);
          setActive((current) => current >= options.length - 1 ? 0 : current + 1);
        } else if (event.key === "ArrowUp" && options.length) {
          event.preventDefault();
          setOpen(true);
          setActive((current) => current <= 0 ? options.length - 1 : current - 1);
        } else if (event.key === "Enter" && open && active >= 0 && options[active]) {
          event.preventDefault();
          choose(options[active]);
        } else if (event.key === "Enter" || event.key === ",") {
          event.preventDefault();
          add();
        } else if (event.key === "Escape") {
          setOpen(false);
          setActive(-1);
        }
      }}
      placeholder={values.length ? "Add another" : placeholder}
      autoComplete="off"
      role="combobox"
      aria-autocomplete="list"
      aria-expanded={open && options.length > 0}
      aria-controls={listId}
      aria-activedescendant={open && active >= 0 ? `${listId}-${active}` : undefined}
    />
    <SuggestionList id={listId} open={open} options={options} active={active} onChoose={choose} onActive={setActive} />
  </div></div>;
}

export function CareerEmploymentEditor({ entry, onChange, onRemove, companySuggestions, roleSuggestions, locationSuggestions }: {
  entry: EmploymentEntry;
  onChange(entry: EmploymentEntry): void;
  onRemove(): void;
  companySuggestions: string[];
  roleSuggestions: string[];
  locationSuggestions: string[];
}) {
  const previousEndDate = useRef(entry.end_date);
  if (!entry.current && entry.end_date) previousEndDate.current = entry.end_date;
  const toggleCurrent = () => {
    if (!entry.current && entry.end_date) previousEndDate.current = entry.end_date;
    onChange({
      ...entry,
      current: !entry.current,
      end_date: entry.current ? previousEndDate.current : "",
    });
  };
  const highlightsId = useId();
  return <div className="entry-editor">
    <div className="entry-editor-title"><BriefcaseBusiness size={17} /><strong>{entry.title || entry.company || "New role"}</strong><button type="button" title="Remove role" aria-label={`Remove ${entry.title || entry.company || "role"}`} onClick={onRemove}><Trash2 size={15} /></button></div>
    <div className="form-grid two">
      <CareerField label="Company" value={entry.company} onChange={(value) => onChange({ ...entry, company: value })} suggestions={companySuggestions} />
      <CareerField label="Title" value={entry.title} onChange={(value) => onChange({ ...entry, title: value })} suggestions={roleSuggestions} />
      <CareerField label="Location" value={entry.location} onChange={(value) => onChange({ ...entry, location: value })} suggestions={locationSuggestions} />
      <div className="field current-role-field"><span>Role status</span><button type="button" className={`choice-button ${entry.current ? "active" : ""}`} onClick={toggleCurrent}>{entry.current ? "Current role" : "Past role"}</button></div>
      <CareerField label="Start" value={entry.start_date} onChange={(value) => onChange({ ...entry, start_date: value })} placeholder="2022-03" />
      <CareerField label="End" value={entry.end_date} onChange={(value) => onChange({ ...entry, end_date: value })} placeholder={entry.current ? "Present" : "2024-06"} disabled={entry.current} />
    </div>
    <div className="field"><label htmlFor={highlightsId}>Highlights (one per line)</label><textarea id={highlightsId} rows={4} value={entry.highlights.join("\n")} onChange={(event) => onChange({ ...entry, highlights: event.target.value.split("\n").map((value) => value.trim()).filter(Boolean) })} /></div>
  </div>;
}

export function CareerEducationEditor({ entry, onChange, onRemove, locationSuggestions = [] }: {
  entry: EducationEntry;
  onChange(entry: EducationEntry): void;
  onRemove(): void;
  locationSuggestions?: string[];
}) {
  return <div className="entry-editor">
    <div className="entry-editor-title"><GraduationCap size={17} /><strong>{entry.school || "Education"}</strong><button type="button" title="Remove education" aria-label={`Remove ${entry.school || "education"}`} onClick={onRemove}><Trash2 size={15} /></button></div>
    <div className="form-grid two">
      <CareerField label="School" value={entry.school} onChange={(value) => onChange({ ...entry, school: value })} />
      <CareerField label="Degree" value={entry.degree} onChange={(value) => onChange({ ...entry, degree: value })} />
      <CareerField label="Field of study" value={entry.field} onChange={(value) => onChange({ ...entry, field: value })} />
      <CareerField label="Location" value={entry.location} onChange={(value) => onChange({ ...entry, location: value })} suggestions={locationSuggestions} />
      <CareerField label="Start" value={entry.start_date} onChange={(value) => onChange({ ...entry, start_date: value })} placeholder="2017" />
      <CareerField label="Graduation" value={entry.end_date} onChange={(value) => onChange({ ...entry, end_date: value })} placeholder="2021" />
    </div>
  </div>;
}

export function CareerProjectEditor({ entry, onChange, onRemove, skillSuggestions = [] }: {
  entry: ProjectEntry;
  onChange(entry: ProjectEntry): void;
  onRemove(): void;
  skillSuggestions?: string[];
}) {
  const summaryId = useId();
  return <div className="entry-editor">
    <div className="entry-editor-title"><FolderKanban size={17} /><strong>{entry.name || "Project"}</strong><button type="button" title="Remove project" aria-label={`Remove ${entry.name || "project"}`} onClick={onRemove}><Trash2 size={15} /></button></div>
    <div className="form-grid two">
      <CareerField label="Project name" value={entry.name} onChange={(value) => onChange({ ...entry, name: value })} />
      <CareerField label="Your role" value={entry.role} onChange={(value) => onChange({ ...entry, role: value })} />
      <CareerField label="Project URL" value={entry.url} onChange={(value) => onChange({ ...entry, url: value })} inputMode="url" placeholder="https://..." />
    </div>
    <div className="field"><label htmlFor={summaryId}>Summary</label><textarea id={summaryId} rows={3} value={entry.summary} onChange={(event) => onChange({ ...entry, summary: event.target.value })} /></div>
    <CareerTagField label="Technologies" values={entry.technologies} onChange={(values) => onChange({ ...entry, technologies: values })} placeholder="Add a technology" suggestions={skillSuggestions} />
  </div>;
}

function SuggestionList({ id, open, options, active, onChoose, onActive }: {
  id: string;
  open: boolean;
  options: string[];
  active: number;
  onChoose(option: string): void;
  onActive(index: number): void;
}) {
  if (!open || options.length === 0) return null;
  return <div id={id} className="suggestion-list" role="listbox">
    {options.map((option, index) => <div
      id={`${id}-${index}`}
      key={option}
      role="option"
      aria-selected={active === index}
      className={active === index ? "active" : ""}
      onMouseEnter={() => onActive(index)}
      onMouseDown={(event) => { event.preventDefault(); onChoose(option); }}
    >{option}</div>)}
  </div>;
}

export function emptyCareerEmployment(): EmploymentEntry {
  return { id: crypto.randomUUID(), company: "", title: "", location: "", start_date: "", end_date: "", current: false, highlights: [] };
}

export function emptyCareerEducation(): EducationEntry {
  return { id: crypto.randomUUID(), school: "", degree: "", field: "", start_date: "", end_date: "", location: "" };
}

export function emptyCareerProject(): ProjectEntry {
  return { id: crypto.randomUUID(), name: "", role: "", summary: "", technologies: [], url: "" };
}
