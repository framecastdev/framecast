-- Revert: Remove 'character' from artifact_kind enum
-- NOTE: PostgreSQL does not support ALTER TYPE DROP VALUE.
-- Workaround: recreate the enum without 'character'.

-- Ensure no rows reference the 'character' kind before reverting
DELETE FROM artifacts
WHERE kind = 'character';

-- Drop constraints that reference the enum
ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS media_has_file_fields;
ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS spec_required_for_spec_kinds;

-- Rename old type, create new without 'character', migrate column, drop old
ALTER TYPE artifact_kind RENAME TO artifact_kind_old;
CREATE TYPE artifact_kind AS ENUM ('storyboard', 'image', 'audio', 'video');
ALTER TABLE artifacts
ALTER COLUMN kind TYPE artifact_kind
USING kind::text::artifact_kind;
DROP TYPE artifact_kind_old;

-- Restore original constraints (without 'character')
ALTER TABLE artifacts ADD CONSTRAINT media_has_file_fields CHECK (
    (
        kind IN ('image', 'audio', 'video')
        AND filename IS NOT NULL
        AND s3_key IS NOT NULL
        AND content_type IS NOT NULL AND size_bytes IS NOT NULL
    )
    OR kind = 'storyboard'
);

ALTER TABLE artifacts ADD CONSTRAINT storyboard_has_spec CHECK (
    (kind = 'storyboard' AND spec IS NOT NULL) OR kind != 'storyboard'
);
