-- Migration: add_character_artifact_kind
-- Description: Add 'character' variant to artifact_kind enum and update
-- CHECK constraints so character artifacts (spec-based, no file fields)
-- are handled correctly.

-- ============================================================================
-- ADD CHARACTER TO artifact_kind ENUM
-- ============================================================================

ALTER TYPE artifact_kind ADD VALUE 'character';

-- ============================================================================
-- UPDATE CHECK CONSTRAINTS
-- ============================================================================

-- media_has_file_fields: character (like storyboard) has no file fields
ALTER TABLE artifacts DROP CONSTRAINT media_has_file_fields;
ALTER TABLE artifacts ADD CONSTRAINT media_has_file_fields CHECK (
    (
        kind IN ('image', 'audio', 'video')
        AND filename IS NOT NULL
        AND s3_key IS NOT NULL
        AND content_type IS NOT NULL AND size_bytes IS NOT NULL
    )
    OR kind IN ('storyboard', 'character')
);

-- storyboard_has_spec: both storyboard and character require spec
ALTER TABLE artifacts DROP CONSTRAINT storyboard_has_spec;
ALTER TABLE artifacts ADD CONSTRAINT spec_required_for_spec_kinds CHECK (
    (kind IN ('storyboard', 'character') AND spec IS NOT NULL)
    OR kind NOT IN ('storyboard', 'character')
);
