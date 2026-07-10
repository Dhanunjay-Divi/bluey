import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { AlertCircle, LoaderCircle } from "lucide-react";
import { accessToken, jobsApi, loginUrl } from "./api";
import { previewWorkspace } from "./data/preview";
import type {
  AccountSummary,
  ApplicationIdentity,
  BrowserSession,
  CareerProfile,
  CareerTrack,
  Intervention,
  JobApplication,
  JobPosting,
  JobPreferences,
  JobsIntegration,
  JobsWorkspace,
  MailboxConnection,
  ResumeVersion,
} from "./types";
import { AppShell } from "./components/AppShell";
import { Onboarding } from "./components/Onboarding";
import blueyIcon from "../../../web/assets/bluey-logo.svg";
import blueyWordmark from "../../../web/assets/bluey-wordmark.svg";

const MatchesView = lazy(() => import("./views/MatchesView").then((module) => ({ default: module.MatchesView })));
const ApplicationsView = lazy(() => import("./views/ApplicationsView").then((module) => ({ default: module.ApplicationsView })));
const ResumeView = lazy(() => import("./views/ResumeView").then((module) => ({ default: module.ResumeView })));
const BrowserView = lazy(() => import("./views/BrowserView").then((module) => ({ default: module.BrowserView })));
const SettingsView = lazy(() => import("./views/SettingsView").then((module) => ({ default: module.SettingsView })));

const isPreview = new URLSearchParams(window.location.search).get("preview") === "1";
const previewSearch = isPreview ? "?preview=1" : "";

