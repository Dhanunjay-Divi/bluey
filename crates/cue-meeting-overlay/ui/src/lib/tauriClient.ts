// Real adapter — speaks the EXACT live wire contract the daemon's socket
// transport carries through the Tauri overlay shell.
//
// Transport (the two-pipe model, proven in crates/cue-overlay-tauri/src/ipc.rs):
//   daemon → UI : the daemon serializes a `cue_core::overlay::OverlayCommand`
//                 to one NDJSON line; the Rust shell re-emits each line VERBATIM
//                 as the Tauri event "overlay://command" (a JSON string). We
//                 parse the "type" tag and translate to MeetingClient callbacks.
//   UI → daemon : we call invoke("overlay_send", { event }) where `event` is a
//                 raw `OverlayEvent` JSON STRING; the shell injects the session
//                 "token" and writes it back as NDJSON.
//
// IMPORTANT — field casing. The shell forwards the daemon's NDJSON verbatim, so
// the event payload carries the daemon's serde wire shape: snake_case struct
// fields (display_name, connector_count, auth_tier, cost_label, …) and
// snake_case "type" tags. (Tauri's camelCase conversion only applies to invoke
// command ARGS, never to raw event strings.) So this adapter translates the
// snake_case wire DTOs into the camelCase UI types in ./types.ts — the UI types
// stay stable and the mock adapter is untouched.
//
// The MeetingClient interface is promise-based for discovery/attach, but the
// wire is request/response over events: we fire an OverlayEvent and resolve the
// promise when the matching OverlayCommand reply arrives. ask() is a pure
// stream: fire ask_requested, then map the answer card's update_card deltas to
// onChunk.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AskHandle, MeetingClient } from "./client";
import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  AnswerStatusStep,
  CalendarConnection,
  ContextItem,
  ContinueResult,
  FixProposal,
  ListeningState,
  MeetingConversationTurn,
  MeetingState,
  MeetingSummary,
  MeetingTranscriptLine,
  MeetingViewState,
  TranscriptLine,
} from "./types";

// ---------------------------------------------------------------------------
// Wire DTOs — the daemon's serde shapes (snake_case), as they arrive on the
// "overlay://command" event. Only the fields this UI consumes are typed; the
// rest of each command is ignored.
// ---------------------------------------------------------------------------

interface WireAgentSummary {
  kind: string;
  display_name: string;
  capability: string;
  connector_count: number;
  ready_connector_count: number;
  session_count: number | null;
  attached: boolean;
}

interface WireAgentSessionSummary {
  id: string;
  title: string | null;
  updated_at: string;
  project: string | null;
}

// The daemon's OverlayContextItem — one attached context artifact. `thumbnail`
// is a small inline `data:` URI, present only for image/diagram kinds.
interface WireContextItem {
  id: string;
  title: string;
  kind: string;
  path?: string | null;
  thumbnail?: string | null;
  anchor_segment_id?: string | null;
  text_preview?: string | null;
}

// One first-run setup prerequisite (daemon `SetupItem`).
interface WireSetupItem {
  state: string;
  detail: string;
  percent?: number | null;
  kind?: string | null;
}

interface WireAgentConnectorInfo {
  name: string;
  auth_tier: string;
  ready: boolean;
}

// The active meeting's rehydration snapshot (Fix B). A minimal, stable wire
// surface — the daemon deliberately does NOT expose TranscriptSegment /
// ConversationTurn here (they carry diarization/audio-clock internals the UI
// must not depend on). `final` is always true (only finalized segments persist).
interface WireMeetingTranscriptLine {
  id: string;
  source: string;
  speaker?: string | null;
  speaker_id?: number | null;
  text: string;
  final: boolean;
}

interface WireMeetingConversationTurn {
  id: string;
  question: string;
  answer: string;
  source?: string | null;
}

// One past meeting for the MEETINGS lens (the daemon's MeetingSummary serde
// shape). agent_session_id is present only when the meeting chained an agent
// thread; the UI derives hasAgentSession from its presence.
interface WireMeetingSummary {
  id: string;
  title: string;
  started_at: string;
  ended_at?: string | null;
  transcript_count: number;
  turn_count: number;
  preview?: string | null;
  is_active: boolean;
  agent_session_id?: string | null;
  agent_kind?: string | null;
}

interface WireCueCard {
  id: string;
  kind: string; // CardKind, snake_case: answer | transcript | question | …
  title: string;
  body: string;
  created_at: string;
  source: string | null;
  cost_label?: string | null;
  // artifact omitted — not consumed on push_card.
}

