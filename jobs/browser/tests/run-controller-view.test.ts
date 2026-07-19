import { describe, expect, it } from "vitest";
import type { BrowserShell } from "../src/browser-shell.js";
import type { ControllerViewState } from "../src/controller-contract.js";
import {
  RunControllerView,
  type RunDisplay,
} from "../src/run-controller-view.js";

describe("run controller focus", () => {
  it("keeps a remaining Needs-you run visible when another run completes", () => {
    let rendered: ControllerViewState | undefined;
    const remaining: RunDisplay[] = [
      { runId: "run-3", context: { company: "Employer 3", role: "Role 3" } },
      {
        runId: "run-2",
        interventionKind: "captcha",
        context: {
          company: "Employer 2",
          role: "Role 2",
          identityAvailable: true,
        },
      },
    ];
    const shell = {
      backgroundEnabled: false,
      online: true,
      update(state: ControllerViewState) {
        rendered = state;
      },
    } as BrowserShell;
    const view = new RunControllerView(shell, () => remaining);

    view.showCompleted({ runId: "run-1" }, true, false);

    expect(rendered).toMatchObject({
      status: "needs_you",
      title: "CAPTCHA needs you",
      currentRunCount: 2,
      primaryAction: "continue",
      canOpenBrowser: true,
      company: "Employer 2",
      role: "Role 2",
    });
    expect(view.current()?.runId).toBe("run-2");
  });
});
