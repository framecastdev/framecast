-- Revert: Drop all functions and triggers added in 002_functions

-- Drop triggers
DROP TRIGGER IF EXISTS trigger_generations_update_project_status ON generations;
DROP TRIGGER IF EXISTS
trigger_generations_validate_status_transitions ON generations;
DROP TRIGGER IF EXISTS
trigger_generations_check_concurrency_limits ON generations;
DROP TRIGGER IF EXISTS
trigger_generation_events_auto_sequence ON generation_events;
DROP TRIGGER IF EXISTS trigger_api_keys_check_owner_constraints ON api_keys;

-- Drop functions
DROP FUNCTION IF EXISTS get_team_stats(UUID);
DROP FUNCTION IF EXISTS get_generation_stats();
DROP FUNCTION IF EXISTS check_api_key_owner_constraints();
DROP FUNCTION IF EXISTS is_valid_urn(TEXT);
DROP FUNCTION IF EXISTS cleanup_old_webhook_deliveries();
DROP FUNCTION IF EXISTS cleanup_old_generation_events();
DROP FUNCTION IF EXISTS update_project_status_from_generations();
DROP FUNCTION IF EXISTS validate_generation_status_transitions();
DROP FUNCTION IF EXISTS check_generation_concurrency_limits();
DROP FUNCTION IF EXISTS auto_set_generation_event_sequence();
DROP FUNCTION IF EXISTS get_next_generation_event_sequence(UUID);
