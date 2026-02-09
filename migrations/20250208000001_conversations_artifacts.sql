-- Migration: conversations_artifacts
-- Description: Add conversations, messages, artifacts,
-- and message_artifacts tables for the conversational
-- LLM interface and unified creative output entity.

-- ============================================================================
-- NEW ENUM TYPES
-- ============================================================================

CREATE TYPE artifact_kind AS ENUM ('storyboard', 'image', 'audio', 'video');
CREATE TYPE artifact_source AS ENUM ('upload', 'conversation', 'job');
CREATE TYPE conversation_status AS ENUM ('active', 'archived');
CREATE TYPE message_role AS ENUM ('user', 'assistant');

-- ============================================================================
-- CONVERSATIONS TABLE
-- ============================================================================

CREATE TABLE conversations (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title varchar(200),
    model varchar(100) NOT NULL, -- noqa: RF04
    system_prompt text CHECK (
        system_prompt IS NULL OR length(system_prompt) <= 10000
    ),
    status conversation_status NOT NULL DEFAULT 'active',
    message_count integer NOT NULL DEFAULT 0 CHECK (message_count >= 0),
    last_message_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Indexes for conversations
CREATE INDEX idx_conversations_user_status ON conversations (user_id, status);
CREATE INDEX idx_conversations_user_last_message ON conversations (
    user_id, last_message_at DESC
);

-- Trigger for updated_at
CREATE TRIGGER trigger_conversations_updated_at
BEFORE UPDATE ON conversations
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE conversations IS 'Chat threads between users and the LLM';

-- ============================================================================
-- ARTIFACTS TABLE
-- ============================================================================

-- Note: Reuses existing asset_status enum (pending/ready/failed)
CREATE TABLE artifacts (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner varchar(500) NOT NULL,       -- noqa: RF04
    created_by uuid NOT NULL REFERENCES users (id),
    project_id uuid REFERENCES projects (id) ON DELETE CASCADE,
    kind artifact_kind NOT NULL,
    status asset_status NOT NULL DEFAULT 'pending',
    source artifact_source NOT NULL DEFAULT 'upload',
    -- Media fields (required for image/audio/video, NULL for storyboard)
    filename varchar(255),
    s3_key varchar(500),
    content_type varchar(255),
    size_bytes bigint CHECK (
        size_bytes IS NULL OR (size_bytes > 0 AND size_bytes <= 52428800)
    ),
    -- Storyboard field (required for storyboard, NULL for media)
    spec jsonb,
    -- Provenance (FK to conversations added below, after conversations table)
    conversation_id uuid,
    source_job_id uuid REFERENCES jobs (id) ON DELETE SET NULL,
    -- General
    metadata jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    -- Constraints
    CONSTRAINT artifacts_s3_key_unique UNIQUE (s3_key),
    CONSTRAINT media_has_file_fields CHECK (
        (
            kind IN ('image', 'audio', 'video')
            AND filename IS NOT NULL
            AND s3_key IS NOT NULL
            AND content_type IS NOT NULL AND size_bytes IS NOT NULL
        )
        OR kind = 'storyboard'
    ),
    CONSTRAINT storyboard_has_spec CHECK (
        (kind = 'storyboard' AND spec IS NOT NULL) OR kind != 'storyboard'
    ),
    CONSTRAINT source_conversation_consistency CHECK (
        (source = 'conversation' AND conversation_id IS NOT NULL)
        OR source != 'conversation'
    ),
    CONSTRAINT source_job_consistency CHECK (
        (source = 'job' AND source_job_id IS NOT NULL) OR source != 'job'
    ),
    CONSTRAINT project_artifacts_team_owned CHECK (
        project_id IS NULL OR owner LIKE 'framecast:team:%'
    )
);

-- Add FK from artifacts.conversation_id to conversations.id
ALTER TABLE artifacts
ADD CONSTRAINT fk_artifacts_conversation
FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE SET NULL;

-- Indexes for artifacts
CREATE INDEX idx_artifacts_owner ON artifacts (owner);
CREATE INDEX idx_artifacts_project_id ON artifacts (project_id);
CREATE INDEX idx_artifacts_kind ON artifacts (kind);
CREATE INDEX idx_artifacts_created_by ON artifacts (created_by);

-- Trigger for updated_at
CREATE TRIGGER trigger_artifacts_updated_at
BEFORE UPDATE ON artifacts
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE artifacts
IS 'Creative outputs: storyboards, media';

-- ============================================================================
-- MESSAGES TABLE
-- ============================================================================

CREATE TABLE messages (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id uuid NOT NULL REFERENCES conversations (
        id
    ) ON DELETE CASCADE,
    role message_role NOT NULL, -- noqa: RF04
    content text NOT NULL CHECK (length(trim(content)) > 0), -- noqa: RF04
    artifacts jsonb,
    model varchar(100), -- noqa: RF04
    input_tokens integer,
    output_tokens integer,
    sequence integer NOT NULL CHECK (sequence >= 1), -- noqa: RF04
    created_at timestamptz NOT NULL DEFAULT now(),

    -- Constraints
    CONSTRAINT messages_unique_sequence UNIQUE (
        conversation_id, sequence
    )
);

-- Indexes for messages
CREATE INDEX idx_messages_conversation_sequence ON messages (
    conversation_id, sequence ASC
);

COMMENT ON TABLE messages
IS 'Individual turns in a conversation';

-- ============================================================================
-- MESSAGE_ARTIFACTS JOIN TABLE
-- ============================================================================

CREATE TABLE message_artifacts (
    message_id uuid NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    artifact_id uuid NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, artifact_id)
);

-- Index for reverse lookup (find messages referencing a given artifact)
CREATE INDEX idx_message_artifacts_artifact_id ON message_artifacts (
    artifact_id
);

COMMENT ON TABLE message_artifacts
IS 'Join table: messages to artifacts';
