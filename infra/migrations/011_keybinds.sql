-- Migration 011: User-customized keybinds table.
-- The ensure_keybinds_table() method in db/mod.rs uses CREATE TABLE IF NOT EXISTS
-- so this migration is a no-op at runtime but documents the schema for reference.
CREATE TABLE IF NOT EXISTS user_keybinds (
    action TEXT PRIMARY KEY,
    accelerator TEXT NOT NULL
);
