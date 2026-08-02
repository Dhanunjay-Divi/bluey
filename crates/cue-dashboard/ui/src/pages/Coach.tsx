import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent as ReactKeyboardEvent } from "react";
import {
  Activity,
  BookOpenText,
  BriefcaseBusiness,
  Check,
  Code2,
  FilePenLine,
  FileText,
  FolderKanban,
  Link2,
  LoaderCircle,
  MessageSquareText,
  Network,
  Plus,
  RotateCcw,
  Save,
  ShieldCheck,
  Sparkles,
  Trash2,
  Users,
  X,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { JobsHandoffBanner, useJobsHandoff } from "../components/JobsHandoffProvider";
import {
  WORKSPACE_STALE_REVISION_ERROR,
  workspaceActivate,
  workspaceCreate,
  workspaceDelete,
  workspaceGet,
  workspaceInstructionsPatch,
  workspaceList,
  workspaceUpdate,
  type WorkspaceListResponse,
  type WorkspaceRecord,
} from "../lib/workspaces";
import {
  applyWorkspaceActivation,
  applyWorkspaceDelete,
  mergeStaleWorkspaceDraft,
  reconcileWorkspaceRefresh,
  workspaceDraftDirty,
  workspaceDraftFromRecord,
  workspaceReferenceSections,
  workspaceSwitchNeedsConfirmation,
  type WorkspaceDraft,
} from "./coachWorkspaceState";
import {
  ASSISTANT_MODE_OPTIONS,
  MAX_ASSISTANT_COMPANY_CHARS,
  MAX_ASSISTANT_INSTRUCTIONS_CHARS,
  MAX_ASSISTANT_ROLE_CHARS,
  MAX_PRIORITY_QUESTION_CHARS,
  MAX_PRIORITY_QUESTIONS,
  assistantProfilesEqual,
  charCount,
  hasAssistantProfileErrors,
  normalizeAssistantProfile,
  priorityQuestionErrorKey,
  validateAssistantProfile,
  type AssistantMode,
  type AssistantProfile,
  type AssistantProfileErrors,
} from "./assistantProfile";

type LoadState = "loading" | "ready" | "error";
type Operation = "idle" | "saving" | "saved" | "creating" | "activating" | "deleting" | "refreshing";

const MAX_WORKSPACE_TITLE_CHARS = 120;
const MAX_WORKSPACE_INSTRUCTIONS_CHARS = 4_000;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;

const MODE_ICONS: Record<AssistantMode, LucideIcon> = {
  general: Sparkles,
  interview: BriefcaseBusiness,
  behavioral_interview: MessageSquareText,
  coding: Code2,
  system_design: Network,
  meeting: Users,
  writing: FilePenLine,
};

export function Coach() {
  const { state: jobsHandoff, dismiss: dismissJobsHandoff } = useJobsHandoff();
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceRecord[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [selectedWorkspace, setSelectedWorkspace] = useState<WorkspaceRecord | null>(null);
  const [draft, setDraft] = useState<WorkspaceDraft | null>(null);
  const [operation, setOperation] = useState<Operation>("idle");
  const [operationError, setOperationError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [showValidation, setShowValidation] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [createTitle, setCreateTitle] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [deleteCandidate, setDeleteCandidate] = useState<WorkspaceRecord | null>(null);
  const [pendingSwitch, setPendingSwitch] = useState<WorkspaceRecord | null>(null);
  const requestGenerationRef = useRef(0);
  const saveGenerationRef = useRef(0);
  const draftGenerationRef = useRef(0);
  const selectedIdRef = useRef<string | null>(null);
  const selectedWorkspaceRef = useRef<WorkspaceRecord | null>(null);
  const draftRef = useRef<WorkspaceDraft | null>(null);
  const workspacesRef = useRef<WorkspaceRecord[]>([]);
  const deleteButtonRef = useRef<HTMLButtonElement | null>(null);
  const jobsRevisionRef = useRef(jobsHandoff.successful_import_revision);
  const currentJobsRevisionRef = useRef(jobsHandoff.successful_import_revision);

  selectedIdRef.current = selectedWorkspace?.id ?? null;
  selectedWorkspaceRef.current = selectedWorkspace;
  draftRef.current = draft;
  workspacesRef.current = workspaces;
  currentJobsRevisionRef.current = jobsHandoff.successful_import_revision;

  useEffect(() => {
    let cancelled = false;
    const generation = ++requestGenerationRef.current;
    setLoadState("loading");
    setLoadError(null);
    setOperationError(null);
    void workspaceList()
      .then((response) => {
        if (cancelled || generation !== requestGenerationRef.current) return;
        const safe = validateWorkspaceListForCoach(response);
        const selected = workspaceForId(safe.workspaces, safe.active_workspace_id)
          ?? safe.workspaces[0]
          ?? null;
        setWorkspaces(safe.workspaces);
        setActiveWorkspaceId(safe.active_workspace_id);
        installSelectedWorkspace(selected);
        setLoadState("ready");
        setOperation("idle");
      })
      .catch((error) => {
        if (cancelled || generation !== requestGenerationRef.current) return;
        setWorkspaces([]);
        setActiveWorkspaceId(null);
        installSelectedWorkspace(null);
        setLoadError(errorMessage(error));
        setLoadState("error");
      });
    return () => {
      cancelled = true;
    };
  }, [loadAttempt]);

  useEffect(() => {
    if (jobsHandoff.successful_import_revision === jobsRevisionRef.current) return;
    jobsRevisionRef.current = jobsHandoff.successful_import_revision;
    const generation = ++requestGenerationRef.current;
    setOperation("refreshing");
    setOperationError(null);
    void workspaceList()
      .then((response) => {
        if (generation !== requestGenerationRef.current) return;
        const safe = validateWorkspaceListForCoach(response);
        const refreshed = reconcileWorkspaceRefresh(
          selectedWorkspaceRef.current,
          draftRef.current,
          safe,
        );
        setWorkspaces(refreshed.workspaces);
        setActiveWorkspaceId(refreshed.active_workspace_id);
        setSelectedWorkspace(refreshed.selected_workspace);
        draftGenerationRef.current += 1;
        setDraft(refreshed.draft);
        setOperation("idle");
        if (refreshed.conflicted_fields.length) {
          setOperationError(
            `Jobs refreshed this workspace. Bluey kept the newest ${fieldLabels(refreshed.conflicted_fields)} and preserved only non-conflicting draft changes. Review before saving.`,
          );
        } else if (refreshed.preserved_fields.length) {
          setNotice("Verified Jobs context refreshed. Your non-conflicting unsaved changes were preserved.");
        } else {
          setNotice("Verified Jobs context refreshed in the active workspace.");
        }
      })
      .catch((error) => {
        if (generation !== requestGenerationRef.current) return;
        setOperation("idle");
        setOperationError(`Verified Jobs context was linked, but Coach could not refresh: ${errorMessage(error)}`);
      });
  }, [jobsHandoff.successful_import_revision]);

  const profileErrors = useMemo<AssistantProfileErrors>(
    () => (draft ? validateAssistantProfile(draft.profile) : {}),
    [draft],
  );
  const titleError = draft ? validateWorkspaceTitle(draft.title) : null;
  const instructionsError = draft
    && charCount(draft.instructions) > MAX_WORKSPACE_INSTRUCTIONS_CHARS
    ? `Workspace instructions must be ${MAX_WORKSPACE_INSTRUCTIONS_CHARS.toLocaleString()} characters or fewer.`
    : null;
  const dirty = Boolean(selectedWorkspace && draft && workspaceDraftDirty(selectedWorkspace, draft));
  const busy = ["saving", "creating", "activating", "deleting", "refreshing"].includes(operation);
  const isActive = Boolean(selectedWorkspace && selectedWorkspace.id === activeWorkspaceId);
  const selectedMode = ASSISTANT_MODE_OPTIONS.find((option) => option.value === draft?.profile.mode);
  const sections = selectedWorkspace ? workspaceReferenceSections(selectedWorkspace) : null;
  const activity = uniqueBy(sections?.activity ?? [], (item) => item.meeting_id);
  const context = uniqueBy(sections?.context ?? [], (item) => item.context_id);
  const artifacts = uniqueBy(
    sections?.artifacts ?? [],
    (item) => `${item.meeting_id}:${item.conversation_turn_id}:${item.artifact_type}`,
  );
  const linkedToJob = Boolean(
    selectedWorkspace?.linked_job
    || draft?.profile.source?.application_id
    || draft?.profile.source?.receipt_id
    || draft?.profile.source?.resume_version_id,
  );
  const jobsBanner = jobsHandoff.notice ? (
    <JobsHandoffBanner
      notice={jobsHandoff.notice}
      profileState={loadState === "ready" && operation === "refreshing" ? "loading" : loadState}
      onDismiss={dismissJobsHandoff}
    />
  ) : null;

  function installSelectedWorkspace(workspace: WorkspaceRecord | null) {
    draftGenerationRef.current += 1;
    setSelectedWorkspace(workspace);
    setDraft(workspace ? workspaceDraftFromRecord(workspace) : null);
    setShowValidation(false);
    setOperationError(null);
  }

  function updateDraft(update: (current: WorkspaceDraft) => WorkspaceDraft) {
    draftGenerationRef.current += 1;
    setDraft((current) => (current ? update(current) : current));
    setOperation((current) => (current === "saved" ? "idle" : current));
    setOperationError(null);
    setNotice(null);
  }

  function updateProfile(update: (current: AssistantProfile) => AssistantProfile) {
    updateDraft((current) => ({
      ...current,
      profile: update(current.profile),
    }));
  }

  async function retryWorkspaceRefresh() {
    if (busy) return;
    const generation = ++requestGenerationRef.current;
    setOperation("refreshing");
    setOperationError(null);
    try {
      const safe = validateWorkspaceListForCoach(await workspaceList());
      if (generation !== requestGenerationRef.current) return;
      const refreshed = reconcileWorkspaceRefresh(
        selectedWorkspaceRef.current,
        draftRef.current,
        safe,
      );
      setWorkspaces(refreshed.workspaces);
      setActiveWorkspaceId(refreshed.active_workspace_id);
      setSelectedWorkspace(refreshed.selected_workspace);
      draftGenerationRef.current += 1;
      setDraft(refreshed.draft);
      setOperation("idle");
      if (refreshed.conflicted_fields.length) {
        setOperationError(
          `Reload found newer ${fieldLabels(refreshed.conflicted_fields)}. Bluey kept only non-conflicting local draft changes. Review before saving.`,
        );
      } else {
        setNotice(refreshed.preserved_fields.length
          ? "Workspaces reloaded and non-conflicting unsaved changes were preserved."
          : "Workspaces reloaded.");
      }
    } catch (error) {
      if (generation !== requestGenerationRef.current) return;
      setOperation("idle");
      setOperationError(errorMessage(error));
    }
  }

  function requestWorkspaceSelection(workspaceId: string) {
    const target = workspaceForId(workspaces, workspaceId);
    if (!target || target.id === selectedWorkspace?.id || busy) return;
    if (workspaceSwitchNeedsConfirmation(selectedWorkspace?.id ?? null, target.id, dirty)) {
      setPendingSwitch(target);
      return;
    }
    installSelectedWorkspace(target);
    setNotice(target.id === activeWorkspaceId ? "Viewing the active workspace." : "Viewing a non-active workspace. Activate it when you want Bluey to use it.");
  }

  function confirmWorkspaceSelection() {
    if (!pendingSwitch) return;
    installSelectedWorkspace(pendingSwitch);
    setNotice(pendingSwitch.id === activeWorkspaceId ? "Viewing the active workspace." : "Unsaved changes were discarded. This workspace is not active yet.");
    setPendingSwitch(null);
  }

  async function activateSelectedWorkspace() {
    if (!selectedWorkspace || isActive || busy) return;
    const workspaceId = selectedWorkspace.id;
    const generation = ++requestGenerationRef.current;
    setOperation("activating");
    setOperationError(null);
    try {
      const response = await workspaceActivate(workspaceId);
      if (generation !== requestGenerationRef.current || selectedIdRef.current !== workspaceId) return;
      const activated = validateWorkspaceForCoach(response.workspace);
      const result = applyWorkspaceActivation(workspacesRef.current, workspaceId, {
        ...response,
        workspace: activated,
      });
      setWorkspaces(result.workspaces);
      setActiveWorkspaceId(result.active_workspace_id);
      setSelectedWorkspace(result.selected_workspace);
      draftGenerationRef.current += 1;
      setDraft(result.draft);
      setShowValidation(false);
      setOperationError(null);
      setOperation("idle");
      setNotice(`${activated.title} is now the active Bluey workspace.`);
    } catch (error) {
      if (generation !== requestGenerationRef.current) return;
      setOperation("idle");
      setOperationError(errorMessage(error));
    }
  }

  async function createWorkspace() {
    const normalizedTitle = normalizeSingleLine(createTitle);
    const error = validateWorkspaceTitle(normalizedTitle);
    if (error || busy) {
      setCreateError(error ?? "Bluey is finishing another workspace operation.");
      return;
    }
    const generation = ++requestGenerationRef.current;
    setOperation("creating");
    setCreateError(null);
    try {
      const response = await workspaceCreate({ title: normalizedTitle });
      if (generation !== requestGenerationRef.current) return;
      const created = validateWorkspaceForCoach(response.workspace);
      setWorkspaces(replaceWorkspace(workspacesRef.current, created));
      setActiveWorkspaceId(response.active_workspace_id);
      installSelectedWorkspace(created);
      setShowCreate(false);
      setCreateTitle("");
      setCreateError(null);
      setOperation("idle");
      setNotice(`Created ${created.title}. It will not replace the active workspace until you choose Activate workspace.`);
    } catch (error) {
      if (generation !== requestGenerationRef.current) return;
      setOperation("idle");
      setCreateError(errorMessage(error));
    }
  }

  async function saveWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedWorkspace || !draft || busy) return;
    const errors = validateAssistantProfile(draft.profile);
    setShowValidation(true);
    if (titleError || instructionsError || hasAssistantProfileErrors(errors)) {
      focusFirstWorkspaceError(titleError, instructionsError, errors);
      return;
    }

    const base = selectedWorkspace;
    const normalizedDraft: WorkspaceDraft = {
      title: normalizeSingleLine(draft.title),
      profile: normalizeAssistantProfile(draft.profile),
      instructions: normalizeMultiline(draft.instructions),
    };
    const saveGeneration = ++saveGenerationRef.current;
    const workspaceId = base.id;
    const jobsRevisionAtSave = currentJobsRevisionRef.current;
    const draftGenerationAtSave = draftGenerationRef.current;
    setOperation("saving");
    setOperationError(null);
    setNotice(null);
    try {
      const response = await workspaceUpdate({
        workspace_id: workspaceId,
        expected_revision: base.revision,
        title: normalizedDraft.title === base.title ? undefined : normalizedDraft.title,
        profile: assistantProfilesEqual(normalizedDraft.profile, base.profile)
          ? undefined
          : normalizedDraft.profile,
        instructions: workspaceInstructionsPatch(base.instructions, normalizedDraft.instructions),
      });
      if (
        saveGeneration !== saveGenerationRef.current
        || selectedIdRef.current !== workspaceId
        || currentJobsRevisionRef.current !== jobsRevisionAtSave
        || draftGenerationRef.current !== draftGenerationAtSave
      ) return;
      const persisted = validateWorkspaceForCoach(response.workspace);
      if (persisted.id !== workspaceId) throw new Error("Bluey saved a different workspace than requested.");
      setWorkspaces(replaceWorkspace(workspacesRef.current, persisted));
      setActiveWorkspaceId(response.active_workspace_id);
      setSelectedWorkspace(persisted);
      draftGenerationRef.current += 1;
      setDraft(workspaceDraftFromRecord(persisted));
      setShowValidation(false);
      setOperation("saved");
      setNotice("Workspace saved.");
    } catch (error) {
      if (
        saveGeneration !== saveGenerationRef.current
        || selectedIdRef.current !== workspaceId
        || currentJobsRevisionRef.current !== jobsRevisionAtSave
        || draftGenerationRef.current !== draftGenerationAtSave
      ) return;
      if (isStaleRevisionError(error)) {
        await reloadAfterStaleSave(base, normalizedDraft, saveGeneration);
        return;
      }
      setOperation("idle");
      setOperationError(errorMessage(error));
    }
  }

  async function reloadAfterStaleSave(
    base: WorkspaceRecord,
    localDraft: WorkspaceDraft,
    saveGeneration: number,
  ) {
    try {
      const response = await workspaceGet(base.id);
      if (saveGeneration !== saveGenerationRef.current || selectedIdRef.current !== base.id) return;
      const latest = validateWorkspaceForCoach(response.workspace);
      if (latest.id !== base.id) {
        throw new Error("Bluey reloaded a different workspace than requested.");
      }
      const merged = mergeStaleWorkspaceDraft(base, localDraft, latest);
      setWorkspaces(replaceWorkspace(workspacesRef.current, latest));
      setActiveWorkspaceId(response.active_workspace_id);
      setSelectedWorkspace(latest);
      draftGenerationRef.current += 1;
      setDraft(merged.draft);
      setOperation("idle");
      const conflict = merged.conflicted_fields.length
        ? ` Bluey kept the newest ${fieldLabels(merged.conflicted_fields)} because both views changed them.`
        : "";
      const preserved = merged.preserved_fields.length
        ? ` Your non-conflicting ${fieldLabels(merged.preserved_fields)} draft remains unsaved.`
        : "";
      setOperationError(`This workspace changed in another view, so Bluey reloaded revision ${latest.revision}.${conflict}${preserved} Review and save again.`);
    } catch (reloadError) {
      setOperation("idle");
      setOperationError(`This workspace changed in another view, and reload failed: ${errorMessage(reloadError)} Retry loading before saving.`);
    }
  }

  function discardChanges() {
    if (!selectedWorkspace || busy) return;
    draftGenerationRef.current += 1;
    setDraft(workspaceDraftFromRecord(selectedWorkspace));
    setShowValidation(false);
    setOperation("idle");
    setOperationError(null);
    setNotice("Unsaved workspace changes discarded.");
  }

  async function confirmDeleteWorkspace() {
    if (!deleteCandidate || busy) return;
    const candidate = deleteCandidate;
    const generation = ++requestGenerationRef.current;
    setOperation("deleting");
    setOperationError(null);
    try {
      const receipt = await workspaceDelete(candidate.id, candidate.revision);
      if (generation !== requestGenerationRef.current) return;
      if (receipt.workspace_id !== candidate.id) {
        throw new Error("Bluey returned a delete receipt for a different workspace.");
      }
      const deleted = applyWorkspaceDelete(
        workspacesRef.current,
        selectedIdRef.current,
        receipt,
      );
      setWorkspaces(deleted.workspaces);
      setActiveWorkspaceId(deleted.active_workspace_id);
      const replacement = workspaceForId(deleted.workspaces, deleted.selected_workspace_id);
      installSelectedWorkspace(replacement);
      setDeleteCandidate(null);
      setOperation("idle");
      setNotice(receipt.deleted
        ? `${candidate.title} was removed. Linked meetings and history were retained.`
        : `${candidate.title} was already removed. Linked meetings and history remain retained.`);
    } catch (error) {
      if (generation !== requestGenerationRef.current) return;
      setOperation("idle");
      setOperationError(errorMessage(error));
    }
  }

  function addQuestion() {
    if (!draft || draft.profile.priority_questions.length >= MAX_PRIORITY_QUESTIONS) return;
    const index = draft.profile.priority_questions.length;
    updateProfile((profile) => ({
      ...profile,
      priority_questions: [...profile.priority_questions, ""],
    }));
    window.setTimeout(() => document.getElementById(`priority-question-${index}`)?.focus(), 0);
  }

  function onCreateKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void createWorkspace();
  }

  if (loadState === "loading") {
    return (
      <div className="mx-auto max-w-6xl space-y-5">
        {jobsBanner}
        <CoachLoading label="Loading your workspaces" detail="Reading workspace profiles and reference-only activity from Bluey…" />
      </div>
    );
  }

  if (loadState === "error") {
    return (
      <div className="mx-auto max-w-3xl space-y-4">
        {jobsBanner}
        <ErrorPanel
          title="Coach workspaces are unavailable"
          message={loadError ?? "The workspace list did not respond."}
          onRetry={() => setLoadAttempt((attempt) => attempt + 1)}
        />
      </div>
    );
  }

  return (
    <form className="mx-auto max-w-6xl space-y-5" onSubmit={saveWorkspace} noValidate>
      <header className="rounded-xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-2xl shadow-black/20">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="inline-flex items-center gap-2 rounded-full border border-cyan-400/25 bg-cyan-400/10 px-3 py-1 text-xs font-semibold text-cyan-200">
              <FolderKanban className="h-3.5 w-3.5" aria-hidden="true" />
              Workspace-first Coach
            </div>
            <h1 className="mt-3 text-3xl font-semibold tracking-tight text-zinc-50">Build the context Bluey should keep together</h1>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-zinc-400">
              Each workspace keeps a coach profile and lightweight references to its meetings, context, answer artifacts, and linked job. Bluey does not duplicate transcripts, files, or artifact bodies here.
            </p>
          </div>
          <button
            type="button"
            onClick={() => {
              setCreateTitle("");
              setCreateError(null);
              setShowCreate(true);
              setOperationError(null);
            }}
            disabled={busy || dirty}
            title={dirty ? "Save or discard the current draft before creating another workspace" : undefined}
            className="inline-flex min-h-10 items-center gap-2 rounded-md border border-blue-400/30 bg-blue-500/10 px-4 text-sm font-semibold text-blue-100 hover:bg-blue-500/15 disabled:opacity-50"
          >
            <Plus className="h-4 w-4" aria-hidden="true" />
            New workspace
          </button>
        </div>
      </header>

      {jobsBanner}

      {notice ? (
        <div className="rounded-lg border border-emerald-400/25 bg-emerald-400/10 px-4 py-3 text-sm text-emerald-100" role="status">
          {notice}
        </div>
      ) : null}
      {operationError ? (
        <div className="rounded-lg border border-red-500/35 bg-red-950/35 px-4 py-3 text-sm text-red-100" role="alert">
          <strong className="font-semibold">Workspace action needs attention.</strong>
          <span className="mt-1 block">{operationError}</span>
          <button
            type="button"
            onClick={() => void retryWorkspaceRefresh()}
            disabled={busy}
            className="mt-3 inline-flex min-h-9 items-center gap-2 rounded-md border border-red-300/25 px-3 font-semibold hover:bg-red-400/10"
          >
            <RotateCcw className="h-4 w-4" aria-hidden="true" />
            Retry loading workspaces
          </button>
        </div>
      ) : null}

      <section className="rounded-xl border border-zinc-800 bg-zinc-900 p-5" aria-labelledby="workspace-selector-heading">
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-end">
          <div>
            <label id="workspace-selector-heading" htmlFor="workspace-selector" className="text-sm font-semibold text-zinc-200">
              Workspace to view
            </label>
            <select
              id="workspace-selector"
              value={selectedWorkspace?.id ?? ""}
              onChange={(event) => requestWorkspaceSelection(event.target.value)}
              disabled={!workspaces.length || busy}
              aria-describedby="workspace-selector-help"
              className="mt-2 min-h-11 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 text-sm text-zinc-100 outline-none focus:border-blue-400 disabled:opacity-55"
            >
              {!workspaces.length ? <option value="">No workspaces</option> : null}
              {workspaces.map((workspace) => (
                <option key={workspace.id} value={workspace.id}>
                  {workspace.title}{workspace.id === activeWorkspaceId ? " (active)" : ""}
                </option>
              ))}
            </select>
            <p id="workspace-selector-help" className="mt-1.5 text-xs leading-5 text-zinc-600">
              Viewing does not activate a workspace. Use Activate workspace explicitly after reviewing it.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => void activateSelectedWorkspace()}
              disabled={!selectedWorkspace || isActive || busy || dirty}
              className="inline-flex min-h-10 items-center gap-2 rounded-md bg-blue-500 px-4 text-sm font-semibold text-white hover:bg-blue-400 disabled:opacity-45"
              title={dirty ? "Save or discard changes before activation" : undefined}
            >
              {operation === "activating" ? <LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Check className="h-4 w-4" aria-hidden="true" />}
              {isActive ? "Active workspace" : operation === "activating" ? "Activating…" : "Activate workspace"}
            </button>
            <button
              ref={deleteButtonRef}
              type="button"
              onClick={() => {
                setOperationError(null);
                if (selectedWorkspace) setDeleteCandidate(selectedWorkspace);
              }}
              disabled={!selectedWorkspace || busy}
              className="inline-flex min-h-10 items-center gap-2 rounded-md border border-red-500/30 px-4 text-sm font-semibold text-red-200 hover:bg-red-500/10 disabled:opacity-45"
            >
              <Trash2 className="h-4 w-4" aria-hidden="true" />
              Delete
            </button>
          </div>
        </div>
      </section>

      {!selectedWorkspace || !draft ? (
        <EmptyWorkspaceSections onCreate={() => setShowCreate(true)} />
      ) : (
        <fieldset disabled={busy} className="contents">
          <section className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
            <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
              <TextField
                id="workspace-title"
                label="Workspace name"
                value={draft.title}
                placeholder="e.g. Data engineering interview"
                max={MAX_WORKSPACE_TITLE_CHARS}
                error={showValidation ? titleError ?? undefined : undefined}
                onChange={(value) => updateDraft((current) => ({ ...current, title: value }))}
              />
              <div className="flex flex-wrap items-center gap-2 pb-0.5">
                <span className={`rounded-full border px-3 py-1 text-xs font-semibold ${isActive ? "border-emerald-400/25 bg-emerald-400/10 text-emerald-200" : "border-zinc-700 bg-zinc-950 text-zinc-400"}`}>
                  {isActive ? "Active" : "Not active"}
                </span>
                <span className="rounded-full border border-zinc-700 bg-zinc-950 px-3 py-1 text-xs font-semibold text-zinc-500">
                  Revision {selectedWorkspace.revision}
                </span>
              </div>
            </div>
          </section>

          <nav aria-label="Workspace sections" className="flex flex-wrap gap-2 rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-3">
            {[
              ["activity", "Activity"],
              ["context", "Context"],
              ["instructions", "Instructions"],
              ["artifacts", "Artifacts"],
              ["linked-job", "Linked Job"],
            ].map(([id, label]) => (
              <a key={id} href={`#workspace-${id}`} className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-semibold text-zinc-300 hover:border-blue-400/40 hover:text-blue-200">
                {label}
              </a>
            ))}
          </nav>

          <ReferenceSection
            id="activity"
            icon={Activity}
            title="Activity"
            description="Meeting references linked to this workspace. Transcript content remains in each meeting record."
            empty="No meetings are linked to this workspace yet."
          >
            {activity.map((item) => (
              <ReferenceRow
                key={item.meeting_id}
                title={safeText(item.title, "Untitled meeting")}
                detail={`${formatTimestamp(item.started_at)}${item.ended_at ? ` · ended ${formatTimestamp(item.ended_at)}` : " · active or open"}`}
              />
            ))}
          </ReferenceSection>

          <ReferenceSection
            id="context"
            icon={FileText}
            title="Context"
            description="Reference metadata only. File bytes, screenshots, and extracted text stay in their original meeting context."
            empty="No context references are linked yet."
          >
            {context.map((item) => (
              <ReferenceRow
                key={item.context_id}
                title={safeText(item.title, "Untitled context")}
                detail={`${humanize(item.kind)} · ${humanize(item.processing_status)} · ${formatTimestamp(item.created_at)}`}
              />
            ))}
          </ReferenceSection>

          <section id="workspace-instructions" aria-labelledby="workspace-instructions-heading" className="scroll-mt-4 rounded-xl border border-zinc-800 bg-zinc-900 p-5">
            <div className="flex items-center gap-2">
              <BookOpenText className="h-5 w-5 text-cyan-200" aria-hidden="true" />
              <h2 id="workspace-instructions-heading" className="text-lg font-semibold text-zinc-100">Instructions</h2>
            </div>
            <p className="mt-1 text-sm leading-6 text-zinc-500">
              Workspace instructions, mode, profile fields, and priority questions shape Coach while this workspace is active.
            </p>

            <label htmlFor="workspace-instructions-text" className="mt-5 block text-sm font-semibold text-zinc-200">
              Workspace answer instructions
            </label>
            <textarea
              id="workspace-instructions-text"
              value={draft.instructions}
              onChange={(event) => updateDraft((current) => ({ ...current, instructions: event.target.value }))}
              rows={5}
              aria-invalid={Boolean(showValidation && instructionsError)}
              aria-describedby="workspace-instructions-status"
              placeholder="e.g. Lead with the direct answer, then explain the key trade-off."
              className={`mt-2 w-full resize-y rounded-lg border bg-zinc-950 px-3 py-3 text-sm leading-6 text-zinc-100 outline-none placeholder:text-zinc-600 ${showValidation && instructionsError ? "border-red-400 focus:border-red-300" : "border-zinc-700 focus:border-blue-400"}`}
            />
            <div id="workspace-instructions-status" className="mt-1.5 flex justify-between gap-3 text-xs">
              <span className={showValidation && instructionsError ? "text-red-300" : "text-zinc-600"}>
                {showValidation && instructionsError ? instructionsError : "Saved with this workspace; clearing it sends an explicit clear action."}
              </span>
              <CharacterCount value={draft.instructions} max={MAX_WORKSPACE_INSTRUCTIONS_CHARS} />
            </div>

            <fieldset className="mt-6 border-t border-zinc-800 pt-5" aria-describedby="mode-help">
              <legend className="text-base font-semibold text-zinc-100">Coach mode</legend>
              <p id="mode-help" className="mt-1 text-sm text-zinc-500">All seven modes remain available per workspace.</p>
              <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {ASSISTANT_MODE_OPTIONS.map((option) => {
                  const Icon = MODE_ICONS[option.value];
                  const selected = draft.profile.mode === option.value;
                  return (
                    <label key={option.value} className={`cursor-pointer rounded-lg border p-3 focus-within:ring-2 focus-within:ring-blue-400 ${selected ? "border-blue-400/60 bg-blue-500/10" : "border-zinc-800 bg-zinc-950 hover:border-zinc-600"}`}>
                      <input
                        type="radio"
                        name="assistant-mode"
                        value={option.value}
                        checked={selected}
                        onChange={() => updateProfile((profile) => ({ ...profile, mode: option.value }))}
                        className="sr-only"
                      />
                      <span className="flex items-center gap-2 text-sm font-semibold text-zinc-100">
                        <Icon className="h-4 w-4 text-blue-200" aria-hidden="true" />
                        {option.label}
                      </span>
                      <span className="mt-1 block text-xs leading-5 text-zinc-500">{option.description}</span>
                    </label>
                  );
                })}
              </div>
            </fieldset>

            <div className="mt-5 grid gap-4 md:grid-cols-2">
              <TextField
                id="target-role"
                label="Role or responsibility"
                value={draft.profile.target_role ?? ""}
                placeholder="e.g. Staff software engineer"
                max={MAX_ASSISTANT_ROLE_CHARS}
                error={visibleError(showValidation, profileErrors.target_role)}
                onChange={(value) => updateProfile((profile) => ({ ...profile, target_role: value }))}
              />
              <TextField
                id="company"
                label="Company or organization"
                value={draft.profile.company ?? ""}
                placeholder="e.g. Acme"
                max={MAX_ASSISTANT_COMPANY_CHARS}
                error={visibleError(showValidation, profileErrors.company)}
                onChange={(value) => updateProfile((profile) => ({ ...profile, company: value }))}
              />
            </div>

            <div className="mt-6 rounded-lg border border-zinc-800 bg-zinc-950 p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h3 className="font-semibold text-zinc-100">Priority questions</h3>
                  <p className="mt-1 text-xs leading-5 text-zinc-500">Topics to keep close at hand, never facts or embedded commands.</p>
                </div>
                <button
                  type="button"
                  onClick={addQuestion}
                  disabled={draft.profile.priority_questions.length >= MAX_PRIORITY_QUESTIONS || busy}
                  className="inline-flex min-h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-semibold text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
                >
                  <Plus className="h-4 w-4" aria-hidden="true" />
                  Add question
                </button>
              </div>
              {draft.profile.priority_questions.length ? (
                <ol className="mt-4 space-y-3">
                  {draft.profile.priority_questions.map((question, index) => {
                    const error = profileErrors[priorityQuestionErrorKey(index)];
                    const showError = Boolean(error && (showValidation || charCount(question) > MAX_PRIORITY_QUESTION_CHARS));
                    return (
                      <li key={index} className="rounded-md border border-zinc-800 bg-zinc-900 p-3">
                        <div className="flex items-center justify-between gap-3">
                          <label htmlFor={`priority-question-${index}`} className="text-xs font-semibold text-zinc-500">Question {index + 1}</label>
                          <button
                            type="button"
                            onClick={() => updateProfile((profile) => ({
                              ...profile,
                              priority_questions: profile.priority_questions.filter((_, questionIndex) => questionIndex !== index),
                            }))}
                            className="inline-flex min-h-8 items-center gap-1 rounded-md px-2 text-xs font-semibold text-zinc-500 hover:bg-red-500/10 hover:text-red-300"
                            aria-label={`Remove priority question ${index + 1}`}
                          >
                            <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                            Remove
                          </button>
                        </div>
                        <input
                          id={`priority-question-${index}`}
                          value={question}
                          onChange={(event) => updateProfile((profile) => ({
                            ...profile,
                            priority_questions: profile.priority_questions.map((item, questionIndex) => questionIndex === index ? event.target.value : item),
                          }))}
                          aria-invalid={showError}
                          aria-describedby={`priority-question-${index}-status`}
                          className={`mt-2 w-full rounded-md border bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none ${showError ? "border-red-400" : "border-zinc-700 focus:border-blue-400"}`}
                        />
                        <div id={`priority-question-${index}-status`} className="mt-1.5 flex justify-between gap-3 text-xs">
                          <span className={showError ? "text-red-300" : "text-zinc-600"}>{showError ? error : "One focused question."}</span>
                          <CharacterCount value={question} max={MAX_PRIORITY_QUESTION_CHARS} />
                        </div>
                      </li>
                    );
                  })}
                </ol>
              ) : (
                <EmptyReference text="No priority questions yet." />
              )}
            </div>

            <label htmlFor="custom-instructions" className="mt-6 block text-sm font-semibold text-zinc-200">
              Profile custom instructions
            </label>
            <textarea
              id="custom-instructions"
              value={draft.profile.custom_instructions ?? ""}
              onChange={(event) => updateProfile((profile) => ({ ...profile, custom_instructions: event.target.value }))}
              rows={5}
              aria-invalid={Boolean(showValidation && profileErrors.custom_instructions)}
              aria-describedby="custom-instructions-status"
              className={`mt-2 w-full resize-y rounded-lg border bg-zinc-950 px-3 py-3 text-sm leading-6 text-zinc-100 outline-none ${showValidation && profileErrors.custom_instructions ? "border-red-400" : "border-zinc-700 focus:border-blue-400"}`}
            />
            <div id="custom-instructions-status" className="mt-1.5 flex justify-between gap-3 text-xs">
              <span className={showValidation && profileErrors.custom_instructions ? "text-red-300" : "text-zinc-600"}>
                {showValidation && profileErrors.custom_instructions
                  ? profileErrors.custom_instructions
                  : "Explicit user-authored rules for this coach profile."}
              </span>
              <CharacterCount value={draft.profile.custom_instructions ?? ""} max={MAX_ASSISTANT_INSTRUCTIONS_CHARS} />
            </div>

            <div className="mt-5 rounded-lg border border-cyan-400/15 bg-cyan-400/5 p-4 text-sm text-zinc-400">
              <strong className="text-cyan-100">Current mode: {selectedMode?.shortLabel ?? "Bluey coach"}</strong>
              <p className="mt-1 leading-6">{selectedMode?.guidance ?? "Choose a mode to shape Bluey's response."}</p>
            </div>
          </section>

          <ReferenceSection
            id="artifacts"
            icon={Code2}
            title="Artifacts"
            description="Reference-only links to answer workbenches. Code, design, and document bodies stay in their conversation turns."
            empty="No answer artifacts are linked yet."
          >
            {artifacts.map((item) => (
              <ReferenceRow
                key={`${item.meeting_id}:${item.conversation_turn_id}:${item.artifact_type}`}
                title={safeText(item.title, "Untitled artifact")}
                detail={`${humanize(item.artifact_type)} · ${formatTimestamp(item.created_at)}`}
              />
            ))}
          </ReferenceSection>

          <section id="workspace-linked-job" aria-labelledby="workspace-linked-job-heading" className="scroll-mt-4 rounded-xl border border-zinc-800 bg-zinc-900 p-5">
            <div className="flex items-center gap-2">
              <Link2 className="h-5 w-5 text-cyan-200" aria-hidden="true" />
              <h2 id="workspace-linked-job-heading" className="text-lg font-semibold text-zinc-100">Linked Job</h2>
            </div>
            <p className="mt-1 text-sm leading-6 text-zinc-500">Receipt-backed Bluey Jobs provenance for this workspace, without editable private identifiers.</p>
            {linkedToJob ? (
              <div className="mt-4 flex items-start gap-3 rounded-lg border border-emerald-400/25 bg-emerald-400/10 p-4 text-sm text-emerald-100">
                <ShieldCheck className="mt-0.5 h-5 w-5 shrink-0" aria-hidden="true" />
                <div>
                  <strong>Verified submitted-application context is linked.</strong>
                  <p className="mt-1 leading-6 text-emerald-100/75">
                    Role: {draft.profile.target_role?.trim() || "Not specified"}. Company: {draft.profile.company?.trim() || "Not specified"}. Saving preserves the immutable receipt and evidence binding.
                  </p>
                  {selectedWorkspace.linked_job?.linked_at ? (
                    <p className="mt-2 text-xs text-emerald-100/60">Linked {formatTimestamp(selectedWorkspace.linked_job.linked_at)}</p>
                  ) : null}
                </div>
              </div>
            ) : (
              <EmptyReference text="No Bluey Jobs application is linked to this workspace." />
            )}
          </section>

          <footer className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-zinc-700 bg-zinc-900/95 px-4 py-3 shadow-2xl shadow-black/40 backdrop-blur">
            <p className="text-sm text-zinc-400" aria-live="polite">
              {operation === "saving"
                ? "Saving workspace…"
                : operation === "saved"
                  ? "Workspace saved."
                  : dirty
                    ? "You have unsaved workspace changes."
                    : "Workspace is up to date."}
            </p>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={discardChanges}
                disabled={!dirty || busy}
                className="inline-flex min-h-10 items-center gap-2 rounded-md border border-zinc-700 px-4 text-sm font-semibold text-zinc-300 hover:bg-zinc-800 disabled:opacity-40"
              >
                <RotateCcw className="h-4 w-4" aria-hidden="true" />
                Discard changes
              </button>
              <button
                type="submit"
                disabled={!dirty || busy}
                className="inline-flex min-h-10 items-center gap-2 rounded-md bg-blue-500 px-4 text-sm font-semibold text-white hover:bg-blue-400 disabled:opacity-50"
              >
                {operation === "saving" ? <LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Save className="h-4 w-4" aria-hidden="true" />}
                {operation === "saving" ? "Saving…" : "Save workspace"}
              </button>
            </div>
          </footer>
        </fieldset>
      )}

      <div className="sr-only" aria-live="polite" role="status">
        {operation === "refreshing" ? "Refreshing workspace after Jobs import." : operation === "activating" ? "Activating workspace." : operation === "deleting" ? "Deleting workspace record while retaining meetings." : ""}
      </div>

      {showCreate ? (
        <Dialog title="Create workspace" onClose={() => {
          setShowCreate(false);
          setCreateError(null);
        }}>
          <label htmlFor="new-workspace-title" className="text-sm font-semibold text-zinc-200">Workspace name</label>
          <input
            id="new-workspace-title"
            autoFocus
            value={createTitle}
            maxLength={MAX_WORKSPACE_TITLE_CHARS + 20}
            onChange={(event) => {
              setCreateTitle(event.target.value);
              setCreateError(null);
            }}
            onKeyDown={onCreateKeyDown}
            aria-invalid={Boolean(createError)}
            aria-describedby="new-workspace-status"
            className={`mt-2 min-h-11 w-full rounded-md border bg-zinc-950 px-3 text-sm text-zinc-100 outline-none ${createError ? "border-red-400 focus:border-red-300" : "border-zinc-700 focus:border-blue-400"}`}
          />
          <div id="new-workspace-status" className="mt-1.5 flex justify-between gap-3 text-xs text-zinc-500">
            <span className={createError ? "text-red-300" : undefined}>{createError ?? "Creation does not activate the workspace."}</span>
            <CharacterCount value={createTitle} max={MAX_WORKSPACE_TITLE_CHARS} />
          </div>
          <div className="mt-5 flex justify-end gap-2">
            <DialogButton onClick={() => {
              setShowCreate(false);
              setCreateError(null);
            }}>Cancel</DialogButton>
            <DialogButton primary onClick={() => void createWorkspace()} disabled={operation === "creating"}>
              {operation === "creating" ? "Creating…" : "Create workspace"}
            </DialogButton>
          </div>
        </Dialog>
      ) : null}

      {pendingSwitch ? (
        <Dialog title="Discard unsaved changes?" onClose={() => setPendingSwitch(null)}>
          <p className="text-sm leading-6 text-zinc-400">
            Switching to {pendingSwitch.title} will discard the unsaved draft in {selectedWorkspace?.title}. It will not activate the destination workspace.
          </p>
          {operationError ? (
            <p className="mt-3 rounded-md border border-red-400/25 bg-red-500/10 px-3 py-2 text-sm text-red-100" role="alert">
              {operationError} You can retry deletion or cancel safely.
            </p>
          ) : null}
          <div className="mt-5 flex justify-end gap-2">
            <DialogButton primary autoFocus onClick={() => setPendingSwitch(null)}>Keep editing</DialogButton>
            <DialogButton danger onClick={confirmWorkspaceSelection}>Discard and switch</DialogButton>
          </div>
        </Dialog>
      ) : null}

      {deleteCandidate ? (
        <Dialog
          title={`Delete ${deleteCandidate.title}?`}
          onClose={() => {
            setDeleteCandidate(null);
            setOperationError(null);
            window.setTimeout(() => deleteButtonRef.current?.focus(), 0);
          }}
        >
          <p className="text-sm leading-6 text-zinc-400">
            This removes the workspace record only. Its linked meetings, transcripts, context, answer history, and artifacts are retained and can still be accessed from Sessions.
            {dirty ? " Your current unsaved workspace edits will be discarded." : ""}
          </p>
          <div className="mt-5 flex justify-end gap-2">
            <DialogButton autoFocus onClick={() => {
              setDeleteCandidate(null);
              setOperationError(null);
            }}>Cancel</DialogButton>
            <DialogButton danger onClick={() => void confirmDeleteWorkspace()} disabled={operation === "deleting"}>
              {operation === "deleting" ? "Deleting…" : "Delete workspace"}
            </DialogButton>
          </div>
        </Dialog>
      ) : null}
    </form>
  );
}

