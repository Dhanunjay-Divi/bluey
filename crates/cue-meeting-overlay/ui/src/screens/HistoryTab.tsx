// The History tab — one tab, two lenses under a segmented sub-toggle:
//   • Meetings — my past recorded meetings (MeetingsScreen).
//   • Sessions — the attached agent's prior threads to resume (HistoryScreen).
// This merges what used to be a separate "Meetings" top tab and the
// agent-session list. The Agents tab (attach/detach + its own embedded PAST
// SESSIONS list) stays its own tab — only these two HISTORY lenses merge here.
//
// Each lens is its own component that mounts/unmounts as the sub-toggle flips, so
// a lens's on-mount effect ("revalidate on focus") runs whenever it re-shows.
// Both lenses read the shared SWR store, so flipping shows cached data instantly
// (no refetch spinner after the first load) while a background revalidate keeps
// it fresh.

import { useEffect, useState } from "react";
import { SegmentedTabs } from "../components/primitives";
import { useDataStore } from "../lib/dataStore";
import { MeetingsScreen } from "./MeetingsScreen";
import { HistoryScreen } from "./HistoryScreen";

type Lens = "Meetings" | "Sessions";
const LENSES: readonly Lens[] = ["Meetings", "Sessions"];

export function HistoryTab({
  attachedKind,
  onResumeAgentThread,
  onResumeSession,
}: {
  /** The attached agent's kind, or null when nothing is attached — drives the
   *  Sessions lens (null → its "attach an agent" empty state). */
  attachedKind: string | null;
  /** Resume the agent thread a past meeting chained (from the Meetings lens). */
  onResumeAgentThread: (kind: string | undefined, sessionId: string) => void;
  /** Resume a prior thread of an agent (from the Sessions lens). */
  onResumeSession: (kind: string, sessionId: string) => void;
}) {
  const [lens, setLens] = useState<Lens>("Meetings");

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "center",
          padding: "8px 12px 4px",
        }}
      >
        <SegmentedTabs tabs={LENSES} value={lens} onChange={setLens} />
      </div>

      <div
        style={{
          // Flex column that fills below the sub-toggle; the active lens
          // (Meetings/Sessions) owns its OWN internal scroll (flex:1 +
          // overflowY), so this wrapper must NOT also scroll — a nested
          // scroll-in-scroll would strand the inner list at a short height.
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {lens === "Meetings" && (
          <MeetingsLens onResumeAgentThread={onResumeAgentThread} />
        )}
        {lens === "Sessions" && (
          <HistoryScreen
            kind={attachedKind}
            onResume={(sid) =>
              attachedKind && onResumeSession(attachedKind, sid)
            }
          />
        )}
      </div>
    </div>
  );
}

// A thin wrapper so mounting the Meetings lens (i.e. flipping to it) triggers a
// background revalidate of past meetings — "revalidate on focus" for free,
// without a visibilitychange listener. Cached meetings show instantly meanwhile.
function MeetingsLens({
  onResumeAgentThread,
}: {
  onResumeAgentThread: (kind: string | undefined, sessionId: string) => void;
}) {
  const { revalidateMeetings } = useDataStore();
  useEffect(() => {
    revalidateMeetings();
  }, [revalidateMeetings]);
  return <MeetingsScreen onResumeAgentThread={onResumeAgentThread} />;
}