// The set of OverlayCommand variants this adapter reacts to. `type` is the
// serde tag; remaining fields are per-variant. Unhandled variants fall through.
type OverlayCommand =
  | { type: "set_agents"; agents: WireAgentSummary[] }
  | {
      type: "set_meeting_candidates";
      candidates: { name: string; email: string }[];
    }
  | {
      type: "set_agent_sessions";
      kind: string;
      sessions: WireAgentSessionSummary[];
    }
  | {
      type: "set_agent_connectors";
      kind: string;
      connectors: WireAgentConnectorInfo[];
    }
  | { type: "set_agent_models"; kind: string; models: string[] }
  | {
      type: "set_meeting_state";
      transcript: WireMeetingTranscriptLine[];
      conversation: WireMeetingConversationTurn[];
      // The Key Decisions ledger; absent (omitted on the wire) when empty.
      decisions?: { id: string; text: string }[];
      // Attached context (screenshots/files) in the snapshot; absent when empty.
      context?: WireContextItem[];
      // Present only for a PAST-meeting VIEW reply (Decision 2). Absent on the
      // active-rehydrate reply — the discriminator that keeps the two request()
      // pickers on the shared bus from stealing each other's replies.
      meeting_id?: string;
      read_only?: boolean;
    }
  | { type: "set_meetings"; meetings: WireMeetingSummary[] }
  | {
      type: "set_context_items";
      items: WireContextItem[];
      turns?: number;
    }
  | {
      type: "listening_state_changed";
      state: string;
      system?: boolean;
      microphone?: boolean;
      permission_denied_source?: string;
    }
  | { type: "push_card"; card: WireCueCard }
  | {
      type: "show_meeting_banner";
      event_id: string;
      title: string;
      start_epoch_secs: number;
      end_epoch_secs: number;
      participant_count: number;
      accepted_count: number;
      online: boolean;
    }
  // Diarization resolved a speaker for an already-pushed transcript line
  // (labels lag lines by up to one live-diarize tick). `id` is the segment id
  // the transcript card was pushed with; `speaker` is the display label.
  | {
      type: "transcript_speaker";
      id: string;
      speaker: string;
      speaker_id?: number | null;
    }
  | {
      type: "update_card";
      id: string;
      body: string;
      done?: boolean;
      cost_label?: string | null;
      is_error?: boolean;
      /** True when the answer proposes a concrete change/action — the UI shows
       *  the "Fix this" affordance only then. Absent on old daemons → false. */
      fixable?: boolean;
    }
  | {
      type: "set_answer_status";
      id: string;
      steps: WireAnswerStatusStep[];
      done?: boolean;
    }
  | {
      type: "set_setup_status";
      status: {
        model: WireSetupItem;
        agent: WireSetupItem;
        all_ready: boolean;
      };
    }
  | {
      type: "push_agent_install";
      kind: string;
      display_name: string;
      command: string;
      prerequisite?: string | null;
    }
  // A review-gated fix proposal for an agent answer (Fix-button slice F3). The
  // daemon drove the attached agent in propose-only mode; nothing is applied.
  // `diff` is absent for commands-only / prose-only fixes.
  | {
      type: "push_fix_proposal";
      proposal_id: string;
      diagnosis: string;
      reasoning: string;
      fix: string;
      diff?: string | null;
      apply_supported: boolean;
    }
  | { type: string; [k: string]: unknown };

// The daemon's AnswerStatusStep wire shape (serde: `kind` tag, snake_case
// fields + states) — identical to the UI's AnswerStatusStep, so it passes
// through. Typed here so the adapter validates the shape.
type WireAnswerStatusStep =
  | { kind: "reasoning"; text: string }
  | { kind: "tool"; id: string; title: string; state: string };

// ---------------------------------------------------------------------------
// Wire → UI translation.
// ---------------------------------------------------------------------------

function toAgentSummary(w: WireAgentSummary): AgentSummary {
  return {
    kind: w.kind,
    displayName: w.display_name,
    capability: w.capability,
    connectorCount: w.connector_count,
    readyConnectorCount: w.ready_connector_count,
    sessionCount: w.session_count ?? null,
    attached: w.attached,
  };
}

function toContextItem(w: WireContextItem): ContextItem {
  return {
    id: w.id,
    title: w.title,
    kind: w.kind,
    path: w.path ?? undefined,
    thumbnail: w.thumbnail ?? undefined,
    anchorSegmentId: w.anchor_segment_id ?? undefined,
    textPreview: w.text_preview ?? undefined,
  };
}

function toAgentSessionSummary(
  w: WireAgentSessionSummary,
): AgentSessionSummary {
  return {
    id: w.id,
    title: w.title ?? null,
    updatedAt: w.updated_at,
    project: w.project ?? null,
  };
}

function toAgentConnectorInfo(w: WireAgentConnectorInfo): AgentConnectorInfo {
  return { name: w.name, authTier: w.auth_tier, ready: w.ready };
}

function toMeetingTranscriptLine(
  w: WireMeetingTranscriptLine,
): MeetingTranscriptLine {
  return {
    id: w.id,
    source: w.source,
    speaker: w.speaker ?? undefined,
    speakerId: w.speaker_id ?? undefined,
    text: w.text,
    final: w.final,
  };
}

function toMeetingConversationTurn(
  w: WireMeetingConversationTurn,
): MeetingConversationTurn {
  return {
    id: w.id,
    question: w.question,
    answer: w.answer,
    source: w.source ?? undefined,
  };
}

