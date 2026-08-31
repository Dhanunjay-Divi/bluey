import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { AlertCircle, LoaderCircle } from "lucide-react";
import {
  accessToken,
  ApiError,
  jobsApi,
  loadJobsPortal,
  signOutOfBluey,
  type JobsBetaAccess,
} from "./api";
import { previewWorkspace, previewWorkspaceForScenario } from "./data/preview";
import type {
  AccountSummary,
  AnswerMemory,
  ApplicationIdentity,
  AutoSubmitAuthorization,
  BrowserSession,
  CandidateEvent,
  CandidateEventInput,
  CareerProfile,
  CareerTrack,
  DiscoverySource,
  DiscoverySourceCatalogEntry,
  DiscoverySourceCatalogResponse,
  Intervention,
  JobApplication,
  JobPosting,
  JobPreferences,
  JobsWorkspace,
  MailboxConnection,
  MailboxMessage,
  MailboxProviderAvailability,
  MailboxSyncState,
  ResumeVersion,
  UploadResumeSourceResponse,
  UserJobInput,
} from "./types";
import { AppShell } from "./components/AppShell";
import { AuthGate } from "./components/AuthGate";
import { Onboarding } from "./components/Onboarding";
import { LoadError, LoadingScreen } from "./components/PageState";
import { PublicBetaGate } from "./components/PublicBetaGate";
import { previewPosting, previewResume } from "./lib/preview-application";
import { runnerAvailabilityOrLocked } from "./lib/runner-access";
import { automationRoute, portalPreviewState } from "./lib/portal-navigation";
import { portalEligibilityDecision } from "./lib/ats-certification";
import {
  applicationAfterInterventionResolution,
  browserSessionAfterInterventionResolution,
  isCloudAutomationEligibleApplication,
  interventionActionResumesApplication,
  interventionResolutionToast,
  workspaceNeedsCloudAutomationRefresh,
} from "./lib/application-flow";
import { validateMailboxOAuthAuthorizationUrl } from "./lib/mailbox-oauth";

const MatchesView = lazy(() => import("./views/MatchesView").then((module) => ({ default: module.MatchesView })));
const ApplicationsView = lazy(() => import("./views/ApplicationsView").then((module) => ({ default: module.ApplicationsView })));
const ResumeView = lazy(() => import("./views/ResumeView").then((module) => ({ default: module.ResumeView })));
const AutomationView = lazy(() => import("./views/AutomationView").then((module) => ({
  default: module.AutomationView,
})));
const SettingsView = lazy(() => import("./views/SettingsView").then((module) => ({ default: module.SettingsView })));
const CareerCommandCenterView = lazy(() => import("./views/CareerCommandCenterView").then((module) => ({
  default: module.CareerCommandCenterView,
})));

const previewState = portalPreviewState(window.location.search);
const isPreview = previewState.enabled;
const previewScenario = previewState.scenario;
const previewSearch = previewState.search;
const initialPreviewWorkspace = previewWorkspaceForScenario(previewWorkspace, previewScenario);

export function jobsPortalHomeDestination(search = ""): string {
  return `/overview${search}`;
}

export function trackSaveToast(wasExisting: boolean): string {
  return wasExisting ? "Career Track updated." : "Career Track started.";
}

type ResumeUploadRequestId = ReturnType<Crypto["randomUUID"]>;

type ResumeSourceUploader = (
  file: File,
  profile: CareerProfile,
  pageCount: number | undefined,
  requestId: ResumeUploadRequestId,
) => Promise<UploadResumeSourceResponse>;

interface ResumeUploadAttempt {
  fingerprint: string;
  requestId: ResumeUploadRequestId;
}

export class ResumeUploadAttemptLineage {
  private readonly attempts = new WeakMap<File, ResumeUploadAttempt>();
  private activeAttempt?: ResumeUploadAttempt;

  constructor(
    private readonly createRequestId: () => ResumeUploadRequestId = () => crypto.randomUUID(),
  ) {}

  requestId(file: File, profile: CareerProfile, pageCount?: number): ResumeUploadRequestId {
    const fingerprint = resumeUploadAttemptFingerprint(profile, pageCount);
    const current = this.attempts.get(file);
    if (current && current === this.activeAttempt && current.fingerprint === fingerprint) {
      return current.requestId;
    }

    const requestId = this.createRequestId();
    const next = { fingerprint, requestId };
    this.attempts.set(file, next);
    this.activeAttempt = next;
    return requestId;
  }

  clear(
    file: File,
    profile: CareerProfile,
    pageCount: number | undefined,
    requestId: ResumeUploadRequestId,
  ): void {
    const current = this.attempts.get(file);
    if (
      current === this.activeAttempt &&
      current?.requestId === requestId &&
      current.fingerprint === resumeUploadAttemptFingerprint(profile, pageCount)
    ) {
      this.attempts.delete(file);
      this.activeAttempt = undefined;
    }
  }
}

export async function uploadResumeSourceWithLineage(
  lineage: ResumeUploadAttemptLineage,
  file: File,
  profile: CareerProfile,
  pageCount?: number,
  upload: ResumeSourceUploader = jobsApi.uploadResumeSource,
): Promise<UploadResumeSourceResponse> {
  const requestId = lineage.requestId(file, profile, pageCount);
  try {
    const result = await upload(file, profile, pageCount, requestId);
    lineage.clear(file, profile, pageCount, requestId);
    return result;
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      lineage.clear(file, profile, pageCount, requestId);
    }
    throw error;
  }
}

export async function openMailboxCommunicationAuthorization(
  preview: boolean,
  connection: MailboxConnection,
  start: (connectionId: string) => Promise<{ authorization_url: string }>,
  redirect: (authorizationUrl: string) => void,
  portalOrigin: string,
): Promise<void> {
  if (preview) return;
  const result = await start(connection.id);
  redirect(validateMailboxOAuthAuthorizationUrl(
    result,
    connection.provider,
    "communication_write",
    portalOrigin,
  ));
}

function resumeUploadAttemptFingerprint(profile: CareerProfile, pageCount?: number): string {
  return `${pageCount ?? ""}:${canonicalResumeUploadValue(profile)}`;
}

function canonicalResumeUploadValue(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalResumeUploadValue).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalResumeUploadValue(record[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value) ?? "undefined";
}

const PORTAL_AUTHORITY_REFRESH_REASON = "portal_authority_refresh_required";

