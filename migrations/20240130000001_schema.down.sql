-- Revert: Drop all core tables, triggers, functions, and enum types

-- Drop triggers first (depend on functions)
DROP TRIGGER IF EXISTS trigger_invitations_check_constraints ON invitations;
DROP TRIGGER IF EXISTS trigger_memberships_prevent_empty_teams ON memberships;
DROP TRIGGER IF EXISTS trigger_memberships_check_constraints ON memberships;
DROP TRIGGER IF EXISTS trigger_teams_generate_slug ON teams;
DROP TRIGGER IF EXISTS trigger_webhooks_updated_at ON webhooks;
DROP TRIGGER IF EXISTS trigger_asset_files_updated_at ON asset_files;
DROP TRIGGER IF EXISTS trigger_generations_updated_at ON generations;
DROP TRIGGER IF EXISTS trigger_projects_updated_at ON projects;
DROP TRIGGER IF EXISTS trigger_teams_updated_at ON teams;
DROP TRIGGER IF EXISTS trigger_users_updated_at ON users;

-- Drop functions (depend on tables for types)
DROP FUNCTION IF EXISTS check_invitation_constraints();
DROP FUNCTION IF EXISTS prevent_empty_teams();
DROP FUNCTION IF EXISTS check_membership_constraints();
DROP FUNCTION IF EXISTS generate_team_slug();
DROP FUNCTION IF EXISTS update_updated_at_column();

-- Drop tables in reverse dependency order
DROP TABLE IF EXISTS usage;
DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS webhooks;
DROP TABLE IF EXISTS system_assets;
DROP TABLE IF EXISTS asset_files;
DROP TABLE IF EXISTS generation_events;
DROP TABLE IF EXISTS generations;
DROP TABLE IF EXISTS projects;
DROP TABLE IF EXISTS api_keys;
DROP TABLE IF EXISTS invitations;
DROP TABLE IF EXISTS memberships;
DROP TABLE IF EXISTS teams;
DROP TABLE IF EXISTS users;

-- Drop enum types
DROP TYPE IF EXISTS webhook_event_type;
DROP TYPE IF EXISTS system_asset_category;
DROP TYPE IF EXISTS webhook_delivery_status;
DROP TYPE IF EXISTS asset_status;
DROP TYPE IF EXISTS generation_event_type;
DROP TYPE IF EXISTS generation_failure_type;
DROP TYPE IF EXISTS generation_status;
DROP TYPE IF EXISTS project_status;
DROP TYPE IF EXISTS invitation_role;
DROP TYPE IF EXISTS membership_role;
DROP TYPE IF EXISTS user_tier;

-- Drop extensions (optional — other schemas may use them)
-- DROP EXTENSION IF EXISTS "pgcrypto";
-- DROP EXTENSION IF EXISTS "uuid-ossp";
