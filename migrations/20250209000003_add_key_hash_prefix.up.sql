-- Add key_hash_prefix column for O(1) API key lookup.
--
-- Stores the first 16 hex chars of an unsalted SHA-256 hash of the raw key.
-- Used as a fast lookup index during authentication to reduce candidates
-- from O(N) to O(1). The full salted hash is still used for verification.
--
-- Nullable: existing keys can't be backfilled (raw key is not stored).
-- New keys populate this on creation. Old keys fall back to full scan.

ALTER TABLE api_keys ADD COLUMN key_hash_prefix VARCHAR(16);

-- Index for fast prefix lookup during authentication
CREATE INDEX idx_api_keys_hash_prefix ON api_keys (key_hash_prefix)
WHERE key_hash_prefix IS NOT NULL;
