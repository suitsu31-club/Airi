-- Invitation slots are now modeled as `Free` rows in auth.invite, and the
-- available-count is derived from slot status rather than a separate counter.
-- Convert each member's remaining count into that many Free slots so existing
-- users keep their available invitations, then drop the redundant column.
INSERT INTO auth.invite (owner, invite_token, status, source, will_expire_at)
SELECT m.account,
       replace(gen_random_uuid()::text, '-', '') || replace(gen_random_uuid()::text, '-', ''),
       'free',
       'migrated_count',
       NULL
FROM auth.membership m
CROSS JOIN LATERAL generate_series(1, m.available_invitation_count) AS g
WHERE m.available_invitation_count > 0;

ALTER TABLE auth.membership DROP COLUMN available_invitation_count;
