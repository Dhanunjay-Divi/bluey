import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import {
  BriefcaseBusiness,
  CheckCircle2,
  History,
  LoaderCircle,
  RotateCcw,
  TriangleAlert,
  X,
} from "lucide-react";
import { invoke } from "../lib/tauri";
import {
  INITIAL_JOBS_HANDOFF_STATE,
  jobsHandoffReducer,
  jobsHandoffTargetLabel,
  normalizeJobsHandoffPayload,
  safeJobsHandoffRetryError,
  type JobsHandoffImportPayload,
  type JobsHandoffNotice,
  type JobsHandoffState,
} from "./jobsHandoffState";

interface JobsHandoffContextValue {
  state: JobsHandoffState;
  dismiss: () => void;
}

const JobsHandoffContext = createContext<JobsHandoffContextValue | null>(null);

export function JobsHandoffProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(jobsHandoffReducer, INITIAL_JOBS_HANDOFF_STATE);
  const navigate = useNavigate();
  const eventSequence = useRef(0);

  useEffect(() => {
    let disposed = false;
    let removeListener: (() => void) | null = null;

    const receive = (value: unknown) => {
      const payload = normalizeJobsHandoffPayload(value);
      eventSequence.current += 1;
      dispatch({ type: "received", id: eventSequence.current, payload });
      navigate("/coach", { replace: true });
    };

    void listen<JobsHandoffImportPayload>("jobs_handoff_import", (event) => {
      receive(event.payload);
    })
      .then(async (remove) => {
        if (disposed) {
          remove();
        } else {
          removeListener = remove;
          try {
            const stored = await invoke<JobsHandoffImportPayload | null>("jobs_handoff_frontend_ready");
            if (!disposed && stored) receive(stored);
            if (!disposed) await invoke<number>("resume_jobs_handoff_recovery");
          } catch (error) {
            console.warn("failed to resume saved jobs handoffs", error);
          }
        }
      })
      .catch((error) => {
        console.warn("failed to listen for jobs handoff imports", error);
      });

    return () => {
      disposed = true;
      removeListener?.();
    };
  }, [navigate]);

  const value = useMemo<JobsHandoffContextValue>(() => ({
    state,
    dismiss: () => {
      const resultId = state.notice?.payload.result_id;
      dispatch({ type: "dismissed" });
      if (resultId) {
        void invoke("acknowledge_jobs_handoff_result", { resultId }).catch((error) => {
          console.warn("failed to acknowledge jobs handoff result", error);
        });
      }
    },
  }), [state]);

  return <JobsHandoffContext.Provider value={value}>{children}</JobsHandoffContext.Provider>;
}

export function useJobsHandoff(): JobsHandoffContextValue {
  const value = useContext(JobsHandoffContext);
  if (!value) throw new Error("useJobsHandoff must be used inside JobsHandoffProvider");
  return value;
}