export default function App() {
  const [workspace, setWorkspace] = useState<JobsWorkspace | null>(isPreview ? previewWorkspace : null);
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

  const saveOnboarding = useCallback(
    async (profile: CareerProfile, preferences: JobPreferences, track: CareerTrack) => {
      setError("");
      if (isPreview) {
        setWorkspace((current) =>
          current
            ? {
                ...current,
                profile,
                preferences,
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
        const [savedProfile, savedPreferences, savedTrack] = await Promise.all([
          jobsApi.saveProfile(profile),
          jobsApi.savePreferences(preferences),
          jobsApi.saveTrack(track),
        ]);
        setWorkspace((current) =>
          current
            ? {
                ...current,
                profile: savedProfile,
                preferences: savedPreferences,
                tracks: [savedTrack, ...current.tracks.filter((item) => item.id !== savedTrack.id)],
              }
            : current,
        );
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

  const savePreferences = useCallback(async (preferences: JobPreferences) => {
    const saved = isPreview ? preferences : await jobsApi.savePreferences(preferences);
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
    async (job: JobPosting) => {
      const saved = isPreview
        ? {
            ...job,
            id: `job-${Date.now()}`,
            canonical_key: `preview-${Date.now()}`,
            match_score: job.match_score || 84,
            matched_reasons: job.matched_reasons.length ? job.matched_reasons : ["Matches your active Career Track"],
            updated_at_ms: Date.now(),
            created_at_ms: Date.now(),
          }
        : await jobsApi.saveMatch(job);
      setWorkspace((current) =>
        current ? { ...current, matches: [saved, ...current.matches.filter((item) => item.id !== saved.id)] } : current,
      );
      setToast("Job added. Bluey scored it against your profile.");
      return saved;
    },
    [],
  );

  const prepareApplication = useCallback(
    async (job: JobPosting, mode: string, submissionMode: string) => {
      if (!workspace) return;
      if (isPreview) {
        const resume = previewResume(workspace, job, `resume-${job.id}-${Date.now()}`, mode);
        const autoSubmitEligible = submissionMode === "auto_submit"
          && job.match_score >= workspace.profile.auto_submit_threshold
          && job.missing_requirements.length === 0
          && !job.source.endsWith("_handoff");
        const application: JobApplication = {
          id: `application-${job.id}`,
          job_id: job.id,
          resume_version_id: resume.id,
          state: autoSubmitEligible ? "queued" : "awaiting_review",
          submission_mode: submissionMode === "auto_submit" ? "auto_submit" : "review_first",
          match_score: job.match_score,
          answers: [],
          cover_letter: "",
          receipt: {},
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
        const metering = response.application.state === "queued"
          ? await jobsApi.commitPacket(response.application.id)
          : null;
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
      setToast("Job-specific application packet is ready.");
      navigate(`/applications${previewSearch}`);
    },
    [navigate, workspace],
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
      if (["queued", "submitted"].includes(state)) await commitApplication(application);
      updated = await jobsApi.updateApplication(application.id, state, application.submission_mode);
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
        const resume = previewResume(workspace, job, id, "factual");
        setResumeVersions((current) => ({ ...current, [id]: resume }));
        return resume;
      }
      const resume = await jobsApi.resumeVersion(id);
      setResumeVersions((current) => ({ ...current, [id]: resume }));
      return resume;
    },
    [resumeVersions, workspace],
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

  const queueCloudRun = useCallback(async (application: JobApplication) => {
    if (!workspace) return;
    const job = workspace.matches.find((item) => item.id === application.job_id);
    if (!job) throw new Error("That job is no longer available.");
    let updatedApplication = application;
    let session: BrowserSession = {
      id: "",
      runner: "cloud",
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
      if (application.state === "awaiting_review") await commitApplication(application);
      updatedApplication = await jobsApi.updateApplication(application.id, "queued", application.submission_mode);
      session = await jobsApi.saveBrowserSession(session);
    }
    setWorkspace((current) => current ? {
      ...current,
      applications: current.applications.map((item) => item.id === application.id ? updatedApplication : item),
      browser_sessions: [session, ...current.browser_sessions.filter((item) => item.id !== session.id)],
    } : current);
    setToast(`${job.company} is queued for the cloud runner.`);
  }, [commitApplication, workspace]);

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

  const resolveIntervention = useCallback(async (intervention: Intervention, action: string) => {
    const saved = isPreview
      ? {
          ...intervention,
          status: action === "approve_email_otp" ? "approved" : "resolved",
          resolved_at_ms: action === "approve_email_otp" ? undefined : Date.now(),
          metadata: { ...intervention.metadata, approved_at_ms: Date.now() },
        }
      : await jobsApi.resolveIntervention(intervention.id, "resolved", action);
    setWorkspace((current) => current ? {
      ...current,
      interventions: current.interventions.map((item) => item.id === saved.id ? saved : item),
      browser_sessions: current.browser_sessions.map((session) => session.application_id === saved.application_id
        ? { ...session, status: "queued", current_step: "Resuming application", updated_at_ms: Date.now() }
        : session),
    } : current);
    setToast(action === "approve_email_otp" ? "Email code approved. Bluey is resuming." : "Browser run is ready to resume.");
  }, []);

  if (!isPreview && !accessToken()) return <AuthGate />;
  if (loading && !workspace) return <LoadingScreen />;
  if (!workspace) return <LoadError message={error} onRetry={refresh} />;
  if (!workspace.profile.onboarding_complete) {
    return <Onboarding workspace={workspace} error={error} onComplete={saveOnboarding} />;
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
            />
          }
        />
        <Route path="*" element={<Navigate to={`matches${previewSearch}`} replace />} />
      </Routes>
      </Suspense>
    </AppShell>
  );
}

function AuthGate() {
  return (
    <main className="auth-gate">
      <a className="brand-lockup" href="/" aria-label="Bluey home">
        <img className="brand-icon" src={blueyIcon} alt="" />
        <img className="brand-wordmark" src={blueyWordmark} alt="" />
        <b>jobs</b>
      </a>
      <section>
        <p className="eyebrow">ONE PROFILE. EVERY APPLICATION.</p>
        <h1>Your job search,<br /><span>already moving.</span></h1>
        <p>Sign in with Bluey to build tailored resumes, review matched roles, and keep every application in one calm workspace.</p>
        <div className="auth-actions">
          <a className="button primary" href={loginUrl()}>Sign in to Bluey</a>
          <a className="button secondary" href="/login?mode=signup&next=%2Fjobs">Create account</a>
        </div>
      </section>
      <footer>Bluey identity, balance, and privacy controls carry into Jobs.</footer>
    </main>
  );
}

function previewResume(workspace: JobsWorkspace, job: JobPosting, id: string, mode: string): ResumeVersion {
  const track = workspace.tracks.find((item) => item.id === job.track_id);
  const applicationIdentity = workspace.application_identities.find((item) => item.id === track?.application_identity_id)
    || workspace.application_identities.find((item) => item.is_default && item.verification_status === "verified");
  return {
    id,
    job_id: job.id,
    version_no: 1,
    mode: mode === "enhance" ? "enhance" : "factual",
    content: {
      target: { company: job.company, title: job.title, location: job.location },
      contact: {
        name: workspace.profile.full_name,
        email: applicationIdentity?.email || workspace.profile.email,
        phone: workspace.profile.phone,
        location: workspace.profile.current_location,
      },
      headline: workspace.profile.headline || job.title,
      summary: `${workspace.profile.summary} Focused for the ${job.title} opportunity at ${job.company}.`,
      skills: workspace.profile.skills,
      employment: workspace.profile.employment,
      education: workspace.profile.education,
      projects: workspace.profile.projects,
      certifications: workspace.profile.certifications,
    },
    diff: {
      summary: `Focused on ${job.title}`,
      skills: "Reordered for the job description",
      claims_added: [],
    },
    claim_ids: workspace.facts.filter((fact) => fact.verification_status === "confirmed").map((fact) => fact.id),
    checksum: `${job.id}-${id}`,
    created_at_ms: Date.now(),
  };
}

function LoadingScreen() {
  return <main className="center-screen"><LoaderCircle className="spin" /><p>Opening Bluey Jobs...</p></main>;
}

function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <main className="center-screen">
      <AlertCircle />
      <h1>Jobs did not open</h1>
      <p>{message || "Please try again."}</p>
      <button className="button primary" onClick={onRetry}>Try again</button>
    </main>
  );
}
