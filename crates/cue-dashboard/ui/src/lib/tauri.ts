import { invoke as tauriInvoke } from "@tauri-apps/api/core";

type TauriInvokeArgs = Parameters<typeof tauriInvoke>[1];

interface FrontendErrorReport {
  source: string;
  message: string;
  command?: string;
  url?: string;
  stack?: string;
}

export function invoke<T = unknown>(command: string, args?: TauriInvokeArgs): Promise<T> {
  return tauriInvoke<T>(command, args).catch((error) => {
    void reportFrontendError({
      source: "tauri.invoke",
      command,
      message: stringifyError(error),
      stack: stackFromError(error),
      url: window.location.href,
    });
    throw error;
  });
}

export function installFrontendErrorHandlers() {
  window.addEventListener("error", (event) => {
    void reportFrontendError({
      source: "window.error",
      message: event.message || stringifyError(event.error),
      stack: stackFromError(event.error),
      url: event.filename || window.location.href,
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    void reportFrontendError({
      source: "window.unhandledrejection",
      message: stringifyError(event.reason),
      stack: stackFromError(event.reason),
      url: window.location.href,
    });
  });
}

function reportFrontendError(payload: FrontendErrorReport): Promise<void> {
  return tauriInvoke<void>("report_frontend_error", { payload }).catch(() => {
    // Avoid recursive failure loops if the reporting command itself is broken.
  });
}

function stringifyError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

function stackFromError(error: unknown): string | undefined {
  return error instanceof Error ? error.stack : undefined;
}
