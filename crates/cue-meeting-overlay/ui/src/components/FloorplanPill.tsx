// The Dynamic-Island pill — the collapsed surface for the Open Floor Plan.
//
// Unlike the old static Pill (one caption bar), this MORPHS by meeting state,
// the way the iOS Dynamic Island does: it grows to surface a detected question,
// a live "thinking" turn, or a fresh answer peek, then settles back to the
// compact caption bar when idle/listening. Each state has its own shape, accent
// and affordance, and the whole content springs/crossfades between them.
//
// It reads the SAME live meeting state the Open Floor document does
// (useMeetingState → transcript / detectedQ / turns) so the pill and the open
// panel never disagree, plus the ambient listening state off the client. All
// styling flows through the nude `[data-floorplan]` tokens, so it re-skins with
// the rest of the UI and needs no palette of its own.

import { useEffect, useMemo, useRef, useState } from "react";
import type { MutableRefObject, RefObject } from "react";
import type { MeetingClient } from "../lib/client";
import type { AgentSummary, ListeningState } from "../lib/types";
import { useMeetingState } from "../lib/meetingState";
import type { PillSize } from "../lib/useCollapse";
import { SparkleIcon, SpinnerIcon, SystemAudioIcon, StopIcon } from "./icons";

/** The derived pill state, most-urgent first. */
type PillPhase =
  | "issue"
  | "answering"
  | "peek"
  | "detected"
  | "listening"
  | "idle";

/** Which morph-window size each phase wants. */
const PHASE_SIZE: Record<PillPhase, PillSize> = {
  issue: "compact",
  answering: "alert",
  peek: "peek",
  detected: "alert",
  listening: "compact",
  idle: "compact",
};

function tail(text: string, max: number): string {
  const t = text.replace(/^\s+/, "");
  return t.length > max ? "…" + t.slice(-max) : t;
}

