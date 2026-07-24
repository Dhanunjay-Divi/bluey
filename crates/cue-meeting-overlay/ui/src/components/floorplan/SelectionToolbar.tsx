// The transcript SELECTION toolbar — the "select which part a speaker spoke"
// interaction. When you highlight text inside a transcript line, a small
// floating toolbar appears offering "assign THIS span to a speaker". This is how
// you fix a section where the diarizer merged two people, or mislabeled part of
// a line: select the misattributed words and reassign just them.
//
// Mechanism: a selection lands inside one `.fp-line-text` (which carries its
// grouped-line id via data-line-id). We compute the char offsets of the
// selection within that line's text. Then:
//   • whole line selected → reassignSpan([lineId], speaker)
//   • a prefix/suffix/middle → splitSegment at the boundaries so the selected
//     words become their own segment assigned to the chosen speaker, and the
//     rest keeps its speaker. This is what lets MULTIPLE speakers live in one
//     section — each selected span is split off and assigned independently.

import { useEffect, useRef, useState } from "react";
import { getClient } from "../../lib";
import type { KnownSpeaker } from "./SpeakerEditor";
import { UserIcon, PlusIcon } from "../icons";

interface SelInfo {
  /** The grouped line's member segment ids, in order (the segments the daemon
   *  will walk to locate + split the selection precisely). */
  memberIds: string[];
  /** The char range of the selection within the grouped line's joined text. The
   *  daemon maps this range onto the concatenated member-segment texts, so ANY
   *  selection — mid-segment, spanning segments, anywhere — resolves precisely:
   *  fully-covered segments are reassigned whole; partially-covered boundary
   *  segments are split at the exact char, and only the covered part reassigned. */
  start: number;
  end: number;
  x: number; // viewport coords for the toolbar
  y: number;
}

/** Compute the char offset of a (node, offset) point within `root`'s text. */
function offsetWithin(root: Node, node: Node, nodeOffset: number): number {
  let acc = 0;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let n: Node | null = walker.nextNode();
  while (n) {
    if (n === node) return acc + nodeOffset;
    acc += (n.textContent ?? "").length;
    n = walker.nextNode();
  }
  return acc;
}

export function SelectionToolbar({
  knownSpeakers,
}: {
  knownSpeakers: KnownSpeaker[];
}) {
  const [sel, setSel] = useState<SelInfo | null>(null);
  const [picking, setPicking] = useState(false);
  const [naming, setNaming] = useState(false);
  const [newName, setNewName] = useState("");
  const barRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onSelect = () => {
      const s = window.getSelection();
      if (!s || s.isCollapsed || s.rangeCount === 0) {
        // keep the toolbar if the user is interacting with it
        if (!barRef.current?.contains(document.activeElement)) setSel(null);
        return;
      }
      const range = s.getRangeAt(0);
      // The selection must be INSIDE a single transcript line's text.
      const startEl = (range.startContainer.parentElement ?? null)?.closest(
        ".fp-line-text",
      ) as HTMLElement | null;
      const endEl = (range.endContainer.parentElement ?? null)?.closest(
        ".fp-line-text",
      ) as HTMLElement | null;
      if (!startEl || startEl !== endEl) {
        setSel(null);
        return;
      }
      const lineId = startEl.dataset.lineId ?? "";
      if (!lineId) {
        setSel(null);
        return;
      }
      // The raw segment ids this line is composed of (the grouper merges many).
      // We reassign these WHOLE segments — the UI has no per-segment text to
      // safely split at a sub-segment offset, and a grouped-line offset does NOT
      // map to any single segment (the bug that split " Scale " at offset 35).
      const memberIds = (startEl.dataset.memberIds ?? "")
        .split(",")
        .filter(Boolean);
      const start = offsetWithin(
        startEl,
        range.startContainer,
        range.startOffset,
      );
      const end = offsetWithin(startEl, range.endContainer, range.endOffset);
      if (end - start < 1) {
        setSel(null);
        return;
      }
      const rect = range.getBoundingClientRect();
      setSel({
        memberIds,
        start: Math.min(start, end),
        end: Math.max(start, end),
        x: rect.left + rect.width / 2,
        y: rect.top,
      });
      setPicking(false);
      setNaming(false);
      setNewName("");
    };
    document.addEventListener("selectionchange", onSelect);
    return () => document.removeEventListener("selectionchange", onSelect);
  }, []);

  if (!sel) return null;

  // A fresh speaker id = one past the max known.
  const newSpeakerId =
    knownSpeakers.reduce((m, s) => Math.max(m, s.id), -1) + 1;

  const assign = (speakerId: number, name?: string) => {
    const { memberIds, start, end } = sel;
    // ONE precise command: the daemon maps [start,end) onto the concatenated
    // member-segment texts, reassigns fully-covered segments whole, and splits
    // partially-covered boundary segments at the exact char — so ANY selection
    // (mid-segment, spanning segments, anywhere) is honored precisely. Replaces
    // the old grouped-line-offset splitSegment that mis-mapped (" Scale " split
    // at offset 35) and dropped the name.
    getClient().reassignRange(memberIds, start, end, speakerId, name);
    window.getSelection()?.removeAllRanges();
    setSel(null);
  };

  const style: React.CSSProperties = {
    position: "fixed",
    left: sel.x,
    top: sel.y - 8,
    transform: "translate(-50%, -100%)",
  };

  return (
    <div
      ref={barRef}
      className="fp-seltool"
      style={style}
      onMouseDown={(e) => e.preventDefault()} // keep the selection alive
    >
      {!picking ? (
        <button className="fp-seltool-btn" onClick={() => setPicking(true)}>
          <UserIcon size={13} />
          Assign speaker
        </button>
      ) : naming ? (
        <form
          className="fp-seltool-name"
          onSubmit={(e) => {
            e.preventDefault();
            const n = newName.trim();
            assign(newSpeakerId, n || undefined);
          }}
        >
          <input
            autoFocus
            className="fp-seltool-input"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="Name this speaker…"
            onKeyDown={(e) => {
              if (e.key === "Escape") setNaming(false);
            }}
          />
          <button
            type="submit"
            className="fp-seltool-apply"
            disabled={!newName.trim()}
          >
            Assign to “{newName.trim() || "…"}”
          </button>
        </form>
      ) : (
        <div className="fp-seltool-menu">
          <div className="fp-seltool-kicker">Assign selection to</div>
          {knownSpeakers.map((s) => (
            <button
              key={s.id}
              className="fp-seltool-row"
              onClick={() => assign(s.id)}
            >
              <span className="fp-seltool-avatar" aria-hidden>
                {s.label.slice(0, 1).toUpperCase()}
              </span>
              {s.label}
            </button>
          ))}
          <button
            className="fp-seltool-row is-new"
            onClick={() => setNaming(true)}
          >
            <span className="fp-seltool-avatar is-new" aria-hidden>
              <PlusIcon size={12} />
            </span>
            New speaker…
          </button>
        </div>
      )}
    </div>
  );
}