function ReferenceSection({
  id,
  icon: Icon,
  title,
  description,
  empty,
  children,
}: {
  id: string;
  icon: LucideIcon;
  title: string;
  description: string;
  empty: string;
  children: React.ReactNode[];
}) {
  return (
    <section id={`workspace-${id}`} aria-labelledby={`workspace-${id}-heading`} className="scroll-mt-4 rounded-xl border border-zinc-800 bg-zinc-900 p-5">
      <div className="flex items-center gap-2">
        <Icon className="h-5 w-5 text-cyan-200" aria-hidden="true" />
        <h2 id={`workspace-${id}-heading`} className="text-lg font-semibold text-zinc-100">{title}</h2>
      </div>
      <p className="mt-1 text-sm leading-6 text-zinc-500">{description}</p>
      {children.length ? <ul className="mt-4 grid gap-3 md:grid-cols-2">{children}</ul> : <EmptyReference text={empty} />}
    </section>
  );
}

function ReferenceRow({ title, detail }: { title: string; detail: string }) {
  return (
    <li className="rounded-lg border border-zinc-800 bg-zinc-950 p-4">
      <p className="truncate text-sm font-semibold text-zinc-200" title={title}>{title}</p>
      <p className="mt-1 text-xs leading-5 text-zinc-500">{detail}</p>
    </li>
  );
}