/**
 * Immediately fail closed while the portal reads the server's current policy projection.
 * Every account-semantic mutation invalidates every Track because each reviewed
 * policy binds the account-wide semantic-input generation.
 */
export function invalidateWorkspacePolicyAuthority(
  workspace: JobsWorkspace,
): JobsWorkspace {
  return invalidateWorkspacePolicyAuthorityForTracks(
    workspace,
    new Set(workspace.tracks.map((track) => track.id)),
  );
}

function invalidateWorkspacePolicyAuthorityForTracks(
  workspace: JobsWorkspace,
  affectedTrackIds: ReadonlySet<string>,
): JobsWorkspace {
  return {
    ...workspace,
    tracks: workspace.tracks.map((track) => {
      if (!affectedTrackIds.has(track.id) || !track.policy.authority) return track;
      const reviewReasonCodes = Array.from(new Set([
        ...track.policy.authority.review_reason_codes,
        PORTAL_AUTHORITY_REFRESH_REASON,
      ]));
      return {
        ...track,
        policy: {
          ...track.policy,
          authority: {
            ...track.policy.authority,
            review_state: "needs_review",
            review_reason_codes: reviewReasonCodes,
          },
        },
      };
    }),
    auto_submit_authorizations: workspace.auto_submit_authorizations.map((authorization) =>
      affectedTrackIds.has(authorization.career_track_id) && authorization.status === "active"
        ? { ...authorization, status: "needs_review" }
        : authorization),
  };
}

type JobsWorkspaceStateSetter = (
  update: (current: JobsWorkspace | null) => JobsWorkspace | null,
) => void;

interface WorkspaceAuthorityRefreshToken {
  authorityEpoch: number;
  refreshGeneration: number;
}

interface WorkspaceAuthorityMutationCycle {
  settlement: Promise<void>;
  resolve(): void;
  reject(error: unknown): void;
}

interface WorkspaceAuthorityMutationToken {
  authorityEpoch: number;
  cycle: WorkspaceAuthorityMutationCycle;
}

interface WorkspaceAuthorityReadbackToken extends WorkspaceAuthorityRefreshToken {
  cycle: WorkspaceAuthorityMutationCycle;
}

/**
 * Orders workspace reads against authority-sensitive writes. A write advances
 * the authority epoch before its network request begins, so an older refresh
 * can never reinstall a pre-write approved projection. Concurrent writes keep
 * authority fail-closed and share one readback after every write has settled.
 */
export class WorkspaceAuthorityEpoch {
  private authorityEpoch = 0;
  private refreshGeneration = 0;
  private readonly activeMutationEpochs = new Set<number>();
  private currentMutationCycle: WorkspaceAuthorityMutationCycle | null = null;
  private readbackCycle: WorkspaceAuthorityMutationCycle | null = null;

  beginRefresh(): WorkspaceAuthorityRefreshToken {
    this.refreshGeneration += 1;
    return {
      authorityEpoch: this.authorityEpoch,
      refreshGeneration: this.refreshGeneration,
    };
  }

  beginMutation(): WorkspaceAuthorityMutationToken {
    this.authorityEpoch += 1;
    this.activeMutationEpochs.add(this.authorityEpoch);
    const cycle = this.currentMutationCycle ?? this.createMutationCycle();
    this.currentMutationCycle = cycle;
    return { authorityEpoch: this.authorityEpoch, cycle };
  }

  finishMutation(
    token: WorkspaceAuthorityMutationToken,
  ): WorkspaceAuthorityReadbackToken | null {
    if (!this.activeMutationEpochs.delete(token.authorityEpoch)) {
      throw new Error("Workspace authority mutation was already settled.");
    }
    return this.acquireMutationReadback(token.cycle);
  }

  completeMutationReadback(
    token: WorkspaceAuthorityReadbackToken,
    installed: boolean,
  ): WorkspaceAuthorityReadbackToken | null {
    this.releaseMutationReadback(token);
    if (installed && this.canInstall(token)) {
      token.cycle.resolve();
      if (this.currentMutationCycle === token.cycle) this.currentMutationCycle = null;
      return null;
    }
    return this.acquireMutationReadback(token.cycle);
  }

  failMutationReadback(
    token: WorkspaceAuthorityReadbackToken,
    error: unknown,
  ): WorkspaceAuthorityReadbackToken | null {
    const currentFailure = this.canInstall(token);
    this.releaseMutationReadback(token);
    if (currentFailure) {
      token.cycle.reject(error);
      if (this.currentMutationCycle === token.cycle) this.currentMutationCycle = null;
      return null;
    }
    return this.acquireMutationReadback(token.cycle);
  }

  canInstall(token: WorkspaceAuthorityRefreshToken): boolean {
    return token.authorityEpoch === this.authorityEpoch
      && token.refreshGeneration === this.refreshGeneration
      && this.activeMutationEpochs.size === 0;
  }

  private createMutationCycle(): WorkspaceAuthorityMutationCycle {
    let resolve: () => void = () => undefined;
    let reject: (error: unknown) => void = () => undefined;
    const settlement = new Promise<void>((resolvePromise, rejectPromise) => {
      resolve = resolvePromise;
      reject = rejectPromise;
    });
    void settlement.catch(() => undefined);
    return { settlement, resolve, reject };
  }

  private acquireMutationReadback(
    cycle: WorkspaceAuthorityMutationCycle,
  ): WorkspaceAuthorityReadbackToken | null {
    if (
      this.currentMutationCycle !== cycle
      || this.activeMutationEpochs.size > 0
      || this.readbackCycle
    ) return null;
    this.readbackCycle = cycle;
    return { ...this.beginRefresh(), cycle };
  }

  private releaseMutationReadback(token: WorkspaceAuthorityReadbackToken): void {
    if (this.readbackCycle !== token.cycle) {
      throw new Error("Workspace authority readback was already settled.");
    }
    this.readbackCycle = null;
  }
}

export function installWorkspaceAtAuthorityEpoch(
  setWorkspace: JobsWorkspaceStateSetter,
  authorityEpoch: WorkspaceAuthorityEpoch,
  token: WorkspaceAuthorityRefreshToken,
  nextWorkspace: JobsWorkspace,
): boolean {
  if (!authorityEpoch.canInstall(token)) return false;
  setWorkspace(() => ({
    ...nextWorkspace,
    runner_availability: runnerAvailabilityOrLocked(nextWorkspace.runner_availability),
  }));
  return true;
}

