import { useEffect, useReducer, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowRight,
  Check,
  CircleAlert,
  Loader2,
  Mic,
  Monitor,
  RotateCcw,
  Settings,
  Shield,
} from "lucide-react";
import { invoke } from "../lib/tauri";
import {
  AUDIO_READINESS_SOURCES,
  INITIAL_AUDIO_READINESS_FLOW,
  readinessIsFullyReady,
  reduceAudioReadiness,
  sourceResult,
  sourceStatusCopy,
  type AudioReadinessProbeResult,
  type AudioReadinessSource,
  type AudioReadinessState,
} from "./onboardingReadiness";

type Step = "welcome" | "authorizing" | "readiness" | "error";

interface DeepLinkResult {
  success: boolean;
  email?: string | null;
  error?: string | null;
}

export function Onboarding({ onComplete }: { onComplete?: () => void }) {
  const [step, setStep] = useState<Step>("welcome");
  const [email, setEmail] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [finishError, setFinishError] = useState<string | null>(null);
  const [finishing, setFinishing] = useState(false);
  const [readiness, dispatchReadiness] = useReducer(
    reduceAudioReadiness,
    INITIAL_AUDIO_READINESS_FLOW,
  );

  useEffect(() => {
    const unlisten = listen<DeepLinkResult>("deep_link_login", (event) => {
      const result = event.payload;
      if (result.success) {
        setEmail(result.email?.trim() || null);
        setError(null);
        setStep("readiness");
      } else {
        setError("Browser sign-in did not complete. Try the secure link again.");
        setStep("error");
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  async function startSignIn() {
    setError(null);
    setStep("authorizing");
    try {
      const url = await invoke<string>("get_signin_url");
      window.open(url, "_blank", "noopener,noreferrer");
    } catch {
      setError("Bluey could not open the secure sign-in flow. Please try again.");
      setStep("error");
    }
  }

  async function runReadinessProbe() {
    dispatchReadiness({ type: "start" });
    try {
      const result = await invoke<AudioReadinessProbeResult>("run_audio_readiness_probe");
      dispatchReadiness({ type: "succeed", result });
    } catch (probeError) {
      dispatchReadiness({ type: "fail", error: probeError });
    }
  }

  async function finish() {
    if (finishing) return;
    setFinishing(true);
    setFinishError(null);
    try {
      await invoke("complete_onboarding");
      if (onComplete) onComplete();
      else window.location.href = "/";
    } catch {
      setFinishError("Bluey could not save setup yet. Try again; your sign-in is still safe.");
      setFinishing(false);
    }
  }

  return (
    <main className="min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-6">
      <div className="w-full max-w-lg">
        {step === "welcome" && <WelcomeStep onSignIn={startSignIn} />}
        {step === "authorizing" && (
          <AuthorizingStep onCancel={() => setStep("welcome")} />
        )}
        {step === "readiness" && (
          <ReadinessStep
            email={email}
            flow={readiness}
            finishError={finishError}
            finishing={finishing}
            onProbe={runReadinessProbe}
            onFinish={finish}
          />
        )}
        {step === "error" && error && (
          <ErrorStep error={error} onRetry={startSignIn} />
        )}
      </div>
    </main>
  );
}

function WelcomeStep({ onSignIn }: { onSignIn: () => void }) {
  return (
    <div className="space-y-8">
      <BrandHeader
        title="Welcome to Bluey"
        subtitle="A fast, context-aware coach for interviews, meetings, coding, writing, and focused work."
      />

      <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-4 space-y-4">
        <Disclosure
          icon={<Shield className="h-4 w-4" />}
          title="Browser-based sign-in"
          body="You sign in in your browser. Bluey never asks you to type your account password into the desktop app."
        />
        <Disclosure
          icon={<Monitor className="h-4 w-4" />}
          title="You control visibility"
          body="Bluey is a desktop window. Hide it before sharing or recording your screen if you do not want it visible."
        />
      </div>

      <button
        type="button"
        onClick={onSignIn}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-500 hover:bg-blue-400 text-white font-medium py-2.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-300"
      >
        Sign in with browser
        <ArrowRight className="h-4 w-4" />
      </button>
      <p className="text-xs text-zinc-500 text-center">
        New to Bluey? You can create an account in the same flow.
      </p>
    </div>
  );
}

function AuthorizingStep({ onCancel }: { onCancel: () => void }) {
  return (
    <div className="space-y-6 text-center" role="status" aria-live="polite">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-blue-500/15 text-blue-400">
        <Loader2 className="h-6 w-6 animate-spin" />
      </div>
      <div className="space-y-2">
        <h1 className="text-xl font-semibold">Waiting for your browser…</h1>
        <p className="text-sm text-zinc-400 leading-relaxed">
          Complete sign-in there. Bluey will continue automatically.
        </p>
      </div>
      <button
        type="button"
        onClick={onCancel}
        className="text-sm text-blue-400 hover:text-blue-300 underline underline-offset-4"
      >
        Browser did not open? Try again
      </button>
    </div>
  );
}

function ReadinessStep({
  email,
  flow,
  finishError,
  finishing,
  onProbe,
  onFinish,
}: {
  email: string | null;
  flow: ReturnType<typeof reduceAudioReadiness>;
  finishError: string | null;
  finishing: boolean;
  onProbe: () => void;
  onFinish: () => void;
}) {
  const fullyReady = readinessIsFullyReady(flow.result);
  const hasResult = flow.result !== null;
  const running = flow.status === "running";

  return (
    <div className="space-y-6">
      <BrandHeader
        title="Check your audio"
        subtitle={email ? `Signed in as ${email}. One quick local check remains.` : "Signed in. One quick local check remains."}
        complete
      />

      <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-4 space-y-2">
        <p className="text-sm font-medium text-zinc-100">Private, disposable test</p>
        <p className="text-xs text-zinc-400 leading-relaxed">
          Bluey records a short sample from each available audio source only when you press the button.
          It does not transcribe or upload the sample, and discards the in-memory bytes before returning the result.
        </p>
      </div>

      <div className="grid gap-3 sm:grid-cols-2" aria-live="polite">
        {AUDIO_READINESS_SOURCES.map((source) => (
          <SourceCard key={source} source={source} state={sourceResult(flow.result, source)?.state ?? null} />
        ))}
      </div>

      {flow.error && (
        <p className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-200" role="alert">
          {flow.error}
        </p>
      )}

      {flow.result?.sources.some((item) => item.state === "permission_denied") && (
        <div className="flex flex-wrap gap-2">
          {flow.result.sources
            .filter((item) => item.state === "permission_denied")
            .map((item) => (
              <button
                type="button"
                key={item.source}
                onClick={() => void invoke("open_privacy_settings", { source: item.source })}
                className="inline-flex items-center gap-2 rounded-lg border border-zinc-700 px-3 py-2 text-xs text-zinc-200 hover:bg-zinc-800"
              >
                <Settings className="h-3.5 w-3.5" />
                Open {sourceLabel(item.source).toLowerCase()} settings
              </button>
            ))}
        </div>
      )}

      <button
        type="button"
        onClick={onProbe}
        disabled={running || finishing}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-500 hover:bg-blue-400 disabled:bg-zinc-700 disabled:text-zinc-400 text-white font-medium py-2.5 transition-colors"
      >
        {running ? <Loader2 className="h-4 w-4 animate-spin" /> : hasResult ? <RotateCcw className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
        {running ? "Checking microphone and system audio…" : hasResult ? "Run the check again" : "Run private audio check"}
      </button>

      {fullyReady && (
        <button
          type="button"
          onClick={onFinish}
          disabled={finishing}
          className="w-full inline-flex items-center justify-center gap-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 hover:bg-emerald-500/15 text-emerald-100 font-medium py-2.5"
        >
          {finishing ? <Loader2 className="h-4 w-4 animate-spin" /> : <Check className="h-4 w-4" />}
          Start using Bluey
        </button>
      )}

      {!fullyReady && (
        <div className="space-y-2 text-center">
          <button
            type="button"
            onClick={onFinish}
            disabled={running || finishing}
            className="text-sm text-zinc-400 hover:text-zinc-200 underline underline-offset-4 disabled:text-zinc-600"
          >
            {finishing ? "Saving setup…" : hasResult ? "Continue with limited audio" : "Set up audio later"}
          </button>
          <p className="text-xs text-zinc-500">
            Jobs, writing, and text-based Coach features remain available without audio.
          </p>
        </div>
      )}

      {finishError && <p className="text-xs text-red-300 text-center" role="alert">{finishError}</p>}
    </div>
  );
}

function SourceCard({
  source,
  state,
}: {
  source: AudioReadinessSource;
  state: AudioReadinessState | null;
}) {
  const ready = state === "ready";
  const configured = ready || state === "silent";
  const Icon = source === "microphone" ? Mic : Monitor;
  return (
    <div className={`rounded-xl border p-4 ${configured ? "border-emerald-500/30 bg-emerald-500/5" : "border-zinc-800 bg-zinc-900"}`}>
      <div className="flex items-center gap-2">
        <Icon className={`h-4 w-4 ${ready ? "text-emerald-400" : "text-zinc-400"}`} />
        <span className="text-sm font-medium">{sourceLabel(source)}</span>
      </div>
      <div className={`mt-2 flex items-center gap-1.5 text-xs ${ready ? "text-emerald-300" : state ? "text-amber-300" : "text-zinc-500"}`}>
        {state && state !== "ready" && <CircleAlert className="h-3.5 w-3.5" />}
        {ready && <Check className="h-3.5 w-3.5" />}
        {state ? sourceStatusCopy(state) : "Not checked yet"}
      </div>
    </div>
  );
}

function ErrorStep({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <div className="space-y-6 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-red-500/15 text-red-400">
        <CircleAlert className="h-6 w-6" />
      </div>
      <div className="space-y-2">
        <h1 className="text-xl font-semibold">Sign-in did not complete</h1>
        <p className="text-sm text-zinc-400 leading-relaxed">{error}</p>
      </div>
      <button
        type="button"
        onClick={onRetry}
        className="w-full rounded-lg bg-blue-500 hover:bg-blue-400 text-white font-medium py-2.5"
      >
        Try again
      </button>
    </div>
  );
}

function BrandHeader({
  title,
  subtitle,
  complete = false,
}: {
  title: string;
  subtitle: string;
  complete?: boolean;
}) {
  return (
    <div className="text-center space-y-3">
      <div className={`inline-flex h-12 w-12 items-center justify-center rounded-2xl ${complete ? "bg-emerald-500/15 text-emerald-400" : "bg-blue-500/15 text-blue-400"}`}>
        {complete ? <Check className="h-6 w-6" /> : <BlueyMark />}
      </div>
      <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
      <p className="text-sm text-zinc-400 leading-relaxed">{subtitle}</p>
    </div>
  );
}

function Disclosure({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
  return (
    <div className="flex items-start gap-3">
      <span className="text-zinc-400 mt-0.5 shrink-0">{icon}</span>
      <p className="text-xs text-zinc-300 leading-relaxed">
        <span className="font-medium text-zinc-100">{title}.</span> {body}
      </p>
    </div>
  );
}

function sourceLabel(source: AudioReadinessSource): string {
  return source === "microphone" ? "Microphone" : "System audio";
}

function BlueyMark() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="22"
      height="22"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M8 13.5c1.2 1.2 2.6 1.8 4 1.8s2.8-.6 4-1.8" />
      <circle cx="9" cy="10" r="0.8" fill="currentColor" />
      <circle cx="15" cy="10" r="0.8" fill="currentColor" />
    </svg>
  );
}