function EmptyReference({ text }: { text: string }) {
  return (
    <div className="mt-4 rounded-lg border border-dashed border-zinc-700 bg-zinc-950 px-4 py-5 text-center text-sm text-zinc-500">
      {text}
    </div>
  );
}

function EmptyWorkspaceSections({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="space-y-5">
      <section className="rounded-xl border border-blue-400/25 bg-blue-500/5 p-6 text-center">
        <FolderKanban className="mx-auto h-7 w-7 text-blue-200" aria-hidden="true" />
        <h2 className="mt-3 text-xl font-semibold text-zinc-100">Create your first workspace</h2>
        <p className="mx-auto mt-2 max-w-xl text-sm leading-6 text-zinc-500">A workspace groups coach settings with lightweight meeting and context references. Creation does not activate it.</p>
        <button type="button" onClick={onCreate} className="mt-4 inline-flex min-h-10 items-center gap-2 rounded-md bg-blue-500 px-4 text-sm font-semibold text-white hover:bg-blue-400">
          <Plus className="h-4 w-4" aria-hidden="true" />
          Create workspace
        </button>
      </section>
      {[
        ["Activity", "No meeting references yet."],
        ["Context", "No context references yet."],
        ["Instructions", "Create a workspace to configure its coach."],
        ["Artifacts", "No artifact references yet."],
        ["Linked Job", "No submitted application is linked."],
      ].map(([title, text]) => (
        <section key={title} className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
          <h2 className="text-lg font-semibold text-zinc-100">{title}</h2>
          <EmptyReference text={text} />
        </section>
      ))}
    </div>
  );
}

