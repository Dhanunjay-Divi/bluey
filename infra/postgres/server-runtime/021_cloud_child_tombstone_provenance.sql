-- Preserve identifier-only RAG provenance on child tombstones so every
-- account-scoped device can remove derived local memory without restoring the
-- deleted text or embedding payload.
-- Target: PostgreSQL 16+.

ALTER TABLE cloud_child_tombstones
  ADD COLUMN IF NOT EXISTS source_kind TEXT,
  ADD COLUMN IF NOT EXISTS source_id TEXT,
  ADD COLUMN IF NOT EXISTS chunk_index BIGINT;
