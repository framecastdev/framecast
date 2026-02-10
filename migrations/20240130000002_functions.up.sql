-- Migration: 002_functions.sql
-- Description: Add sequence generation, generation constraints,
--              status transitions, project status automation,
--              retention policies, URN validation,
--              API key constraints, and utility functions
-- Depends on: 001_schema.sql

-- ============================================================================
-- SEQUENCE FUNCTIONS FOR GENERATION EVENTS
-- ============================================================================

-- Function to get next sequence number for generation events
CREATE OR REPLACE FUNCTION get_next_generation_event_sequence(
    generation_uuid UUID
)
RETURNS BIGINT AS $$
DECLARE
    next_seq BIGINT;
BEGIN
    -- Get the next sequence number for this generation
    SELECT COALESCE(MAX(sequence), 0) + 1
    INTO next_seq
    FROM generation_events
    WHERE generation_id = generation_uuid;

    RETURN next_seq;
END;
$$ LANGUAGE plpgsql;

-- Function to automatically set sequence on generation event insert
CREATE OR REPLACE FUNCTION auto_set_generation_event_sequence()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.sequence IS NULL OR NEW.sequence = 0 THEN
        NEW.sequence := get_next_generation_event_sequence(NEW.generation_id);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-set sequence
CREATE TRIGGER trigger_generation_events_auto_sequence
BEFORE INSERT ON generation_events
FOR EACH ROW
EXECUTE FUNCTION auto_set_generation_event_sequence();

-- ============================================================================
-- GENERATION CONCURRENCY CONSTRAINTS
-- ============================================================================

-- Function to check generation concurrency limits
CREATE OR REPLACE FUNCTION check_generation_concurrency_limits()
RETURNS TRIGGER AS $$
DECLARE
    active_generations_count INTEGER;
    user_tier user_tier;
    team_id UUID;
BEGIN
    -- Skip checks for non-queued generations or updates that don't change status
    IF NEW.status != 'queued' AND (TG_OP = 'UPDATE' AND OLD.status = NEW.status) THEN
        RETURN NEW;
    END IF;

    -- Get user tier
    SELECT tier INTO user_tier FROM users WHERE id = NEW.triggered_by;

    -- CARD-6: Max 1 concurrent generation per starter user
    IF user_tier = 'starter' THEN
        SELECT COUNT(*) INTO active_generations_count
        FROM generations
        WHERE triggered_by = NEW.triggered_by
        AND status IN ('queued', 'processing')
        AND (TG_OP = 'INSERT' OR id != NEW.id);

        IF active_generations_count >= 1 THEN
            RAISE EXCEPTION 'Starter users can have maximum 1 concurrent generation (CARD-6)';
        END IF;
    END IF;

    -- CARD-5: Max 5 concurrent generations per team (for team-owned generations)
    IF NEW.owner LIKE 'framecast:team:%' THEN
        -- Extract team ID from URN
        team_id := substring(NEW.owner from 'framecast:team:(.*)$')::UUID;

        SELECT COUNT(*) INTO active_generations_count
        FROM generations
        WHERE owner LIKE 'framecast:team:' || team_id::text || '%'
        AND status IN ('queued', 'processing')
        AND (TG_OP = 'INSERT' OR id != NEW.id);

        IF active_generations_count >= 5 THEN
            RAISE EXCEPTION 'Teams can have maximum 5 concurrent generations (CARD-5)';
        END IF;
    END IF;

    -- INV-G12: Max 1 active generation per project
    IF NEW.project_id IS NOT NULL THEN
        SELECT COUNT(*) INTO active_generations_count
        FROM generations
        WHERE project_id = NEW.project_id
        AND status IN ('queued', 'processing')
        AND (TG_OP = 'INSERT' OR id != NEW.id);

        IF active_generations_count >= 1 THEN
            RAISE EXCEPTION 'Projects can have maximum 1 active generation at a time (INV-G12)';
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for generation concurrency limits
CREATE TRIGGER trigger_generations_check_concurrency_limits
BEFORE INSERT OR UPDATE ON generations
FOR EACH ROW
EXECUTE FUNCTION check_generation_concurrency_limits();

-- ============================================================================
-- GENERATION STATUS TRANSITION VALIDATION
-- ============================================================================