function Dialog({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) {
  const dialogRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const dialog = dialogRef.current;
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>(
      "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href]",
    ) ?? []);
    window.setTimeout(() => {
      const preferred = dialog?.querySelector<HTMLElement>("[autofocus]");
      (preferred ?? focusable()[0])?.focus();
    }, 0);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusable();
      if (!items.length) {
        event.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      window.setTimeout(() => previousFocus?.focus(), 0);
    };
  }, []);
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="workspace-dialog-title" className="w-full max-w-lg rounded-xl border border-zinc-700 bg-zinc-900 p-5 shadow-2xl">
        <div className="flex items-start justify-between gap-3">
          <h2 id="workspace-dialog-title" className="text-lg font-semibold text-zinc-100">{title}</h2>
          <button type="button" onClick={onClose} className="grid h-9 w-9 place-items-center rounded-md text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200" aria-label="Close dialog">
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        <div className="mt-4">{children}</div>
      </section>
    </div>
  );
}

function DialogButton({ children, onClick, primary, danger, disabled, autoFocus }: { children: React.ReactNode; onClick: () => void; primary?: boolean; danger?: boolean; disabled?: boolean; autoFocus?: boolean }) {
  const tone = danger
    ? "border-red-400/30 bg-red-500/10 text-red-100 hover:bg-red-500/15"
    : primary
      ? "border-blue-400/30 bg-blue-500 text-white hover:bg-blue-400"
      : "border-zinc-700 text-zinc-300 hover:bg-zinc-800";
  return <button type="button" autoFocus={autoFocus} disabled={disabled} onClick={onClick} className={`min-h-10 rounded-md border px-4 text-sm font-semibold disabled:opacity-50 ${tone}`}>{children}</button>;
}

