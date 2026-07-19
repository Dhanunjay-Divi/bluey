import type { ControllerViewState } from "../controller-contract.js";
import { backgroundAvailabilityDetail } from "./background-copy.js";

const elements = {
  statusCard: required<HTMLElement>("status-card"),
  statusLabel: required<HTMLElement>("status-label"),
  title: required<HTMLElement>("title"),
  detail: required<HTMLElement>("detail"),
  footnote: required<HTMLElement>("footnote"),
  identity: required<HTMLElement>("identity"),
  mode: required<HTMLElement>("mode"),
  offline: required<HTMLElement>("offline"),
  jobContext: required<HTMLElement>("job-context"),
  company: required<HTMLElement>("company"),
  role: required<HTMLElement>("role"),
  runCount: required<HTMLElement>("run-count"),
  progress: required<HTMLOListElement>("progress"),
  primary: required<HTMLButtonElement>("primary"),
  openBrowser: required<HTMLButtonElement>("open-browser"),
  pause: required<HTMLButtonElement>("pause"),
  stop: required<HTMLButtonElement>("stop"),
  openJobs: required<HTMLButtonElement>("open-jobs"),
  background: required<HTMLInputElement>("background-enabled"),
  backgroundTitle: required<HTMLElement>("background-title"),
  backgroundDetail: required<HTMLElement>("background-detail"),
  activity: required<HTMLUListElement>("activity"),
  announcer: required<HTMLElement>("announcer"),
};

let lastAnnouncement = "";

elements.primary.addEventListener("click", () => window.blueyBrowser.command("primary"));
elements.openBrowser.addEventListener("click", () => window.blueyBrowser.command("open-browser"));
elements.pause.addEventListener("click", () => window.blueyBrowser.command("toggle-pause"));
elements.stop.addEventListener("click", () => window.blueyBrowser.command("stop"));
elements.openJobs.addEventListener("click", () => window.blueyBrowser.command("open-jobs"));
elements.background.addEventListener("change", async () => {
  elements.background.disabled = true;
  const requested = elements.background.checked;
  try {
    elements.background.checked = await window.blueyBrowser.setBackgroundEnabled(requested);
  } catch {
    elements.background.checked = !requested;
  } finally {
    elements.background.disabled = false;
  }
});

window.blueyBrowser.onState(render);
void window.blueyBrowser.getState().then(render);

function render(state: ControllerViewState): void {
  elements.statusCard.dataset.status = state.status;
  elements.statusLabel.textContent = statusLabel(state.status);
  elements.title.textContent = state.title;
  elements.detail.textContent = state.detail;
  elements.footnote.textContent = state.footnote;
  elements.mode.textContent = state.modeLabel;
  elements.identity.textContent = state.identityLabel || "Separate application identity";
  elements.offline.hidden = state.online;

  const hasContext = Boolean(state.company || state.role);
  elements.jobContext.hidden = !hasContext;
  elements.company.textContent = state.company || "Current application";
  elements.role.textContent = state.role || "Reviewed role";

  elements.runCount.textContent = state.currentRunCount === 0
    ? "No open application"
    : state.currentRunCount === 1
      ? "1 open application"
      : `${state.currentRunCount} open applications`;
  renderProgress(state);
  renderActivity(state);

  elements.primary.hidden = state.primaryAction === "none";
  elements.primary.textContent = state.primaryLabel || "Continue";
  elements.openBrowser.disabled = !state.canOpenBrowser;
  elements.openBrowser.hidden = !state.canOpenBrowser;
  elements.pause.disabled = !state.canPause;
  elements.pause.textContent = state.paused ? "Resume" : "Pause";
  elements.stop.disabled = !state.canStop;
  elements.stop.hidden = !state.canStop;
  elements.background.checked = state.backgroundEnabled;
  elements.backgroundTitle.textContent = state.loginItemSupported
    ? "Start at sign-in and keep available"
    : "Keep available in the background";
  elements.backgroundDetail.textContent = backgroundAvailabilityDetail(state);

  const announcement = `${statusLabel(state.status)}. ${state.title}`;
  if (announcement !== lastAnnouncement) {
    lastAnnouncement = announcement;
    elements.announcer.setAttribute(
      "aria-live",
      state.status === "needs_you" || state.status === "failed_unknown" ? "assertive" : "polite",
    );
    elements.announcer.textContent = announcement;
  }
}

function renderProgress(state: ControllerViewState): void {
  const items = elements.progress.querySelectorAll<HTMLElement>("li[data-step]");
  items.forEach((item, index) => {
    item.className = index < state.progressStep
      ? "done"
      : index === state.progressStep
        ? state.status === "needs_you" || state.status === "failed_unknown"
          ? "attention"
          : state.status === "completed"
            ? "done"
            : state.status === "paused"
              ? ""
              : "active"
        : "";
    if (index === state.progressStep) item.setAttribute("aria-current", "step");
    else item.removeAttribute("aria-current");
  });
}

function renderActivity(state: ControllerViewState): void {
  elements.activity.replaceChildren(...state.activity.map((activity) => {
    const item = document.createElement("li");
    item.className = activity.state;
    item.textContent = activity.label;
    return item;
  }));
}

function statusLabel(status: ControllerViewState["status"]): string {
  switch (status) {
    case "needs_you": return "Needs you";
    case "failed_unknown": return "Needs review";
    case "running": return "Running";
    case "completed": return "Completed";
    case "paused": return "Paused";
    default: return "Ready";
  }
}

function required<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing controller element: ${id}`);
  return element as T;
}
