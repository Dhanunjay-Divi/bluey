import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { AlertCircle, LoaderCircle } from "lucide-react";
import { accessToken, ApiError, jobsApi } from "./api";
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

export default function App() {
  const [workspace, setWorkspace] = useState<JobsWorkspace | null>(isPreview ? initialPreviewWorkspace : null);
  const [account, setAccount] = useState<AccountSummary | null>(
    isPreview ? { email: "taylor@example.com", balance_cents: 2450 } : null,
  );
  const [resumeVersions, setResumeVersions] = useState<Record<string, ResumeVersion>>({});
  const [loading, setLoading] = useState(!isPreview && Boolean(accessToken()));
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const resumeUploadAttempts = useRef(new ResumeUploadAttemptLineage());
  const navigate = useNavigate();

  const refresh = useCallback(async () => {
    if (isPreview) return;
    setLoading(true);
    setError("");
    try {
      const [nextWorkspace, nextAccount] = await Promise.all([jobsApi.workspace(), jobsApi.account()]);
      setWorkspace({
        ...nextWorkspace,
        runner_availability: runnerAvailabilityOrLocked(nextWorkspace.runner_availability),
      });
      setAccount(nextAccount);
    } catch (requestError) {
      const message = requestError instanceof Error ? requestError.message : "Bluey Jobs could not load.";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [isPreview]);

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
      if (isPreview) {
        setWorkspace((current) =>
          current ? { ...current, profile, preferences: localizedPreferences } : current,
        );
        return;
      }
      try {
        const [savedProfile, savedPreferences] = await Promise.all([
          jobsApi.saveProfile(profile),
          jobsApi.savePreferences(localizedPreferences),
        ]);
        setWorkspace((current) =>
          current ? { ...current, profile: savedProfile, preferences: savedPreferences } : current,
        );
      } catch (requestError) {
        setError(requestError instanceof Error ? requestError.message : "Could not save setup progress.");
        throw requestError;
      }
    },
    [],
  );

  const saveOnboarding = useCallback(
    async (profile: CareerProfile, preferences: JobPreferences, track: CareerTrack) => {
      setError("");
      const localizedPreferences = {
        ...preferences,
        time_zone_offset_minutes: -new Date().getTimezoneOffset(),
      };
      if (isPreview) {
        setWorkspace((current) =>
          current
            ? {
                ...current,
                profile,
                preferences: localizedPreferences,
                tracks: current.tracks.some((item) => item.id === track.id)
                  ? current.tracks.map((item) => (item.id === track.id ? track : item))
                  : [track, ...current.tracks],
              }
            : current,
        );
        navigate(`/matches${previewSearch}`);
        return;
      }
      try {
        const completedWorkspace = await jobsApi.completeOnboarding(
          profile,
          localizedPreferences,
          track,
        );
        setWorkspace(completedWorkspace);
        setToast("Career Profile ready. Bluey is finding your first matches.");
        navigate("/matches");
      } catch (requestError) {
        setError(requestError instanceof Error ? requestError.message : "Could not save your Career Profile.");
        throw requestError;
      }
    },
    [navigate],
  );

  const saveProfile = useCallback(async (profile: CareerProfile) => {
    const saved = isPreview ? profile : await jobsApi.saveProfile(profile);
    setWorkspace((current) => (current ? { ...current, profile: saved } : current));
    setToast("Career Profile saved.");
  }, [isPreview]);

  const importResumeSource = useCallback(
    async (file: File, profile: CareerProfile, pageCount?: number): Promise<CareerProfile> => {
      setError("");
      try {
        let saved: CareerProfile;
        if (isPreview) {
          const extension = file.name.split(".").pop()?.toLowerCase() || "";
          saved = {
            ...profile,
            source_resume_name: file.name,
            source_resume_asset_id: `preview-resume-${Date.now()}`,
            source_resume_sha256: "preview",
            source_resume_media_type: extension === "docx"
              ? "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
              : extension === "pdf" ? "application/pdf" : "text/plain",
            source_resume_template_status: extension === "docx" ? "exact_docx" : "ats_layout",
          };
        } else {
          const result = await uploadResumeSourceWithLineage(
            resumeUploadAttempts.current,
            file,
            profile,
            pageCount,
          );
          saved = result.profile;
        }
        setWorkspace((current) => (current ? { ...current, profile: saved } : current));
        setToast(saved.source_resume_template_status === "exact_docx"
          ? "Resume imported. Bluey will preserve its Word layout for tailored downloads."
          : "Resume imported. Bluey will use a clean ATS layout for tailored downloads.");
        return saved;
      } catch (requestError) {
        setError(requestError instanceof Error ? requestError.message : "Could not save that resume.");
        throw requestError;
      }
    },
    [],
  );

  const savePreferences = useCallback(async (preferences: JobPreferences) => {
    const localized = {
      ...preferences,
      time_zone_offset_minutes: -new Date().getTimezoneOffset(),
    };
    const saved = isPreview ? localized : await jobsApi.savePreferences(localized);
    setWorkspace((current) => (current ? { ...current, preferences: saved } : current));
    setToast("Job preferences saved.");
  }, [isPreview]);

  const saveTrack = useCallback(async (track: CareerTrack) => {
    const saved = isPreview
      ? { ...track, id: track.id || `track-${Date.now()}`, updated_at_ms: Date.now() }
      : await jobsApi.saveTrack(track);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            tracks: [saved, ...current.tracks.filter((item) => item.id !== saved.id)],
            auto_submit_authorizations: current.auto_submit_authorizations.map((authorization) =>
              authorization.career_track_id === saved.id
                ? { ...authorization, status: "needs_review" }
                : authorization,
            ),
          }
        : current,
    );
    setToast(track.id ? "Career Track updated." : "Career Track started.");
  }, []);

  const authorizeTrackAutoSubmit = useCallback(async (track: CareerTrack) => {
    const saved: AutoSubmitAuthorization = isPreview
      ? {
          id: `auto-submit-${track.id}-${Date.now()}`,
          career_track_id: track.id,
          application_identity_id: track.application_identity_id || "",
          source_resume_asset_id: workspace?.profile.source_resume_asset_id || "",
          revision_no: 1,
          authorized_at_ms: Date.now(),
          status: "active",
        }
      : await jobsApi.authorizeTrackAutoSubmit(track.id);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            auto_submit_authorizations: [
              saved,
              ...current.auto_submit_authorizations.filter(
                (authorization) => authorization.career_track_id !== track.id,
              ),
            ],
          }
        : current,
    );
    setToast(`Auto-submit enabled for ${track.name}.`);
  }, [workspace?.profile.source_resume_asset_id]);

  const revokeTrackAutoSubmit = useCallback(async (track: CareerTrack) => {
    if (!isPreview) await jobsApi.revokeTrackAutoSubmit(track.id);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            auto_submit_authorizations: current.auto_submit_authorizations.filter(
              (authorization) => authorization.career_track_id !== track.id,
            ),
          }
        : current,
    );
    setToast(`Auto-submit turned off for ${track.name}.`);
  }, []);

  const deleteTrack = useCallback(async (track: CareerTrack) => {
    if (!isPreview) await jobsApi.deleteTrack(track.id);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            tracks: current.tracks.filter((item) => item.id !== track.id),
            auto_submit_authorizations: current.auto_submit_authorizations.filter(
              (authorization) => authorization.career_track_id !== track.id,
            ),
            matches: current.matches.map((job) =>
              job.track_id === track.id ? { ...job, track_id: "" } : job,
            ),
          }
        : current,
    );
    setToast(`${track.name} deleted.`);
  }, []);

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
    const saved = isPreview
      ? { ...identity, id: `identity-${Date.now()}`, verification_status: "pending" as const, is_default: false, created_at_ms: Date.now(), updated_at_ms: Date.now() }
      : await jobsApi.createApplicationIdentity(identity);
    setWorkspace((current) => current ? {
      ...current,
      application_identities: [saved, ...current.application_identities.filter((item) => item.id !== saved.id)],
    } : current);
    setToast(`Verification sent to ${saved.email}.`);
    return saved;
  }, []);

  const updateApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    const saved = isPreview ? { ...identity, updated_at_ms: Date.now() } : await jobsApi.updateApplicationIdentity(identity);
    setWorkspace((current) => current ? {
      ...current,
      application_identities: current.application_identities.map((item) => item.id === saved.id
        ? saved
        : saved.is_default ? { ...item, is_default: false } : item),
    } : current);
    setToast(saved.is_default ? `${saved.email} is now the default.` : "Application email updated.");
    return saved;
  }, []);

  const verifyApplicationIdentity = useCallback(async (identity: ApplicationIdentity, code: string) => {
    const saved = isPreview
      ? { ...identity, verification_status: "verified" as const, updated_at_ms: Date.now() }
      : await jobsApi.verifyApplicationIdentity(identity.id, code);
    setWorkspace((current) => current ? {
      ...current,
      application_identities: current.application_identities.map((item) => item.id === saved.id ? saved : item),
    } : current);
    setToast(`${saved.email} verified.`);
    return saved;
  }, []);

  const resendApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    if (!isPreview) await jobsApi.resendApplicationIdentity(identity.id);
    setToast(`New code sent to ${identity.email}.`);
  }, []);

  const deleteApplicationIdentity = useCallback(async (identity: ApplicationIdentity) => {
    if (!isPreview) await jobsApi.deleteApplicationIdentity(identity.id);
    setWorkspace((current) => current ? {
      ...current,
      application_identities: current.application_identities.filter((item) => item.id !== identity.id),
    } : current);
    setToast(`${identity.email} removed.`);
  }, []);

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