function TextField({ id, label, value, placeholder, max, error, onChange }: { id: string; label: string; value: string; placeholder: string; max: number; error?: string; onChange: (value: string) => void }) {
  return (
    <label htmlFor={id} className="block">
      <span className="text-sm font-semibold text-zinc-200">{label}</span>
      <input
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        aria-invalid={Boolean(error)}
        aria-describedby={`${id}-status`}
        className={`mt-2 min-h-11 w-full rounded-md border bg-zinc-950 px-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 ${error ? "border-red-400 focus:border-red-300" : "border-zinc-700 focus:border-blue-400"}`}
      />
      <span id={`${id}-status`} className="mt-1.5 flex justify-between gap-3 text-xs">
        <span className={error ? "text-red-300" : "text-zinc-600"}>{error ?? "Optional"}</span>
        <CharacterCount value={value} max={max} />
      </span>
    </label>
  );
}

function CharacterCount({ value, max }: { value: string; max: number }) {
  const count = charCount(value);
  return <span className={`shrink-0 tabular-nums ${count > max ? "font-semibold text-red-300" : "text-zinc-600"}`}>{count.toLocaleString()}/{max.toLocaleString()}</span>;
}

function CoachLoading({ label, detail }: { label: string; detail: string }) {
  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900 p-6" role="status" aria-live="polite">
      <div className="flex items-center gap-3">
        <LoaderCircle className="h-5 w-5 animate-spin text-blue-300" aria-hidden="true" />
        <div>
          <h1 className="text-xl font-semibold text-zinc-100">{label}</h1>
          <p className="mt-1 text-sm text-zinc-500">{detail}</p>
        </div>
      </div>
    </section>
  );
}

