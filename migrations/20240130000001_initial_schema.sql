-- Migration: 001_initial_schema.sql
-- Description: Create initial database schema with all core entities
-- Based on: docs/spec/04_Entities.md and docs/spec/06_Invariants.md

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- ENUM TYPES
-- ============================================================================

-- User tier enum
CREATE TYPE user_tier AS ENUM ('starter', 'creator');

-- Membership role enum
CREATE TYPE membership_role AS ENUM ('owner', 'admin', 'member', 'viewer');

-- Invitation role enum (excludes 'owner')
CREATE TYPE invitation_role AS ENUM ('admin', 'member', 'viewer');

-- Project status enum
CREATE TYPE project_status AS ENUM ('draft', 'rendering', 'completed', 'archived');

-- Job status enum
CREATE TYPE job_status AS ENUM ('queued', 'processing', 'completed', 'failed', 'canceled');

-- Job failure type enum
CREATE TYPE job_failure_type AS ENUM ('system', 'validation', 'timeout', 'canceled');

-- Job event type enum
CREATE TYPE job_event_type AS ENUM ('queued', 'started', 'progress', 'scene_complete', 'completed', 'failed', 'canceled');

-- Asset status enum
CREATE TYPE asset_status AS ENUM ('pending', 'ready', 'failed');

-- Webhook delivery status enum
CREATE TYPE webhook_delivery_status AS ENUM ('pending', 'retrying', 'delivered', 'failed');

-- System asset category enum
CREATE TYPE system_asset_category AS ENUM ('sfx', 'ambient', 'music', 'transition');

-- Valid webhook events
CREATE TYPE webhook_event_type AS ENUM ('job.queued', 'job.started', 'job.progress', 'job.completed', 'job.failed', 'job.canceled');

-- ============================================================================
-- TABLES
-- ============================================================================

