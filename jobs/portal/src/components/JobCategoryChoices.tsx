export const EMPLOYMENT_TYPE_OPTIONS = [
  { value: "full_time", label: "Full-time" },
  { value: "part_time", label: "Part-time" },
  { value: "contract", label: "Contract" },
  { value: "temporary", label: "Temporary" },
  { value: "internship", label: "Internship" },
  { value: "apprenticeship", label: "Apprenticeship" },
  { value: "seasonal", label: "Seasonal" },
  { value: "per_diem", label: "Per diem" },
] as const;

export const ENGAGEMENT_TYPE_OPTIONS = [
  { value: "w2", label: "W-2" },
  { value: "c2c", label: "C2C" },
  { value: "1099", label: "1099" },
  { value: "direct_hire", label: "Direct hire" },
] as const;

interface JobCategoryChoicesProps {
  label: string;
  description: string;
  values: string[];
  options: ReadonlyArray<{ value: string; label: string }>;
  onChange(values: string[]): void;
}

export function JobCategoryChoices({
  label,
  description,
  values,
  options,
  onChange,
}: JobCategoryChoicesProps) {
  const selected = new Set(values);
  const toggle = (value: string) => {
    onChange(selected.has(value) ? values.filter((item) => item !== value) : [...values, value]);
  };

  return (
    <fieldset className="job-category-choices">
      <legend>{label}</legend>
      <p>{description}</p>
      <div>
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            className={selected.has(option.value) ? "selected" : ""}
            role="checkbox"
            aria-checked={selected.has(option.value)}
            onClick={() => toggle(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </fieldset>
  );
}