export function FloorplanPill({
  client,
  attached,
  onExpand,
  setPillSize,
  dragRef,
  didDragRef,
}: {
  client: MeetingClient;
  attached: AgentSummary | null;
  onExpand: () => void;
  setPillSize: (size: PillSize) => void;
  dragRef: RefObject<HTMLDivElement | null>;
  didDragRef: MutableRefObject<boolean>;
}) {
  const {
    transcript,
    detectedQ,
    turns,
    listenState: persistedListen,
    systemInputOn: persistedSystem,
    micInputOn: persistedMicrophone,
    permissionDeniedSource,
    permissionSettingsOpenedFor,
    preparePermissionRetry,
  } = useMeetingState();
  const [listen, setListen] = useState<ListeningState>(persistedListen);
  const [sources, setSources] = useState({
    system: persistedSystem,
    microphone: persistedMicrophone,
  });
  // The pill mounts only when the panel collapses. Seed and synchronize it from
  // the never-unmounted provider so capture that started before collapse is
  // correct even before the next daemon push arrives.
  useEffect(() => {
    setListen(persistedListen);
    setSources({
      system: persistedSystem,
      microphone: persistedMicrophone,
    });
  }, [persistedListen, persistedSystem, persistedMicrophone]);
  useEffect(
    () =>
      client.onListeningState((state, nextSources) => {
        setListen(state);
        if (nextSources) {
          setSources(nextSources);
          return;
        }
        // Compatibility with an older daemon that only sent the aggregate
        // state: the collapsed shortcut historically controlled system audio.
        if (state === "listening" || state === "connecting") {
          setSources((current) => ({ ...current, system: true }));
        } else if (
          state === "idle" ||
          state === "paused" ||
          state === "failed" ||
          state === "permission_denied"
        ) {
          setSources({ system: false, microphone: false });
        }
      }),
    [client],
  );

  // The newest turn drives the answering/peek states.
  const lastTurn = turns.length ? turns[turns.length - 1] : null;
  const answering =
    !!lastTurn && !lastTurn.answer.done && !lastTurn.answer.error;
  // Show a fresh answer as a "peek" only briefly after it lands, then fall
  // through to the ambient states — the pill shouldn't camp on a stale answer.
  const [peekTurnId, setPeekTurnId] = useState<number | null>(null);
  useEffect(() => {
    if (!lastTurn) return;
    if (
      lastTurn.answer.done &&
      lastTurn.answer.text &&
      !lastTurn.answer.error
    ) {
      setPeekTurnId(lastTurn.id);
      const t = window.setTimeout(() => setPeekTurnId(null), 6000);
      return () => window.clearTimeout(t);
    }
  }, [
    lastTurn?.id,
    lastTurn?.answer.done,
    lastTurn?.answer.text,
    lastTurn?.answer.error,
  ]);
  const peeking =
    !!lastTurn && lastTurn.id === peekTurnId && lastTurn.answer.done;

  const capturing = listen === "listening" || listen === "connecting";
  const systemCapturing = sources.system;
  const live = listen === "listening";
  const permissionIssue =
    permissionDeniedSource !== null || listen === "permission_denied";
  const systemPermissionDenied =
    permissionDeniedSource === "system" ||
    (permissionDeniedSource === null && listen === "permission_denied");
  const systemSettingsOpened = permissionSettingsOpenedFor.includes("system");
  const issue = listen === "failed" || permissionIssue;

  const phase: PillPhase = issue
    ? "issue"
    : answering
      ? "answering"
      : peeking
        ? "peek"
        : detectedQ
          ? "detected"
          : capturing
            ? "listening"
            : "idle";

  // Drive the OS window morph as the phase changes. Debounced by the phase
  // value itself (effect only re-runs when the mapped size changes).
  const wantSize = PHASE_SIZE[phase];
  useEffect(() => {
    setPillSize(wantSize);
  }, [wantSize, setPillSize]);

  // A brief flash ring when a new caption fragment lands (ambient "heard you").
  const [flash, setFlash] = useState(false);
  const flashTimer = useRef<number | null>(null);
  useEffect(() => {
    if (!transcript?.text) return;
    setFlash(true);
    if (flashTimer.current) window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlash(false), 650);
    return () => {
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
    };
  }, [transcript?.text]);

  // A live "thinking" timer for the answering state.
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!answering) {
      setElapsed(0);
      return;
    }
    const start = Date.now();
    const iv = window.setInterval(
      () => setElapsed(Math.floor((Date.now() - start) / 1000)),
      500,
    );
    return () => window.clearInterval(iv);
  }, [answering, lastTurn?.id]);

  const caption = useMemo(
    () => (transcript?.text ? tail(transcript.text, 64) : ""),
    [transcript?.text],
  );
  const expandUnlessDragged = () => {
    if (didDragRef.current) {
      didDragRef.current = false;
      return;
    }
    onExpand();
  };

  return (
    <div className="fp-pill-root" data-phase={phase}>
      <div className={`fp-pill${flash ? " is-flash" : ""}`} data-phase={phase}>
        {/* LEADING — the state glyph (also the accent anchor). */}
        <span className="fp-pill-lead" aria-hidden>
          {phase === "answering" ? (
            <span className="fp-pill-spin">
              <SpinnerIcon size={15} />
            </span>
          ) : phase === "detected" || phase === "peek" ? (
            <SparkleIcon size={15} />
          ) : (
            <span
              className={`fp-pill-dot${live ? " is-live" : ""}${
                issue ? " is-issue" : ""
              }`}
              title={
                permissionIssue
                  ? "permission needed"
                  : listen === "failed"
                    ? "audio issue"
                    : live
                      ? "listening"
                      : "idle"
              }
            />
          )}
        </span>

        {/* BODY — tap to expand; also the drag handle. Content morphs by phase. */}
        <div
          ref={dragRef}
          onClick={expandUnlessDragged}
          role="button"
          aria-label="Open Bluey"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === " ") e.preventDefault();
            if (e.key === "Enter" || e.key === " ") onExpand();
          }}
          className="fp-pill-body"
          style={{ userSelect: "none", WebkitUserSelect: "none" }}
        >
          {phase === "answering" && (
            <>
              <span className="fp-pill-label">Thinking</span>
              <span className="fp-pill-sub">{elapsed}s</span>
            </>
          )}

          {phase === "peek" && lastTurn && (
            <>
              <span className="fp-pill-label">Answer</span>
              <span className="fp-pill-sub fp-pill-clamp">
                {tail(lastTurn.answer.text, 72)}
              </span>
            </>
          )}

          {phase === "detected" && detectedQ && (
            <>
              <span className="fp-pill-label">They asked</span>
              <span className="fp-pill-sub fp-pill-clamp">
                {tail(detectedQ.title || detectedQ.text, 64)}
              </span>
            </>
          )}

          {phase === "listening" && (
            <span className="fp-pill-caption">{caption || "listening…"}</span>
          )}

          {phase === "issue" && (
            <span className="fp-pill-caption is-issue">
              {permissionIssue
                ? "Permission needed · tap to open"
                : "Audio issue · tap to open"}
            </span>
          )}

          {phase === "idle" && (
            <span className="fp-pill-caption is-muted">
              {attached
                ? `${attached.displayName} · tap to open`
                : "Bluey · tap to open"}
            </span>
          )}
        </div>

        {/* TRAILING — the one-tap affordance for this phase. */}
        {phase === "detected" || phase === "peek" ? (
          <button
            type="button"
            className="fp-pill-cta"
            aria-label="Open to answer"
            onClick={(e) => {
              e.stopPropagation();
              onExpand();
            }}
          >
            Ask
          </button>
        ) : phase === "answering" ? null : (
          <button
            type="button"
            className={`fp-pill-toggle${systemCapturing ? " is-on" : ""}`}
            aria-label={
              systemPermissionDenied
                ? systemSettingsOpened
                  ? "Retry system audio"
                  : "Grant system audio permission"
                : systemCapturing
                  ? "Stop system audio"
                  : "Listen to system audio"
            }
            title={
              systemPermissionDenied
                ? systemSettingsOpened
                  ? "Retry system audio"
                  : "Grant system audio permission"
                : systemCapturing
                  ? "Stop system audio"
                  : "Listen (system audio)"
            }
            onClick={(e) => {
              e.stopPropagation();
              if (systemPermissionDenied && preparePermissionRetry("system")) {
                return;
              }
              const system = !systemCapturing;
              setSources((current) => ({ ...current, system }));
              if (!system && !sources.microphone) {
                client.stopListening();
              } else {
                client.startListening({
                  microphone: sources.microphone,
                  system,
                });
              }
            }}
          >
            {systemCapturing ? (
              <StopIcon size={14} />
            ) : (
              <SystemAudioIcon size={14} />
            )}
          </button>
        )}
      </div>
    </div>
  );
}
