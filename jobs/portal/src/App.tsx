import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { AlertCircle, LoaderCircle } from "lucide-react";
import { accessToken, jobsApi } from "./api";
import { previewWorkspace, previewWorkspaceForScenario } from "./data/preview";
import type {
  AccountSummary,
  AnswerMemory,
  ApplicationIdentity,
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
  JobsIntegration,
  JobsWorkspace,
  MailboxConnection,
  ResumeVersion,
  UserJobInput,
} from "./types";
import { AppShell } from "./components/AppShell";
import { AuthGate } from "./components/AuthGate";
import { Onboarding } from "./components/Onboarding";
import { LoadError, LoadingScreen } from "./components/PageState";
import { previewPosting, previewResume } from "./lib/preview-application";

const MatchesView = lazy(() => import("./views/MatchesView").then((module) => ({ default: module.MatchesView })));
const ApplicationsView = lazy(() => import("./views/ApplicationsView").then((module) => ({ default: module.ApplicationsView })));
const ResumeView = lazy(() => import("./views/ResumeView").then((module) => ({ default: module.ResumeView })));
const BrowserView = lazy(() => import("./views/BrowserView").then((module) => ({ default: module.BrowserView })));
const SettingsView = lazy(() => import("./views/SettingsView").then((module) => ({ default: module.SettingsView })));

const pageQuery = new URLSearchParams(window.location.search);
const isPreview = pageQuery.get("preview") === "1";
const previewScenario = pageQuery.get("scenario") || "";
const previewSearch = isPreview
  ? `?${new URLSearchParams({ preview: "1", ...(previewScenario ? { scenario: previewScenario } : {}) })}`
  : "";
const initialPreviewWorkspace = previewWorkspaceForScenario(previewWorkspace, previewScenario);

