import { parseLocalRunClaim } from "./local-capabilities.js";
import {
  jobsApiOrigin,
  type LocalRunDelivery,
  type StartRunRequest,
} from "./local-run-contracts.js";
import { LocalBrowserError } from "./local-failure.js";
import { parseBlueyJobsProtocol } from "./protocol.js";
import type { AdmissionBlockReason } from "./run-admission.js";
import { ProtocolCommandSingleFlight } from "./protocol-command-single-flight.js";

const protocolCommands = new ProtocolCommandSingleFlight();

export interface ProtocolActiveRun {
  request: StartRunRequest;
  delivery: LocalRunDelivery;
}

export interface LocalProtocolDependencies {
  showController(): void;
  isOnline(): boolean;
  admissionDecision(): { allowed: true } | { allowed: false; reason: AdmissionBlockReason };
  showPaused(
    reason: "manual" | "offline" | "unavailable" | "stop_requested" | "device_unavailable",
    run?: ProtocolActiveRun,
  ): void;
  showPreparing(): void;
  showResuming(run: ProtocolActiveRun): void;
  activeRun(runId: string): ProtocolActiveRun | undefined;
  execute(request: StartRunRequest, delivery: LocalRunDelivery, resume: boolean): Promise<void>;
  handleExecutionFailure(
    request: StartRunRequest,
    delivery: LocalRunDelivery,
    error: unknown,
  ): Promise<void>;
  handleUnexpected(error: unknown): void;
}

export function handleLocalProtocol(
  rawUrl: string,
  dependencies: LocalProtocolDependencies,
): Promise<void> {
  return protocolCommands.run(
    rawUrl,
    () => handleLocalProtocolSingleFlight(rawUrl, dependencies),
    () => {
      dependencies.handleUnexpected(new LocalBrowserError("launch_expired"));
    },
  );
}

async function handleLocalProtocolSingleFlight(
  rawUrl: string,
  dependencies: LocalProtocolDependencies,
): Promise<void> {
  try {
    const command = parseBlueyJobsProtocol(rawUrl);
    dependencies.showController();
    if (command.action === "open") return;
    if (command.action === "run") {
      if (!dependencies.isOnline()) {
        dependencies.showPaused("offline");
        return;
      }
      const decision = dependencies.admissionDecision();
      if (!decision.allowed) {
        dependencies.showPaused(decision.reason === "paused" ? "manual" : decision.reason);
        return;
      }
      dependencies.showPreparing();
      const apiOrigin = jobsApiOrigin();
      const response = await fetch(
        `${apiOrigin}/api/jobs/local-runs/${encodeURIComponent(command.runId)}/claim`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ ticket: command.ticket }),
        },
      );
      if (!response.ok) {
        if (response.status === 401 || response.status === 403) {
          dependencies.showPaused("unavailable");
          return;
        }
        throw new LocalBrowserError("launch_expired");
      }
      let claimed: ReturnType<typeof parseLocalRunClaim<StartRunRequest>>;
      try {
        claimed = parseLocalRunClaim<StartRunRequest>(await response.json(), command.runId);
      } catch {
        throw new LocalBrowserError("launch_expired");
      }
      const delivery = { apiOrigin, capabilities: claimed.capabilities };
      try {
        await dependencies.execute(claimed.request, delivery, false);
      } catch (error) {
        await dependencies.handleExecutionFailure(claimed.request, delivery, error);
      }
      return;
    }

    const active = dependencies.activeRun(command.runId);
    if (!active || active.delivery.capabilities.resume !== command.capability) {
      throw new LocalBrowserError("run_not_active");
    }
    if (!dependencies.isOnline()) {
      dependencies.showPaused("offline", active);
      return;
    }
    const decision = dependencies.admissionDecision();
    if (!decision.allowed) {
      dependencies.showPaused(
        decision.reason === "paused" ? "manual" : decision.reason,
        active,
      );
      return;
    }
    dependencies.showResuming(active);
    try {
      await dependencies.execute(active.request, active.delivery, true);
    } catch (error) {
      await dependencies.handleExecutionFailure(active.request, active.delivery, error);
    }
  } catch (error) {
    dependencies.handleUnexpected(error);
  }
}