-- Function to validate generation status transitions
CREATE OR REPLACE FUNCTION validate_generation_status_transitions()
RETURNS TRIGGER AS $$
BEGIN
    -- Skip validation for inserts
    IF TG_OP = 'INSERT' THEN
        RETURN NEW;
    END IF;

    -- Skip if status hasn't changed
    IF OLD.status = NEW.status THEN
        RETURN NEW;
    END IF;

    -- Valid transitions:
    -- queued -> processing, failed, canceled
    -- processing -> completed, failed, canceled
    -- completed/failed/canceled -> no transitions allowed

    CASE OLD.status
        WHEN 'queued' THEN
            IF NEW.status NOT IN ('processing', 'failed', 'canceled') THEN
                RAISE EXCEPTION 'Invalid generation status transition from % to %', OLD.status, NEW.status;
            END IF;

        WHEN 'processing' THEN
            IF NEW.status NOT IN ('completed', 'failed', 'canceled') THEN
                RAISE EXCEPTION 'Invalid generation status transition from % to %', OLD.status, NEW.status;
            END IF;

        WHEN 'completed', 'failed', 'canceled' THEN
            RAISE EXCEPTION 'Cannot transition from terminal status %', OLD.status;

        ELSE
            RAISE EXCEPTION 'Unknown generation status %', OLD.status;
    END CASE;

    -- Set completion timestamp for terminal states
    IF NEW.status IN ('completed', 'failed', 'canceled') AND NEW.completed_at IS NULL THEN
        NEW.completed_at := NOW();
    END IF;

    -- Set started_at when moving to processing
    IF NEW.status = 'processing' AND NEW.started_at IS NULL THEN
        NEW.started_at := NOW();
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for generation status transitions
CREATE TRIGGER trigger_generations_validate_status_transitions
BEFORE UPDATE ON generations
FOR EACH ROW
EXECUTE FUNCTION validate_generation_status_transitions();

-- ============================================================================
-- PROJECT STATUS AUTOMATION
-- ============================================================================

-- Function to update project status based on generations
CREATE OR REPLACE FUNCTION update_project_status_from_generations()
RETURNS TRIGGER AS $$
DECLARE
    project_uuid UUID;
    active_generations_count INTEGER;
    completed_generations_count INTEGER;
BEGIN
    -- Determine project ID
    project_uuid := COALESCE(NEW.project_id, OLD.project_id);

    -- Skip if not a project generation
    IF project_uuid IS NULL THEN
        RETURN COALESCE(NEW, OLD);
    END IF;

    -- Count active and completed generations for this project
    SELECT
        COUNT(*) FILTER (WHERE status IN ('queued', 'processing')),
        COUNT(*) FILTER (WHERE status = 'completed')
    INTO active_generations_count, completed_generations_count
    FROM generations
    WHERE project_id = project_uuid;

    -- Update project status based on generation states
    IF active_generations_count > 0 THEN
        -- Has active generations -> rendering
        UPDATE projects SET status = 'rendering' WHERE id = project_uuid AND status != 'rendering';
    ELSIF completed_generations_count > 0 THEN
        -- Has completed generations, no active -> completed
        UPDATE projects SET status = 'completed' WHERE id = project_uuid AND status != 'completed';
    ELSE
        -- No generations or only failed/canceled -> draft
        UPDATE projects SET status = 'draft' WHERE id = project_uuid AND status NOT IN ('draft', 'archived');
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Triggers for project status updates
CREATE TRIGGER trigger_generations_update_project_status
AFTER INSERT OR UPDATE OR DELETE ON generations
FOR EACH ROW
EXECUTE FUNCTION update_project_status_from_generations();

-- ============================================================================
-- RETENTION POLICIES
-- ============================================================================

-- Function to clean up old generation events (called by cron/scheduler)
CREATE OR REPLACE FUNCTION cleanup_old_generation_events()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM generation_events
    WHERE created_at < NOW() - INTERVAL '7 days';

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Function to clean up old webhook deliveries (called by cron/scheduler)
CREATE OR REPLACE FUNCTION cleanup_old_webhook_deliveries()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM webhook_deliveries
    WHERE created_at < NOW() - INTERVAL '30 days';

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- URN VALIDATION FUNCTIONS
-- ============================================================================