export function JobsHandoffBanner({
  notice,
  profileState,
  onDismiss,
}: {
  notice: JobsHandoffNotice;
  profileState: "loading" | "ready" | "error";
  onDismiss: () => void;
}) {
  const { payload } = notice;
  const target = jobsHandoffTargetLabel(payload);
  const [retryState, setRetryState] = useState<"idle" | "retrying" | "waiting">("idle");
  const [retryError, setRetryError] = useState<string | null>(null);
  const retryTimeoutRef = useRef<number | null>(null);
  const noticeIdRef = useRef(notice.id);
  const mountedRef = useRef(true);
  noticeIdRef.current = notice.id;

  useEffect(() => {
    if (retryTimeoutRef.current !== null) window.clearTimeout(retryTimeoutRef.current);
    retryTimeoutRef.current = null;
    setRetryState("idle");
    setRetryError(null);
  }, [notice.id]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (retryTimeoutRef.current !== null) window.clearTimeout(retryTimeoutRef.current);
    };
  }, []);

  async function retrySavedHandoff() {
    if (retryState !== "idle") return;
    const requestedNoticeId = notice.id;
    setRetryState("retrying");
    setRetryError(null);
    try {
      const attempted = await invoke<number>("retry_jobs_handoff_import");
      if (!mountedRef.current || noticeIdRef.current !== requestedNoticeId) return;
      if (!Number.isSafeInteger(attempted) || attempted < 0) {
        throw new Error("Bluey returned an unreadable retry result.");
      }
      if (attempted === 0) {
        setRetryState("idle");
        setRetryError("No saved handoff was available to retry. Return to Bluey Jobs and open it again.");
        return;
      }
      setRetryState("waiting");
      retryTimeoutRef.current = window.setTimeout(() => {
        if (!mountedRef.current || noticeIdRef.current !== requestedNoticeId) return;
        setRetryState("idle");
        setRetryError("Bluey did not report a completed retry yet. You can safely try the saved handoff again.");
      }, 15_000);
    } catch (error) {
      if (!mountedRef.current || noticeIdRef.current !== requestedNoticeId) return;
      setRetryState("idle");
      setRetryError(safeJobsHandoffRetryError(error));
    }
  }

  if (payload.success) {
    return (
      <section
        className="relative rounded-xl border border-emerald-400/30 bg-emerald-400/10 px-4 py-4 pr-12 text-emerald-50"
        role="status"
        aria-label="Job application context imported"
      >
        <div className="flex items-start gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-emerald-400/15 text-emerald-200">
            <CheckCircle2 className="h-5 w-5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="font-semibold text-emerald-50">Verified submitted application context linked</h2>
              {payload.recovered ? (
                <span className="inline-flex items-center gap-1 rounded-full border border-emerald-300/25 bg-emerald-300/10 px-2 py-0.5 text-[11px] font-semibold text-emerald-100">
                  <History className="h-3 w-3" aria-hidden="true" />
                  Recovered after restart
                </span>
              ) : null}
            </div>
            {target ? (
              <p className="mt-1 inline-flex items-center gap-1.5 text-sm font-medium text-emerald-100">
                <BriefcaseBusiness className="h-3.5 w-3.5" aria-hidden="true" />
                {target}
              </p>
            ) : null}
            <p className="mt-2 max-w-3xl text-sm leading-6 text-emerald-100/80">
              {payload.recovered
                ? "Bluey recovered the interrupted one-time handoff and safely linked its receipt-backed context."
                : "Bluey safely redeemed the one-time handoff and linked its receipt-backed context."}
              {" "}{profileState === "ready"
                ? "The Coach profile has reloaded with the imported role, company, and verified source references."
                : profileState === "error"
                  ? "The verified context is linked, but Coach could not reload it. Use Retry loading below."
                  : "Coach is reloading the imported role, company, and verified source references now."}
            </p>
          </div>
        </div>
        <DismissButton onDismiss={onDismiss} tone="success" />
      </section>
    );
  }

  return (
    <section
      className="relative rounded-xl border border-red-500/35 bg-red-950/35 px-4 py-4 pr-12 text-red-100"
      role="alert"
      aria-label="Job application context import failed"
    >
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-red-500/10 text-red-300">
          <TriangleAlert className="h-5 w-5" aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h2 className="font-semibold text-red-100">Job context wasn’t linked</h2>
          <p className="mt-1 break-words text-sm leading-6 text-red-100/80">
            {payload.error || "Bluey could not import this job handoff. Return to Jobs and create a new handoff."}
          </p>
          {retryState === "waiting" ? (
            <p className="mt-2 text-xs leading-5 text-red-100/75" role="status">
              Retry requested. Waiting for Bluey to report the saved handoff result…
            </p>
          ) : null}
          {retryError ? (
            <p className="mt-2 rounded-md border border-red-400/20 bg-red-500/10 px-3 py-2 text-xs leading-5 text-red-50" role="alert">
              {retryError}
            </p>
          ) : null}
          <button
            type="button"
            onClick={() => void retrySavedHandoff()}
            disabled={retryState !== "idle"}
            className="mt-3 inline-flex min-h-9 items-center gap-2 rounded-md border border-red-300/30 bg-red-300/10 px-3 text-sm font-semibold text-red-50 hover:bg-red-300/15 disabled:cursor-wait disabled:opacity-55"
          >
            {retryState === "idle" ? (
              <RotateCcw className="h-4 w-4" aria-hidden="true" />
            ) : (
              <LoaderCircle className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            )}
            {retryState === "idle" ? "Retry saved handoff" : "Retrying saved handoff…"}
          </button>
        </div>
      </div>
      <DismissButton onDismiss={onDismiss} tone="error" />
    </section>
  );
}

function DismissButton({
  onDismiss,
  tone,
}: {
  onDismiss: () => void;
  tone: "success" | "error";
}) {
  return (
    <button
      type="button"
      onClick={onDismiss}
      aria-label="Dismiss job handoff message"
      className={`absolute right-3 top-3 grid h-8 w-8 place-items-center rounded-md ${
        tone === "success"
          ? "text-emerald-100/70 hover:bg-emerald-300/10 hover:text-emerald-50"
          : "text-red-100/70 hover:bg-red-400/10 hover:text-red-50"
      }`}
    >
      <X className="h-4 w-4" aria-hidden="true" />
    </button>
  );
}
