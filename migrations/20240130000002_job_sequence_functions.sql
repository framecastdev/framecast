-- Migration: 002_job_sequence_functions.sql
-- Description: Add sequence generation and job constraint functions
-- Depends on: 001_initial_schema.sql

-- ============================================================================
-- SEQUENCE FUNCTIONS FOR JOB EVENTS
-- ============================================================================

-- Function to get next sequence number for job events
CREATE OR REPLACE FUNCTION get_next_job_event_sequence(job_uuid UUID)
RETURNS BIGINT AS $$
DECLARE
    next_seq BIGINT;
BEGIN
    -- Get the next sequence number for this job
    SELECT COALESCE(MAX(sequence), 0) + 1
    INTO next_seq
    FROM job_events
    WHERE job_id = job_uuid;

    RETURN next_seq;
END;
$$ LANGUAGE plpgsql;

-- Function to automatically set sequence on job event insert
CREATE OR REPLACE FUNCTION auto_set_job_event_sequence()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.sequence IS NULL OR NEW.sequence = 0 THEN
        NEW.sequence := get_next_job_event_sequence(NEW.job_id);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-set sequence
CREATE TRIGGER trigger_job_events_auto_sequence
BEFORE INSERT ON job_events
FOR EACH ROW
EXECUTE FUNCTION auto_set_job_event_sequence();

-- ============================================================================
-- JOB CONCURRENCY CONSTRAINTS
-- ============================================================================

-- Function to check job concurrency limits
CREATE OR REPLACE FUNCTION check_job_concurrency_limits()
RETURNS TRIGGER AS $$
DECLARE
    active_jobs_count INTEGER;
    user_tier user_tier;
    team_id UUID;
BEGIN
    -- Skip checks for non-queued jobs or updates that don't change status
    IF NEW.status != 'queued' AND (TG_OP = 'UPDATE' AND OLD.status = NEW.status) THEN
        RETURN NEW;
    END IF;

    -- Get user tier
    SELECT tier INTO user_tier FROM users WHERE id = NEW.triggered_by;

    -- CARD-6: Max 1 concurrent job per starter user
    IF user_tier = 'starter' THEN
        SELECT COUNT(*) INTO active_jobs_count
        FROM jobs
        WHERE triggered_by = NEW.triggered_by
        AND status IN ('queued', 'processing')
        AND (TG_OP = 'INSERT' OR id != NEW.id);

        IF active_jobs_count >= 1 THEN
            RAISE EXCEPTION 'Starter users can have maximum 1 concurrent job (CARD-6)';
        END IF;
    END IF;

    -- CARD-5: Max 5 concurrent jobs per team (for team-owned jobs)
    IF NEW.owner LIKE 'framecast:team:%' THEN
        -- Extract team ID from URN
        team_id := substring(NEW.owner from 'framecast:team:(.*)$')::UUID;

        SELECT COUNT(*) INTO active_jobs_count
        FROM jobs
        WHERE owner LIKE 'framecast:team:' || team_id::text || '%'
        AND status IN ('queued', 'processing')
        AND (TG_OP = 'INSERT' OR id != NEW.id);

        IF active_jobs_count >= 5 THEN
            RAISE EXCEPTION 'Teams can have maximum 5 concurrent jobs (CARD-5)';
        END IF;
    END IF;

    -- INV-J12: Max 1 active job per project
    IF NEW.project_id IS NOT NULL THEN
        SELECT COUNT(*) INTO active_jobs_count
        FROM jobs
        WHERE project_id = NEW.project_id
        AND status IN ('queued', 'processing')
        AND (TG_OP = 'INSERT' OR id != NEW.id);

        IF active_jobs_count >= 1 THEN
            RAISE EXCEPTION 'Projects can have maximum 1 active job at a time (INV-J12)';
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for job concurrency limits
CREATE TRIGGER trigger_jobs_check_concurrency_limits
BEFORE INSERT OR UPDATE ON jobs
FOR EACH ROW
EXECUTE FUNCTION check_job_concurrency_limits();

-- ============================================================================
-- JOB STATUS TRANSITION VALIDATION
-- ============================================================================

-- Function to validate job status transitions
CREATE OR REPLACE FUNCTION validate_job_status_transitions()
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
                RAISE EXCEPTION 'Invalid job status transition from % to %', OLD.status, NEW.status;
            END IF;

        WHEN 'processing' THEN
            IF NEW.status NOT IN ('completed', 'failed', 'canceled') THEN
                RAISE EXCEPTION 'Invalid job status transition from % to %', OLD.status, NEW.status;
            END IF;

        WHEN 'completed', 'failed', 'canceled' THEN
            RAISE EXCEPTION 'Cannot transition from terminal status %', OLD.status;

        ELSE
            RAISE EXCEPTION 'Unknown job status %', OLD.status;
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