export default function App() {
  const [workspace, setWorkspace] = useState<JobsWorkspace | null>(isPreview ? initialPreviewWorkspace : null);
  const [account, setAccount] = useState<AccountSummary | null>(
    isPreview ? { email: "taylor@example.com", balance_cents: 2450 } : null,
  );
  const [resumeVersions, setResumeVersions] = useState<Record<string, ResumeVersion>>({});
  const [loading, setLoading] = useState(!isPreview && Boolean(accessToken()));
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const navigate = useNavigate();

  const refresh = useCallback(async () => {
    if (isPreview) return;
    setLoading(true);
    setError("");
    try {
      const [nextWorkspace, nextAccount] = await Promise.all([jobsApi.workspace(), jobsApi.account()]);
      setWorkspace(nextWorkspace);
      setAccount(nextAccount);
    } catch (requestError) {
      const message = requestError instanceof Error ? requestError.message : "Bluey Jobs could not load.";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!isPreview && accessToken()) void refresh();
  }, [refresh]);

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
  }, []);

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
          const result = await jobsApi.uploadResumeSource(file, profile, pageCount);
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
  }, []);

  const saveTrack = useCallback(async (track: CareerTrack) => {
    const saved = isPreview
      ? { ...track, id: track.id || `track-${Date.now()}`, updated_at_ms: Date.now() }
      : await jobsApi.saveTrack(track);
    setWorkspace((current) =>
      current
        ? { ...current, tracks: [saved, ...current.tracks.filter((item) => item.id !== saved.id)] }
        : current,
    );
    setToast(track.id ? "Career Track updated." : "Career Track started.");
  }, []);

  const deleteTrack = useCallback(async (track: CareerTrack) => {
    if (!isPreview) await jobsApi.deleteTrack(track.id);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            tracks: current.tracks.filter((item) => item.id !== track.id),
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
        const autoSubmitEligible = submissionMode === "auto_submit" && job.eligibility?.can_auto_submit === true;
        const application: JobApplication = {
          id: `application-${job.id}`,
          job_id: job.id,
          resume_version_id: resume.id,
          state: autoSubmitEligible ? "queued" : "awaiting_review",
          submission_mode: submissionMode === "auto_submit" ? "auto_submit" : "review_first",
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
        if (state === "submitted") await commitApplication(application);
        updated = await jobsApi.updateApplication(application.id, state, application.submission_mode);
      }
    }
    setWorkspace((current) =>
      current
        ? { ...current, applications: current.applications.map((item) => (item.id === updated.id ? updated : item)) }
        : current,
    );
    setToast(state === "queued" ? "Application queued." : "Application updated.");
  }, [commitApplication]);

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

  const saveIntegration = useCallback(async (integration: JobsIntegration) => {
    const saved = isPreview
      ? { ...integration, updated_at_ms: Date.now() }
      : await jobsApi.saveIntegration(integration);
    setWorkspace((current) =>
      current
        ? {
            ...current,
            integrations: [saved, ...current.integrations.filter((item) => item.provider !== saved.provider)],
          }
        : current,
    );
  }, []);

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

  const requestMailboxConnection = useCallback(async (connection: MailboxConnection) => {
    const saved = isPreview
      ? { ...connection, id: `mailbox-${Date.now()}`, status: "pending" as const, created_at_ms: Date.now(), updated_at_ms: Date.now() }
      : await jobsApi.requestMailboxConnection(connection);
    setWorkspace((current) => current ? {
      ...current,
      mailbox_connections: [saved, ...current.mailbox_connections.filter((item) => item.id !== saved.id)],
    } : current);
    setToast(`${saved.account_label} is ready for authorization.`);
    return saved;
  }, []);

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

  const queueRun = useCallback(async (application: JobApplication, runner: "local" | "cloud") => {
    if (!workspace) return;
    const job = workspace.matches.find((item) => item.id === application.job_id);
    if (!job) throw new Error("That job is no longer available.");
    if (application.state !== "queued") {
      throw new Error("Approve this application before choosing a browser runner.");
    }
    let updatedApplication = application;
    let session: BrowserSession = {
      id: "",
      runner,
      status: "queued",
      current_company: job.company,
      current_step: "Waiting to start",
      application_id: application.id,
      created_at_ms: 0,
      updated_at_ms: 0,
    };
    if (isPreview) {
      const now = Date.now();
      updatedApplication = { ...application, state: "queued", updated_at_ms: now };
      session = { ...session, id: `run-${now}`, created_at_ms: now, updated_at_ms: now };
    } else {
      const queued = await jobsApi.queueApplicationRun(application.id, runner);
      updatedApplication = queued.application;
      session = queued.browser_session;
      if (runner === "local" && queued.launch_url) window.location.assign(queued.launch_url);
    }
    setWorkspace((current) => current ? {
      ...current,
      applications: current.applications.map((item) => item.id === application.id ? updatedApplication : item),
      browser_sessions: [session, ...current.browser_sessions.filter((item) => item.id !== session.id)],
    } : current);
    setToast(runner === "local"
      ? `${job.company} is opening in Bluey Browser.`
      : `${job.company} is queued for the cloud runner.`);
  }, [workspace]);

  const queueCloudRun = useCallback(
    (application: JobApplication) => queueRun(application, "cloud"),
    [queueRun],
  );

  const queueLocalRun = useCallback(
    (application: JobApplication) => queueRun(application, "local"),
    [queueRun],
  );

  const updateBrowserSession = useCallback(async (session: BrowserSession, status: string) => {
    const next: BrowserSession = {
      ...session,
      status,
      current_step: status === "paused" ? "Paused by you" : session.current_step,
      updated_at_ms: Date.now(),
    };
    const saved = isPreview ? next : await jobsApi.saveBrowserSession(next);
    setWorkspace((current) => current ? {
      ...current,
      browser_sessions: current.browser_sessions.map((item) => item.id === saved.id ? saved : item),
    } : current);
    setToast(status === "paused" ? "Browser run paused." : "Browser run updated.");
  }, []);

  const resolveIntervention = useCallback(async (
    intervention: Intervention,
    action: string,
    resolution?: { answer?: string; remember?: boolean; scope?: string; scope_id?: string },
  ) => {
    const now = Date.now();
    const approved = action === "approve_email_otp" || action === "approve_submission";
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
    setWorkspace((current) => current ? {
      ...current,
      interventions: current.interventions.map((item) => item.id === saved.id ? saved : item),
      applications: current.applications.map((application) => application.id === saved.application_id
        ? result.application || { ...application, state: "queued", updated_at_ms: Date.now() }
        : application),
      answer_memory: result.answer_memory
        ? [result.answer_memory, ...current.answer_memory.filter((item) => item.id !== result.answer_memory?.id && !(item.key === result.answer_memory?.key && item.scope === result.answer_memory?.scope && (item.scope_id || "") === (result.answer_memory?.scope_id || "")))]
        : current.answer_memory,
      browser_sessions: current.browser_sessions.map((session) => session.application_id === saved.application_id
        ? { ...session, status: "queued", current_step: "Resuming application", updated_at_ms: Date.now() }
        : session),
    } : current);
    setToast(action === "approve_submission" ? "Submission approved. Bluey is completing the application." : action === "approve_email_otp" ? "Email code approved. Bluey is resuming." : resolution?.remember ? "Answer saved. Bluey is resuming." : "Answer sent. Bluey is resuming.");
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
    >
      {error && <div className="global-message error"><AlertCircle size={16} />{error}</div>}
      {toast && <div className="toast" role="status">{toast}</div>}
      <Suspense fallback={<div className="view-loading"><LoaderCircle className="spin" size={20} /><span>Opening view...</span></div>}>
      <Routes>
        <Route index element={<Navigate to={`matches${previewSearch}`} replace />} />
        <Route
          path="matches"
          element={
            <MatchesView
              workspace={workspace}
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
              resumeVersions={resumeVersions}
              onUpdate={updateApplication}
              onCommit={commitApplication}
              onLoadResume={loadResumeVersion}
              onResolveIntervention={resolveIntervention}
              onSaveCandidateEvent={saveCandidateEvent}
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
          path="browser"
          element={
            <BrowserView
              workspace={workspace}
              onQueueLocal={queueLocalRun}
              onQueueCloud={queueCloudRun}
              onUpdateSession={updateBrowserSession}
              onResolveIntervention={resolveIntervention}
            />
          }
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
              onSaveIntegration={saveIntegration}
              onCreateIdentity={createApplicationIdentity}
              onUpdateIdentity={updateApplicationIdentity}
              onVerifyIdentity={verifyApplicationIdentity}
              onResendIdentity={resendApplicationIdentity}
              onDeleteIdentity={deleteApplicationIdentity}
              onRequestMailbox={requestMailboxConnection}
              onDeleteMailbox={deleteMailboxConnection}
              onSaveAnswerMemory={saveAnswerMemory}
              onDeleteAnswerMemory={deleteAnswerMemory}
            />
          }
        />
        <Route path="*" element={<Navigate to={`matches${previewSearch}`} replace />} />
      </Routes>
      </Suspense>
    </AppShell>
  );
}

function normalizeAnswerKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}
