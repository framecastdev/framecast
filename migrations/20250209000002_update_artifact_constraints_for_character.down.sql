-- Revert: Restore original constraints (storyboard-only, no character)

ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS media_has_file_fields;
ALTER TABLE artifacts ADD CONSTRAINT media_has_file_fields CHECK (
    (
        kind IN ('image', 'audio', 'video')
        AND filename IS NOT NULL
        AND s3_key IS NOT NULL
        AND content_type IS NOT NULL AND size_bytes IS NOT NULL
    )
    OR kind = 'storyboard'
);

ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS spec_required_for_spec_kinds;
ALTER TABLE artifacts ADD CONSTRAINT storyboard_has_spec CHECK (
    (kind = 'storyboard' AND spec IS NOT NULL) OR kind != 'storyboard'
);
