-- Revert: Remove key_hash_prefix column from api_keys

DROP INDEX IF EXISTS idx_api_keys_hash_prefix;
ALTER TABLE api_keys DROP COLUMN IF EXISTS key_hash_prefix;
