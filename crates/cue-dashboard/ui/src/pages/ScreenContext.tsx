import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  Clock3,
  Eye,
  FileImage,
  HardDrive,
  ImageOff,
  LoaderCircle,
  Maximize2,
  MousePointer2,
  Paperclip,
  Scan,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { invoke } from "../lib/tauri";
import {
  INITIAL_SCREENSHOT_FLOW_STATE,
  MAX_SCREENSHOT_TITLE_CHARS,
  captureKindLabel,
  classifyScreenshotError,
  formatScreenshotBytes,
  isWindowsUserAgent,
  normalizeScreenshotTitle,
  screenshotFlowReducer,
  unicodeCharCount,
  validateScreenshotTitle,
  type ScreenshotCaptureRequest,
  type ScreenshotContextAttachReceipt,
  type ScreenshotFlowError,
  type ScreenshotFlowState,
  type ScreenshotPreview,
} from "./screenshotPreviewState";

type ImageState = "loading" | "ready" | "error";

const isWindows = typeof navigator !== "undefined" && isWindowsUserAgent(navigator.userAgent);

export function ScreenContext() {
  const [state, dispatch] = useReducer(screenshotFlowReducer, INITIAL_SCREENSHOT_FLOW_STATE);
  const [imageState, setImageState] = useState<ImageState>("loading");
  const [titleTouched, setTitleTouched] = useState(false);
  const mountedRef = useRef(true);
  const stateRef = useRef<ScreenshotFlowState>(state);
  stateRef.current = state;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      const latest = stateRef.current;
      const ambiguousAttach =
        latest.error?.operation === "attach" && latest.error.kind === "ambiguous";
      if (latest.phase === "preview" && latest.preview && !ambiguousAttach) {
        void discardPreview(latest.preview.operation_id).catch((error) => {
          console.warn("failed to discard screenshot preview after leaving page", error);
        });
      }
    };
  }, []);

  useEffect(() => {
    setImageState("loading");
    setTitleTouched(false);
  }, [state.preview?.path]);

  const previewUrl = useMemo(() => {
    if (!state.preview) return null;
    try {
      return convertFileSrc(state.preview.path);
    } catch {
      return null;
    }
  }, [state.preview]);
  const titleError = validateScreenshotTitle(state.title);
  const showTitleError = Boolean(titleError && (titleTouched || state.title.length === 0));
  const operationBusy = state.phase === "attaching" || state.phase === "discarding";
  const attachmentNeedsReconciliation =
    state.error?.operation === "attach" && state.error.kind === "ambiguous";

  async function capture(request: ScreenshotCaptureRequest) {
    if (state.phase !== "idle" && state.phase !== "attached") return;
    if (request === "selection" && isWindows) return;
    dispatch({ type: "capture_started", request });
    try {
      const preview = await invoke<ScreenshotPreview>("capture_screenshot_preview", {
        fullScreen: request === "full_screen",
      });
      assertScreenshotPreview(preview);
      if (!mountedRef.current) {
        await discardPreview(preview.operation_id).catch((discardError) => {
          console.warn("failed to discard screenshot captured after leaving page", discardError);
        });
        return;
      }
      dispatch({ type: "capture_succeeded", preview });
    } catch (error) {
      if (!mountedRef.current) return;
      dispatch({
        type: "operation_failed",
        error: classifyScreenshotError("capture", error),
      });
    }
  }

  async function attach() {
    if (state.phase !== "preview" || !state.preview) return;
    setTitleTouched(true);
    if (titleError || imageState !== "ready") return;

    const preview = state.preview;
    dispatch({ type: "attach_started" });
    try {
      const receipt = await invoke<ScreenshotContextAttachReceipt>("attach_screenshot_preview", {
        operationId: preview.operation_id,
        title: normalizeScreenshotTitle(state.title),
      });
      assertScreenshotAttachReceipt(receipt, preview.operation_id);
      if (!mountedRef.current) return;
      dispatch({ type: "attach_succeeded", receipt });
    } catch (error) {
      if (!mountedRef.current) return;
      dispatch({
        type: "operation_failed",
        error: classifyScreenshotError("attach", error),
      });
    }
  }

  async function discard() {
    if (state.phase !== "preview" || !state.preview) return;
    const preview = state.preview;
    dispatch({ type: "discard_started" });
    try {
      await discardPreview(preview.operation_id);
      if (!mountedRef.current) return;
      dispatch({ type: "discard_succeeded" });
    } catch (error) {
      if (!mountedRef.current) return;
      dispatch({
        type: "operation_failed",
        error: classifyScreenshotError("discard", error),
      });
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <header className="rounded-xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-2xl shadow-black/20">
        <div className="inline-flex items-center gap-2 rounded-full border border-cyan-400/25 bg-cyan-400/10 px-3 py-1 text-xs font-semibold text-cyan-200">
          <ShieldCheck className="h-3.5 w-3.5" aria-hidden="true" />
          Consent-first screen context
        </div>
        <h1 className="mt-3 text-3xl font-semibold tracking-tight text-zinc-50">Share only the screen context you choose</h1>
        <p className="mt-2 max-w-3xl text-sm leading-6 text-zinc-400">
          Capture creates a private local preview first. Bluey does not attach or upload it automatically;
          you review the image and title before explicitly adding it to session context.
        </p>
      </header>

      <section aria-labelledby="before-capture-heading" className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
        <div>
          <h2 id="before-capture-heading" className="text-lg font-semibold text-zinc-100">Before you capture</h2>
          <p className="mt-1 text-sm leading-6 text-zinc-500">Source, destination, and retention stay visible before any screen picker opens.</p>
        </div>
        <div className="mt-4 grid gap-3 md:grid-cols-3">
          <ConsentCard
            icon={<MousePointer2 className="h-4 w-4" aria-hidden="true" />}
            title="Source"
            body={isWindows
              ? "This Windows build captures the full display after you deliberately choose it. Bluey does not continuously record from this page."
              : "Choose a region or window, or deliberately capture the full display. Bluey does not continuously record from this page."}
          />
          <ConsentCard
            icon={<HardDrive className="h-4 w-4" aria-hidden="true" />}
            title="Destination"
            body="The first copy is a private local preview. It is not session context and is not uploaded merely because you captured it. Attached screenshots remain local-only and are excluded from background cloud sync."
          />
          <ConsentCard
            icon={<Trash2 className="h-4 w-4" aria-hidden="true" />}
            title="Retention"
            body="Attach copies the reviewed bytes into retained local session context, then removes the preview only after a trustworthy receipt. Discard removes only the preview and retry record; it never removes a possibly attached retained copy."
          />
        </div>
      </section>

      {state.notice && state.phase === "idle" ? (
        <div className="flex items-start gap-3 rounded-lg border border-emerald-400/25 bg-emerald-400/10 px-4 py-3 text-sm text-emerald-100" role="status">
          <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
          <span>{state.notice}</span>
        </div>
      ) : null}

      {state.error ? <ScreenshotErrorNotice error={state.error} /> : null}

      {state.phase === "idle" || state.phase === "capturing" ? (
        <CaptureChooser state={state} selectionUnavailable={isWindows} onCapture={capture} />
      ) : null}

      {state.preview ? (
        <section aria-labelledby="review-heading" className="grid gap-5 rounded-xl border border-zinc-800 bg-zinc-900 p-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <div className="min-w-0">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <p className="text-xs font-semibold uppercase tracking-wide text-cyan-300">Private local preview</p>
                <h2 id="review-heading" className="mt-1 text-xl font-semibold text-zinc-100">Review before attaching</h2>
              </div>
              <span className="rounded-full border border-amber-400/25 bg-amber-400/10 px-3 py-1 text-xs font-semibold text-amber-200">
                Not attached
              </span>
            </div>

            <div className="relative mt-4 grid min-h-72 place-items-center overflow-hidden rounded-lg border border-zinc-700 bg-black/30">
              {previewUrl ? (
                <img
                  src={previewUrl}
                  alt={`${captureKindLabel(state.preview.capture_kind)} screenshot preview. It has not been attached to the session.`}
                  onLoad={() => setImageState("ready")}
                  onError={() => setImageState("error")}
                  className={`max-h-[520px] w-full object-contain ${imageState === "error" ? "invisible" : "visible"}`}
                />
              ) : null}
              {imageState === "loading" && previewUrl ? (
                <div className="absolute inset-0 grid place-items-center bg-zinc-950/50" role="status">
                  <span className="inline-flex items-center gap-2 text-sm text-zinc-300">
                    <LoaderCircle className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
                    Loading private preview…
                  </span>
                </div>
              ) : null}
              {imageState === "error" || !previewUrl ? (
                <div className="absolute inset-0 grid place-items-center p-6 text-center" role="alert">
                  <div>
                    <ImageOff className="mx-auto h-7 w-7 text-red-300" aria-hidden="true" />
                    <p className="mt-3 text-sm font-semibold text-zinc-200">Preview could not be displayed</p>
                    <p className="mt-1 max-w-sm text-xs leading-5 text-zinc-500">
                      The local file has not been attached. Discard it and capture again; Bluey will not attach an image you could not review.
                    </p>
                  </div>
                </div>
              ) : null}
            </div>

            <div className="mt-3 flex flex-wrap gap-x-5 gap-y-2 text-xs text-zinc-500">
              <PreviewMetadata icon={<Scan className="h-3.5 w-3.5" aria-hidden="true" />} label={captureKindLabel(state.preview.capture_kind)} />
              <PreviewMetadata icon={<FileImage className="h-3.5 w-3.5" aria-hidden="true" />} label={formatScreenshotBytes(state.preview.file_size_bytes)} />
              <PreviewMetadata icon={<Clock3 className="h-3.5 w-3.5" aria-hidden="true" />} label={formatCreatedAt(state.preview.created_at)} />
            </div>
          </div>

          <div className="flex min-w-0 flex-col rounded-lg border border-zinc-800 bg-zinc-950 p-4">
            <label htmlFor="screenshot-title" className="text-sm font-semibold text-zinc-200">Context title</label>
            <p id="screenshot-title-help" className="mt-1 text-xs leading-5 text-zinc-600">
              Give the current session a clear, specific label for this image.
            </p>
            <input
              id="screenshot-title"
              value={state.title}
              disabled={operationBusy}
              onChange={(event) => {
                setTitleTouched(true);
                dispatch({ type: "title_changed", title: event.target.value });
              }}
              onBlur={() => setTitleTouched(true)}
              aria-invalid={showTitleError}
              aria-describedby="screenshot-title-help screenshot-title-status"
              className={`mt-3 w-full rounded-md border bg-zinc-900 px-3 py-2.5 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 disabled:opacity-60 ${
                showTitleError ? "border-red-400 focus:border-red-300" : "border-zinc-700 focus:border-blue-400"
              }`}
            />
            <div id="screenshot-title-status" className="mt-1.5 flex items-start justify-between gap-3 text-xs">
              <span className={showTitleError ? "text-red-300" : "text-zinc-600"}>
                {showTitleError ? titleError : "Required before attaching"}
              </span>
              <span className={`shrink-0 tabular-nums ${unicodeCharCount(normalizeScreenshotTitle(state.title)) > MAX_SCREENSHOT_TITLE_CHARS ? "font-semibold text-red-300" : "text-zinc-600"}`}>
                {unicodeCharCount(normalizeScreenshotTitle(state.title))}/{MAX_SCREENSHOT_TITLE_CHARS}
              </span>
            </div>

            <div className="mt-5 rounded-md border border-cyan-400/15 bg-cyan-400/5 p-3 text-xs leading-5 text-zinc-400">
              <div className="flex items-center gap-2 font-semibold text-cyan-100">
                <Eye className="h-3.5 w-3.5" aria-hidden="true" />
                What Attach changes
              </div>
              <p className="mt-1.5">
                Attach copies these exact reviewed bytes into Bluey’s private retained captures and adds them to the capture-bound session. The screenshot stays local-only and is excluded from background cloud sync. Only when you explicitly use this image in an Answer request may Bluey send it once to your configured vision provider; later answers use saved text memory unless you attach or capture again.
              </p>
            </div>

            {imageState !== "ready" ? (
              <p className="mt-3 text-xs leading-5 text-amber-200">
                Attach stays unavailable until the image is visible for review.
              </p>
            ) : null}

            <div className="mt-auto grid gap-2 pt-5">
              <button
                type="button"
                onClick={() => void attach()}
                disabled={operationBusy || Boolean(titleError) || imageState !== "ready"}
                className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md bg-blue-500 px-4 text-sm font-semibold text-white hover:bg-blue-400 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {state.phase === "attaching" ? (
                  <LoaderCircle className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
                ) : (
                  <Paperclip className="h-4 w-4" aria-hidden="true" />
                )}
                {state.phase === "attaching" ? "Attaching…" : "Attach to current session"}
              </button>
              <button
                type="button"
                onClick={() => void discard()}
                disabled={operationBusy || attachmentNeedsReconciliation}
                className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border border-red-500/30 bg-red-500/5 px-4 text-sm font-semibold text-red-200 hover:bg-red-500/10 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {state.phase === "discarding" ? (
                  <LoaderCircle className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
                ) : (
                  <Trash2 className="h-4 w-4" aria-hidden="true" />
                )}
                {state.phase === "discarding"
                  ? "Discarding…"
                  : attachmentNeedsReconciliation
                    ? "Keep for safe retry"
                    : "Discard preview"}
              </button>
            </div>
          </div>
        </section>
      ) : null}

      {state.phase === "attached" ? (
        <AttachedSuccess receipt={state.attached_receipt} onCaptureAnother={() => dispatch({ type: "reset" })} />
      ) : null}

      <div className="sr-only" aria-live="polite" role="status">
        {screenReaderStatus(state, imageState)}
      </div>
    </div>
  );
}

