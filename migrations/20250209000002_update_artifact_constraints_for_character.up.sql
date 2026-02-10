-- Migration: update_artifact_constraints_for_character
-- Description: Update CHECK constraints so character artifacts
-- (spec-based, no file fields) are handled correctly.

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
