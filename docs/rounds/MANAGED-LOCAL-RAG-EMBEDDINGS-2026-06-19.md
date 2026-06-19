# Managed Local RAG Embeddings

Date: 2026-06-19
Branch: `codex/bluey-ai-site`

## Goal

Keep retrieval fast and local while ensuring customers never need OpenAI,
Anthropic, Deepgram, Gemini, or other provider API keys on their laptops.

Local Bluey still owns the vector database and retrieval loop. The only change
is how vectors are created:

1. Bluey chunks local transcripts, Markdown document previews, screen notes, and
   compacted session summaries on-device.
2. For linked paid/trial accounts, the daemon calls authenticated
   `POST /router/embed` on `bluey-server`.
3. `bluey-server` chooses the provider key, applies account/provider rate
   limits, checks balance/trial state, deducts usage, records usage, and returns
   only the embedding vector plus billing metadata.
4. The desktop stores the vector locally in `rag_vectors.db`.

## Security Boundaries

- Provider keys remain server-only.
- Desktop requests are authenticated with the user account token, not provider
  keys.
- The daemon does not log access tokens, refresh tokens, provider keys, or raw
  provider responses.
- Managed embedding errors are mapped to short user-safe messages.
- The server `/router/embed` path already enforces idempotency, balance checks,
  rate limits, provider key health/cooldown, and usage recording.
- A local chunk-size guard caps any single managed embed request to prevent one
  malformed local artifact from becoming an oversized paid request.

## Files Changed

- `crates/cue-cloud-client/src/types.rs`
  - Updated typed `EmbedRequest` / `EmbedResponse` to match the server contract:
    `request_id`, `input`, and returned `vector`.
- `crates/cue-cloud-client/src/client.rs`
  - Added `CloudClient::embed()`.
- `crates/cue-daemon/src/rag_indexer.rs`
  - Added `ManagedBlueyEmbedder`, implementing `cue_rag::EmbeddingProvider`.
  - RAG initialization now prefers managed Bluey embeddings when an account is
    linked.
  - The coordinator can refresh after account linking, so users do not need to
    restart Bluey to enable managed local RAG.
  - Direct OpenAI embeddings remain an explicit developer fallback only.
- `crates/cue-daemon/src/app.rs`
  - Refreshes the RAG pipeline when `CloudStatus` observes a newly linked
    account.

## Operational Behavior

Customer install:

- User runs `bluey on`.
- If linked, RAG initializes with `bluey-managed`.
- Attachments and transcripts become local Markdown/text chunks.
- Embeddings are requested through `bluey-server`.
- Retrieval remains on-device against local SQLite vectors.

Unlinked install:

- RAG is disabled until the user signs in.
- After sign-in, the daemon receives `CloudStatus` and initializes managed RAG
  in-place.
- The overlay can still run local capture/session UI without provider keys.

Developer fallback:

- `BLUEY_DEV_BYOK=1` is required for direct local OpenAI embeddings. Without
  that dev flag, even a local `OPENAI_API_KEY` is ignored and the customer path
  uses Bluey managed `/router/embed`.

## Verification

Run before review:

```bash
cargo fmt --all --check
cargo test -p cue-cloud-client --all-targets
cargo test -p cue-daemon rag_indexer --lib
cargo test -p cue-daemon --test rag_integration
cargo clippy -p cue-cloud-client -p cue-daemon --all-targets -- -D warnings
```

Manual smoke after deploy/funding:

```bash
bluey on
# sign in with a funded test account
# attach a PDF/DOCX/MD/TXT file
# ask a question that requires the document
# confirm the answer references the document and balance moves by embed cost
```

## Areas To Review

- Whether the 8,192 character managed embed input cap should be lower or higher.
- Whether the server should expose a batch embed endpoint later to reduce
  per-chunk HTTP overhead for large documents.