function CaptureChooser({
  state,
  selectionUnavailable,
  onCapture,
}: {
  state: ScreenshotFlowState;
  selectionUnavailable: boolean;
  onCapture: (request: ScreenshotCaptureRequest) => Promise<void>;
}) {
  const capturing = state.phase === "capturing";
  return (
    <section aria-labelledby="capture-heading" className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
      <div>
        <h2 id="capture-heading" className="text-lg font-semibold text-zinc-100">Choose what to capture</h2>
        <p className="mt-1 text-sm leading-6 text-zinc-500">
          Nothing is captured until you activate one of these controls. When available, the selection picker may temporarily move focus outside Bluey.
        </p>
      </div>
      <div className="mt-4 grid gap-3 md:grid-cols-2">
        <button
          type="button"
          onClick={() => void onCapture("selection")}
          disabled={capturing || selectionUnavailable}
          className="group rounded-lg border border-zinc-700 bg-zinc-950 p-4 text-left hover:border-cyan-400/45 hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-55"
        >
          <span className="flex items-start justify-between gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-md border border-cyan-400/20 bg-cyan-400/10 text-cyan-200">
              {capturing && state.capture_request === "selection" ? (
                <LoaderCircle className="h-5 w-5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
              ) : (
                <Scan className="h-5 w-5" aria-hidden="true" />
              )}
            </span>
            <span className={`rounded-full px-2 py-1 text-[11px] font-semibold ${selectionUnavailable ? "bg-zinc-800 text-zinc-400" : "bg-emerald-400/10 text-emerald-200"}`}>
              {selectionUnavailable ? "Unavailable on Windows" : "Recommended"}
            </span>
          </span>
          <span className="mt-3 block text-base font-semibold text-zinc-100">
            {capturing && state.capture_request === "selection" ? "Waiting for selection…" : "Select region or window"}
          </span>
          <span className="mt-1 block text-sm leading-6 text-zinc-500">
            {selectionUnavailable
              ? "This Windows build currently supports reviewed full-screen capture only."
              : "Use the system picker to limit capture to the smallest useful area."}
          </span>
        </button>
        <button
          type="button"
          onClick={() => void onCapture("full_screen")}
          disabled={capturing}
          className="group rounded-lg border border-zinc-700 bg-zinc-950 p-4 text-left hover:border-amber-400/45 hover:bg-zinc-900 disabled:cursor-wait disabled:opacity-55"
        >
          <span className="grid h-10 w-10 place-items-center rounded-md border border-amber-400/20 bg-amber-400/10 text-amber-200">
            {capturing && state.capture_request === "full_screen" ? (
              <LoaderCircle className="h-5 w-5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            ) : (
              <Maximize2 className="h-5 w-5" aria-hidden="true" />
            )}
          </span>
          <span className="mt-3 block text-base font-semibold text-zinc-100">
            {capturing && state.capture_request === "full_screen" ? "Capturing full screen…" : "Full screen"}
          </span>
          <span className="mt-1 block text-sm leading-6 text-zinc-500">
            Captures the entire display, which may include notifications or unrelated private content.
          </span>
        </button>
      </div>
      {capturing ? (
        <p className="mt-4 inline-flex items-center gap-2 text-sm text-cyan-100" role="status">
          <LoaderCircle className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
          {state.capture_request === "selection" ? "Complete the system selection or cancel to return." : "Creating a private full-screen preview…"}
        </p>
      ) : null}
    </section>
  );
}

