-- Revert: Drop conversations, artifacts, messages, and message_artifacts tables

-- Drop tables in reverse dependency order
DROP TABLE IF EXISTS message_artifacts;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS artifacts;
DROP TABLE IF EXISTS conversations;

-- Drop triggers (already dropped via CASCADE, but explicit for clarity)
DROP TRIGGER IF EXISTS trigger_artifacts_updated_at ON artifacts;
DROP TRIGGER IF EXISTS trigger_conversations_updated_at ON conversations;

-- Drop enum types
DROP TYPE IF EXISTS message_role;
DROP TYPE IF EXISTS conversation_status;
DROP TYPE IF EXISTS artifact_source;
DROP TYPE IF EXISTS artifact_kind;
