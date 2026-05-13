# Product Strategy

Bluey is a commercial SaaS product, not an open-source/BYOK clone.

## Product Positioning

- Paid subscription product.
- Managed cloud account and secure sync.
- Managed model/provider routing by default.
- Enterprise-grade privacy, retention, deletion, and audit controls.
- RAG memory across meetings, documents, screenshots, and user-provided context.

## Privacy And Security Direction

- Store customer data securely in cloud infrastructure with encryption in transit and at rest.
- Use per-user and per-workspace authorization boundaries.
- Keep secrets out of logs and local state files.
- Provide deletion/export controls.
- Build visible, consent-based capture flows.
- Avoid product claims around bypassing monitoring, proctoring, or assessment controls.

## AI Direction

- Managed providers first.
- Local AI may remain a dev/offline fallback, but it is not the main product bet.
- RAG memory should combine:
  - meeting transcripts
  - recaps
  - user-attached documents
  - screenshots and OCR/vision summaries
  - decisions and action items
  - answer-style/persona instructions

## Commercial Features To Build

- Authenticated cloud sync.
- Workspace/team accounts.
- Billing and plan enforcement.
- Cloud RAG index.
- Meeting history dashboard.
- Managed model routing, fallbacks, and latency budgets.
- Personas/modes for different meeting types.
- Secure admin controls for retention and deletion.