function toMeetingSummary(w: WireMeetingSummary): MeetingSummary {
  return {
    id: w.id,
    title: w.title,
    startedAt: w.started_at,
    endedAt: w.ended_at ?? undefined,
    transcriptCount: w.transcript_count,
    turnCount: w.turn_count,
    preview: w.preview ?? undefined,
    isActive: w.is_active,
    agentSessionId: w.agent_session_id ?? undefined,
    agentKind: w.agent_kind ?? undefined,
  };
}

function toFixProposal(
  w: Extract<OverlayCommand, { type: "push_fix_proposal" }>,
): FixProposal {
  return {
    id: w.proposal_id,
    diagnosis: w.diagnosis,
    reasoning: w.reasoning,
    fix: w.fix,
    diff: w.diff ?? undefined,
    applySupported: w.apply_supported,
  };
}

// ---------------------------------------------------------------------------
// UI → daemon: send a raw OverlayEvent object. The Rust shell injects "token".
// ---------------------------------------------------------------------------

function sendEvent(event: Record<string, unknown>): void {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return;
  }
  void invoke<void>("overlay_send", { event: JSON.stringify(event) }).catch(
    (e) => {
      // Surface, never silently swallow — but don't crash the UI loop.
      console.error("[tauriClient] overlay_send failed", event.type, e);
    },
  );
}

