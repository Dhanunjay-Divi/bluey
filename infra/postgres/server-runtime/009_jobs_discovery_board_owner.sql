-- Target: PostgreSQL only.
-- One public ATS board has one authoritative Career Track owner per account.
-- The application preflight remains advisory; this unique index is the
-- concurrency boundary for imports and scheduled discovery.

CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_discovery_sources_board_owner
  ON jobs_discovery_sources(account_id, provider, source_key);