-- Trigger for job status transitions
CREATE TRIGGER trigger_jobs_validate_status_transitions
BEFORE UPDATE ON jobs
FOR EACH ROW
EXECUTE FUNCTION validate_job_status_transitions();

-- ============================================================================
-- PROJECT STATUS AUTOMATION
-- ============================================================================

-- Function to update project status based on jobs
CREATE OR REPLACE FUNCTION update_project_status_from_jobs()
RETURNS TRIGGER AS $$
DECLARE
    project_uuid UUID;
    active_jobs_count INTEGER;
    completed_jobs_count INTEGER;
BEGIN
    -- Determine project ID
    project_uuid := COALESCE(NEW.project_id, OLD.project_id);

    -- Skip if not a project job
    IF project_uuid IS NULL THEN
        RETURN COALESCE(NEW, OLD);
    END IF;

    -- Count active and completed jobs for this project
    SELECT
        COUNT(*) FILTER (WHERE status IN ('queued', 'processing')),
        COUNT(*) FILTER (WHERE status = 'completed')
    INTO active_jobs_count, completed_jobs_count
    FROM jobs
    WHERE project_id = project_uuid;

    -- Update project status based on job states
    IF active_jobs_count > 0 THEN
        -- Has active jobs -> rendering
        UPDATE projects SET status = 'rendering' WHERE id = project_uuid AND status != 'rendering';
    ELSIF completed_jobs_count > 0 THEN
        -- Has completed jobs, no active -> completed
        UPDATE projects SET status = 'completed' WHERE id = project_uuid AND status != 'completed';
    ELSE
        -- No jobs or only failed/canceled -> draft
        UPDATE projects SET status = 'draft' WHERE id = project_uuid AND status NOT IN ('draft', 'archived');
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Triggers for project status updates
CREATE TRIGGER trigger_jobs_update_project_status
AFTER INSERT OR UPDATE OR DELETE ON jobs
FOR EACH ROW
EXECUTE FUNCTION update_project_status_from_jobs();

-- ============================================================================
-- RETENTION POLICIES
-- ============================================================================

-- Function to clean up old job events (called by cron/scheduler)
CREATE OR REPLACE FUNCTION cleanup_old_job_events()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM job_events
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

-- Function to get current job stats for monitoring
CREATE OR REPLACE FUNCTION get_job_stats()
RETURNS TABLE (
    status JOB_STATUS,
    count BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT j.status, COUNT(*)::BIGINT
    FROM jobs j
    GROUP BY j.status
    ORDER BY j.status;
END;
$$ LANGUAGE plpgsql;

-- Function to get team statistics
CREATE OR REPLACE FUNCTION get_team_stats(team_uuid UUID)
RETURNS TABLE (
    member_count BIGINT,
    project_count BIGINT,
    active_job_count BIGINT,
    total_credits_used BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        (SELECT COUNT(*) FROM memberships WHERE team_id = team_uuid)::BIGINT,
        (SELECT COUNT(*) FROM projects WHERE team_id = team_uuid)::BIGINT,
        (SELECT COUNT(*) FROM jobs WHERE owner LIKE 'framecast:team:' || team_uuid::text || '%'
         AND status IN ('queued', 'processing'))::BIGINT,
        (SELECT COALESCE(SUM(net_credits), 0) FROM usage
         WHERE owner LIKE 'framecast:team:' || team_uuid::text || '%')::BIGINT;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON FUNCTION get_next_job_event_sequence(
    UUID
) IS 'Generate next sequence number for job events';
COMMENT ON FUNCTION auto_set_job_event_sequence()
IS 'Auto-populate sequence field on job event insert';
COMMENT ON FUNCTION check_job_concurrency_limits()
IS 'Enforce job concurrency limits (CARD-5, CARD-6, INV-J12)';
COMMENT ON FUNCTION validate_job_status_transitions()
IS 'Validate and auto-set timestamps for job status changes';
COMMENT ON FUNCTION update_project_status_from_jobs()
IS 'Auto-update project status based on job states';
COMMENT ON FUNCTION cleanup_old_job_events()
IS 'Remove job events older than 7 days';
COMMENT ON FUNCTION cleanup_old_webhook_deliveries()
IS 'Remove webhook deliveries older than 30 days';
COMMENT ON FUNCTION is_valid_urn(TEXT) IS 'Validate URN format';
COMMENT ON FUNCTION check_api_key_owner_constraints()
IS 'Enforce API key ownership rules based on user tier';
COMMENT ON FUNCTION get_job_stats()
IS 'Get current job status distribution for monitoring';
COMMENT ON FUNCTION get_team_stats(
    UUID
) IS 'Get team statistics for dashboard display';

-- ============================================================================
-- END OF MIGRATION
-- ============================================================================
