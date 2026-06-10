# Live Currency + Memory Readiness — 2026-06-10

## Goal

Prepare Bluey for the next live-currency smoke where a user installs, signs in, adds credits, asks real questions, attaches context, restores an older session, and expects accurate answers without memory drift.

## What changed

- Restored/continued sessions now trigger a background rebuild of the local RAG index.
- Opening a specific old session from session history also triggers the same memory rebuild.
- The rebuild now indexes compacted summaries and prior Bluey Q&A, not only raw transcript and attached documents.
- Answer prompts now include the compacted session summary directly, before bounded transcript, recent Q&A, attached context, and RAG hits.

## Runtime Answer Context

For each answer, Bluey now assembles context in this order:

1. Saved compacted session summary, if present.
2. Last 32 final transcript turns, capped to 8,000 characters.
3. Last 10 Bluey Q&A turns.
4. Up to 12 recent ready attached context artifacts.
5. Local RAG hits from the current session.
6. Local RAG hits from older sessions.

This keeps immediate answers fast while still letting restored sessions warm their deeper index in the background.

## Tomorrow Live Test Checklist

- Confirm account is signed in and balance is visible in the overlay.
- Ask a short typed question and verify answer starts quickly.
- Attach supported docs and verify they show as context chips.
- Ask a doc-dependent question and confirm the answer cites or uses the attached content.
- Stop and restart Bluey, then load the saved session from the drawer.
- Ask a follow-up that depends on earlier session summary or Q&A.
- Confirm usage/cost appears on the web account page after paid cloud requests.
- Confirm no provider keys or debug-only overlay flags are present in desktop/user-visible logs.

## Known Follow-Up

The desktop local RAG pipeline still requires an OpenAI embedding key in the local environment or dev keychain. In managed customer mode, cloud sync stores memory chunks server-side and `/sync/rag/query` can perform lexical retrieval, but true managed cloud vector search needs server-side embedding generation during sync/query. That is the next production-grade memory upgrade after tomorrow's live smoke.

