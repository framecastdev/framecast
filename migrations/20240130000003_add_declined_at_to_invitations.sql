-- Add declined_at for invitee-initiated decline (distinct from admin revoke)
ALTER TABLE invitations ADD COLUMN declined_at TIMESTAMPTZ;

-- Replace old constraint with 3-way exclusion
ALTER TABLE invitations
DROP CONSTRAINT IF EXISTS acceptance_revocation_exclusion;

ALTER TABLE invitations
ADD CONSTRAINT invitation_terminal_state_exclusion
CHECK (
    (
        CASE WHEN accepted_at IS NOT NULL THEN 1 ELSE 0 END
        + CASE WHEN declined_at IS NOT NULL THEN 1 ELSE 0 END
        + CASE WHEN revoked_at IS NOT NULL THEN 1 ELSE 0 END
    ) <= 1
);
