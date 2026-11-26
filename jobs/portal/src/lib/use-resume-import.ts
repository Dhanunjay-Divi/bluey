import { useEffect, useMemo, useState } from "react";
import type { CareerProfile } from "../types";
import { importResume, type ImportedResume } from "./documents/import";
import { prepareResumeImport } from "./documents/profile";
import { createLatestResumeSelection, type ResumeSelectionState } from "./resume-import-selection";

export function useResumeImport(profile: CareerProfile) {
  const [selection] = useState(() => createLatestResumeSelection(importResume));
  const [state, setState] = useState<ResumeSelectionState<File, ImportedResume>>({ kind: "idle" });
  useEffect(() => () => selection.invalidate(), [selection]);

  const ready = state.kind === "ready" ? state : undefined;
  // Review against the currently rendered profile, not the one that was
  // present when extraction started. No state or profile mutation here.
  const review = useMemo(() => {
    if (!ready) return {};
    try {
      return { preview: prepareResumeImport(profile, ready.parsed) };
    } catch {
      return { error: new Error("Bluey could not prepare this resume for review. Try another file.") };
    }
  }, [profile, ready]);

  return {
    select: (file: File) => selection.select(file, setState),
    discard: () => {
      selection.invalidate();
      setState({ kind: "idle" });
    },
    isCurrent: selection.isCurrent,
    loading: state.kind === "loading",
    error: state.kind === "error" ? state.error ?? new Error("That resume could not be read.") : review.error,
    ready,
    preview: review.preview,
  };
}