function ErrorPanel({ title, message, onRetry }: { title: string; message: string; onRetry: () => void }) {
  return (
    <section className="rounded-xl border border-red-500/30 bg-zinc-900 p-6" role="alert">
      <h1 className="text-xl font-semibold text-zinc-100">{title}</h1>
      <p className="mt-3 rounded-md border border-red-500/20 bg-red-950/30 px-3 py-2 text-sm text-red-100">{message}</p>
      <button type="button" onClick={onRetry} className="mt-4 inline-flex min-h-10 items-center gap-2 rounded-md bg-blue-500 px-4 text-sm font-semibold text-white hover:bg-blue-400">
        <RotateCcw className="h-4 w-4" aria-hidden="true" />
        Retry loading
      </button>
    </section>
  );
}

function validateWorkspaceListForCoach(response: WorkspaceListResponse): WorkspaceListResponse {
  const seen = new Set<string>();
  const workspaces = response.workspaces
    .map(validateWorkspaceForCoach)
    .filter((workspace) => workspace.deletion_state.state === "active")
    .filter((workspace) => {
      if (seen.has(workspace.id)) throw new Error("Bluey returned a duplicate workspace identifier.");
      seen.add(workspace.id);
      return true;
    });
  if (response.active_workspace_id && !seen.has(response.active_workspace_id)) {
    throw new Error("Bluey returned an active workspace that is not in the workspace list.");
  }
  return { ...response, workspaces };
}

