export function resumeDiffHasValue(value: unknown): boolean {
  if (Array.isArray(value)) return value.length > 0;
  if (value && typeof value === "object") return Object.keys(value).length > 0;
  return value !== undefined && value !== null && String(value).trim().length > 0;
}

export function formatResumeDiffValue(value: unknown): string {
  if (Array.isArray(value)) return value.map(formatResumeDiffValue).join(" · ") || "None";
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if ("before" in record || "after" in record) {
      return `Before: ${formatResumeDiffValue(record.before)} · After: ${formatResumeDiffValue(record.after)}`;
    }
    return Object.entries(record)
      .filter(([, nested]) => resumeDiffHasValue(nested))
      .map(([key, nested]) => `${resumeDiffLabel(key)}: ${formatResumeDiffValue(nested)}`)
      .join(" · ");
  }
  return String(value ?? "None");
}

export function resumeDiffLabel(key: string): string {
  const labels: Record<string, string> = {
    evidence_policy: "Evidence policy",
    experience_emphasis: "Experience emphasis",
    moved_to_top: "Moved to top",
    previously_first: "Previously first",
    project_emphasis: "Project emphasis",
    skill_emphasis: "Skill emphasis",
  };
  return labels[key] || key.replaceAll("_", " ");
}