function ScreenshotErrorNotice({ error }: { error: ScreenshotFlowError }) {
  const classes = error.kind === "cancelled" || error.kind === "ambiguous"
    ? "border-amber-400/25 bg-amber-400/10 text-amber-100"
    : "border-red-500/35 bg-red-950/35 text-red-100";
  return (
    <div className={`rounded-lg border px-4 py-3 text-sm ${classes}`} role={error.kind === "cancelled" || error.kind === "ambiguous" ? "status" : "alert"}>
      <strong className="font-semibold">{error.title}</strong>
      <p className="mt-1 leading-5 opacity-85">{error.message}</p>
      {error.detail && error.detail !== error.message ? (
        <details className="mt-2 text-xs opacity-75">
          <summary className="cursor-pointer font-semibold">Technical detail</summary>
          <p className="mt-1 break-words">{error.detail}</p>
        </details>
      ) : null}
    </div>
  );
}

function AttachedSuccess({
  receipt,
  onCaptureAnother,
}: {
  receipt: ScreenshotContextAttachReceipt | null;
  onCaptureAnother: () => void;
}) {
  const artifact = receipt?.artifact;
  const title = artifact?.title?.trim() || "Screenshot";
  const isActive = receipt?.active === true;
  const confirmationLabel = isActive
    ? receipt?.already_attached
      ? "Attachment confirmed in current session"
      : "Attached to current session"
    : receipt?.already_attached
      ? "Attachment confirmed in saved session"
      : "Saved to non-active session";
  return (
    <section className="rounded-xl border border-emerald-400/30 bg-emerald-400/10 p-6" role="status">
      <div className="flex items-start gap-3">
        <div className="grid h-11 w-11 shrink-0 place-items-center rounded-full bg-emerald-400/15 text-emerald-200">
          <CheckCircle2 className="h-5 w-5" aria-hidden="true" />
        </div>
        <div className="min-w-0">
          <p className="text-xs font-semibold uppercase tracking-wide text-emerald-200">{confirmationLabel}</p>
          <h2 className="mt-1 text-xl font-semibold text-zinc-100">{title}</h2>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-300">
            Bluey confirmed the reviewed image as local-only context in {isActive ? "the current session" : "a saved session that is not currently active"}.
            It is excluded from background cloud sync and remains until you delete the context or its session.
            An explicit Answer request using this image may send the exact approved bytes once to your configured vision provider; later answers use saved text memory unless you attach or capture again.
          </p>
          {artifact?.processing_status ? (
            <p className="mt-2 text-xs text-emerald-100/75">Context status: {humanizeStatus(artifact.processing_status)}</p>
          ) : null}
          <button
            type="button"
            onClick={onCaptureAnother}
            className="mt-4 inline-flex min-h-10 items-center gap-2 rounded-md border border-emerald-300/30 bg-emerald-300/10 px-4 text-sm font-semibold text-emerald-100 hover:bg-emerald-300/15"
          >
            <Scan className="h-4 w-4" aria-hidden="true" />
            Capture another
          </button>
        </div>
      </div>
    </section>
  );
}

