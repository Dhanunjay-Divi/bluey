// A compact model picker for the header row — lets the user pin the attached
// agent to a specific model (or leave it on "auto" = the agent decides). The
// selection flows back through the existing attach(kind, sessionId?, model?)
// path; "auto" maps to model=undefined (no override).
//
// Native <select> (the overlay UI imports no Radix) styled with the glass
// tokens so it reads as one of the header controls.

import type { CSSProperties } from "react";

const AUTO_LABEL = "Default (agent decides)";

/** Human label for an option — the sentinel "auto" gets a friendly name; every
 *  other model id shows verbatim. */
function optionLabel(model: string): string {
  return model === "auto" ? AUTO_LABEL : model;
}

export function ModelPicker({
  models,
  value,
  onChange,
}: {
  models: string[];
  value: string;
  onChange: (model: string) => void;
}) {
  return (
    <select
      aria-label="Model"
      title="Model"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={pickerStyle}
    >
      {models.map((m) => (
        <option key={m} value={m}>
          {optionLabel(m)}
        </option>
      ))}
    </select>
  );
}

const pickerStyle: CSSProperties = {
  fontSize: 11.5,
  fontWeight: 500,
  color: "var(--ink-2)",
  background: "var(--glass-2)",
  border: "1px solid var(--line)",
  borderRadius: "var(--r-pill)",
  padding: "4px 9px",
  maxWidth: 160,
  cursor: "pointer",
  appearance: "none",
  WebkitAppearance: "none",
};
