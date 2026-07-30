-- Target: SQLite
-- Persist bounded semantic-row quarantine evidence for global discovery runs.

-- Existing SQLite databases are upgraded with schema inspection in
-- server/src/db/mod.rs. The fresh schema already includes these columns.
SELECT 1;
