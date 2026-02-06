-- Migration: Allow cascade delete of memberships when team is deleted
--
-- The prevent_empty_teams trigger blocks membership deletion even during
-- team CASCADE deletes. This update checks if the team still exists before
-- enforcing INV-T1/INV-T2, allowing proper team deletion.

CREATE OR REPLACE FUNCTION prevent_empty_teams()
RETURNS TRIGGER AS $$
DECLARE
    remaining_members INTEGER;
    remaining_owners INTEGER;
    team_exists BOOLEAN;
BEGIN
    -- Check if the team itself is being deleted (CASCADE)
    SELECT EXISTS(SELECT 1 FROM teams WHERE id = OLD.team_id) INTO team_exists;
    IF NOT team_exists THEN
        -- Team is being deleted, allow cascade removal of memberships
        RETURN OLD;
    END IF;

    -- Count remaining members after deletion
    SELECT COUNT(*) INTO remaining_members
    FROM memberships
    WHERE team_id = OLD.team_id AND id != OLD.id;

    -- INV-T1: Every team must have at least one member
    IF remaining_members = 0 THEN
        RAISE EXCEPTION 'Cannot remove last member from team (INV-T1)';
    END IF;

    -- INV-T2: Every team must have at least one owner
    IF OLD.role = 'owner' THEN
        SELECT COUNT(*) INTO remaining_owners
        FROM memberships
        WHERE team_id = OLD.team_id AND role = 'owner' AND id != OLD.id;

        IF remaining_owners = 0 THEN
            RAISE EXCEPTION 'Cannot remove last owner from team (INV-T2)';
        END IF;
    END IF;

    RETURN OLD;
END;
$$ LANGUAGE plpgsql;
