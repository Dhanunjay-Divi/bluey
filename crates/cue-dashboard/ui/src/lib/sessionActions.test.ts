import { describe, expect, it } from "vitest";
import {
  NewSessionActionError,
  newSessionActionErrorMessage,
  startNewSession,
} from "./sessionActions";

describe("startNewSession", () => {
  it("creates, activates, and only then navigates to live", async () => {
    const events: string[] = [];
    let releaseActivation!: () => void;
    const activation = new Promise<void>((resolve) => {
      releaseActivation = () => resolve();
    });

    const started = startNewSession({
      invokeCommand: async (command, args) => {
        events.push(`${command}:${JSON.stringify(args)}`);
        if (command === "create_session") return { id: "session-new" };
        await activation;
        events.push("activation:resolved");
        return undefined;
      },
      navigate: (to) => events.push(`navigate:${to}`),
    });

    await Promise.resolve();
    await Promise.resolve();
    expect(events).toEqual([
      'create_session:{"title":null}',
      'set_active_session:{"id":"session-new"}',
    ]);

    releaseActivation();
    await started;
    expect(events).toEqual([
      'create_session:{"title":null}',
      'set_active_session:{"id":"session-new"}',
      "activation:resolved",
      "navigate:/live",
    ]);
  });

  it("does not navigate when activation fails and reports that creation succeeded", async () => {
    const destinations: string[] = [];

    await expect(
      startNewSession({
        invokeCommand: async (command) => {
          if (command === "create_session") return { id: "saved-session" };
          throw new Error("active session persistence failed");
        },
        navigate: (to) => destinations.push(to),
      }),
    ).rejects.toMatchObject({
      stage: "activate",
      sessionId: "saved-session",
    });
    expect(destinations).toEqual([]);

    const error = new NewSessionActionError(
      "activate",
      new Error("internal"),
      "saved-session",
    );
    expect(newSessionActionErrorMessage(error)).toContain("created and saved");
  });

  it("does not activate or navigate when creation fails", async () => {
    const commands: string[] = [];
    const destinations: string[] = [];

    await expect(
      startNewSession({
        invokeCommand: async (command) => {
          commands.push(command);
          throw new Error("database unavailable");
        },
        navigate: (to) => destinations.push(to),
      }),
    ).rejects.toMatchObject({ stage: "create", sessionId: null });

    expect(commands).toEqual(["create_session"]);
    expect(destinations).toEqual([]);
  });
});
