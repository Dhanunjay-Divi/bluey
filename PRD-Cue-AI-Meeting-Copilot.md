# PRD: Bluey — AI Meeting Copilot

**Author:** pdivii  
**Date:** April 27, 2026  
**Status:** Draft  
**Version:** 1.0

---

## Table of Contents

1. [Vision](#1-vision)
2. [Problem Statement](#2-problem-statement)
3. [Target User](#3-target-user)
4. [Core Principles](#4-core-principles)
5. [User Experience](#5-user-experience)
6. [Architecture](#6-architecture)
7. [Feature Specification](#7-feature-specification)
8. [Screen-Capture Exclusion — Technical Deep Dive](#8-screen-capture-exclusion--technical-deep-dive)
9. [Data Flow — During a Meeting](#9-data-flow--during-a-meeting)
10. [Data Model](#10-data-model)
11. [Privacy & Security](#11-privacy--security)
12. [MVP Roadmap](#12-mvp-roadmap)
13. [Success Metrics](#13-success-metrics)
14. [Technical Risks & Mitigations](#14-technical-risks--mitigations)
15. [Tech Stack Summary](#15-tech-stack-summary)
16. [File/Directory Structure](#16-filedirectory-structure)

---

## 1. Vision

**Bluey** is a real-time AI meeting assistant that listens to your video calls, understands context, proactively surfaces answers and relevant content, and displays it in a private overlay invisible to screen capture. It knows your background, learns from every meeting, and gets smarter over time.

**Tagline**: *Your silent expert in every meeting.*

---

## 2. Problem Statement

Knowledge workers spend 15–30 hours/week in meetings. During these meetings they:

- Get asked questions they know the answer to but can't recall details fast enough
- Miss context from previous meetings ("Didn't we discuss this last week?")
- Lack real-time access to relevant docs, data, and talking points
- Waste time after meetings reconstructing what was said and decided
- Can't use visible note-taking tools during screen shares without exposing their "cheat sheet"

**No existing tool** combines real-time audio understanding, proactive intelligence, private overlay display, and cross-meeting memory in a single lightweight package.

---

## 3. Target User

**Primary (MVP):** Individual knowledge worker — software engineer/manager who attends 5–15 meetings/day across Chime, Zoom, Google Meet, and Teams.

**Secondary (future):** Team-wide deployment where each member has their own Bluey instance with shared team knowledge.

---

## 4. Core Principles

| Principle | Meaning |
|---|---|
| **Private by default** | Overlay is invisible to screen capture. No data leaves your machine without explicit consent. |
| **Zero-interaction** | No typing during meetings. Bluey listens, understands, and acts autonomously. |
| **Lightweight** | CLI-first. Minimal resource usage. No Electron bloat. |
| **Additive intelligence** | Every meeting makes Bluey smarter. Knowledge compounds. |
| **Extensible** | Plugin architecture from day one. New integrations without core changes. |

---

## 5. User Experience

### 5.1 Daily Flow

```
┌─────────────────────────────────────────────────────────┐
│  BEFORE MEETING                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │ $ cue start                                       │  │
│  │ > Bluey is running. Listening for meetings...       │  │
│  │ > 10:00 AM — "Sprint Planning" detected           │  │
│  │ > Pre-brief ready. 3 attendees profiled.          │  │
│  │ > Overlay active (invisible to screen capture).   │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
│  DURING MEETING                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │  [Overlay — only you see this]                    │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │ 🎯 Q: "What's the latency P99 for the      │  │  │
│  │  │     new endpoint?"                           │  │  │
│  │  │ 💡 A: 142ms (CloudWatch, last 7d avg).      │  │  │
│  │  │     Was 230ms before your optimization       │  │  │
│  │  │     on 4/15. See: [metric link]              │  │  │
│  │  │                                              │  │  │
│  │  │ 📋 Action Items (live):                      │  │  │
│  │  │  • John: update the runbook by Friday        │  │  │
│  │  │  • You: review PR #4521                      │  │  │
│  │  │                                              │  │  │
│  │  │ 🔗 Relevant: Design doc from 4/10 standup    │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
│  AFTER MEETING                                          │
│  ┌───────────────────────────────────────────────────┐  │
│  │ > Meeting ended. Summary saved.                   │  │
│  │ > 4 action items extracted.                       │  │
│  │ > Knowledge base updated with 12 new facts.       │  │
│  │ $ cue recap "Sprint Planning"                     │  │
│  │ > [full summary, decisions, action items]         │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### 5.2 Overlay Behavior

The overlay is a **transparent, always-on-top window** that:

- Floats in a corner of the screen (configurable position)
- Is **excluded from screen capture APIs** (macOS `CGWindowSharingNone`, Windows `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`)
- Shows a scrollable feed of proactive cards
- Can be toggled with a global hotkey (e.g., `Ctrl+Shift+C`)
- Auto-hides when no meeting is active
- Adjustable opacity and size

### 5.3 CLI Interface

```bash
# Lifecycle
cue start                     # Start daemon, begin listening
cue stop                      # Stop daemon
cue status                    # Show current state

# Knowledge
cue ingest <file|url|dir>     # Add docs to knowledge base
cue profile edit              # Edit your background/resume
cue knowledge search <query>  # Query the knowledge base

# Meetings
cue meetings list             # List recent meetings
cue recap <meeting>           # Get meeting summary
cue recap --last              # Recap the last meeting
cue action-items              # List open action items
cue action-items --mine       # Just yours

# Overlay
cue overlay show|hide|toggle  # Control overlay visibility
cue overlay position <pos>    # top-left, top-right, bottom-left, bottom-right
cue overlay opacity <0-100>   # Set transparency

# Config
cue config set <key> <value>  # Configure settings
cue config show               # Show current config
```

---

## 6. Architecture

### 6.1 High-Level System Design

```
┌──────────────────────────────────────────────────────────────────┐
│                         CUE SYSTEM                               │
│                                                                  │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────────┐  │
│  │  Audio    │───▶│  Transcriber │───▶│   Meeting Engine      │  │
│  │  Capture  │    │  (Whisper)   │    │                       │  │
│  │  (loopback│    └──────────────┘    │  • Intent Detection   │  │
│  │   + mic)  │                        │  • Q&A Recognition    │  │
│  └──────────┘                         │  • Topic Tracking     │  │
│                                       │  • Action Item Extrac │  │
│                                       └───────────┬───────────┘  │
│                                                   │              │
│                                                   ▼              │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Intelligence Layer                      │  │
│  │                                                           │  │
│  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐  │  │
│  │  │  Knowledge   │  │   LLM        │  │  Content       │  │  │
│  │  │  Base (RAG)  │◀─│   Orchestr.  │─▶│  Fetcher       │  │  │
│  │  │             │  │  (Bedrock)   │  │  (web/internal)│  │  │
│  │  └─────────────┘  └──────────────┘  └────────────────┘  │  │
│  └───────────────────────────┬───────────────────────────────┘  │
│                              │                                   │
│                              ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Presentation Layer                      │  │
│  │                                                           │  │
│  │  ┌──────────────────┐    ┌─────────────────────────────┐ │  │
│  │  │  Private Overlay  │    │  CLI Interface              │ │  │
│  │  │  (screen-capture  │    │  (commands, recap, config)  │ │  │
│  │  │   excluded)       │    │                             │ │  │
│  │  └──────────────────┘    └─────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Storage Layer                           │  │
│  │                                                           │  │
│  │  SQLite: meetings, transcripts, action items, config      │  │
│  │  Vector DB (local): embeddings for RAG                    │  │
│  │  File store: raw audio, uploaded docs                     │  │
│  └───────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 6.2 Component Breakdown

| Component | Technology | Why |
|---|---|---|
| **CLI** | Python (Click/Typer) | Cross-platform, fast to build, rich ecosystem |
| **Daemon** | Python async (asyncio) | Long-running process, event-driven |
| **Audio Capture** | macOS: CoreAudio (loopback) / Windows: WASAPI | OS-native, low latency, captures system audio + mic |
| **Transcription** | Whisper.cpp (local) or Amazon Transcribe Streaming | Local = private, Transcribe = higher accuracy |
| **LLM** | Amazon Bedrock (Claude) | Internal tool, scalable, no data leaves AWS |
| **Knowledge Base** | ChromaDB (local vector store) | Lightweight, embedded, no server needed |
| **Overlay** | macOS: Swift/AppKit window / Windows: Win32 API | Native APIs required for screen-capture exclusion |
| **Storage** | SQLite + local filesystem | Zero-config, portable, fast |
| **Calendar** | Microsoft Graph API / Google Calendar API | Meeting detection and pre-briefs |

### 6.3 Plugin Architecture

```
cue/
├── core/                    # Core engine (never changes for plugins)
│   ├── audio/
│   ├── transcription/
│   ├── intelligence/
│   └── overlay/
├── plugins/                 # Extensible
│   ├── sources/             # Knowledge sources
│   │   ├── web_search.py
│   │   ├── wiki_lookup.py
│   │   └── internal_docs.py
│   ├── integrations/        # External tools
│   │   ├── calendar.py
│   │   ├── slack.py
│   │   └── jira.py
│   ├── presenters/          # How cards are displayed
│   │   ├── overlay_card.py
│   │   └── cli_card.py
│   └── processors/          # Meeting processing
│       ├── action_items.py
│       ├── decisions.py
│       └── sentiment.py
└── config/
    └── plugins.yaml         # Enable/disable plugins
```

New integrations = new plugin file + entry in `plugins.yaml`. No core changes.

---

## 7. Feature Specification

### 7.1 Real-Time Audio Understanding

| Feature | Description | Priority |
|---|---|---|
| System audio capture | Capture all meeting audio via OS loopback | P0 |
| Microphone capture | Capture user's own voice | P0 |
| Speaker diarization | Identify who is speaking | P1 |
| Live transcription | Real-time speech-to-text | P0 |
| Question detection | Identify when a question is asked | P0 |
| Topic tracking | Track what's being discussed | P0 |
| Sentiment detection | Detect tension, confusion, agreement | P2 |

### 7.2 Proactive Intelligence

| Feature | Description | Priority |
|---|---|---|
| Auto-answer | Detect questions and surface answers from knowledge base | P0 |
| Relevant docs | Surface related docs/links based on current topic | P0 |
| Meeting cross-reference | "This was discussed in [meeting] on [date]" | P1 |
| Talking points | Suggest what you should say based on your expertise | P1 |
| Real-time web search | Fetch fresh data when discussion references external topics | P1 |
| Pre-meeting brief | Before meeting starts: attendee profiles, past context, agenda | P0 |
| Contradiction detection | Flag when someone says something that contradicts past decisions | P2 |

### 7.3 Knowledge Base

| Feature | Description | Priority |
|---|---|---|
| Document ingestion | Upload PDFs, docs, markdown, text files | P0 |
| Manual entry | Add facts, expertise, background via CLI | P0 |
| Auto-learning | Extract and store facts from every meeting | P0 |
| Semantic search | Query knowledge base with natural language | P0 |
| Knowledge graph | Connect entities across meetings (people, projects, decisions) | P1 |
| Decay/freshness | Weight recent knowledge higher than old | P2 |
| Conflict resolution | Handle contradictory information across meetings | P2 |

### 7.4 Private Overlay

| Feature | Description | Priority |
|---|---|---|
| Screen-capture exclusion | Invisible to all screen sharing/recording | P0 |
| Card-based UI | Scrollable feed of contextual cards | P0 |
| Global hotkey toggle | Show/hide instantly | P0 |
| Position/size/opacity | Fully configurable | P0 |
| Auto-show/hide | Appears when meeting starts, hides when it ends | P1 |
| Card types | Q&A, suggestion, action item, reference, alert | P1 |

### 7.5 Meeting Memory

| Feature | Description | Priority |
|---|---|---|
| Full transcript storage | Searchable transcripts of all meetings | P0 |
| Auto-summary | AI-generated summary after each meeting | P0 |
| Action item extraction | Detect and track action items with owners | P0 |
| Decision log | Track decisions made in meetings | P1 |
| Meeting linking | Connect related meetings (series, follow-ups) | P1 |
| Search across meetings | "When did we last discuss X?" | P0 |

### 7.6 User Profile

| Feature | Description | Priority |
|---|---|---|
| Resume/background | Your role, skills, experience | P0 |
| Project context | Current projects, responsibilities | P0 |
| Expertise areas | What you're an expert in (for answer generation) | P0 |
| Org chart awareness | Who reports to whom, team structure | P1 |
| Communication style | How you prefer to phrase things | P2 |

---

## 8. Screen-Capture Exclusion — Technical Deep Dive

This is the hardest and most critical feature. Here's the approach:

### macOS

```swift
// NSWindow with sharingType = .none
window.sharingType = .none  // CGWindowSharingNone
window.level = .floating    // Always on top
window.isOpaque = false     // Transparent background
window.backgroundColor = .clear
```

- `CGWindowSharingNone` excludes the window from `CGWindowListCopyWindowInfo`, which is what Zoom/Meet/Teams/Chime use for screen capture.
- **Verified working** with: Zoom, Google Meet (Chrome), Microsoft Teams, Amazon Chime.
- **Limitation:** OBS with specific capture modes may still capture it. Mitigated by using `SCContentFilter` exclusion on macOS 12.3+.

### Windows

```cpp
// SetWindowDisplayAffinity
SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);  // Windows 10 2004+
```

- Excludes window from `BitBlt`, `PrintWindow`, and DXGI capture.
- Works with all major meeting apps.

### Fallback

If OS-level exclusion fails for a specific app, fall back to a **companion display mode**: render content on a secondary device (phone/tablet) via local network.

---

## 9. Data Flow — During a Meeting

```
Audio Stream (system + mic)
    │
    ▼
┌─────────────────────┐
│ Audio Chunker        │  Splits into 3-5 second chunks
│ (VAD + buffering)    │  Voice Activity Detection to skip silence
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Transcriber          │  Whisper.cpp (local) or Transcribe Streaming
│                      │  Outputs: text + timestamps + speaker ID
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Meeting Engine       │  Maintains rolling context window (last 5 min)
│                      │  Detects: questions, topics, action items
│                      │  Triggers intelligence queries
└─────────┬───────────┘
          │
          ├──▶ Question detected ──▶ RAG query ──▶ Overlay card
          ├──▶ Topic shift ──▶ Relevant docs fetch ──▶ Overlay card
          ├──▶ Action item ──▶ Extract + store ──▶ Overlay card
          ├──▶ Reference to past ──▶ Meeting search ──▶ Overlay card
          └──▶ Unknown topic ──▶ Web search ──▶ Overlay card
```

**Latency target:** Question asked → answer on overlay in **< 3 seconds**.

---

## 10. Data Model

```sql
-- Core entities
CREATE TABLE meetings (
    id TEXT PRIMARY KEY,
    title TEXT,
    start_time INTEGER,
    end_time INTEGER,
    attendees JSON,        -- [{name, email, role}]
    calendar_event_id TEXT,
    summary TEXT,           -- AI-generated post-meeting
    tags JSON
);

CREATE TABLE transcripts (
    id TEXT PRIMARY KEY,
    meeting_id TEXT REFERENCES meetings(id),
    timestamp INTEGER,
    speaker TEXT,
    text TEXT,
    confidence REAL
);

CREATE TABLE action_items (
    id TEXT PRIMARY KEY,
    meeting_id TEXT REFERENCES meetings(id),
    owner TEXT,
    description TEXT,
    due_date TEXT,
    status TEXT DEFAULT 'open',  -- open, done, cancelled
    created_at INTEGER
);

CREATE TABLE decisions (
    id TEXT PRIMARY KEY,
    meeting_id TEXT REFERENCES meetings(id),
    description TEXT,
    context TEXT,
    decided_by TEXT,
    created_at INTEGER
);

CREATE TABLE knowledge (
    id TEXT PRIMARY KEY,
    source_type TEXT,       -- meeting, document, manual
    source_id TEXT,
    content TEXT,
    embedding BLOB,         -- vector for RAG
    created_at INTEGER,
    last_accessed INTEGER,
    access_count INTEGER DEFAULT 0
);

CREATE TABLE user_profile (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at INTEGER
);
```

---

## 11. Privacy & Security

| Concern | Approach |
|---|---|
| Audio data | Processed locally by default. Never stored raw unless user opts in. |
| Transcripts | Stored locally in encrypted SQLite (SQLCipher). |
| LLM calls | Via Bedrock — data stays in AWS, covered by AWS data policies. |
| PII in logs | All logging masks names, emails, IDs by default. |
| Knowledge base | Local vector store. No cloud sync unless user enables it. |
| Screen capture exclusion | OS-level enforcement, not application-level. |
| Credential storage | OS keychain (macOS Keychain / Windows Credential Manager). |
| Data retention | Configurable. Default: transcripts 90 days, summaries forever. |
| Team mode (future) | End-to-end encryption for shared knowledge. Per-user access controls. |

---

## 12. MVP Roadmap

### Phase 1 — Foundation (Weeks 1–3)

- [ ] CLI skeleton (`bluey start/stop/status`)
- [ ] Daemon process with health monitoring
- [ ] System audio capture (macOS first)
- [ ] Local transcription with Whisper.cpp
- [ ] SQLite storage layer
- [ ] Basic overlay window with screen-capture exclusion (macOS)

### Phase 2 — Intelligence (Weeks 4–6)

- [ ] Knowledge base with ChromaDB
- [ ] Document ingestion pipeline (PDF, markdown, text)
- [ ] User profile system
- [ ] Bedrock integration for LLM queries
- [ ] Question detection from transcript stream
- [ ] RAG-based answer generation
- [ ] Overlay card rendering (Q&A cards)

### Phase 3 — Meeting Memory (Weeks 7–8)

- [ ] Auto-summary generation post-meeting
- [ ] Action item extraction
- [ ] Cross-meeting search
- [ ] Meeting linking and topic tracking
- [ ] `bluey recap` and `bluey action-items` commands

### Phase 4 — Proactive Intelligence (Weeks 9–10)

- [ ] Pre-meeting briefs from calendar
- [ ] Topic-based document surfacing
- [ ] Real-time web search integration
- [ ] Talking point suggestions
- [ ] Past meeting cross-referencing

### Phase 5 — Polish & Windows (Weeks 11–13)

- [ ] Windows audio capture (WASAPI)
- [ ] Windows overlay (Win32 `WDA_EXCLUDEFROMCAPTURE`)
- [ ] Speaker diarization
- [ ] Overlay UX refinement (card types, animations, sizing)
- [ ] Performance optimization (< 5% CPU idle, < 15% active)
- [ ] Plugin architecture finalization

### Phase 6 — Team Mode (Future)

- [ ] Shared knowledge base with access controls
- [ ] Team meeting summaries
- [ ] Shared action item tracking
- [ ] Admin dashboard

---

## 13. Success Metrics

| Metric | Target |
|---|---|
| Question → answer latency | < 3 seconds |
| Transcription accuracy | > 90% WER |
| Answer relevance (self-rated) | > 80% useful |
| CPU usage (idle/active) | < 5% / < 15% |
| Memory usage | < 500MB |
| Overlay invisible to screen capture | 100% of tested apps |
| Meetings before knowledge base is "useful" | < 10 |
| Daily active usage after 2 weeks | Still using it every day |

---

## 14. Technical Risks & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Screen capture exclusion bypassed by specific app | Overlay content leaked | Test against all target apps. Fallback to companion device mode. |
| Whisper.cpp too slow on CPU | Transcription lag > 5s | Use Whisper small/tiny model. Fall back to Transcribe Streaming. |
| LLM hallucination in answers | Wrong info surfaced | RAG with source attribution. Confidence scoring. Show sources. |
| System audio capture blocked by OS permissions | No audio input | Clear setup wizard. Fallback to mic-only mode. |
| High CPU/memory during meetings | Laptop fans, battery drain | Aggressive chunking, model quantization, idle throttling. |
| Cross-platform overlay differences | Inconsistent UX | Native code per platform behind shared interface. |

---

## 15. Tech Stack Summary

| Layer | Technology |
|---|---|
| Language | Python 3.11+ (core) + Swift (macOS overlay) + C++ (Windows overlay) |
| CLI | Typer |
| Daemon | asyncio + watchdog |
| Audio | pyaudio + OS-native loopback |
| Transcription | whisper.cpp (via python bindings) \| Amazon Transcribe |
| LLM | Amazon Bedrock (Claude 3.5 Sonnet) |
| Vector Store | ChromaDB (local) |
| Database | SQLite (SQLCipher for encryption) |
| IPC | Unix domain sockets (macOS/Linux) / Named pipes (Windows) |
| Build | PyInstaller for distribution |
| Config | YAML (`~/.cue/config.yaml`) |

---

## 16. File/Directory Structure

```
~/.cue/
├── config.yaml              # User configuration
├── profile.yaml             # User background/resume
├── cue.db                   # SQLite database (encrypted)
├── knowledge/               # Vector store data
│   └── chroma/
├── meetings/                # Meeting artifacts
│   └── 2026-04-27/
│       ├── sprint-planning.transcript.json
│       └── sprint-planning.summary.md
├── documents/               # Ingested documents
├── plugins/                 # User-installed plugins
└── logs/
    └── cue.log
```

---

## Appendix: Open Questions

1. **Recording consent** — Should Bluey notify meeting participants that audio is being processed locally? (Legal implications vary by jurisdiction.)
2. **Multi-language support** — Should transcription support languages other than English?
3. **Offline mode** — Should Bluey work fully offline (no Bedrock), using a local LLM?
4. **Mobile companion** — Should there be a phone/tablet app as a fallback display?
5. **Data export** — What formats should meeting data be exportable in?

---

*This document is a living PRD. Update as requirements evolve.*
