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
  lineId: string;
  start: number; // char offset of selection start within the line text
  end: number; // char offset of selection end
  lineLen: number;
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
      const text = startEl.textContent ?? "";
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
        lineId,
        start: Math.min(start, end),
        end: Math.max(start, end),
        lineLen: text.length,
        x: rect.left + rect.width / 2,
        y: rect.top,
      });
      setPicking(false);
    };
    document.addEventListener("selectionchange", onSelect);
    return () => document.removeEventListener("selectionchange", onSelect);
  }, []);

  if (!sel) return null;

  // A fresh speaker id = one past the max known.
  const newSpeakerId =
    knownSpeakers.reduce((m, s) => Math.max(m, s.id), -1) + 1;

  const assign = (speakerId: number) => {
    const client = getClient();
    const { lineId, start, end, lineLen } = sel;
    // A near-whole-line selection → reassign the whole line (the common case:
    // "this line is actually X"). A genuine PARTIAL selection → split the line at
    // the selection boundaries so the highlighted words become their own segment
    // assigned to the chosen speaker, and the rest keeps its speaker. This is
    // what lets two speakers share one section: highlight each person's words and
    // assign them separately.
    const nearWhole = start <= 1 && end >= lineLen - 1;
    if (nearWhole) {
      client.reassignSpan([lineId], speakerId);
    } else if (end >= lineLen - 1) {
      // Selection runs to the end → one split: [head keeps speaker | tail=target].
      client.splitSegment(lineId, start, -1, speakerId);
    } else if (start <= 1) {
      // Selection from the start → one split: [head=target | tail keeps speaker].
      client.splitSegment(lineId, end, speakerId, -1);
    } else {
      // Middle selection → split off the tail after the selection first (both
      // halves keep the original), then split the remaining head at `start` so
      // the selected middle (now the tail of the first split) becomes the target.
      client.splitSegment(lineId, end, -1, -1);
      client.splitSegment(lineId, start, -1, speakerId);
    }
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
            onClick={() => assign(newSpeakerId)}
          >
            <span className="fp-seltool-avatar is-new" aria-hidden>
              <PlusIcon size={12} />
            </span>
            New speaker
          </button>
        </div>
      )}
    </div>
  );
}