-- Function to validate URN format
CREATE OR REPLACE FUNCTION is_valid_urn(urn_text TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN urn_text ~ '^framecast:(user:[a-zA-Z0-9_]+|team:[a-zA-Z0-9_]+|[a-zA-Z0-9_]+:[a-zA-Z0-9_]+)$';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to validate API key owner URN constraints
CREATE OR REPLACE FUNCTION check_api_key_owner_constraints()
RETURNS TRIGGER AS $$
DECLARE
    user_tier user_tier;
BEGIN
    -- Get user tier
    SELECT tier INTO user_tier FROM users WHERE id = NEW.user_id;

    -- INV-K1: Starter users can only have personal URNs
    IF user_tier = 'starter' AND NEW.owner != ('framecast:user:' || NEW.user_id::text) THEN
        RAISE EXCEPTION 'Starter users can only have personal API keys';
    END IF;

    -- INV-K2: Team/team-user URNs require creator tier
    IF (NEW.owner LIKE 'framecast:team:%' OR NEW.owner ~ 'framecast:[a-zA-Z0-9_]+:[a-zA-Z0-9_]+')
       AND user_tier != 'creator' THEN
        RAISE EXCEPTION 'Team API keys require creator tier';
    END IF;

    -- Validate URN format
    IF NOT is_valid_urn(NEW.owner) THEN
        RAISE EXCEPTION 'Invalid URN format: %', NEW.owner;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for API key owner constraints
CREATE TRIGGER trigger_api_keys_check_owner_constraints
BEFORE INSERT OR UPDATE ON api_keys
FOR EACH ROW
EXECUTE FUNCTION check_api_key_owner_constraints();

-- ============================================================================
-- UTILITY FUNCTIONS
-- ============================================================================

-- Function to get current generation stats for monitoring
CREATE OR REPLACE FUNCTION get_generation_stats()
RETURNS TABLE (
    status GENERATION_STATUS,
    count BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT g.status, COUNT(*)::BIGINT
    FROM generations g
    GROUP BY g.status
    ORDER BY g.status;
END;
$$ LANGUAGE plpgsql;

-- Function to get team statistics
CREATE OR REPLACE FUNCTION get_team_stats(team_uuid UUID)
RETURNS TABLE (
    member_count BIGINT,
    project_count BIGINT,
    active_generation_count BIGINT,
    total_credits_used BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        (SELECT COUNT(*) FROM memberships WHERE team_id = team_uuid)::BIGINT,
        (SELECT COUNT(*) FROM projects WHERE team_id = team_uuid)::BIGINT,
        (SELECT COUNT(*) FROM generations WHERE owner LIKE 'framecast:team:' || team_uuid::text || '%'
         AND status IN ('queued', 'processing'))::BIGINT,
        (SELECT COALESCE(SUM(credits_used - credits_refunded), 0) FROM usage
         WHERE owner LIKE 'framecast:team:' || team_uuid::text || '%')::BIGINT;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON FUNCTION get_next_generation_event_sequence(
    UUID
) IS 'Generate next sequence number for generation events';
COMMENT ON FUNCTION auto_set_generation_event_sequence()
IS 'Auto-populate sequence field on generation event insert';
COMMENT ON FUNCTION check_generation_concurrency_limits()
IS 'Enforce generation concurrency limits (CARD-5, CARD-6, INV-G12)';
COMMENT ON FUNCTION validate_generation_status_transitions()
IS 'Validate and auto-set timestamps for generation status changes';
COMMENT ON FUNCTION update_project_status_from_generations()
IS 'Auto-update project status based on generation states';
COMMENT ON FUNCTION cleanup_old_generation_events()
IS 'Remove generation events older than 7 days';
COMMENT ON FUNCTION cleanup_old_webhook_deliveries()
IS 'Remove webhook deliveries older than 30 days';
COMMENT ON FUNCTION is_valid_urn(TEXT) IS 'Validate URN format';
COMMENT ON FUNCTION check_api_key_owner_constraints()
IS 'Enforce API key ownership rules based on user tier';
COMMENT ON FUNCTION get_generation_stats()
IS 'Get current generation status distribution for monitoring';
COMMENT ON FUNCTION get_team_stats(
    UUID
) IS 'Get team statistics for dashboard display';
