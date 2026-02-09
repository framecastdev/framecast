-- Migration: add_character_artifact_kind
-- Description: Add 'character' variant to artifact_kind enum.
-- NOTE: ALTER TYPE ADD VALUE cannot run inside a transaction in PostgreSQL.
-- sqlx detects this and runs the migration outside a transaction automatically.

ALTER TYPE artifact_kind ADD VALUE 'character';
