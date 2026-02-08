-- Fix URN validation to accept hyphens in UUID components.
-- The original regex [a-zA-Z0-9_]+ did not match UUID hyphens,
-- causing all API key insertions to fail with "Invalid URN format".

CREATE OR REPLACE FUNCTION is_valid_urn(urn_text TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN urn_text ~ '^framecast:(user:[a-zA-Z0-9_-]+|team:[a-zA-Z0-9_-]+|[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]+)$';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Also fix the regex inside check_api_key_owner_constraints
-- so the INV-K2 team URN check matches hyphenated UUIDs.
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
    -- Exclude framecast:user:* URNs — the generic regex framecast:X:Y
    -- would otherwise match personal user URNs too.
    IF (NEW.owner LIKE 'framecast:team:%'
        OR (NEW.owner ~ 'framecast:[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]+'
            AND NEW.owner NOT LIKE 'framecast:user:%'))
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