export function workspaceAfterAutoSubmitAuthorization(
  workspace: JobsWorkspace,
  saved: AutoSubmitAuthorization,
): JobsWorkspace {
  return {
    ...workspace,
    auto_submit_authorizations: [
      saved,
      ...workspace.auto_submit_authorizations.filter(
        (authorization) => authorization.career_track_id !== saved.career_track_id,
      ),
    ],
  };
}

export function workspaceAfterAutoSubmitRevocation(
  workspace: JobsWorkspace,
  trackId: string,
): JobsWorkspace {
  return {
    ...workspace,
    auto_submit_authorizations: workspace.auto_submit_authorizations.filter(
      (authorization) => authorization.career_track_id !== trackId,
    ),
  };
}

export function workspaceAfterTrackDeletion(
  workspace: JobsWorkspace,
  trackId: string,
): JobsWorkspace {
  return {
    ...workspace,
    tracks: workspace.tracks.filter((item) => item.id !== trackId),
    auto_submit_authorizations: workspace.auto_submit_authorizations.filter(
      (authorization) => authorization.career_track_id !== trackId,
    ),
    matches: workspace.matches.map((job) =>
      job.track_id === trackId ? { ...job, track_id: "" } : job),
  };
}

interface WorkspacePolicyMutationOptions {
  preview: boolean;
  loadWorkspace?: () => Promise<JobsWorkspace>;
  invalidateWorkspace?: (workspace: JobsWorkspace) => JobsWorkspace;
}

async function driveWorkspacePolicyAuthorityReadback(
  setWorkspace: JobsWorkspaceStateSetter,
  authorityEpoch: WorkspaceAuthorityEpoch,
  initialReadbackToken: WorkspaceAuthorityReadbackToken | null,
  preview: boolean,
  loadWorkspace: () => Promise<JobsWorkspace>,
): Promise<void> {
  let readbackToken = initialReadbackToken;
  while (readbackToken) {
    const currentReadbackToken = readbackToken;
    if (preview) {
      readbackToken = authorityEpoch.completeMutationReadback(
        currentReadbackToken,
        authorityEpoch.canInstall(currentReadbackToken),
      );
      continue;
    }
    try {
      const nextWorkspace = await loadWorkspace();
      const installed = installWorkspaceAtAuthorityEpoch(
        setWorkspace,
        authorityEpoch,
        currentReadbackToken,
        nextWorkspace,
      );
      readbackToken = authorityEpoch.completeMutationReadback(
        currentReadbackToken,
        installed,
      );
    } catch (requestError) {
      readbackToken = authorityEpoch.failMutationReadback(
        currentReadbackToken,
        requestError,
      );
    }
  }
}

export async function runWorkspacePolicyAuthorityMutation<T>(
  setWorkspace: JobsWorkspaceStateSetter,
  authorityEpoch: WorkspaceAuthorityEpoch,
  mutate: () => Promise<T>,
  updateWorkspace: (current: JobsWorkspace, result: T) => JobsWorkspace,
  {
    preview,
    loadWorkspace = jobsApi.workspace,
    invalidateWorkspace = invalidateWorkspacePolicyAuthority,
  }: WorkspacePolicyMutationOptions,
): Promise<T> {
  const mutationToken = authorityEpoch.beginMutation();
  setWorkspace((current) => current ? invalidateWorkspace(current) : current);

  let result: T | undefined;
  let mutationFailed = false;
  let mutationError: unknown;
  try {
    result = await mutate();
    setWorkspace((current) => current
      ? invalidateWorkspace(updateWorkspace(current, result as T))
      : current);
  } catch (requestError) {
    mutationFailed = true;
    mutationError = requestError;
  }

  const readbackToken = authorityEpoch.finishMutation(mutationToken);
  await driveWorkspacePolicyAuthorityReadback(
    setWorkspace,
    authorityEpoch,
    readbackToken,
    preview,
    loadWorkspace,
  );

  let readbackFailed = false;
  let readbackError: unknown;
  try {
    await mutationToken.cycle.settlement;
  } catch (requestError) {
    readbackFailed = true;
    readbackError = requestError;
  }

  if (mutationFailed) throw mutationError;
  if (readbackFailed) throw readbackError;
  return result as T;
}

interface WorkspacePolicyReconciliationOptions {
  preview: boolean;
  loadWorkspace?: () => Promise<JobsWorkspace>;
  authorityEpoch?: WorkspaceAuthorityEpoch;
}

export async function reconcileWorkspacePolicyAuthority(
  setWorkspace: JobsWorkspaceStateSetter,
  updateWorkspace: (current: JobsWorkspace) => JobsWorkspace,
  {
    preview,
    loadWorkspace = jobsApi.workspace,
    authorityEpoch = new WorkspaceAuthorityEpoch(),
  }: WorkspacePolicyReconciliationOptions,
): Promise<void> {
  const mutationToken = authorityEpoch.beginMutation();
  setWorkspace((current) => current
    ? invalidateWorkspacePolicyAuthority(updateWorkspace(current))
    : current);
  const readbackToken = authorityEpoch.finishMutation(mutationToken);
  await driveWorkspacePolicyAuthorityReadback(
    setWorkspace,
    authorityEpoch,
    readbackToken,
    preview,
    loadWorkspace,
  );
  await mutationToken.cycle.settlement;
}

