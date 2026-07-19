import type { ControllerStatus, ControllerViewState } from "./controller-contract.js";

export interface SafeNotification {
  title: string;
  body: string;
  urgency: "normal" | "critical";
}

export interface NotificationSink {
  show(notification: SafeNotification): void;
}

const NOTIFICATION_COPY: Partial<Record<ControllerStatus, SafeNotification>> = {
  needs_you: {
    title: "Bluey Browser needs you",
    body: "A reviewed application is waiting for your attention.",
    urgency: "critical",
  },
  completed: {
    title: "Bluey Browser finished",
    body: "An application result is ready in Bluey Jobs.",
    urgency: "normal",
  },
  failed_unknown: {
    title: "Bluey Browser needs review",
    body: "An application stopped safely and needs review.",
    urgency: "critical",
  },
  paused: {
    title: "Bluey Browser paused",
    body: "No new local applications will start while Bluey Browser is paused.",
    urgency: "normal",
  },
};

export class ControllerNotificationCenter {
  private lastStatus?: ControllerStatus;
  private readonly lastShownAt = new Map<ControllerStatus, number>();

  constructor(
    private readonly sink: NotificationSink,
    private readonly now: () => number = Date.now,
    private readonly coalesceWindowMs = 30_000,
  ) {}

  observe(state: ControllerViewState): boolean {
    const status = state.status;
    const notification = NOTIFICATION_COPY[status];
    if (!notification) {
      this.lastStatus = status;
      return false;
    }
    const now = this.now();
    const lastAt = this.lastShownAt.get(status);
    const changed = status !== this.lastStatus;
    this.lastStatus = status;
    if (!changed || (lastAt !== undefined && now - lastAt < this.coalesceWindowMs)) return false;
    this.lastShownAt.set(status, now);
    this.sink.show({ ...notification });
    return true;
  }
}

export function notificationForStatus(status: ControllerStatus): SafeNotification | undefined {
  const notification = NOTIFICATION_COPY[status];
  return notification ? { ...notification } : undefined;
}
