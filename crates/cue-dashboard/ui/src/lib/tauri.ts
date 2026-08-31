import { invoke as tauriInvoke } from "@tauri-apps/api/core";

type TauriInvokeArgs = Parameters<typeof tauriInvoke>[1];

interface FrontendErrorReport {
  source: "tauri_invoke" | "window_error" | "unhandled_rejection";
  category: "invoke_rejected" | "runtime_error" | "unhandled_rejection";
  command?: string;
}

export function invoke<T = unknown>(command: string, args?: TauriInvokeArgs): Promise<T> {
  return tauriInvoke<T>(command, args).catch((error) => {
    void reportFrontendError({
      source: "tauri_invoke",
      category: "invoke_rejected",
      command,
    });
    throw error;
  });
}

export function installFrontendErrorHandlers() {
  window.addEventListener("error", () => {
    void reportFrontendError({
      source: "window_error",
      category: "runtime_error",
    });
  });

  window.addEventListener("unhandledrejection", () => {
    void reportFrontendError({
      source: "unhandled_rejection",
      category: "unhandled_rejection",
    });
  });
}

function reportFrontendError(payload: FrontendErrorReport): Promise<void> {
  return tauriInvoke<void>("report_frontend_error", { payload }).catch(() => {
    // Avoid recursive failure loops if the reporting command itself is broken.
  });
}