function validateWorkspaceForCoach(workspace: WorkspaceRecord): WorkspaceRecord {
  if (!validUuid(workspace.id) || !Number.isSafeInteger(workspace.revision) || workspace.revision <= 0) {
    throw new Error("Bluey returned an invalid workspace identity or revision.");
  }
  if (validateWorkspaceTitle(workspace.title)) throw new Error("Bluey returned an invalid workspace title.");
  const profile = normalizeAssistantProfile(workspace.profile);
  if (hasAssistantProfileErrors(validateAssistantProfile(profile))) {
    throw new Error("Bluey returned an invalid workspace coach profile.");
  }
  if (charCount(workspace.instructions ?? "") > MAX_WORKSPACE_INSTRUCTIONS_CHARS) {
    throw new Error("Bluey returned oversized workspace instructions.");
  }
  const sections = workspaceReferenceSections(workspace);
  if (!sections.activity.every((item) => validUuid(item.meeting_id) && validReferenceText(item.title) && validTimestamp(item.started_at) && (!item.ended_at || validTimestamp(item.ended_at)))) {
    throw new Error("Bluey returned invalid workspace activity references.");
  }
  if (!sections.context.every((item) => validUuid(item.meeting_id)
    && validUuid(item.context_id)
    && validReferenceText(item.title)
    && ["image", "diagram", "code", "document", "text", "other"].includes(item.kind)
    && ["pending", "ready", "unsupported", "failed"].includes(item.processing_status)
    && validTimestamp(item.created_at))) {
    throw new Error("Bluey returned invalid workspace context references.");
  }
  if (!sections.artifacts.every((item) => validUuid(item.meeting_id)
    && validUuid(item.conversation_turn_id)
    && ["code", "system_design", "screen", "document", "structured"].includes(item.artifact_type)
    && validReferenceText(item.title)
    && validTimestamp(item.created_at))) {
    throw new Error("Bluey returned invalid workspace artifact references.");
  }
  if (!validTimestamp(workspace.created_at) || !validTimestamp(workspace.updated_at)) {
    throw new Error("Bluey returned invalid workspace timestamps.");
  }
  if (!workspace.deletion_state || !["active", "deleted"].includes(workspace.deletion_state.state)) {
    throw new Error("Bluey returned an invalid workspace deletion state.");
  }
  if (workspace.deletion_state.state === "deleted" && !validTimestamp(workspace.deletion_state.deleted_at)) {
    throw new Error("Bluey returned an invalid workspace deletion state.");
  }
  if (workspace.linked_job && (
    !validReferenceText(workspace.linked_job.import_id)
    || !/^[0-9a-f]{64}$/iu.test(workspace.linked_job.context_sha256)
    || !validTimestamp(workspace.linked_job.linked_at)
  )) {
    throw new Error("Bluey returned invalid linked-job metadata.");
  }
  return {
    ...workspace,
    profile,
    activity: sections.activity,
    context: sections.context,
    artifacts: sections.artifacts,
  };
}

