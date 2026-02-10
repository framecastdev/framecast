-- Revert: Restore original URN validation without hyphen support

CREATE OR REPLACE FUNCTION is_valid_urn(urn_text TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN urn_text ~ '^framecast:(user:[a-zA-Z0-9_]+|team:[a-zA-Z0-9_]+|[a-zA-Z0-9_]+:[a-zA-Z0-9_]+)$';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION check_api_key_owner_constraints()
RETURNS TRIGGER AS $$
DECLARE
    user_tier user_tier;
BEGIN
    SELECT tier INTO user_tier FROM users WHERE id = NEW.user_id;

    IF user_tier = 'starter' AND NEW.owner != ('framecast:user:' || NEW.user_id::text) THEN
        RAISE EXCEPTION 'Starter users can only have personal API keys';
    END IF;

    IF (NEW.owner LIKE 'framecast:team:%' OR NEW.owner ~ 'framecast:[a-zA-Z0-9_]+:[a-zA-Z0-9_]+')
       AND user_tier != 'creator' THEN
        RAISE EXCEPTION 'Team API keys require creator tier';
    END IF;

    IF NOT is_valid_urn(NEW.owner) THEN
        RAISE EXCEPTION 'Invalid URN format: %', NEW.owner;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
