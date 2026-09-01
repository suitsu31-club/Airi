-- Restore the counter and reconstruct it from the Free slots minted by the
-- forward migration, then remove those migrated slots.
ALTER TABLE auth.membership
    ADD COLUMN available_invitation_count int NOT NULL DEFAULT 0;

UPDATE auth.membership m
SET available_invitation_count = COALESCE(sub.cnt, 0)
FROM (
    SELECT owner, count(*)::int AS cnt
    FROM auth.invite
    WHERE status = 'free' AND source = 'migrated_count'
    GROUP BY owner
) sub
WHERE m.account = sub.owner;

DELETE FROM auth.invite WHERE status = 'free' AND source = 'migrated_count';
