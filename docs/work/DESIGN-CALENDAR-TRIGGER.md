# Calendar Trigger Architecture

**Branch:** `agent/calendar-push-trigger`
**Status:** design locked, not yet built
**Goal:** trigger pre-context + warm backend + participant roster when a meeting is
about to start — leanly, cross-platform, privacy-preserving (local-first).

---

## Decisions (locked)

1. **Providers: Google Calendar + Microsoft (Outlook/M365) only.** These cover ~all
   business meetings. No CalDAV / Fastmail / niche.
2. **EventKit is DROPPED as a path.** It only sees calendars the user manually added
   to macOS Calendar — most people live in Google/Outlook directly, so EventKit is
   blind for the majority and macOS-only. Good mechanism, wrong foundation.
3. **Local-first preserved.** Calendar tokens + event data live ON THE DEVICE. The
   backend (which we run anyway for OAuth secret / licensing) is a *content-free
   doorbell* — it never sees titles, attendees, or notes.

## Why not the alternatives

| Path | Free OS push | Works w/o Apple Calendar | Cross-platform | Verdict |
|------|-------------|--------------------------|----------------|---------|
| EventKit | ✅ local, no server | ❌ NO | ❌ macOS only | dropped |
| Google/MS **poll** | ❌ | ✅ | ✅ | fallback layer |
| Google/MS **webhook** | ✅ (needs server) | ✅ | ✅ | primary (we have a server) |

## How push actually works (web-confirmed)

- **Google** `events.watch`: register a channel with a **verified HTTPS callback**.
  Google POSTs a **bare "something changed" ping** (no data) to that URL. We then run
  **incremental sync** with the persisted `syncToken` → only added/updated/deleted
  events. (developers.google.com/workspace/calendar/api/guides/push + /sync)
- **Microsoft Graph** `subscription` + **`delta` query**: webhook is server-initiated
  (arrives in seconds); `delta` is pull-based incremental sync you poll. Subscriptions
  are stateful with an **expiry → renewal loop** required.
- **Both require a public HTTPS endpoint** for the webhook. A desktop daemon has no
  public URL → the webhook must land on OUR server, which relays a content-free nudge
  to the device.

## The architecture — "doorbell relay", local-first

```
Google/MS  --webhook "changed" ping-->  OUR SERVER (doorbell)
                                          |  (learns THAT user X changed, never WHAT)
                                          v  content-free "re-sync" nudge (push channel)
DEVICE DAEMON  --incremental sync (syncToken/delta) directly--> Google/MS
   |  (tokens + event data stay on device)
   v
 schedule ONE timer per upcoming meeting @ T-minus WARM_LEAD_SECS (180s)
   v
 warmup_open(title, participants)  ->  pre-context + roster to overlay   [ALREADY BUILT]
```

**Server sees:** metadata only (user X had a calendar change at time T). Never content.
**Device does:** all data fetching, all scheduling, all triggering. Stays local-first.

## Hybrid = production standard (web-confirmed)

Webhooks are unreliable/expiring, so BOTH Google and MS docs recommend the hybrid:
1. **Webhook** for fast "re-sync now" nudges (primary).
2. **Incremental sync** (`syncToken` / `delta`) — cheap, only changes.
3. **Long safety poll** (6–24h, or a few min if no webhook yet) — backstop for missed
   pings + expired subscriptions. Granola/Otter do exactly this.

## What ALREADY exists (do not rebuild)

- `CalendarSource` trait + `upcoming()` — `cue-core/src/calendar.rs`
- Cloud source fetching events + attendees for **Google AND Microsoft** —
  `cue-calendar-cloud/src/{google,microsoft}.rs`
- `UpcomingEvent { title, start, participants(name+email+organizer) }`
- `warmup_open()` pre-context/backend trigger, already fed the roster — `app.rs:1755`
- `due_for_warmup` + `fired_key` dedupe + `BLUEY_CALENDAR_FAKE_EVENTS` test hook

## What to BUILD (phased, lean-first)

**Phase 1 — device-only, no server (validatable TODAY):**
- Replace the 30s busy-poll (`app.rs:~1743`) with: on boot + each sync, read
  `upcoming()`, **schedule one `tokio::time::sleep_until` timer per event** at its
  T-minus-180s moment. Daemon sleeps to the next meeting, not a 30s heartbeat.
- Add **incremental sync** to the cloud source: persist `syncToken` (Google) /
  `delta` link (MS); each sync fetches only changes.
- Keep a **lean safety poll** (few min) driving re-sync.
- Test with `BLUEY_CALENDAR_FAKE_EVENTS`: timer fires warmup once per (event,
  occurrence), re-arms on a moved event, safety poll backstops a missed sync.

**Phase 2 — webhook doorbell (needs the server):**
- Server registers Google `watch` / MS `subscription` pointing at itself.
- On webhook POST, server sends the device a content-free "re-sync" nudge.
- Device runs the same incremental sync + timer scheduling from Phase 1.
- Subscription **renewal loop** on the server (Google ~days, MS ~3 days expiry).

**Blocker for live cloud validation:** real OAuth client IDs must be injected
(`BLUEY_GOOGLE_CLIENT_ID` / `BLUEY_MICROSOFT_CLIENT_ID`) — currently placeholders.
Phase 1 scheduler validates fully via the fake-events hook without them.

## Privacy guarantee (the point)

- Device holds tokens + all event content. Fetches directly from Google/MS.
- Server (Phase 2) only ever learns "user X changed at time T" — a doorbell, not a
  data processor. Meeting titles/attendees/notes never leave the device.
- This is strictly better than EventKit on privacy (no third surface) and works for
  every user regardless of macOS Calendar setup.