function ConsentCard({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return (
    <article className="rounded-lg border border-zinc-800 bg-zinc-950 p-4">
      <div className="flex items-center gap-2 text-zinc-200">
        <span className="grid h-8 w-8 place-items-center rounded-md bg-zinc-800 text-cyan-200">{icon}</span>
        <h3 className="text-sm font-semibold">{title}</h3>
      </div>
      <p className="mt-3 text-xs leading-5 text-zinc-500">{body}</p>
    </article>
  );
}

function PreviewMetadata({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-1.5">
      {icon}
      <span>{label}</span>
    </div>
  );
}

function discardPreview(operationId: string): Promise<void> {
  return invoke<void>("discard_screenshot_preview", { operationId });
}

function assertScreenshotPreview(value: ScreenshotPreview): asserts value is ScreenshotPreview {
  if (
    !value
    || typeof value !== "object"
    || typeof value.operation_id !== "string"
    || !value.operation_id
    || typeof value.path !== "string"
    || !value.path
    || typeof value.file_size_bytes !== "number"
    || typeof value.capture_kind !== "string"
    || (typeof value.created_at !== "string" && typeof value.created_at !== "number")
  ) {
    throw new Error("Bluey returned an unreadable screenshot preview.");
  }
}

function assertScreenshotAttachReceipt(
  value: ScreenshotContextAttachReceipt,
  expectedOperationId: string,
): asserts value is ScreenshotContextAttachReceipt {
  if (
    !value
    || typeof value !== "object"
    || value.operation_id !== expectedOperationId
    || typeof value.session_id !== "string"
    || !value.session_id
    || !value.artifact
    || typeof value.artifact !== "object"
    || value.artifact.id !== expectedOperationId
    || typeof value.already_attached !== "boolean"
    || typeof value.active !== "boolean"
  ) {
    throw new Error("Bluey returned an unreadable screenshot attachment receipt. Retry Attach to reconcile it safely.");
  }
}

function formatCreatedAt(value: string | number): string {
  const numeric = typeof value === "number" ? value : /^\d+$/.test(value) ? Number(value) : NaN;
  const date = new Date(Number.isFinite(numeric) ? numeric : value);
  if (Number.isNaN(date.getTime())) return "Captured just now";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function humanizeStatus(status: string): string {
  return status.replaceAll("_", " ").replace(/^./, (character) => character.toUpperCase());
}

function screenReaderStatus(state: ScreenshotFlowState, imageState: ImageState): string {
  if (state.phase === "capturing") return "Screen capture in progress.";
  if (state.phase === "attaching") return "Attaching screenshot to the current session.";
  if (state.phase === "discarding") return "Discarding the unattached screenshot preview.";
  if (state.phase === "attached") return state.notice ?? "Screenshot attachment confirmed.";
  if (state.phase === "preview" && imageState === "ready") return "Screenshot preview ready for review. It is not attached.";
  if (state.phase === "preview" && imageState === "error") return "Screenshot preview could not be displayed and cannot be attached.";
  return state.notice ?? "Ready to choose screen context.";
}