-- User table
-- Note: id matches Supabase Auth user ID, authentication handled externally
CREATE TABLE users (
    id                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email                   VARCHAR(255) NOT NULL UNIQUE,
    name                    VARCHAR(100),
    avatar_url              TEXT,
    tier                    user_tier NOT NULL DEFAULT 'starter',
    credits                 INTEGER NOT NULL DEFAULT 0 CHECK (credits >= 0),
    ephemeral_storage_bytes BIGINT NOT NULL DEFAULT 0 CHECK (ephemeral_storage_bytes >= 0),
    upgraded_at             TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_email CHECK (email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
    CONSTRAINT tier_upgrade_consistency CHECK (
        (tier = 'creator' AND upgraded_at IS NOT NULL) OR
        (tier = 'starter' AND upgraded_at IS NULL)
    )
);

-- Indexes for users
CREATE INDEX idx_users_tier ON users(tier);
CREATE INDEX idx_users_email ON users(email);

-- Team table
CREATE TABLE teams (
    id                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name                    VARCHAR(100) NOT NULL CHECK (LENGTH(TRIM(name)) >= 1),
    slug                    VARCHAR(50) NOT NULL UNIQUE,
    credits                 INTEGER NOT NULL DEFAULT 0 CHECK (credits >= 0),
    ephemeral_storage_bytes BIGINT NOT NULL DEFAULT 0 CHECK (ephemeral_storage_bytes >= 0),
    settings                JSONB NOT NULL DEFAULT '{}',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_slug CHECK (slug ~ '^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$'),
    CONSTRAINT timestamps_order CHECK (created_at <= updated_at)
);

-- Indexes for teams
CREATE UNIQUE INDEX idx_teams_slug ON teams(slug);

-- Membership table
CREATE TABLE memberships (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    team_id    UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       membership_role NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT unique_team_user UNIQUE (team_id, user_id)
);

-- Indexes for memberships
CREATE INDEX idx_memberships_user_id ON memberships(user_id);
CREATE INDEX idx_memberships_team_id ON memberships(team_id);

-- Invitation table
CREATE TABLE invitations (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    team_id      UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    invited_by   UUID NOT NULL REFERENCES users(id),
    email        VARCHAR(255) NOT NULL,
    role         invitation_role NOT NULL DEFAULT 'member',
    token        VARCHAR(255) NOT NULL UNIQUE,
    expires_at   TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '7 days',
    accepted_at  TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_email_invitation CHECK (email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
    CONSTRAINT acceptance_revocation_exclusion CHECK (
        NOT (accepted_at IS NOT NULL AND revoked_at IS NOT NULL)
    )
);

-- Indexes for invitations
CREATE UNIQUE INDEX idx_invitations_token ON invitations(token);
CREATE INDEX idx_invitations_email ON invitations(email);
CREATE INDEX idx_invitations_team_email ON invitations(team_id, email);

-- ApiKey table
CREATE TABLE api_keys (
    id            UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    owner         VARCHAR(500) NOT NULL, -- URN format
    name          VARCHAR(100) NOT NULL DEFAULT 'Default',
    key_prefix    VARCHAR(20) NOT NULL,
    key_hash      VARCHAR(255) NOT NULL UNIQUE,
    scopes        JSONB NOT NULL DEFAULT '["*"]',
    last_used_at  TIMESTAMPTZ,
    expires_at    TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for api_keys
CREATE UNIQUE INDEX idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_api_keys_owner ON api_keys(owner);

-- Project table
CREATE TABLE projects (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    team_id    UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id),
    name       VARCHAR(200),
    status     project_status NOT NULL DEFAULT 'draft',
    spec       JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for projects
CREATE INDEX idx_projects_team_id ON projects(team_id);
CREATE INDEX idx_projects_created_by ON projects(created_by);
CREATE INDEX idx_projects_status ON projects(status);
CREATE INDEX idx_projects_team_updated ON projects(team_id, updated_at DESC);

-- Job table
CREATE TABLE jobs (
    id                 UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner              VARCHAR(500) NOT NULL, -- URN format
    triggered_by       UUID NOT NULL REFERENCES users(id),
    project_id         UUID REFERENCES projects(id) ON DELETE SET NULL,
    status             job_status NOT NULL DEFAULT 'queued',
    spec_snapshot      JSONB NOT NULL,
    options            JSONB NOT NULL DEFAULT '{}',
    progress           JSONB NOT NULL DEFAULT '{}',
    output             JSONB,
    output_size_bytes  BIGINT,
    error              JSONB,
    credits_charged    INTEGER NOT NULL DEFAULT 0 CHECK (credits_charged >= 0),
    failure_type       job_failure_type,
    credits_refunded   INTEGER NOT NULL DEFAULT 0 CHECK (credits_refunded >= 0),
    idempotency_key    VARCHAR(255),
    started_at         TIMESTAMPTZ,
    completed_at       TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT refund_not_exceeding_charge CHECK (credits_refunded <= credits_charged),
    CONSTRAINT terminal_jobs_have_completion CHECK (
        (status IN ('completed', 'failed', 'canceled') AND completed_at IS NOT NULL) OR
        status NOT IN ('completed', 'failed', 'canceled')
    ),
    CONSTRAINT project_jobs_team_owned CHECK (
        (project_id IS NULL) OR
        (project_id IS NOT NULL AND owner LIKE 'framecast:team:%')
    ),
    CONSTRAINT failure_type_when_failed CHECK (
        (status IN ('failed', 'canceled') AND failure_type IS NOT NULL) OR
        status NOT IN ('failed', 'canceled')
    )
);

-- Indexes for jobs
CREATE INDEX idx_jobs_owner ON jobs(owner);
CREATE INDEX idx_jobs_triggered_by ON jobs(triggered_by);
CREATE INDEX idx_jobs_project_id ON jobs(project_id);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE INDEX idx_jobs_created_at ON jobs(created_at DESC);
CREATE UNIQUE INDEX idx_jobs_idempotency ON jobs(triggered_by, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- JobEvent table
CREATE TABLE job_events (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    job_id     UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    sequence   BIGINT NOT NULL,
    event_type job_event_type NOT NULL,
    payload    JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT unique_job_sequence UNIQUE (job_id, sequence)
);

-- Indexes for job_events
CREATE INDEX idx_job_events_job_created ON job_events(job_id, created_at ASC);
CREATE INDEX idx_job_events_job_sequence ON job_events(job_id, sequence ASC);

-- AssetFile table
CREATE TABLE asset_files (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner        VARCHAR(500) NOT NULL, -- URN format
    uploaded_by  UUID NOT NULL REFERENCES users(id),
    project_id   UUID REFERENCES projects(id) ON DELETE CASCADE,
    filename     VARCHAR(255) NOT NULL,
    s3_key       VARCHAR(500) NOT NULL UNIQUE,
    content_type VARCHAR(255) NOT NULL,
    size_bytes   BIGINT NOT NULL CHECK (size_bytes > 0 AND size_bytes <= 52428800), -- 50MB max
    status       asset_status NOT NULL DEFAULT 'pending',
    metadata     JSONB NOT NULL DEFAULT '{}',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_content_types CHECK (
        content_type IN (
            'image/jpeg', 'image/png', 'image/webp',
            'audio/mpeg', 'audio/wav', 'audio/ogg',
            'video/mp4'
        )
    )
);

-- Indexes for asset_files
CREATE UNIQUE INDEX idx_asset_files_s3_key ON asset_files(s3_key);
CREATE INDEX idx_asset_files_owner ON asset_files(owner);
CREATE INDEX idx_asset_files_project_id ON asset_files(project_id);
CREATE INDEX idx_asset_files_uploaded_by ON asset_files(uploaded_by);

-- Webhook table
CREATE TABLE webhooks (
    id                 UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    team_id            UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    created_by         UUID NOT NULL REFERENCES users(id),
    url                VARCHAR(2048) NOT NULL,
    events             webhook_event_type[] NOT NULL,
    secret             VARCHAR(255) NOT NULL,
    is_active          BOOLEAN NOT NULL DEFAULT TRUE,
    last_triggered_at  TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT webhook_events_not_empty CHECK (array_length(events, 1) > 0),
    CONSTRAINT webhook_https_only CHECK (url LIKE 'https://%')
);

-- Indexes for webhooks
CREATE INDEX idx_webhooks_team_id ON webhooks(team_id);
CREATE INDEX idx_webhooks_team_active ON webhooks(team_id, is_active);

-- WebhookDelivery table
CREATE TABLE webhook_deliveries (
    id               UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    webhook_id       UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    job_id           UUID REFERENCES jobs(id) ON DELETE SET NULL,
    event_type       VARCHAR(50) NOT NULL,
    status           webhook_delivery_status NOT NULL DEFAULT 'pending',
    payload          JSONB NOT NULL,
    response_status  INTEGER,
    response_body    VARCHAR(10240), -- 10KB max
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL DEFAULT 5,
    next_retry_at    TIMESTAMPTZ,
    delivered_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for webhook_deliveries
CREATE INDEX idx_webhook_deliveries_webhook_id ON webhook_deliveries(webhook_id);
CREATE INDEX idx_webhook_deliveries_retry ON webhook_deliveries(status, next_retry_at)
    WHERE status = 'retrying';
CREATE INDEX idx_webhook_deliveries_created ON webhook_deliveries(created_at);

-- Usage table
CREATE TABLE usage (
    id               UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner            VARCHAR(500) NOT NULL, -- URN format
    period           VARCHAR(7) NOT NULL, -- Format: 'YYYY-MM'
    renders_count    INTEGER NOT NULL DEFAULT 0,
    render_seconds   INTEGER NOT NULL DEFAULT 0,
    credits_used     INTEGER NOT NULL DEFAULT 0,
    credits_refunded INTEGER NOT NULL DEFAULT 0,
    api_calls        INTEGER NOT NULL DEFAULT 0,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT unique_owner_period UNIQUE (owner, period),
    CONSTRAINT valid_period_format CHECK (period ~ '^\d{4}-\d{2}$')
);

-- Indexes for usage
CREATE INDEX idx_usage_period ON usage(period);

-- SystemAsset table
CREATE TABLE system_assets (
    id               VARCHAR(100) PRIMARY KEY,
    category         system_asset_category NOT NULL,
    name             VARCHAR(255) NOT NULL,
    description      VARCHAR(500),
    duration_seconds DECIMAL(10,3),
    s3_key           VARCHAR(500) NOT NULL UNIQUE,
    content_type     VARCHAR(255) NOT NULL,
    size_bytes       BIGINT NOT NULL CHECK (size_bytes > 0),
    tags             VARCHAR(50)[] NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_asset_id CHECK (id ~ '^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$')
);

-- Indexes for system_assets
CREATE INDEX idx_system_assets_category ON system_assets(category);
CREATE INDEX idx_system_assets_tags ON system_assets USING GIN(tags);

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- Function to update timestamps
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Triggers for updated_at columns
CREATE TRIGGER trigger_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_teams_updated_at
    BEFORE UPDATE ON teams
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_projects_updated_at
    BEFORE UPDATE ON projects
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_jobs_updated_at
    BEFORE UPDATE ON jobs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_asset_files_updated_at
    BEFORE UPDATE ON asset_files
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_webhooks_updated_at
    BEFORE UPDATE ON webhooks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Function to auto-generate team slug
CREATE OR REPLACE FUNCTION generate_team_slug()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.slug IS NULL OR NEW.slug = '' THEN
        -- Generate base slug from name
        NEW.slug := lower(regexp_replace(NEW.name, '[^a-zA-Z0-9]', '-', 'g'));
        NEW.slug := regexp_replace(NEW.slug, '-+', '-', 'g');
        NEW.slug := trim(both '-' from NEW.slug);

        -- Ensure it's not empty
        IF NEW.slug = '' THEN
            NEW.slug := 'team';
        END IF;

        -- Add random suffix if slug exists
        WHILE EXISTS (SELECT 1 FROM teams WHERE slug = NEW.slug) LOOP
            NEW.slug := NEW.slug || '-' || substring(md5(random()::text) from 1 for 6);
        END LOOP;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for team slug generation
CREATE TRIGGER trigger_teams_generate_slug
    BEFORE INSERT ON teams
    FOR EACH ROW
    EXECUTE FUNCTION generate_team_slug();

-- ============================================================================
-- FUNCTIONS FOR BUSINESS LOGIC ENFORCEMENT
-- ============================================================================

-- Function to check membership constraints
CREATE OR REPLACE FUNCTION check_membership_constraints()
RETURNS TRIGGER AS $$
BEGIN
    -- INV-M4: Only creator users can have memberships
    IF (SELECT tier FROM users WHERE id = NEW.user_id) != 'creator' THEN
        RAISE EXCEPTION 'Only creator users can have team memberships (INV-M4)';
    END IF;

    -- Check team membership limits
    IF (SELECT COUNT(*) FROM memberships WHERE user_id = NEW.user_id) >= 50 THEN
        RAISE EXCEPTION 'User cannot be member of more than 50 teams (INV-T8)';
    END IF;

    -- Check ownership limits for owners
    IF NEW.role = 'owner' THEN
        IF (SELECT COUNT(*) FROM memberships
            WHERE user_id = NEW.user_id AND role = 'owner') >= 10 THEN
            RAISE EXCEPTION 'User cannot own more than 10 teams (INV-T7)';
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for membership constraints
CREATE TRIGGER trigger_memberships_check_constraints
    BEFORE INSERT OR UPDATE ON memberships
    FOR EACH ROW
    EXECUTE FUNCTION check_membership_constraints();

-- Function to prevent removing last owner/member
CREATE OR REPLACE FUNCTION prevent_empty_teams()
RETURNS TRIGGER AS $$
DECLARE
    remaining_members INTEGER;
    remaining_owners INTEGER;
BEGIN
    -- Count remaining members after deletion
    SELECT COUNT(*) INTO remaining_members
    FROM memberships
    WHERE team_id = OLD.team_id AND id != OLD.id;

    -- INV-T1: Every team must have at least one member
    IF remaining_members = 0 THEN
        RAISE EXCEPTION 'Cannot remove last member from team (INV-T1)';
    END IF;

    -- INV-T2: Every team must have at least one owner
    IF OLD.role = 'owner' THEN
        SELECT COUNT(*) INTO remaining_owners
        FROM memberships
        WHERE team_id = OLD.team_id AND role = 'owner' AND id != OLD.id;

        IF remaining_owners = 0 THEN
            RAISE EXCEPTION 'Cannot remove last owner from team (INV-T2)';
        END IF;
    END IF;

    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

-- Trigger to prevent empty teams
CREATE TRIGGER trigger_memberships_prevent_empty_teams
    BEFORE DELETE ON memberships
    FOR EACH ROW
    EXECUTE FUNCTION prevent_empty_teams();

-- Function to validate invitation constraints
CREATE OR REPLACE FUNCTION check_invitation_constraints()
RETURNS TRIGGER AS $$
BEGIN
    -- Cannot invite existing team member
    IF EXISTS (
        SELECT 1 FROM memberships m
        JOIN users u ON m.user_id = u.id
        WHERE m.team_id = NEW.team_id AND u.email = NEW.email
    ) THEN
        RAISE EXCEPTION 'Cannot invite existing team member';
    END IF;

    -- Cannot invite self
    IF EXISTS (
        SELECT 1 FROM users
        WHERE id = NEW.invited_by AND email = NEW.email
    ) THEN
        RAISE EXCEPTION 'Cannot invite yourself';
    END IF;

    -- Check pending invitation limit (max 50 per team)
    IF (SELECT COUNT(*) FROM invitations
        WHERE team_id = NEW.team_id
        AND accepted_at IS NULL
        AND revoked_at IS NULL
        AND expires_at > NOW()) >= 50 THEN
        RAISE EXCEPTION 'Team has reached maximum pending invitations limit (50)';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for invitation constraints
CREATE TRIGGER trigger_invitations_check_constraints
    BEFORE INSERT ON invitations
    FOR EACH ROW
    EXECUTE FUNCTION check_invitation_constraints();

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE users IS 'Application users (authentication via Supabase Auth)';
COMMENT ON TABLE teams IS 'Team workspaces that own projects and assets';
COMMENT ON TABLE memberships IS 'User-team associations with roles';
COMMENT ON TABLE invitations IS 'Pending team invitations';
COMMENT ON TABLE api_keys IS 'API authentication keys';
COMMENT ON TABLE projects IS 'Storyboard projects containing specs';
COMMENT ON TABLE jobs IS 'Video generation jobs (ephemeral or project-based)';
COMMENT ON TABLE job_events IS 'Job progress events for SSE streaming';
COMMENT ON TABLE asset_files IS 'User-uploaded reference files';
COMMENT ON TABLE webhooks IS 'HTTP callback registrations';
COMMENT ON TABLE webhook_deliveries IS 'Webhook delivery attempt records';
COMMENT ON TABLE usage IS 'Aggregated usage metrics for billing';
COMMENT ON TABLE system_assets IS 'System-provided audio/visual assets';

-- ============================================================================
-- GRANTS (if needed for specific users)
-- ============================================================================

-- Grant permissions to application user (adjust as needed)
-- GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO framecast_app;
-- GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO framecast_app;

-- ============================================================================
-- END OF MIGRATION
-- ============================================================================
