import { invoke } from "./tauri";

interface NewSession {
  id: string;
}

export type NewSessionActionStage = "create" | "activate";

export class NewSessionActionError extends Error {
  readonly stage: NewSessionActionStage;
  readonly sessionId: string | null;
  readonly reason: unknown;

  constructor(stage: NewSessionActionStage, reason: unknown, sessionId: string | null = null) {
    super(stage === "create" ? "new session creation failed" : "new session activation failed");
    this.name = "NewSessionActionError";
    this.stage = stage;
    this.sessionId = sessionId;
    this.reason = reason;
  }
}

type SessionInvoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

interface StartNewSessionOptions {
  navigate: (to: string) => void;
  destination?: string | ((session: NewSession) => string);
  invokeCommand?: SessionInvoke;
}

/**
 * Create a session, make it the daemon's active session, and only then open
 * the requested route. Keeping all explicit "New session" entry points on
 * this sequence prevents /live from briefly binding to the previous session.
 */
export async function startNewSession({
  navigate,
  destination = "/live",
  invokeCommand = (command, args) => invoke(command, args),
}: StartNewSessionOptions): Promise<NewSession> {
  let session: NewSession;
  try {
    const created = await invokeCommand("create_session", { title: null });
    if (
      typeof created !== "object" ||
      created === null ||
      !("id" in created) ||
      typeof created.id !== "string" ||
      created.id.trim() === ""
    ) {
      throw new Error("create_session returned no session id");
    }
    session = { id: created.id };
  } catch (error) {
    throw new NewSessionActionError("create", error);
  }

  try {
    await invokeCommand("set_active_session", { id: session.id });
  } catch (error) {
    throw new NewSessionActionError("activate", error, session.id);
  }

  navigate(typeof destination === "function" ? destination(session) : destination);
  return session;
}

export function newSessionActionErrorMessage(error: unknown): string {
  if (error instanceof NewSessionActionError) {
    if (error.stage === "activate") {
      return "The session was created and saved, but Bluey could not start it live. Open it from Saved sessions and choose Continue live.";
    }
    return "Bluey could not create a new session. Try again.";
  }
  return "Bluey could not start a new session. Try again.";
}