export default function App() {
  const [workspace, setWorkspace] = useState<JobsWorkspace | null>(isPreview ? initialPreviewWorkspace : null);
  const [account, setAccount] = useState<AccountSummary | null>(
    isPreview ? { email: "taylor@example.com", balance_cents: 2450 } : null,
  );
  const [betaAccess, setBetaAccess] = useState<JobsBetaAccess | null>(
    isPreview ? { schemaVersion: 1, access: "admitted", reason: "admitted" } : null,
  );
  const [resumeVersions, setResumeVersions] = useState<Record<string, ResumeVersion>>({});
  const [loading, setLoading] = useState(!isPreview && Boolean(accessToken()));
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const resumeUploadAttempts = useRef(new ResumeUploadAttemptLineage());
  const workspaceAuthorityEpoch = useRef(new WorkspaceAuthorityEpoch());
  const workspaceRefreshGeneration = useRef(0);
  const navigate = useNavigate();

  const refresh = useCallback(async () => {
    if (isPreview) return;
    const refreshGeneration = ++workspaceRefreshGeneration.current;
    const refreshToken = workspaceAuthorityEpoch.current.beginRefresh();
    setLoading(true);
    setError("");
    try {
      const loaded = await loadJobsPortal();
      if (!workspaceAuthorityEpoch.current.canInstall(refreshToken)) return;
      setBetaAccess(loaded.betaAccess);
      if (loaded.betaAccess.access !== "admitted") {
        setWorkspace(null);
        setAccount(null);
        setResumeVersions({});
      } else if (loaded.workspace && loaded.account && installWorkspaceAtAuthorityEpoch(
        setWorkspace,
        workspaceAuthorityEpoch.current,
        refreshToken,
        loaded.workspace,
      )) {
        setAccount(loaded.account);
      }
    } catch (requestError) {
      if (workspaceAuthorityEpoch.current.canInstall(refreshToken)) {
        const message = requestError instanceof Error ? requestError.message : "Bluey Jobs could not load.";
        setError(message);
        setBetaAccess({ schemaVersion: 1, access: "not_admitted", reason: "unavailable" });
        setWorkspace(null);
        setAccount(null);
        setResumeVersions({});
      }
    } finally {
      if (refreshGeneration === workspaceRefreshGeneration.current) {
        setLoading(false);
      }
    }
  }, [isPreview]);

  const runAuthoritySensitiveWorkspaceMutation = useCallback(
    <T,>(
      mutate: () => Promise<T>,
      updateWorkspace: (current: JobsWorkspace, result: T) => JobsWorkspace,
      invalidateWorkspace = invalidateWorkspacePolicyAuthority,
    ) => runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      workspaceAuthorityEpoch.current,
      mutate,
      updateWorkspace,
      { preview: isPreview, invalidateWorkspace },
    ),
    [isPreview],
  );

  useEffect(() => {
    if (!isPreview && accessToken()) void refresh();
  }, [refresh]);

  useEffect(() => {
    if (isPreview
      || !workspace
      || !accessToken()
      || !workspaceNeedsCloudAutomationRefresh(workspace)) return;

    let cancelled = false;
    let timer = 0;
    const poll = async () => {
      await refresh();
      if (!cancelled) timer = window.setTimeout(() => void poll(), 5_000);
    };
    timer = window.setTimeout(() => void poll(), 3_000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [refresh, workspace]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(""), 3200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const saveOnboardingProgress = useCallback(
    async (profile: CareerProfile, preferences: JobPreferences) => {
      setError("");
      const localizedPreferences = {
        ...preferences,
        time_zone_offset_minutes: -new Date().getTimezoneOffset(),
      };
      try {
        await runAuthoritySensitiveWorkspaceMutation(
          async () => isPreview
            ? [profile, localizedPreferences] as const
            : Promise.all([
                jobsApi.saveProfile(profile),
                jobsApi.savePreferences(localizedPreferences),
              ]),
          (current, [savedProfile, savedPreferences]) => ({
            ...current,
            profile: savedProfile,
            preferences: savedPreferences,
          }),
        );
      } catch (requestError) {
        setError(requestError instanceof Error ? requestError.message : "Could not save setup progress.");
        throw requestError;
      }
    },
    [isPreview, runAuthoritySensitiveWorkspaceMutation],
  );

  const saveOnboarding = useCallback(
    async (profile: CareerProfile, preferences: JobPreferences, track: CareerTrack) => {
      setError("");
      const localizedPreferences = {
        ...preferences,
        time_zone_offset_minutes: -new Date().getTimezoneOffset(),
      };
      try {
        await runAuthoritySensitiveWorkspaceMutation(
          async () => isPreview
            ? null
            : jobsApi.completeOnboarding(profile, localizedPreferences, track),
          (current, completedWorkspace) => completedWorkspace ?? {
            ...current,
            profile,
            preferences: localizedPreferences,
            tracks: current.tracks.some((item) => item.id === track.id)
              ? current.tracks.map((item) => (item.id === track.id ? track : item))
              : [track, ...current.tracks],
          },
        );
        setToast("Career Profile ready. Bluey is finding your first matches.");
        navigate(`/matches${previewSearch}`);
      } catch (requestError) {
        setError(requestError instanceof Error ? requestError.message : "Could not save your Career Profile.");
        throw requestError;
      }
    },
    [isPreview, navigate, runAuthoritySensitiveWorkspaceMutation],
  );

  const saveProfile = useCallback(async (profile: CareerProfile) => {
    await runAuthoritySensitiveWorkspaceMutation(
      async () => isPreview ? profile : jobsApi.saveProfile(profile),
      (current, saved) => ({ ...current, profile: saved }),
    );
    setToast("Career Profile saved.");
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const importResumeSource = useCallback(
    async (file: File, profile: CareerProfile, pageCount?: number): Promise<CareerProfile> => {
      setError("");
      try {
        const saved = await runAuthoritySensitiveWorkspaceMutation(
          async () => {
            if (isPreview) {
              const extension = file.name.split(".").pop()?.toLowerCase() || "";
              return {
                ...profile,
                source_resume_name: file.name,
                source_resume_asset_id: `preview-resume-${Date.now()}`,
                source_resume_sha256: "preview",
                source_resume_media_type: extension === "docx"
                  ? "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                  : extension === "pdf" ? "application/pdf" : "text/plain",
                source_resume_template_status: extension === "docx" ? "exact_docx" : "ats_layout",
              };
            }
            const result = await uploadResumeSourceWithLineage(
              resumeUploadAttempts.current,
              file,
              profile,
              pageCount,
            );
            return result.profile;
          },
          (current, nextProfile) => ({ ...current, profile: nextProfile }),
        );
        setToast(saved.source_resume_template_status === "exact_docx"
          ? "Resume imported. Bluey will preserve its Word layout for tailored downloads."
          : "Resume imported. Bluey will use a clean ATS layout for tailored downloads.");
        return saved;
      } catch (requestError) {
        setError(requestError instanceof Error ? requestError.message : "Could not save that resume.");
        throw requestError;
      }
    },
    [isPreview, runAuthoritySensitiveWorkspaceMutation],
  );

  const savePreferences = useCallback(async (preferences: JobPreferences) => {
    const localized = {
      ...preferences,
      time_zone_offset_minutes: -new Date().getTimezoneOffset(),
    };
    await runAuthoritySensitiveWorkspaceMutation(
      async () => isPreview ? localized : jobsApi.savePreferences(localized),
      (current, saved) => ({ ...current, preferences: saved }),
    );
    setToast("Job preferences saved.");
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const saveTrack = useCallback(async (track: CareerTrack) => {
    const wasExisting = workspace?.tracks.some((item) => item.id === track.id) ?? false;
    const affectedTrackId = track.id;
    await runAuthoritySensitiveWorkspaceMutation(
      async () => isPreview
        ? { ...track, id: track.id || `track-${Date.now()}`, updated_at_ms: Date.now() }
        : jobsApi.saveTrack(track),
      (current, saved) => ({
        ...current,
        tracks: [saved, ...current.tracks.filter((item) => item.id !== saved.id)],
        auto_submit_authorizations: current.auto_submit_authorizations.map((authorization) =>
          authorization.career_track_id === saved.id
            ? { ...authorization, status: "needs_review" }
            : authorization),
      }),
      (current) => affectedTrackId
        ? invalidateWorkspacePolicyAuthorityForTracks(current, new Set([affectedTrackId]))
        : invalidateWorkspacePolicyAuthority(current),
    );
    setToast(trackSaveToast(wasExisting));
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation, workspace?.tracks]);

  const authorizeTrackAutoSubmit = useCallback(async (track: CareerTrack) => {
    await runAuthoritySensitiveWorkspaceMutation(
      async (): Promise<AutoSubmitAuthorization> => isPreview
        ? {
            id: `auto-submit-${track.id}-${Date.now()}`,
            career_track_id: track.id,
            application_identity_id: track.application_identity_id || "",
            source_resume_asset_id: workspace?.profile.source_resume_asset_id || "",
            revision_no: 1,
            authorized_at_ms: Date.now(),
            status: "active",
          }
        : jobsApi.authorizeTrackAutoSubmit(track.id),
      workspaceAfterAutoSubmitAuthorization,
      (current) => current,
    );
    setToast(`Auto-submit enabled for ${track.name}.`);
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation, workspace?.profile.source_resume_asset_id]);

  const revokeTrackAutoSubmit = useCallback(async (track: CareerTrack) => {
    await runAuthoritySensitiveWorkspaceMutation(
      async () => {
        if (!isPreview) await jobsApi.revokeTrackAutoSubmit(track.id);
      },
      (current) => workspaceAfterAutoSubmitRevocation(current, track.id),
      (current) => current,
    );
    setToast(`Auto-submit turned off for ${track.name}.`);
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const deleteTrack = useCallback(async (track: CareerTrack) => {
    await runAuthoritySensitiveWorkspaceMutation(
      async () => {
        if (!isPreview) await jobsApi.deleteTrack(track.id);
      },
      (current) => workspaceAfterTrackDeletion(current, track.id),
      (current) => current,
    );
    setToast(`${track.name} deleted.`);
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const addJob = useCallback(
    async (input: UserJobInput) => {
      const saved = isPreview ? previewPosting(input) : await jobsApi.saveMatch(input);
      setWorkspace((current) =>
        current ? { ...current, matches: [saved, ...current.matches.filter((item) => item.id !== saved.id)] } : current,
      );
      setToast(isPreview
        ? "Preview mode cannot verify live job facts. Sign in to import and check this listing."
        : "Job added. Bluey scored it against your profile.");
      return saved;
    },
    [],
  );

  const searchDiscoverySources = useCallback(
    async (query: string, trackId: string, provider: string): Promise<DiscoverySourceCatalogResponse> => {
      if (!isPreview) return jobsApi.searchDiscoveryCatalog(query, trackId, provider);
      const samples: DiscoverySourceCatalogEntry[] = [
        { id: "a".repeat(64), company: "Northwind Labs", provider: "greenhouse", connected: false },
        { id: "b".repeat(64), company: "Contoso Systems", provider: "lever", connected: false },
        { id: "c".repeat(64), company: "Fabrikam", provider: "ashby", connected: false },
      ];
      const needle = query.trim().toLowerCase();
      return {
        refreshed_at_ms: Date.now(),
        catalog_generated_at: new Date().toISOString(),
        entries: samples.filter((entry) =>
          entry.company.toLowerCase().includes(needle)
          && (provider === "all" || entry.provider === provider)),
      };
    },
    [],
  );

  const connectDiscoverySource = useCallback(
    async (trackId: string, entry: DiscoverySourceCatalogEntry): Promise<DiscoverySource> => {
      const saved = isPreview
        ? {
            id: `source-${entry.id.slice(0, 12)}`,
            provider: entry.provider,
            config: { company: entry.company },
            status: "active" as const,
            health: "waiting" as const,
            last_success_at_ms: null,
          }
        : await jobsApi.connectDiscoverySource(trackId, entry.id);
      setWorkspace((current) => current ? {
        ...current,
        discovery_sources: [saved, ...current.discovery_sources.filter((item) => item.id !== saved.id)],
      } : current);
      setToast(`${entry.company} is connected. Bluey will check its public careers page.`);
      return saved;
    },
    [],
  );

  const prepareApplication = useCallback(
    async (job: JobPosting, mode: string, submissionMode: string) => {
      if (!workspace) return;
      if (isPreview) {
        const resume = previewResume(workspace, job, `resume-${job.id}-${Date.now()}`, mode, account?.email || "");
        const autoSubmitEligible = submissionMode === "auto_submit"
          && portalEligibilityDecision(job.eligibility).can_auto_submit;
        const application: JobApplication = {
          id: `application-${job.id}`,
          job_id: job.id,
          resume_version_id: resume.id,
          state: autoSubmitEligible ? "queued" : "awaiting_review",
          submission_mode: autoSubmitEligible ? "auto_submit" : "review_first",
          match_score: job.match_score,
          answers: [],
          cover_letter: "",
          receipt: {
            job_snapshot: job,
            eligibility: job.eligibility,
            cover_letter_status: "not_included",
            metering: { status: autoSubmitEligible ? "counts_when_queued" : "counts_when_approved_or_downloaded" },
          },
          created_at_ms: Date.now(),
          updated_at_ms: Date.now(),
        };
        setResumeVersions((current) => ({ ...current, [resume.id]: resume }));
        setWorkspace((current) =>
          current
            ? {
                ...current,
                applications: [
                  application,
                  ...current.applications.filter((item) => item.job_id !== job.id),
                ],
              }
            : current,
        );
      } else {
        const response = await jobsApi.prepareApplication(job.id, mode, submissionMode);
        const metering = response.metering;
        setResumeVersions((current) => ({ ...current, [response.resume_version.id]: response.resume_version }));
        setWorkspace((current) =>
          current
            ? {
                ...current,
                entitlement: metering
                  ? { ...current.entitlement, used_packets: metering.used_packets }
                  : current.entitlement,
                applications: [
                  response.application,
                  ...current.applications.filter((item) => item.job_id !== job.id),
                ],
              }
            : current,
        );
        if (metering?.newly_metered && metering.amount_cents > 0) {
          setAccount((current) => current ? {
            ...current,
            balance_cents: Math.max(0, current.balance_cents - metering.amount_cents),
          } : current);
        }
      }
      setToast("Tailored application is ready.");
      navigate(`/applications${previewSearch}`);
    },
    [account?.email, navigate, workspace],
  );

  const commitApplication = useCallback(async (application: JobApplication) => {
    if (isPreview) return;
    const result = await jobsApi.commitPacket(application.id);
    setWorkspace((current) => current ? {
      ...current,
      entitlement: { ...current.entitlement, used_packets: result.used_packets },
    } : current);
    if (result.newly_metered && result.amount_cents > 0) {
      setAccount((current) => current ? {
        ...current,
        balance_cents: Math.max(0, current.balance_cents - result.amount_cents),
      } : current);
    }
  }, []);

  const updateApplication = useCallback(async (application: JobApplication, state: string) => {
    let updated: JobApplication;
    if (isPreview) {
      updated = { ...application, state: state as JobApplication["state"], updated_at_ms: Date.now() };
    } else {
      if (state === "queued") {
        const approved = await jobsApi.approveApplication(application.id);
        updated = approved.application;
        setWorkspace((current) => current ? {
          ...current,
          entitlement: { ...current.entitlement, used_packets: approved.metering.used_packets },
        } : current);
        if (approved.metering.newly_metered && approved.metering.amount_cents > 0) {
          setAccount((current) => current ? {
            ...current,
            balance_cents: Math.max(0, current.balance_cents - approved.metering.amount_cents),
          } : current);
        }
      } else {
        updated = await jobsApi.updateApplication(application.id, state, application.submission_mode);
      }
    }
    setWorkspace((current) =>
      current
        ? { ...current, applications: current.applications.map((item) => (item.id === updated.id ? updated : item)) }
        : current,
    );
    setToast(state === "queued" ? "Application queued." : "Application updated.");
  }, []);

  const reconcileSubmissionNotSubmitted = useCallback(async (application: JobApplication) => {
    const updated = isPreview
      ? {
          ...application,
          state: "failed" as const,
          updated_at_ms: Date.now(),
          submitted_at_ms: undefined,
          receipt: {
            ...application.receipt,
            submission_reconciliation: {
              schema_version: 1,
              outcome: "not_submitted",
              resolved_by: "account_owner",
              resolved_at_ms: Date.now(),
              run_id: application.run_id,
            },
          },
        }
      : await jobsApi.reconcileSubmissionNotSubmitted(application.id);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            applications: current.applications.map((item) =>
              item.id === updated.id ? updated : item,
            ),
          }
        : current,
    );
    setToast("Uncertain run closed as not submitted.");
  }, [isPreview]);

  const loadResumeVersion = useCallback(
    async (id: string) => {
      if (resumeVersions[id]) return resumeVersions[id];
      if (isPreview) {
        const application = workspace?.applications.find((item) => item.resume_version_id === id);
        const job = application ? workspace?.matches.find((item) => item.id === application.job_id) : undefined;
        if (!workspace || !job) return undefined;
        const resume = previewResume(workspace, job, id, "factual", account?.email || "");
        setResumeVersions((current) => ({ ...current, [id]: resume }));
        return resume;
      }
      const resume = await jobsApi.resumeVersion(id);
      setResumeVersions((current) => ({ ...current, [id]: resume }));
      return resume;
    },
    [account?.email, resumeVersions, workspace],
  );

  const createApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    const saved = await runAuthoritySensitiveWorkspaceMutation(
      async () => isPreview
        ? { ...identity, id: `identity-${Date.now()}`, verification_status: "pending" as const, is_default: false, created_at_ms: Date.now(), updated_at_ms: Date.now() }
        : jobsApi.createApplicationIdentity(identity),
      (current, nextIdentity) => ({
        ...current,
        application_identities: [
          nextIdentity,
          ...current.application_identities.filter((item) => item.id !== nextIdentity.id),
        ],
      }),
    );
    setToast(`Verification sent to ${saved.email}.`);
    return saved;
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const updateApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    const saved = await runAuthoritySensitiveWorkspaceMutation(
      async () => isPreview
        ? { ...identity, updated_at_ms: Date.now() }
        : jobsApi.updateApplicationIdentity(identity),
      (current, nextIdentity) => ({
        ...current,
        application_identities: current.application_identities.map((item) =>
          item.id === nextIdentity.id
            ? nextIdentity
            : nextIdentity.is_default ? { ...item, is_default: false } : item),
      }),
    );
    setToast(saved.is_default ? `${saved.email} is now the default.` : "Application email updated.");
    return saved;
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const verifyApplicationIdentity = useCallback(async (identity: ApplicationIdentity, code: string) => {
    const saved = await runAuthoritySensitiveWorkspaceMutation(
      async () => isPreview
        ? { ...identity, verification_status: "verified" as const, updated_at_ms: Date.now() }
        : jobsApi.verifyApplicationIdentity(identity.id, code),
      (current, nextIdentity) => ({
        ...current,
        application_identities: current.application_identities.map((item) =>
          item.id === nextIdentity.id ? nextIdentity : item),
      }),
    );
    setToast(`${saved.email} verified.`);
    return saved;
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const resendApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    if (!isPreview) await jobsApi.resendApplicationIdentity(identity.id);
    setToast(`New code sent to ${identity.email}.`);
  }, []);

  const deleteApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    await runAuthoritySensitiveWorkspaceMutation(
      async () => {
        if (!isPreview) await jobsApi.deleteApplicationIdentity(identity.id);
      },
      (current) => ({
        ...current,
        application_identities: current.application_identities.filter(
          (item) => item.id !== identity.id,
        ),
      }),
    );
    setToast(`${identity.email} removed.`);
  }, [isPreview, runAuthoritySensitiveWorkspaceMutation]);

  const mailboxOAuthProviders = useCallback(async (): Promise<MailboxProviderAvailability[]> => {
    if (isPreview) {
      return [
        { provider: "gmail", configured: true, capabilities: ["status_sync"] },
        { provider: "outlook", configured: true, capabilities: ["status_sync"] },
      ];
    }
    return jobsApi.mailboxOAuthProviders();
  }, []);

  const connectMailbox = useCallback(async (provider: MailboxConnection["provider"]) => {
    if (isPreview) {
      const now = Date.now();
      const saved: MailboxConnection = {
        id: `mailbox-${now}`,
        provider,
        status: "connected",
        account_label: provider === "gmail" ? "taylor@gmail.com" : "taylor@outlook.com",
        aliases: [],
        capabilities: ["status_sync"],
        created_at_ms: now,
        updated_at_ms: now,
      };
      setWorkspace((current) => current ? {
        ...current,
        mailbox_connections: [saved, ...current.mailbox_connections.filter((item) => item.id !== saved.id)],
      } : current);
      setToast(`${saved.account_label} connected.`);
      return;
    }

    const result = await jobsApi.startMailboxOAuth(provider);
    window.location.assign(validateMailboxOAuthAuthorizationUrl(
      result,
      provider,
      "mailbox_read",
      window.location.origin,
    ));
  }, []);

  const authorizeMailboxCommunication = useCallback(async (connection: MailboxConnection) => {
    await openMailboxCommunicationAuthorization(
      isPreview,
      connection,
      jobsApi.startMailboxCommunicationAuthorization,
      (authorizationUrl) => window.location.assign(authorizationUrl),
      window.location.origin,
    );
  }, []);

  const mailboxSyncState = useCallback(async (connection: MailboxConnection): Promise<MailboxSyncState> => {
    if (isPreview) {
      return {
        connection_id: connection.id,
        provider: connection.provider,
        cursor: {},
        next_sync_at_ms: Date.now() + 60_000,
        last_synced_at_ms: Date.now() - 4 * 60_000,
        last_error: "",
        created_at_ms: connection.created_at_ms,
        updated_at_ms: Date.now(),
      };
    }
    return jobsApi.mailboxSyncState(connection.id);
  }, []);

  const mailboxMessages = useCallback(async (connectionId?: string): Promise<MailboxMessage[]> => {
    if (isPreview) return [];
    return jobsApi.mailboxMessages(connectionId, undefined, 30);
  }, []);

  const syncMailbox = useCallback(async (connection: MailboxConnection): Promise<MailboxSyncState> => {
    const state = isPreview
      ? await mailboxSyncState(connection)
      : await jobsApi.syncMailbox(connection.id);
    setToast(`Checking ${connection.account_label} for application updates.`);
    return state;
  }, [mailboxSyncState]);

  const deleteMailboxConnection = useCallback(async (connection: MailboxConnection) => {
    if (!isPreview) await jobsApi.deleteMailboxConnection(connection.id);
    setWorkspace((current) => current ? {
      ...current,
      mailbox_connections: current.mailbox_connections.filter((item) => item.id !== connection.id),
    } : current);
    setToast(`${connection.account_label} disconnected.`);
  }, []);

  const saveAnswerMemory = useCallback(async (answer: AnswerMemory) => {
    const now = Date.now();
    const saved = isPreview
      ? {
          ...answer,
          id: answer.id || `answer-${now}`,
          key: normalizeAnswerKey(answer.key || answer.question),
          confirmed: true,
          created_at_ms: answer.created_at_ms || now,
          updated_at_ms: now,
        }
      : await jobsApi.saveAnswerMemory(answer);
    setWorkspace((current) => current ? {
      ...current,
      answer_memory: [saved, ...current.answer_memory.filter((item) => item.id !== saved.id && !(item.key === saved.key && item.scope === saved.scope && (item.scope_id || "") === (saved.scope_id || "")))],
    } : current);
    setToast("Answer saved to memory.");
    return saved;
  }, []);

  const deleteAnswerMemory = useCallback(async (answer: AnswerMemory) => {
    if (!isPreview) await jobsApi.deleteAnswerMemory(answer.id);
    setWorkspace((current) => current ? {
      ...current,
      answer_memory: current.answer_memory.filter((item) => item.id !== answer.id),
    } : current);
    setToast("Saved answer removed.");
  }, []);

  const saveCandidateEvent = useCallback(async (input: CandidateEventInput) => {
    const now = Date.now();
    const saved: CandidateEvent = isPreview
      ? {
          ...input,
          id: `candidate-event-${now}`,
          reasons: input.reasons || [],
          note: input.note || "",
          status: input.event_type === "application_issue"
            ? "open"
            : input.event_type === "application_outcome"
              ? "confirmed"
              : "recorded",
          created_at_ms: now,
          updated_at_ms: now,
        }
      : await jobsApi.saveCandidateEvent(input);
    setWorkspace((current) => current ? {
      ...current,
      candidate_events: [saved, ...current.candidate_events.filter((item) => item.id !== saved.id)],
    } : current);
    setToast(
      saved.event_type === "match_feedback"
        ? saved.action === "restore" ? "Match restored." : "Match passed. Your search rules did not change."
        : saved.event_type === "application_outcome"
          ? "Application outcome saved."
          : "Problem report saved.",
    );
    return saved;
  }, []);

  const queueCloudRun = useCallback(async (application: JobApplication) => {
    if (!workspace) return;
    const job = workspace.matches.find((item) => item.id === application.job_id);
    if (!job) throw new Error("That job is no longer available.");
    if (application.state !== "queued") {
      throw new Error("Approve this application before starting cloud automation.");
    }
    if (!isCloudAutomationEligibleApplication(workspace, application)) {
      throw new Error(
        "This application is not currently eligible for a new cloud automation run.",
      );
    }
    let updatedApplication = application;
    let session: BrowserSession = {
      id: "",
      runner: "cloud",
      status: "queued",
      current_company: job.company,
      current_step: "Waiting for cloud automation",
      application_id: application.id,
      created_at_ms: 0,
      updated_at_ms: 0,
    };
    if (isPreview) {
      const now = Date.now();
      updatedApplication = { ...application, state: "queued", updated_at_ms: now };
      session = { ...session, id: `run-${now}`, created_at_ms: now, updated_at_ms: now };
    } else {
      const queued = await jobsApi.queueApplicationRun(application.id, "cloud");
      updatedApplication = queued.application;
      session = queued.browser_session;
    }
    setWorkspace((current) => current ? {
      ...current,
      applications: current.applications.map((item) => item.id === application.id ? updatedApplication : item),
      browser_sessions: [session, ...current.browser_sessions.filter((item) => item.id !== session.id)],
    } : current);
    setToast(`${job.company} is queued for cloud automation.`);
  }, [workspace]);

  const resolveIntervention = useCallback(async (
    intervention: Intervention,
    action: string,
    resolution?: { answer?: string; remember?: boolean; scope?: string; scope_id?: string },
  ) => {
    const now = Date.now();
    const approved = interventionActionResumesApplication(action);
    const result = isPreview
      ? {
          intervention: {
            ...intervention,
            status: approved ? "approved" : "resolved",
            resolved_at_ms: approved ? undefined : now,
            metadata: {
              ...intervention.metadata,
              ...(approved ? { approved_at_ms: now } : { answered_at_ms: now, resolved_answer: resolution?.answer }),
            },
          },
          answer_memory: resolution?.remember ? {
            id: `answer-${now}`,
            key: normalizeAnswerKey(intervention.title),
            question: intervention.title,
            value: resolution.answer || "",
            scope: (resolution.scope || "account") as AnswerMemory["scope"],
            scope_id: resolution.scope_id,
            confirmed: true,
            source: "intervention",
            created_at_ms: now,
            updated_at_ms: now,
            use_count: 0,
          } : undefined,
          application: undefined,
        }
      : await jobsApi.resolveIntervention(intervention.id, "resolved", action, resolution);
    const saved = result.intervention;
    const updatedAtMs = Date.now();
    setWorkspace((current) => current ? {
      ...current,
      interventions: current.interventions.map((item) => item.id === saved.id ? saved : item),
      applications: current.applications.map((application) => application.id === saved.application_id
        ? applicationAfterInterventionResolution(application, result.application, action, updatedAtMs)
        : application),
      answer_memory: result.answer_memory
        ? [result.answer_memory, ...current.answer_memory.filter((item) => item.id !== result.answer_memory?.id && !(item.key === result.answer_memory?.key && item.scope === result.answer_memory?.scope && (item.scope_id || "") === (result.answer_memory?.scope_id || "")))]
        : current.answer_memory,
      browser_sessions: current.browser_sessions.map((session) => session.application_id === saved.application_id
        ? browserSessionAfterInterventionResolution(session, action, updatedAtMs)
        : session),
    } : current);
    setToast(interventionResolutionToast(action));
  }, []);

  if (!isPreview && !accessToken()) return <AuthGate />;
  if (loading && !betaAccess) return <LoadingScreen />;
  if (!betaAccess) return <LoadError message={error} onRetry={refresh} />;
  if (betaAccess.access !== "admitted") {
    return (
      <PublicBetaGate
        betaAccess={betaAccess}
        onRetry={refresh}
        onSignOut={signOutOfBluey}
      />
    );
  }
  if (loading && !workspace) return <LoadingScreen />;
  if (!workspace) return <LoadError message={error} onRetry={refresh} />;
  if (!workspace.profile.onboarding_complete) {
    return (
      <Onboarding
        workspace={workspace}
        error={error}
        onImportResume={importResumeSource}
        onProgress={saveOnboardingProgress}
        onComplete={saveOnboarding}
      />
    );
  }

  return (
    <AppShell
      account={account}
      workspace={workspace}
      onRefresh={refresh}
      preview={isPreview}
      previewSearch={previewSearch}
    >
      {error && <div className="global-message error"><AlertCircle size={16} />{error}</div>}
      {toast && <div className="toast" role="status">{toast}</div>}
      <Suspense fallback={<div className="view-loading"><LoaderCircle className="spin" size={20} /><span>Opening view...</span></div>}>
      <Routes>
        <Route index element={<Navigate to={jobsPortalHomeDestination(previewSearch)} replace />} />
        <Route
          path="overview"
          element={
            <CareerCommandCenterView
              workspace={workspace}
              previewSearch={previewSearch}
            />
          }
        />
        <Route
          path="matches"
          element={
            <MatchesView
              workspace={workspace}
              previewSearch={previewSearch}
              onAddJob={addJob}
              onPrepare={prepareApplication}
              onSaveCandidateEvent={saveCandidateEvent}
              onSearchDiscoverySources={searchDiscoverySources}
              onConnectDiscoverySource={connectDiscoverySource}
            />
          }
        />
        <Route
          path="applications"
          element={
            <ApplicationsView
              workspace={workspace}
              previewSearch={previewSearch}
              resumeVersions={resumeVersions}
              onUpdate={updateApplication}
              onReconcileSubmission={reconcileSubmissionNotSubmitted}
              onCommit={commitApplication}
              onLoadResume={loadResumeVersion}
              onResolveIntervention={resolveIntervention}
              onSaveCandidateEvent={saveCandidateEvent}
              preview={isPreview}
            />
          }
        />
        <Route
          path="resume"
          element={
            <ResumeView
              workspace={workspace}
              resumeVersions={resumeVersions}
              onSave={saveProfile}
              onImportResume={importResumeSource}
              onCommit={commitApplication}
              onLoadResume={loadResumeVersion}
            />
          }
        />
        <Route
          path="automation"
          element={
            <AutomationView
              workspace={workspace}
              previewSearch={previewSearch}
              onQueueCloud={queueCloudRun}
              onResolveIntervention={resolveIntervention}
            />
          }
        />
        <Route
          path="browser"
          element={<Navigate to={automationRoute(previewSearch)} replace />}
        />
        <Route
          path="settings"
          element={
            <SettingsView
              workspace={workspace}
              onSaveProfile={saveProfile}
              onSavePreferences={savePreferences}
              onSaveTrack={saveTrack}
              onDeleteTrack={deleteTrack}
              onAuthorizeTrackAutoSubmit={authorizeTrackAutoSubmit}
              onRevokeTrackAutoSubmit={revokeTrackAutoSubmit}
              onCreateIdentity={createApplicationIdentity}
              onUpdateIdentity={updateApplicationIdentity}
              onVerifyIdentity={verifyApplicationIdentity}
              onResendIdentity={resendApplicationIdentity}
              onDeleteIdentity={deleteApplicationIdentity}
              onMailboxProviders={mailboxOAuthProviders}
              onConnectMailbox={connectMailbox}
              onAuthorizeMailboxCommunication={authorizeMailboxCommunication}
              onMailboxSyncState={mailboxSyncState}
              onMailboxMessages={mailboxMessages}
              onSyncMailbox={syncMailbox}
              onDeleteMailbox={deleteMailboxConnection}
              onSaveAnswerMemory={saveAnswerMemory}
              onDeleteAnswerMemory={deleteAnswerMemory}
            />
          }
        />
        <Route
          path="*"
          element={<Navigate to={jobsPortalHomeDestination(previewSearch)} replace />}
        />
      </Routes>
      </Suspense>
    </AppShell>
  );
}

function normalizeAnswerKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}
