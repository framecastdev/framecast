-- Revert: Drop all functions and triggers added in 002_functions

-- Drop triggers
DROP TRIGGER IF EXISTS trigger_jobs_update_project_status ON jobs;
DROP TRIGGER IF EXISTS trigger_jobs_validate_status_transitions ON jobs;
DROP TRIGGER IF EXISTS trigger_jobs_check_concurrency_limits ON jobs;
DROP TRIGGER IF EXISTS trigger_job_events_auto_sequence ON job_events;
DROP TRIGGER IF EXISTS trigger_api_keys_check_owner_constraints ON api_keys;

-- Drop functions
DROP FUNCTION IF EXISTS get_team_stats(UUID);
DROP FUNCTION IF EXISTS get_job_stats();
DROP FUNCTION IF EXISTS check_api_key_owner_constraints();
DROP FUNCTION IF EXISTS is_valid_urn(TEXT);
DROP FUNCTION IF EXISTS cleanup_old_webhook_deliveries();
DROP FUNCTION IF EXISTS cleanup_old_job_events();
DROP FUNCTION IF EXISTS update_project_status_from_jobs();
DROP FUNCTION IF EXISTS validate_job_status_transitions();
DROP FUNCTION IF EXISTS check_job_concurrency_limits();
DROP FUNCTION IF EXISTS auto_set_job_event_sequence();
DROP FUNCTION IF EXISTS get_next_job_event_sequence(UUID);