function replaceWorkspace(workspaces: WorkspaceRecord[], workspace: WorkspaceRecord): WorkspaceRecord[] {
  const index = workspaces.findIndex((item) => item.id === workspace.id);
  if (index < 0) return [...workspaces, workspace];
  return workspaces.map((item) => item.id === workspace.id ? workspace : item);
}

function workspaceForId(workspaces: WorkspaceRecord[], id: string | null): WorkspaceRecord | null {
  if (!id) return null;
  return workspaces.find((workspace) => workspace.id === id) ?? null;
}

function validateWorkspaceTitle(value: string): string | null {
  const normalized = normalizeSingleLine(value);
  if (!normalized) return "Workspace name is required.";
  if (charCount(normalized) > MAX_WORKSPACE_TITLE_CHARS) return `Workspace name must be ${MAX_WORKSPACE_TITLE_CHARS} characters or fewer.`;
  return null;
}

function focusFirstWorkspaceError(titleError: string | null, instructionsError: string | null, profileErrors: AssistantProfileErrors) {
  const id = titleError
    ? "workspace-title"
    : instructionsError
      ? "workspace-instructions-text"
      : firstProfileErrorId(profileErrors);
  if (id) window.setTimeout(() => document.getElementById(id)?.focus(), 0);
}

function firstProfileErrorId(errors: AssistantProfileErrors): string | null {
  const key = Object.keys(errors)[0];
  if (!key) return null;
  if (key.startsWith("priority_questions.")) return `priority-question-${key.split(".")[1]}`;
  if (key === "target_role") return "target-role";
  if (key === "custom_instructions") return "custom-instructions";
  return key;
}

function visibleError(show: boolean, error: string | undefined): string | undefined {
  return show ? error : undefined;
}

function normalizeSingleLine(value: string): string {
  return Array.from(value)
    .filter((character) => !/\p{Cc}/u.test(character) || /\s/u.test(character))
    .join("")
    .split(/\s+/u)
    .filter(Boolean)
    .join(" ");
}

function normalizeMultiline(value: string): string {
  return Array.from(value)
    .filter((character) => character === "\n" || character === "\t" || !/\p{Cc}/u.test(character))
    .join("")
    .trim();
}

function validReferenceText(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const count = charCount(value);
  return count > 0 && count <= 200 && !/[\u0000-\u001f\u007f]/u.test(value);
}

function validTimestamp(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 32 && /^\d+$/u.test(value);
}

function validUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function safeText(value: string, fallback: string): string {
  const normalized = normalizeSingleLine(value);
  return normalized || fallback;
}

function formatTimestamp(value: string): string {
  const milliseconds = Number(value);
  if (!Number.isSafeInteger(milliseconds)) return "Unknown time";
  const date = new Date(milliseconds);
  return Number.isNaN(date.getTime()) ? "Unknown time" : date.toLocaleString();
}

function humanize(value: string): string {
  return value.replaceAll("_", " ").replace(/\b\w/gu, (letter) => letter.toUpperCase());
}

function uniqueBy<T>(items: T[], key: (item: T) => string): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const value = key(item);
    if (seen.has(value)) return false;
    seen.add(value);
    return true;
  });
}

function fieldLabels(fields: Array<keyof WorkspaceDraft>): string {
  return fields.map((field) => field === "profile" ? "coach profile" : field).join(" and ");
}

function isStaleRevisionError(error: unknown): boolean {
  return errorMessage(error).toLowerCase().includes(WORKSPACE_STALE_REVISION_ERROR);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return "Unknown workspace error.";
  }
}