export function createTauriClient(): MeetingClient {
  // ---- single command bus over the one daemon → UI pipe ----
  // Every "overlay://command" line flows here; we fan it out to whoever is
  // currently waiting (a pending request promise, a stream, a subscriber).
  const handlers = new Set<(cmd: OverlayCommand) => void>();
  let unlistenBus: UnlistenFn | null = null;
  // The bus must be LISTENING before we send any event — otherwise a fast daemon
  // response (the agent list comes back in ~20ms) arrives before listen() is
  // registered and is lost, hanging the UI on "Discovering agents…" forever.
  const inTauri =
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const busReady: Promise<void> = inTauri
    ? listen<string>("overlay://command", (e) => {
        let cmd: OverlayCommand;
        try {
          // The payload is the daemon's NDJSON line, forwarded verbatim as a string.
          cmd = JSON.parse(e.payload) as OverlayCommand;
        } catch (err) {
          console.error("[tauriClient] bad overlay://command payload", err);
          return;
        }
        for (const h of [...handlers]) h(cmd);
      })
        .then((fn) => {
          unlistenBus = fn;
        })
        .catch((error: unknown) => {
          // A capability/configuration mistake must not become an unhandled
          // rejection that leaves every request silently waiting forever.
          console.error("[tauriClient] overlay command listener failed", error);
        })
    : Promise.resolve();
  // Best-effort teardown if the window unloads (the shell also cleans up).
  if (typeof window !== "undefined") {
    window.addEventListener("beforeunload", () => unlistenBus?.());
  }

  /**
   * Fire an OverlayEvent and resolve with the first OverlayCommand the
   * predicate accepts. Used to model the request/response discovery commands
   * over the event bus. Resolves with the translated value the picker returns.
   */
  function request<T>(
    event: Record<string, unknown>,
    pick: (cmd: OverlayCommand) => T | undefined,
  ): Promise<T> {
    return new Promise<T>((resolve) => {
      const handler = (cmd: OverlayCommand) => {
        const out = pick(cmd);
        if (out !== undefined) {
          handlers.delete(handler);
          resolve(out);
        }
      };
      handlers.add(handler);
      // Wait until the bus is actually listening before firing the event, so the
      // daemon's (fast) response can never arrive before we're ready to catch it.
      void busReady.then(() => sendEvent(event));
    });
  }

  return {
    listAgents: () =>
      request<AgentSummary[]>({ type: "agent_list_requested" }, (cmd) =>
        cmd.type === "set_agents"
          ? (cmd as Extract<OverlayCommand, { type: "set_agents" }>).agents.map(
              toAgentSummary,
            )
          : undefined,
      ),

    // attach/detach both confirm by the daemon re-pushing the full agent list.
    attach: (kind, sessionId, model) =>
      request<AgentSummary[]>(
        {
          type: "agent_attach_requested",
          kind,
          ...(sessionId ? { session_id: sessionId } : {}),
          ...(model ? { model } : {}),
        },
        (cmd) =>
          cmd.type === "set_agents"
            ? (
                cmd as Extract<OverlayCommand, { type: "set_agents" }>
              ).agents.map(toAgentSummary)
            : undefined,
      ),

    detach: () =>
      request<AgentSummary[]>({ type: "agent_detach_requested" }, (cmd) =>
        cmd.type === "set_agents"
          ? (cmd as Extract<OverlayCommand, { type: "set_agents" }>).agents.map(
              toAgentSummary,
            )
          : undefined,
      ),

    sessions: (kind) =>
      request<AgentSessionSummary[]>(
        { type: "agent_sessions_requested", kind },
        (cmd) => {
          if (cmd.type !== "set_agent_sessions") return undefined;
          const c = cmd as Extract<
            OverlayCommand,
            { type: "set_agent_sessions" }
          >;
          // Guard against a reply for a different agent on the shared bus.
          if (c.kind !== kind) return undefined;
          return c.sessions.map(toAgentSessionSummary);
        },
      ),

    models: (kind) =>
      request<string[]>({ type: "agent_models_requested", kind }, (cmd) => {
        if (cmd.type !== "set_agent_models") return undefined;
        const c = cmd as Extract<OverlayCommand, { type: "set_agent_models" }>;
        // Guard against a reply for a different agent on the shared bus.
        if (c.kind !== kind) return undefined;
        return c.models;
      }),

    sourceCoverage: async () => {
      // Direct tauri command (no bus roundtrip): wire rows are snake_case.
      type Wire = {
        source: string;
        label: string;
        connected: boolean;
        via?: string | null;
        connect_hint?: string | null;
      };
      const rows = await invoke<Wire[]>("source_coverage");
      return rows.map((r) => ({
        source: r.source,
        label: r.label,
        connected: r.connected,
        via: r.via ?? null,
        connectHint: r.connect_hint ?? null,
      }));
    },

    // ---- cloud calendar. Direct tauri commands (like sourceCoverage): the
    // shell forwards a DaemonRequest over the socket and returns the
    // DaemonResponse. A daemon-side error rejects the invoke promise with the
    // error message, which the caller renders. ----
    calendarConnect: async (provider) => {
      // Resolves when the daemon finishes the browser round-trip + token store;
      // rejects (invoke throws) with the daemon's error message on failure.
      await invoke<void>("calendar_connect", { provider });
    },

    calendarStatus: async () => {
      // The daemon's CalendarConnection fields are single-word, so its snake_case
      // serde already matches the UI shape — pass the rows through as-is.
      const rows = await invoke<CalendarConnection[]>("calendar_status");
      return rows.map((r) => ({
        provider: r.provider,
        configured: r.configured,
        connected: r.connected,
        email: r.email,
        error: r.error,
      }));
    },

    calendarDisconnect: async (provider) => {
      await invoke<void>("calendar_disconnect", { provider });
    },

    connectors: (kind) =>
      request<AgentConnectorInfo[]>(
        { type: "agent_connectors_requested", kind },
        (cmd) => {
          if (cmd.type !== "set_agent_connectors") return undefined;
          const c = cmd as Extract<
            OverlayCommand,
            { type: "set_agent_connectors" }
          >;
          if (c.kind !== kind) return undefined;
          return c.connectors.map(toAgentConnectorInfo);
        },
      ),

    meetingState: () =>
      request<MeetingState>({ type: "meeting_state_requested" }, (cmd) => {
        // Exactly one active meeting, so no kind-guard is needed — the first
        // set_meeting_state reply is ours. Empty arrays resolve the promise even
        // when no meeting is active (the daemon still replies).
        if (cmd.type !== "set_meeting_state") return undefined;
        const c = cmd as Extract<OverlayCommand, { type: "set_meeting_state" }>;
        // CRITICAL: the active-rehydrate reply carries NO meeting_id; a
        // PAST-meeting VIEW reply (openMeeting) carries one. Both share this bus,
        // so reject any reply bearing a meeting_id — otherwise an openMeeting
        // reply could resolve this rehydrate request and reseed the live view
        // with a past meeting.
        if (c.meeting_id != null) return undefined;
        return {
          transcript: c.transcript.map(toMeetingTranscriptLine),
          conversation: c.conversation.map(toMeetingConversationTurn),
          decisions: c.decisions ?? [],
          context: (c.context ?? []).map(toContextItem),
        };
      }),

    meetings: () =>
      request<MeetingSummary[]>({ type: "meetings_requested" }, (cmd) => {
        if (cmd.type !== "set_meetings") return undefined;
        const c = cmd as Extract<OverlayCommand, { type: "set_meetings" }>;
        return c.meetings.map(toMeetingSummary);
      }),

    openMeeting: (id) =>
      request<MeetingViewState>(
        { type: "meeting_open_requested", id },
        (cmd) => {
          if (cmd.type !== "set_meeting_state") return undefined;
          const c = cmd as Extract<
            OverlayCommand,
            { type: "set_meeting_state" }
          >;
          // Discriminate MY reply from an active-rehydrate reply (meeting_id
          // undefined) or a reply for a DIFFERENT open on the shared bus.
          if (c.meeting_id !== id) return undefined;
          return {
            transcript: c.transcript.map(toMeetingTranscriptLine),
            conversation: c.conversation.map(toMeetingConversationTurn),
            decisions: c.decisions ?? [],
            context: (c.context ?? []).map(toContextItem),
            meetingId: c.meeting_id,
            readOnly: c.read_only ?? false,
          };
        },
      ),

    // Continue a past meeting into the ACTIVE slot. The reply rides the SHARED
    // set_meeting_state bus, so the picker discriminates:
    //   • BLOCKED  → the dedicated guard reply: meeting_id === id && read_only
    //     (a past-VIEW-shaped reply the daemon sends when a live recording
    //     prevented the switch) → resolve { ok:false, blocked:true }.
    //   • SUCCESS  → the active-rehydrate reseed: meeting_id absent → resolve
    //     { ok:true }. That SAME broadcast ALSO reaches the persistent
    //     onMeetingReseed handler (the bus fans out to every handler), so
    //     resolving this one-shot request does NOT consume the provider reseed.
    //   • anything else (a different open's meeting_id, or read_only false) →
    //     undefined (not ours).
    // CAUTION: the openMeeting picker also matches meeting_id === id; a blocked
    // continue reply is meeting_id === id && read_only === true. Open vs Continue
    // are separate user gestures never fired together for the same id, so no code
    // guard is needed here.
    continueMeeting: (id) =>
      request<ContinueResult>(
        { type: "meeting_continue_requested", id },
        (cmd) => {
          if (cmd.type !== "set_meeting_state") return undefined;
          const c = cmd as Extract<
            OverlayCommand,
            { type: "set_meeting_state" }
          >;
          if (c.meeting_id === id && c.read_only === true) {
            return { ok: false, blocked: true };
          }
          if (c.meeting_id == null) return { ok: true, blocked: false };
          return undefined;
        },
      ),

    onMeetingReseed(cb) {
      // Persistent subscriber to daemon-PUSHED active reseeds — the
      // meeting_id-absent set_meeting_state emitted when a past meeting is
      // continued into the active slot. Unlike request()'s one-shot handler this
      // stays registered. NOTE it also fires for the reply to meetingState()'s
      // own request (same shape); that is harmless because the provider applies
      // the same data idempotently and gates on `rehydrated` to skip the very
      // first mount seed.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "set_meeting_state") return;
        const c = cmd as Extract<OverlayCommand, { type: "set_meeting_state" }>;
        if (c.meeting_id != null) return;
        cb({
          transcript: c.transcript.map(toMeetingTranscriptLine),
          conversation: c.conversation.map(toMeetingConversationTurn),
          decisions: c.decisions ?? [],
          context: (c.context ?? []).map(toContextItem),
        });
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    setSessionHistoryConsent: (enabled) => {
      // First-class consent toggle: the daemon persists it the same way the IPC
      // SetAgentSessionHistory path does, then refreshes the agent list.
      sendEvent({ type: "session_history_consent_requested", enabled });
      return Promise.resolve();
    },

    renameSpeaker: (speakerId, name) => {
      // Fire-and-forget: the daemon persists the name + enrolls the voiceprint,
      // then pushes the updated label back via onSpeakerUpdate.
      sendEvent({
        type: "rename_speaker_requested",
        speaker_id: speakerId,
        name,
      });
    },

    reassignSpan: (segmentIds, speakerId, name) => {
      // Reassign a span of transcript segments to a speaker (+ optional name).
      // The daemon rewrites speaker_id, re-broadcasts, and (live) re-enrolls the
      // voiceprint from the span's audio.
      sendEvent({
        type: "reassign_span_requested",
        segment_ids: segmentIds,
        speaker_id: speakerId,
        ...(name ? { name } : {}),
      });
    },

    splitSegment: (segmentId, charOffset, firstSpeakerId, secondSpeakerId) => {
      sendEvent({
        type: "split_segment_requested",
        segment_id: segmentId,
        char_offset: charOffset,
        first_speaker_id: firstSpeakerId,
        second_speaker_id: secondSpeakerId,
      });
    },

    reassignRange: (memberIds, charStart, charEnd, speakerId, name) => {
      // Precise char-range reassign: the daemon maps [charStart,charEnd) onto the
      // ordered member segments' concatenated text, reassigns fully-covered
      // segments and splits boundary segments at the exact char.
      sendEvent({
        type: "reassign_range_requested",
        member_ids: memberIds,
        char_start: charStart,
        char_end: charEnd,
        speaker_id: speakerId,
        ...(name ? { name } : {}),
      });
    },

    newMeeting: () => {
      // Archive the active meeting + start a fresh one. Fire-and-forget: the
      // daemon replies with a meeting_id-absent set_meeting_state, delivered via
      // onMeetingReseed, which clears the transcript/Q&A/decisions/context.
      sendEvent({ type: "meeting_new_requested" });
    },

    onMeetingBanner(cb) {
      // The daemon pushes show_meeting_banner ~lead time before a calendar
      // meeting. Map the wire (snake_case) to the MeetingBanner shape.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "show_meeting_banner") return;
        const c = cmd as Extract<
          OverlayCommand,
          { type: "show_meeting_banner" }
        >;
        cb({
          eventId: c.event_id,
          title: c.title,
          startEpochSecs: c.start_epoch_secs,
          endEpochSecs: c.end_epoch_secs,
          participantCount: c.participant_count,
          acceptedCount: c.accepted_count,
          online: c.online,
        });
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    respondMeetingPrep: (eventId, startEpochSecs, approved) => {
      // Warm (approve) or dismiss the meeting-prep banner. Fire-and-forget.
      sendEvent({
        type: "meeting_prep_responded",
        event_id: eventId,
        start_epoch_secs: startEpochSecs,
        approved,
      });
    },

    onListeningState(cb) {
      // The daemon pushes listening_state_changed with the full pipeline state
      // (idle | connecting | listening | paused | failed). Pass it through so
      // the UI can show connecting/failed, not just on/off — otherwise a failed
      // start silently snaps the mic button back and looks dead.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "listening_state_changed") return;
        const c = cmd as Extract<
          OverlayCommand,
          { type: "listening_state_changed" }
        >;
        const s = c.state;
        const known: ListeningState =
          s === "connecting" ||
          s === "listening" ||
          s === "paused" ||
          s === "failed" ||
          s === "permission_denied"
            ? s
            : "idle";
        cb(
          known,
          typeof c.system === "boolean" && typeof c.microphone === "boolean"
            ? {
                system: c.system,
                microphone: c.microphone,
                permissionDeniedSource:
                  c.permission_denied_source === "system" ||
                  c.permission_denied_source === "microphone"
                    ? c.permission_denied_source
                    : undefined,
              }
            : undefined,
        );
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    startListening(sources) {
      // Start AUDIO capture. Optional per-source selection (Audio tab toggles);
      // omitted = both, matching the daemon's serde defaults. The daemon replies
      // with listening_state_changed, which onListeningState reflects.
      // (recording_* = audio; capture_* = the screen "eye" — different feature.)
      sendEvent({
        type: "recording_start_requested",
        ...(sources?.microphone !== undefined
          ? { enable_microphone: sources.microphone }
          : {}),
        ...(sources?.system !== undefined
          ? { enable_system: sources.system }
          : {}),
      });
    },

    stopListening() {
      sendEvent({ type: "recording_stop_requested" });
    },

    turnOff() {
      // `close_requested` is the daemon's existing shutdown path
      // (`shutdown_daemon` + exit) — the overlay simply never called it. This is
      // Bluey OFF, not a collapse: the pill goes away and the daemon stops.
      sendEvent({ type: "close_requested" });
    },

    openPermissionSettings(pane) {
      // The daemon maps `pane` (a fixed enum) to a known System Settings URL and
      // opens it — no plugin/permission needed UI-side, no arbitrary URL.
      sendEvent({ type: "open_settings_requested", pane });
    },

    pickSystemAudio() {
      // Daemon spawns the helper in --pick mode → macOS content-sharing picker →
      // captures the chosen app's audio into the live transcript pipeline.
      sendEvent({ type: "pick_system_audio_requested" });
    },

    capturePage() {
      // "+" → Capture page: grab the active BROWSER page's text as a context
      // artifact (daemon "overlay page" path). Needs a browser frontmost; the
      // daemon pushes an honest warning card if no readable page is found.
      sendEvent({ type: "active_page_capture_requested" });
    },

    openAttachPicker() {
      // "+" → Attach files: open the NATIVE macOS file picker from the overlay's
      // OWN GUI process (via the `pick_context_files` command → tauri-plugin-dialog),
      // then hand the chosen paths to the daemon as `attach_files_requested`. The
      // daemon is headless and a daemon-spawned helper has no window-server access,
      // so the dialog must originate here. (`attach_requested` — the old
      // daemon-driven picker — is retired.)
      void invoke<string[]>("pick_context_files")
        .then((paths) => {
          if (Array.isArray(paths) && paths.length > 0) {
            sendEvent({ type: "attach_files_requested", paths });
          }
        })
        .catch((error) => {
          console.error("pick_context_files failed", error);
        });
    },

    captureScreenshot() {
      // "+" → Take a screenshot: capture the screen from the OVERLAY's own process
      // (Screen Recording permission is keyed to the capturing process; the
      // headless daemon's grant is fragile, so the overlay — a stable GUI app — is
      // the robust capturer). Hand the PNG to the daemon as `attach_files_requested`;
      // it classifies the .png as an image and sends it to the agent as pixels over
      // ACP. A thumbnail chip appears via the set_context_items push.
      void invoke<string>("capture_screenshot")
        .then((path) => {
          if (path)
            sendEvent({ type: "attach_files_requested", paths: [path] });
        })
        .catch((error) => {
          console.error("capture_screenshot failed", error);
        });
    },

    onContextItems(cb) {
      // Persistent subscriber to the daemon's attached-context list. Every
      // set_context_items line (an attach, a screenshot, a remove) carries the
      // FULL current list, so the composer chip strip is a pure mirror of it.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "set_context_items") return;
        const c = cmd as Extract<OverlayCommand, { type: "set_context_items" }>;
        cb(c.items.map(toContextItem));
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    removeContextItem(id) {
      // The chip's ✕ — ask the daemon to drop this artifact from the meeting; it
      // replies with a fresh set_context_items the onContextItems mirror applies.
      sendEvent({ type: "remove_context_requested", id });
    },

    onAgents(cb) {
      // Persistent subscriber to daemon-PUSHED agent lists. Every set_agents line
      // — the reply to a listAgents() request, the SWR background full-refresh, or
      // an attach/detach flag-flip — fans out here so a shared store stays live.
      // Pure subscribe: sends no event.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "set_agents") return;
        const c = cmd as Extract<OverlayCommand, { type: "set_agents" }>;
        cb(c.agents.map(toAgentSummary));
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    onMeetingCandidates(cb) {
      // Daemon-pushed calendar attendees for the active meeting; the speaker-
      // rename input offers them as tap-to-pick names. Pure subscribe.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "set_meeting_candidates") return;
        const c = cmd as Extract<
          OverlayCommand,
          { type: "set_meeting_candidates" }
        >;
        cb(c.candidates);
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    onTranscript(cb) {
      // Transcript lines arrive as push_card with kind "transcript".
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "push_card") return;
        const card = (cmd as Extract<OverlayCommand, { type: "push_card" }>)
          .card;
        if (card.kind !== "transcript") return;
        const line: TranscriptLine = {
          // The daemon sets the live transcript card's id to the persisted
          // segment's id, so the provider can reconcile this live line against
          // the same segment already in its rehydration snapshot (id-upsert).
          id: card.id,
          // The daemon tags transcript origin in `source` ("system" | "mic") —
          // the SAME channel the snapshot's MeetingTranscriptLine.source uses, so
          // the seed/live seam agrees; fall back to "system" when absent.
          source: card.source ?? "system",
          // card.title is the CHANNEL label ("System" | "Mic"), not a diarized
          // speaker — the UI already renders the channel from `source`. Real
          // speaker labels arrive later via transcript_speaker upgrades.
          speaker: undefined,
          text: card.body,
          // push_card carries a fully-formed (finalized) line.
          final: true,
        };
        cb(line);
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    onSpeakerUpdate(cb) {
      // Diarization label upgrades for already-rendered transcript lines. The
      // daemon's live diarize tick resolves "who said it" a few seconds after
      // the text was pushed; this patches the label in place by segment id.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "transcript_speaker") return;
        const c = cmd as Extract<
          OverlayCommand,
          { type: "transcript_speaker" }
        >;
        // `id` empty = a rename echo for a whole speaker (by speaker_id), not a
        // single segment. Pass both through; the grouper decides how to apply.
        if (c.speaker) cb(c.id, c.speaker, c.speaker_id ?? null);
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    onForMeQuestion(cb) {
      // The daemon emits a push_card with kind "question" when it detects a
      // for-me question in the transcript (master doc §6). This is the distinct
      // signal that drives the Ask view's "They asked…" hero card — separate
      // from plain transcript lines, so the UI never has to re-detect.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "push_card") return;
        const card = (cmd as Extract<OverlayCommand, { type: "push_card" }>)
          .card;
        if (card.kind !== "question") return;
        // body = the question text; title = the framing ("Alex, this looks…").
        const text = (card.body || card.title || "").trim();
        if (text) cb({ text, title: card.title || undefined });
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    onAgentInstall(cb) {
      // The daemon pushes push_agent_install when the attached agent's CLI is
      // missing but installable (a vetted registry recipe). We surface an
      // Install / Cancel card; the answer goes back as agent_install_responded.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "push_agent_install") return;
        const c = cmd as Extract<
          OverlayCommand,
          { type: "push_agent_install" }
        >;
        cb({
          kind: c.kind,
          displayName: c.display_name,
          command: c.command,
          prerequisite: c.prerequisite ?? undefined,
        });
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    respondAgentInstall(kind, approved) {
      sendEvent({ type: "agent_install_responded", kind, approved });
    },

    requestAgentInstall(kind) {
      // UI-initiated (onboarding "no agent found"). The daemon replies with the
      // SAME push_agent_install offer the drive path uses, so onAgentInstall
      // renders the consent card and nothing installs without approval.
      sendEvent({ type: "agent_install_requested", kind: kind ?? null });
    },

    requestAgentLogin(kind) {
      // Launches the agent's OWN login flow (e.g. `cursor-agent login`).
      // Bluey never handles credentials.
      sendEvent({ type: "agent_login_requested", kind });
    },

    onSetupStatus(cb) {
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "set_setup_status") return;
        const c = cmd as Extract<OverlayCommand, { type: "set_setup_status" }>;
        const s = c.status;
        cb({
          model: {
            state: s.model.state,
            detail: s.model.detail,
            percent: s.model.percent ?? null,
            kind: s.model.kind ?? null,
          },
          agent: {
            state: s.agent.state,
            detail: s.agent.detail,
            percent: s.agent.percent ?? null,
            kind: s.agent.kind ?? null,
          },
          allReady: s.all_ready,
        });
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    requestSetupStatus() {
      sendEvent({ type: "setup_status_requested" });
    },

    requestFix(question, cardId) {
      // Fire the daemon's fix_requested event: it drives the attached agent in
      // propose-only mode and later PUSHES a push_fix_proposal we catch in
      // onFixProposal. Include card_id only when given so an unattached fix
      // carries exactly { type, question } and the daemon's serde default (None)
      // applies.
      sendEvent({
        type: "fix_requested",
        question,
        ...(cardId ? { card_id: cardId } : {}),
      });
    },

    onFixProposal(cb) {
      // Persistent subscriber to daemon-PUSHED fix proposals. Mirrors
      // onForMeQuestion: the proposal arrives asynchronously (driving a real
      // agent is slow) as its own command, so this stays registered rather than
      // resolving a one-shot request. Nothing is applied — the UI previews it.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "push_fix_proposal") return;
        cb(
          toFixProposal(
            cmd as Extract<OverlayCommand, { type: "push_fix_proposal" }>,
          ),
        );
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    ask(question, onChunk, opts): AskHandle {
      // The answer stream (app.rs:5277-5394):
      //   push_card(question) → push_card(answer placeholder "Thinking…")
      //   → update_card(same id, growing body, done:false) × N
      //   → update_card(same id, done:true, optional cost_label/artifact).
      // We latch onto the FIRST answer-kind card pushed after we ask, capture
      // its id, then map subsequent update_card(body) deltas to onChunk text
      // deltas. We diff bodies so onChunk receives true incremental text.
      let answerId: string | null = null;
      let lastBody = "";
      let finished = false;

      const finish = () => {
        if (finished) return;
        finished = true;
        handlers.delete(handler);
      };

      const handler = (cmd: OverlayCommand) => {
        if (finished) return;

        if (cmd.type === "push_card") {
          const card = (cmd as Extract<OverlayCommand, { type: "push_card" }>)
            .card;
          // Bind to the first answer card that appears after we asked. The
          // question card (kind "question") is skipped.
          if (answerId === null && card.kind === "answer") {
            answerId = card.id;
            // The placeholder body is "Thinking…" — treat as empty so the
            // first real delta isn't polluted by the placeholder text.
            lastBody = "";
          }
          return;
        }

        if (cmd.type === "set_answer_status") {
          const s = cmd as Extract<
            OverlayCommand,
            { type: "set_answer_status" }
          >;
          if (answerId === null || s.id !== answerId) return;
          // The agent's REAL reasoning + tool calls, surfaced live. Map the wire
          // steps (already the same shape) into typed UI steps; the daemon
          // re-sends the whole list each change, so we replace, not append.
          const steps: AnswerStatusStep[] = s.steps.map((w) =>
            w.kind === "reasoning"
              ? { kind: "reasoning", text: w.text }
              : {
                  kind: "tool",
                  id: w.id,
                  title: w.title,
                  state:
                    w.state === "pending" ||
                    w.state === "running" ||
                    w.state === "done" ||
                    w.state === "failed"
                      ? w.state
                      : "running",
                },
          );
          onChunk({ status: steps, statusDone: s.done ?? false });
          return;
        }

        if (cmd.type === "update_card") {
          const u = cmd as Extract<OverlayCommand, { type: "update_card" }>;
          if (answerId === null || u.id !== answerId) return;

          // Emit the incremental text. Bodies are cumulative; if the new body
          // doesn't extend the old (rare reset), emit it whole.
          const body = u.body ?? "";
          const delta = body.startsWith(lastBody)
            ? body.slice(lastBody.length)
            : body;
          lastBody = body;
          if (delta.length > 0) {
            const chunk: AnswerChunk = { text: delta };
            onChunk(chunk);
          }

          if (u.done) {
            // An error card carries the failure as its body; flag it so the UI
            // renders a distinct retryable error state instead of styling the
            // failure message as if it were the answer. `fixable` gates the
            // "Fix this" affordance — only a completed, non-error, actionable
            // answer carries it (the daemon derives it from the agent's tag).
            const doneChunk: AnswerChunk = u.is_error
              ? { done: true, error: true }
              : { done: true, fixable: u.fixable ?? false };
            onChunk(doneChunk);
            finish();
          }
        }
      };

      handlers.add(handler);
      // Forward the optional answer-shaping fields verbatim under the daemon's
      // wire names (mode / provider / model). Only include keys that are set so
      // an unpinned ask carries exactly { type, question } as before and the
      // daemon's serde defaults (Option::None) kick in.
      sendEvent({
        type: "ask_requested",
        question,
        ...(opts?.mode ? { mode: opts.mode } : {}),
        ...(opts?.provider ? { provider: opts.provider } : {}),
        ...(opts?.model ? { model: opts.model } : {}),
      });

      return {
        cancel: () => {
          if (finished) return;
          finish();
          // First-class cancel: the local handler is already detached (finish()),
          // and this tells the daemon to reset its overlay UI state so the next
          // ask is clean.
          sendEvent({ type: "ask_cancel_requested" });
        },
      };
    },
  };
}
