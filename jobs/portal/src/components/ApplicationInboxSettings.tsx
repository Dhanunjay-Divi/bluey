import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  CalendarDays,
  ChevronRight,
  Mail,
  MailCheck,
  Plus,
} from "lucide-react";
import type {
  JobsWorkspace,
  MailboxConnection,
  MailboxMessage,
  MailboxProviderAvailability,
  MailboxSyncState,
} from "../types";
import { relativeTime } from "../lib/format";
import { ConfirmDialog, Dialog } from "./Dialog";

interface Props {
  workspace: JobsWorkspace;
  onMailboxProviders(): Promise<MailboxProviderAvailability[]>;
  onConnectMailbox(provider: MailboxConnection["provider"]): Promise<void>;
  onAuthorizeMailboxCommunication(connection: MailboxConnection): Promise<void>;
  onMailboxSyncState(connection: MailboxConnection): Promise<MailboxSyncState>;
  onMailboxMessages(connectionId?: string): Promise<MailboxMessage[]>;
  onSyncMailbox(connection: MailboxConnection): Promise<MailboxSyncState>;
  onDeleteMailbox(connection: MailboxConnection): Promise<void>;
  onError(message: string): void;
}

export function ApplicationInboxSettings({
  workspace,
  onMailboxProviders,
  onConnectMailbox,
  onAuthorizeMailboxCommunication,
  onMailboxSyncState,
  onMailboxMessages,
  onSyncMailbox,
  onDeleteMailbox,
  onError,
}: Props) {
  const [mailboxOpen, setMailboxOpen] = useState(false);
  const [disconnectingMailbox, setDisconnectingMailbox] = useState<MailboxConnection | null>(null);
  const [providers, setProviders] = useState<MailboxProviderAvailability[]>([]);
  const [syncStates, setSyncStates] = useState<Record<string, MailboxSyncState>>({});
  const [messages, setMessages] = useState<MailboxMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [syncingId, setSyncingId] = useState("");
  const [authorizingId, setAuthorizingId] = useState("");

  const activeConnections = workspace.mailbox_connections.filter(
    (item) => item.status !== "disconnected",
  );

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setProviders(mailboxProvidersAfterLoad(null));
      try {
        const [nextProviders, states, nextMessages] = await Promise.all([
          onMailboxProviders(),
          Promise.all(activeConnections.map(async (connection) => {
            try {
              return [connection.id, await onMailboxSyncState(connection)] as const;
            } catch {
              return null;
            }
          })),
          activeConnections.length ? onMailboxMessages() : Promise.resolve([]),
        ]);
        if (cancelled) return;
        setProviders(mailboxProvidersAfterLoad(nextProviders));
        setSyncStates(Object.fromEntries(
          states.filter(
            (state): state is readonly [string, MailboxSyncState] => state !== null,
          ),
        ));
        setMessages(nextMessages);
      } catch (requestError) {
        if (!cancelled) {
          setProviders(mailboxProvidersAfterLoad(null));
          onError(errorMessage(requestError));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [
    onError,
    onMailboxMessages,
    onMailboxProviders,
    onMailboxSyncState,
    workspace.mailbox_connections,
  ]);

  const syncConnection = async (connection: MailboxConnection) => {
    if (syncingId) return;
    setSyncingId(connection.id);
    onError("");
    try {
      const nextState = await onSyncMailbox(connection);
      setSyncStates((current) => ({ ...current, [connection.id]: nextState }));
      setMessages(await onMailboxMessages());
    } catch (requestError) {
      onError(errorMessage(requestError));
    } finally {
      setSyncingId("");
    }
  };

  const reconnect = async (connection: MailboxConnection) => {
    if (syncingId) return;
    setSyncingId(connection.id);
    onError("");
    try {
      await onConnectMailbox(connection.provider);
    } catch (requestError) {
      onError(errorMessage(requestError));
    } finally {
      setSyncingId("");
    }
  };

  const authorizeCommunication = async (connection: MailboxConnection) => {
    if (authorizingId || syncingId) return;
    setAuthorizingId(connection.id);
    onError("");
    try {
      await onAuthorizeMailboxCommunication(connection);
    } catch (requestError) {
      onError(errorMessage(requestError));
    } finally {
      setAuthorizingId("");
    }
  };

  const recentMessages = messages.slice(0, 5);
  const reviewCount = messages.filter(
    (message) => message.processing_status === "needs_input",
  ).length;
  const calendarActionState = mailboxCalendarActionState(activeConnections, providers);

  return <>
    <section className="settings-section" id="application-inbox">
      <div className="settings-section-title">
        <span><Mail /></span>
        <div>
          <p>APPLICATION INBOX</p>
          <h2>Track employer replies</h2>
          <small>Connect Gmail or Outlook. Bluey matches application updates and asks you to review any outcome change.</small>
        </div>
        <button
          className="button secondary compact"
          onClick={() => setMailboxOpen(true)}
          disabled={activeConnections.length >= workspace.entitlement.connected_inbox_limit}
        >
          <Plus size={15} />Connect inbox
        </button>
      </div>
      <div className="connection-usage">
        <span><b>{activeConnections.length}</b> of {workspace.entitlement.connected_inbox_limit} inboxes connected</span>
        <span>
          Employer updates stay read-only unless you separately authorize reviewed replies and
          calendar events.
        </span>
      </div>
      <div className="integration-list mailbox-connections">
        {activeConnections.map((connection) => {
          const syncState = syncStates[connection.id];
          const providerAvailability = providers.find(
            (item) => item.provider === connection.provider,
          );
          const communicationAuthorization = mailboxCommunicationAuthorizationState(
            connection,
            providerAvailability,
          );
          return <div key={connection.id}>
            <span className="integration-icon"><Mail /></span>
            <div>
              <b>{connection.account_label}</b>
              <p>
                {connectionName(connection.provider)}
                {syncState?.last_synced_at_ms
                  ? ` · Checked ${relativeTime(syncState.last_synced_at_ms)}`
                  : " · Ready to check"}
                {syncState?.last_error ? " · Needs attention" : ""}
              </p>
              {communicationAuthorization === "authorized" && (
                <small className="mailbox-communication-detail">
                  Send and calendar-write scopes are authorized. Every exact draft still needs
                  your review.
                </small>
              )}
              {communicationAuthorization === "available" && (
                <small className="mailbox-communication-detail">
                  Optional send and calendar-write scopes are available. Every exact draft still
                  needs your review.
                </small>
              )}
            </div>
            <span className={`integration-state ${connection.status}`}>
              {connection.status === "reauthorization_required" ? "Reconnect" : "Connected"}
            </span>
            <span className="integration-actions">
              <CommunicationAuthorizationControl
                connection={connection}
                availability={providerAvailability}
                busy={Boolean(authorizingId || syncingId)}
                authorizing={authorizingId === connection.id}
                onAuthorize={() => void authorizeCommunication(connection)}
              />
              {connection.status === "reauthorization_required"
                ? <button
                  className="button secondary compact"
                  disabled={Boolean(syncingId)}
                  onClick={() => void reconnect(connection)}
                >
                  {syncingId === connection.id ? "Opening..." : "Reconnect"}
                </button>
                : <button
                  className="button secondary compact"
                  disabled={Boolean(syncingId)}
                  onClick={() => void syncConnection(connection)}
                >
                  {syncingId === connection.id ? "Checking..." : "Sync now"}
                </button>}
              <button
                className="button secondary compact"
                onClick={() => setDisconnectingMailbox(connection)}
              >
                Disconnect
              </button>
            </span>
          </div>;
        })}
        {activeConnections.length === 0 && <div className="integration-empty">
          <span className="integration-icon"><MailCheck /></span>
          <div><b>No application inbox connected</b><p>Connect the inbox that receives employer updates.</p></div>
        </div>}
      </div>

      <div className="mailbox-updates">
        <div className="mailbox-updates-heading">
          <div><p>RECENT UPDATES</p><h3>{reviewCount ? `${reviewCount} need review` : "Employer activity"}</h3></div>
          <span>{loading ? "Checking inboxes..." : `${messages.length} matched message${messages.length === 1 ? "" : "s"}`}</span>
        </div>
        <div className="mailbox-message-list">
          {recentMessages.map((message) => <article key={message.id}>
            <span className={`mailbox-message-mark ${message.processing_status}`}><MailCheck size={16} /></span>
            <div>
              <b>{message.subject || "Application update"}</b>
              <p>{messageApplicationLabel(message, workspace)}</p>
            </div>
            {message.processing_status === "needs_input"
              ? <Link className="integration-state pending mailbox-review-link" to="/applications">Review</Link>
              : <span className="integration-state connected">{classificationLabel(message.classification)}</span>}
            <time
              dateTime={new Date(message.received_at_ms).toISOString()}
              title={new Date(message.received_at_ms).toLocaleString()}
            >
              {relativeTime(message.received_at_ms)}
            </time>
          </article>)}
          {!loading && recentMessages.length === 0 && <div className="mailbox-message-empty">
            <MailCheck size={18} />
            <span><b>No employer updates yet</b><p>Bluey will place matched acknowledgements, assessments, interviews, and decisions here.</p></span>
          </div>}
        </div>
      </div>

      <div className="calendar-availability">
        <span className="integration-icon"><CalendarDays /></span>
        <div>
          <b>Interview calendar actions</b>
          <p>{calendarActionDescription(calendarActionState)}</p>
        </div>
        <span className={`integration-state ${calendarActionState === "authorized" ? "connected" : ""}`}>
          {calendarActionState === "authorized"
            ? "Authorized · review required"
            : calendarActionState === "available"
              ? "Authorization available"
              : "Unavailable"}
        </span>
      </div>
    </section>

    <MailboxDialog
      open={mailboxOpen}
      providers={providers}
      onClose={() => setMailboxOpen(false)}
      onConnect={async (provider) => {
        onError("");
        await onConnectMailbox(provider);
      }}
    />
    <ConfirmDialog
      open={Boolean(disconnectingMailbox)}
      title={`Disconnect ${disconnectingMailbox?.account_label || "inbox"}?`}
      description="Bluey will delete its saved connection and stop reading new application updates. Existing application history stays in Jobs."
      confirmLabel="Disconnect"
      tone="danger"
      onClose={() => setDisconnectingMailbox(null)}
      onConfirm={() => {
        const connection = disconnectingMailbox;
        setDisconnectingMailbox(null);
        if (connection) {
          void onDeleteMailbox(connection).catch((error) => onError(errorMessage(error)));
        }
      }}
    />
  </>;
}

const COMMUNICATION_WRITE_CAPABILITIES = [
  "recruiter_reply",
  "interview_calendar",
] as const;

export type MailboxCommunicationAuthorizationState =
  | "authorized"
  | "available"
  | "unavailable";

export function mailboxCommunicationAuthorizationState(
  connection: MailboxConnection,
  availability?: MailboxProviderAvailability,
): MailboxCommunicationAuthorizationState {
  if (connection.status !== "connected") return "unavailable";
  if (COMMUNICATION_WRITE_CAPABILITIES.every(
    (capability) => connection.capabilities.includes(capability),
  )) {
    return "authorized";
  }
  return availability?.configured
    && COMMUNICATION_WRITE_CAPABILITIES.every(
      (capability) => availability.capabilities.includes(capability),
    )
    ? "available"
    : "unavailable";
}

export function mailboxCalendarActionState(
  connections: MailboxConnection[],
  providers: MailboxProviderAvailability[],
): MailboxCommunicationAuthorizationState {
  const states = connections.map((connection) => mailboxCommunicationAuthorizationState(
    connection,
    providers.find((provider) => provider.provider === connection.provider),
  ));
  if (states.includes("authorized")) return "authorized";
  if (states.includes("available")) return "available";
  return "unavailable";
}

export function mailboxProvidersAfterLoad(
  result: MailboxProviderAvailability[] | null,
): MailboxProviderAvailability[] {
  return result ?? [];
}

export function CommunicationAuthorizationControl({
  connection,
  availability,
  busy,
  authorizing,
  onAuthorize,
}: {
  connection: MailboxConnection;
  availability?: MailboxProviderAvailability;
  busy: boolean;
  authorizing: boolean;
  onAuthorize(): void;
}) {
  const state = mailboxCommunicationAuthorizationState(connection, availability);
  if (state === "authorized") {
    return (
      <span
        className="integration-state connected mailbox-communication-authorized"
        role="status"
        title="Send and calendar-write scopes are authorized; every exact draft still requires review."
      >
        Replies &amp; calendar authorized
      </span>
    );
  }
  if (state !== "available") return null;
  return (
    <button
      className="button secondary compact"
      type="button"
      aria-label={`Authorize replies and calendar for ${connection.account_label}`}
      title="Adds send and calendar-write scopes; every exact draft still requires review."
      disabled={busy}
      aria-busy={authorizing}
      onClick={onAuthorize}
    >
      {authorizing ? "Opening authorization…" : "Authorize replies & calendar"}
    </button>
  );
}

function calendarActionDescription(state: MailboxCommunicationAuthorizationState): string {
  if (state === "authorized") {
    return "A connected account has calendar-write access. Every exact event draft still requires review.";
  }
  if (state === "available") {
    return "A connected account can separately authorize calendar-write access before any "
      + "reviewed event can be created.";
  }
  return "No connected account currently has calendar-write access. Calendar drafts cannot be approved.";
}

function MailboxDialog({
  open,
  providers,
  onClose,
  onConnect,
}: {
  open: boolean;
  providers: MailboxProviderAvailability[];
  onClose(): void;
  onConnect(provider: MailboxConnection["provider"]): Promise<void>;
}) {
  const [savingProvider, setSavingProvider] = useState<MailboxConnection["provider"] | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) {
      setSavingProvider(null);
      setError("");
    }
  }, [open]);

  const connect = async (provider: MailboxConnection["provider"]) => {
    setSavingProvider(provider);
    setError("");
    try {
      await onConnect(provider);
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setSavingProvider(null);
    }
  };

  const providerOptions: Array<{
    provider: MailboxConnection["provider"];
    name: string;
    detail: string;
  }> = [
    { provider: "gmail", name: "Gmail", detail: "Read employer updates through Google" },
    { provider: "outlook", name: "Outlook", detail: "Read employer updates through Microsoft" },
  ];

  return <Dialog
    open={open}
    title="Connect application inbox"
    description="Choose the inbox that receives employer updates. Bluey requests read-only access and never asks for your password."
    onClose={onClose}
  >
    <div className="mailbox-provider-list">
      {providerOptions.map((option) => {
        const availability = providers.find((item) => item.provider === option.provider);
        const available = Boolean(availability?.configured);
        return <button
          key={option.provider}
          type="button"
          disabled={!available || savingProvider !== null}
          onClick={() => void connect(option.provider)}
        >
          <span className="mailbox-provider-icon"><Mail size={19} /></span>
          <span>
            <b>{option.name}</b>
            <small>{available ? option.detail : "Connection is not configured yet"}</small>
          </span>
          <ChevronRight size={17} />
          {savingProvider === option.provider && <em>Opening...</em>}
        </button>;
      })}
      {providers.length === 0 && <p className="field-note">Mailbox connection is not configured on this Bluey environment.</p>}
      {error && <div className="inline-error" role="alert">{error}</div>}
    </div>
    <div className="dialog-actions">
      <button className="button secondary" onClick={onClose}>Cancel</button>
    </div>
  </Dialog>;
}

function connectionName(provider: MailboxConnection["provider"]): string {
  return provider === "gmail" ? "Gmail inbox" : "Outlook inbox";
}

function classificationLabel(classification: string): string {
  return ({
    acknowledgement: "Received",
    interview: "Interview",
    assessment: "Assessment",
    rejection: "Decision",
    offer: "Offer",
    information_request: "Question",
    follow_up: "Follow-up",
    unknown: "Review",
  } as Record<string, string>)[classification] || "Update";
}

function messageApplicationLabel(message: MailboxMessage, workspace: JobsWorkspace): string {
  const application = workspace.applications.find(
    (item) => item.id === message.application_id,
  );
  const job = application
    ? workspace.matches.find((item) => item.id === application.job_id)
    : undefined;
  const source = message.sender || connectionName(message.provider);
  if (job) return `${job.company} · ${job.title} · ${source}`;
  return `${source} · ${message.application_id ? "Application matched" : "Needs application match"}`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Bluey Jobs could not finish that request.";
}
