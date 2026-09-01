//! Invitations: slots, sends, and redemption.
//!
//! An invitation is a row in `auth.invite` — a *slot* owned by an account.
//! [`InviteStatus`] drives its whole lifecycle:
//!
//! ```text
//!    admin grant          send (claim)              register
//!  ──────────────▶ Free ───────────────▶ Pending ───────────────▶ Accepted
//!                   │ ▲                     │
//!      slot expiry  │ │  pending expiry     │
//!      (never sent) │ └─────────────────────┘  (hold released; slot reused)
//!                   ▼
//!                Expired          Invalid  ◀── admin, from Free/Pending
//! ```
//!
//! - **Grant.** Only a moderator/site-owner admin mints slots, as `Free`
//!   invites owned by the target (optionally with an expiry). Users cannot
//!   create invitations themselves.
//! - **Send.** [`ClaimFreeInviteAndPin`] atomically consumes one non-expired
//!   `Free` slot: it flips to `Pending`, its token is regenerated, and a
//!   [`PendingInvitationEntity`] pins the recipient email. The registrant must
//!   use that exact email.
//! - **Redeem.** Registration accepts a `Pending` invite whose pinned email
//!   matches; the invite and its pending row both become `Accepted` and the new
//!   member records the invite as `invited_by` (forming the invite tree).
//! - **Expiry.** A `Free` slot that is never sent lapses to `Expired`
//!   ([`ExpireFreeSlots`]) and is wasted. A `Pending` invite not accepted in
//!   time is released back to `Free` ([`ReleaseExpiredPending`]) so the owner
//!   can reuse the slot; the stale token dies and is regenerated on next send.
//! - **Invalidate.** An admin may force a `Free` or `Pending` invite to
//!   `Invalid` ([`InvalidateInvite`]).
//! - **Counting.** "Available invitations" is just the number of usable `Free`
//!   slots ([`CountFreeInvitesByOwner`]); there is no separate counter.

use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use rand::RngCore;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// Strongly typed invite identifier (a transparent wrapper over `i64`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct InviteId(pub i64);

/// A row of `auth.invite` — an invitation *slot* owned by an account.
pub struct InviteEntity {
    pub id: InviteId,
    /// Account that owns this slot (the inviter).
    pub owner: AccountId,
    /// Opaque redemption token; regenerated every time the slot is sent.
    pub invite_token: String,
    pub created_at: PrimitiveDateTime,
    /// When the invite lapses: an optional admin-set slot expiry while `Free`,
    /// or the send deadline while `Pending`. `None` means it never expires.
    pub will_expire_at: Option<PrimitiveDateTime>,
    pub last_status_change: PrimitiveDateTime,
    pub status: InviteStatus,
    /// Provenance tag recorded at creation, e.g. `"admin_grant"`.
    pub source: String,
}

/// Lifecycle status of an invite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "invite_status", rename_all = "snake_case")]
pub enum InviteStatus {
    /// Redeemed — a user registered with this invite.
    Accepted,
    /// Lapsed: a `Free` slot whose expiry passed before it was ever sent; wasted.
    Expired,
    /// Administratively voided (from `Free` or `Pending`); never redeemable.
    Invalid,
    /// Sent to a specific email and awaiting registration; carries a pinned
    /// [`PendingInvitationEntity`] and a freshly regenerated token.
    Pending,
    /// An unused slot the owner holds and may send. Counted as an "available
    /// invitation"; may carry an optional expiry.
    Free,
}

impl InviteStatus {
    /// Snake-case wire representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            InviteStatus::Accepted => "accepted",
            InviteStatus::Expired => "expired",
            InviteStatus::Invalid => "invalid",
            InviteStatus::Pending => "pending",
            InviteStatus::Free => "free",
        }
    }
}

/// A row of `auth.pending_invitation`: one send of a slot to a specific email.
pub struct PendingInvitationEntity {
    pub id: i64,
    /// The slot ([`InviteEntity`]) this send belongs to.
    pub invite: InviteId,
    /// Email the invite is pinned to; the registrant must use exactly this.
    pub email: String,
    pub sent_at: PrimitiveDateTime,
    /// Deadline after which the hold is released and the slot returns to `Free`.
    pub will_release_at: PrimitiveDateTime,
    pub last_status_change: PrimitiveDateTime,
    pub status: PendingInvitationStatus,
}

/// Lifecycle status of a pending invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "pending_invitation_status", rename_all = "snake_case")]
pub enum PendingInvitationStatus {
    /// Awaiting registration by the pinned email.
    Pending,
    /// The pinned recipient registered with this invite.
    Accepted,
    /// The hold elapsed; the underlying slot was released back to `Free`.
    Expired,
}

impl PendingInvitationStatus {
    /// Snake-case wire representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            PendingInvitationStatus::Pending => "pending",
            PendingInvitationStatus::Accepted => "accepted",
            PendingInvitationStatus::Expired => "expired",
        }
    }
}

/// Generate a fresh opaque invite token (URL-safe base32 of 32 bytes).
pub fn generate_invite_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    fast32::base32::RFC4648_NOPAD.encode(&bytes)
}

/// Create an invite.
pub struct CreateInvite {
    pub owner: AccountId,
    pub invite_token: String,
    pub status: InviteStatus,
    pub source: String,
    pub will_expire_at: Option<PrimitiveDateTime>,
}

impl Processor<CreateInvite> for DatabaseProcessor {
    type Output = InviteEntity;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CreateInvite")]
    async fn process(&self, input: CreateInvite) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            InviteEntity,
            r#"INSERT INTO auth.invite (owner, invite_token, status, source, will_expire_at)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id AS "id: InviteId", owner AS "owner: AccountId", invite_token,
                         created_at, will_expire_at, last_status_change,
                         status AS "status: InviteStatus", source"#,
            input.owner.0,
            input.invite_token,
            input.status as InviteStatus,
            input.source,
            input.will_expire_at
        )
        .fetch_one(self.db())
        .await
    }
}

/// Look up an invite by its token.
pub struct FindInviteByToken {
    pub invite_token: String,
}

impl Processor<FindInviteByToken> for DatabaseProcessor {
    type Output = Option<InviteEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindInviteByToken")]
    async fn process(&self, input: FindInviteByToken) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            InviteEntity,
            r#"SELECT id AS "id: InviteId", owner AS "owner: AccountId", invite_token,
                      created_at, will_expire_at, last_status_change,
                      status AS "status: InviteStatus", source
               FROM auth.invite WHERE invite_token = $1"#,
            input.invite_token
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Update an invite's status.
pub struct SetInviteStatus {
    pub id: InviteId,
    pub status: InviteStatus,
}

impl Processor<SetInviteStatus> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:SetInviteStatus")]
    async fn process(&self, input: SetInviteStatus) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.invite SET status = $2, last_status_change = now() WHERE id = $1"#,
            input.id.0,
            input.status as InviteStatus
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// List all invites owned by an account.
pub struct ListInvitesByOwner {
    pub owner: AccountId,
}

impl Processor<ListInvitesByOwner> for DatabaseProcessor {
    type Output = Vec<InviteEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ListInvitesByOwner")]
    async fn process(&self, input: ListInvitesByOwner) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            InviteEntity,
            r#"SELECT id AS "id: InviteId", owner AS "owner: AccountId", invite_token,
                      created_at, will_expire_at, last_status_change,
                      status AS "status: InviteStatus", source
               FROM auth.invite WHERE owner = $1 ORDER BY created_at DESC"#,
            input.owner.0
        )
        .fetch_all(self.db())
        .await
    }
}

/// Look up a pending invitation by id.
pub struct FindPendingInvitation {
    pub id: i64,
}

impl Processor<FindPendingInvitation> for DatabaseProcessor {
    type Output = Option<PendingInvitationEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindPendingInvitation")]
    async fn process(&self, input: FindPendingInvitation) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            PendingInvitationEntity,
            r#"SELECT id, invite AS "invite: InviteId", email, sent_at, will_release_at,
                      last_status_change, status AS "status: PendingInvitationStatus"
               FROM auth.pending_invitation WHERE id = $1"#,
            input.id
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Update a pending invitation's status.
pub struct SetPendingInvitationStatus {
    pub id: i64,
    pub status: PendingInvitationStatus,
}

impl Processor<SetPendingInvitationStatus> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:SetPendingInvitationStatus")]
    async fn process(
        &self,
        input: SetPendingInvitationStatus,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.pending_invitation SET status = $2, last_status_change = now()
               WHERE id = $1"#,
            input.id,
            input.status as PendingInvitationStatus
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Bump a pending invitation's release deadline and resend timestamp.
pub struct TouchPendingInvitation {
    pub id: i64,
    pub will_release_at: PrimitiveDateTime,
}

impl Processor<TouchPendingInvitation> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:TouchPendingInvitation")]
    async fn process(&self, input: TouchPendingInvitation) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.pending_invitation SET will_release_at = $2, sent_at = now()
               WHERE id = $1"#,
            input.id,
            input.will_release_at
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Expire Free slots whose admin-set expiry elapsed before they were sent.
pub struct ExpireFreeSlots {
    pub now: PrimitiveDateTime,
}

impl Processor<ExpireFreeSlots> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ExpireFreeSlots")]
    async fn process(&self, input: ExpireFreeSlots) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"UPDATE auth.invite SET status = 'expired', last_status_change = now()
               WHERE status = 'free' AND will_expire_at IS NOT NULL AND will_expire_at < $1"#,
            input.now
        )
        .execute(self.db())
        .await?;
        Ok(result.rows_affected())
    }
}

/// Mark an invite's pending invitations `accepted` (used at registration) so
/// the release sweep never later returns the now-redeemed slot to `Free`.
pub struct AcceptPendingInvitationsByInvite {
    pub invite: InviteId,
}

impl Processor<AcceptPendingInvitationsByInvite> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:AcceptPendingInvitationsByInvite")]
    async fn process(
        &self,
        input: AcceptPendingInvitationsByInvite,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.pending_invitation SET status = 'accepted', last_status_change = now()
               WHERE invite = $1 AND status = 'pending'"#,
            input.invite.0
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Release pending invitations whose hold has elapsed: mark each pending row
/// `expired` and return its slot to the owner as reusable `Free` (clearing the
/// expiry). Nothing is refunded — availability is derived from `Free` slots.
pub struct ReleaseExpiredPending {
    pub now: PrimitiveDateTime,
}

impl Processor<ReleaseExpiredPending> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL-Transaction:ReleaseExpiredPending")]
    async fn process(&self, input: ReleaseExpiredPending) -> Result<Self::Output, Self::Error> {
        let mut tx = self.db().begin().await?;
        let released = sqlx::query!(
            r#"UPDATE auth.pending_invitation SET status = 'expired', last_status_change = now()
               WHERE status = 'pending' AND will_release_at < $1
               RETURNING invite"#,
            input.now
        )
        .fetch_all(&mut *tx)
        .await?;

        let invite_ids: Vec<i64> = released.iter().map(|r| r.invite).collect();

        if !invite_ids.is_empty() {
            // Return the slot to the owner: reusable `Free`, expiry cleared. The
            // stale token can no longer register (only `Pending` invites can) and
            // is regenerated on the next send.
            sqlx::query!(
                r#"UPDATE auth.invite
                   SET status = 'free', will_expire_at = NULL, last_status_change = now()
                   WHERE id = ANY($1) AND status = 'pending'"#,
                &invite_ids
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(released.len() as u64)
    }
}

/// Look up an invite by id.
pub struct FindInviteById {
    pub id: InviteId,
}

impl Processor<FindInviteById> for DatabaseProcessor {
    type Output = Option<InviteEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindInviteById")]
    async fn process(&self, input: FindInviteById) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            InviteEntity,
            r#"SELECT id AS "id: InviteId", owner AS "owner: AccountId", invite_token,
                      created_at, will_expire_at, last_status_change,
                      status AS "status: InviteStatus", source
               FROM auth.invite WHERE id = $1"#,
            input.id.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// List all pending invitations across an owner's invites.
pub struct ListPendingInvitationsByOwner {
    pub owner: AccountId,
}

impl Processor<ListPendingInvitationsByOwner> for DatabaseProcessor {
    type Output = Vec<PendingInvitationEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ListPendingInvitationsByOwner")]
    async fn process(
        &self,
        input: ListPendingInvitationsByOwner,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            PendingInvitationEntity,
            r#"SELECT p.id, p.invite AS "invite: InviteId", p.email, p.sent_at,
                      p.will_release_at, p.last_status_change,
                      p.status AS "status: PendingInvitationStatus"
               FROM auth.pending_invitation p
               JOIN auth.invite i ON i.id = p.invite
               WHERE i.owner = $1
               ORDER BY p.sent_at DESC"#,
            input.owner.0
        )
        .fetch_all(self.db())
        .await
    }
}

/// Atomically claim one non-expired `Free` slot for `owner` and pin it to
/// `email`: the slot becomes `Pending` with a freshly generated token and a
/// matching `pending_invitation` row, all in one transaction. Returns the
/// claimed ids, or `None` when the owner has no usable slot.
pub struct ClaimFreeInviteAndPin {
    pub owner: AccountId,
    pub new_token: String,
    pub email: String,
    pub expiry: PrimitiveDateTime,
    pub now: PrimitiveDateTime,
}

/// Identifiers produced by [`ClaimFreeInviteAndPin`].
pub struct ClaimedInvite {
    pub invite_id: InviteId,
    pub pending_id: i64,
}

impl Processor<ClaimFreeInviteAndPin> for DatabaseProcessor {
    type Output = Option<ClaimedInvite>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL-Transaction:ClaimFreeInviteAndPin")]
    async fn process(&self, input: ClaimFreeInviteAndPin) -> Result<Self::Output, Self::Error> {
        let mut tx = self.db().begin().await?;
        let claimed = sqlx::query!(
            r#"UPDATE auth.invite SET status = 'pending', invite_token = $2,
                      will_expire_at = $3, last_status_change = now()
               WHERE id = (
                   SELECT id FROM auth.invite
                   WHERE owner = $1 AND status = 'free'
                     AND (will_expire_at IS NULL OR will_expire_at > $4)
                   ORDER BY will_expire_at ASC NULLS LAST, created_at ASC
                   LIMIT 1
                   FOR UPDATE SKIP LOCKED
               )
               RETURNING id"#,
            input.owner.0,
            input.new_token,
            input.expiry,
            input.now
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = claimed else {
            tx.rollback().await?;
            return Ok(None);
        };
        let invite_id = InviteId(row.id);

        let pending = sqlx::query!(
            r#"INSERT INTO auth.pending_invitation (invite, email, will_release_at)
               VALUES ($1, $2, $3) RETURNING id"#,
            invite_id.0,
            input.email,
            input.expiry
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some(ClaimedInvite {
            invite_id,
            pending_id: pending.id,
        }))
    }
}

/// Count an owner's usable `Free` slots (available invitations).
pub struct CountFreeInvitesByOwner {
    pub owner: AccountId,
    pub now: PrimitiveDateTime,
}

impl Processor<CountFreeInvitesByOwner> for DatabaseProcessor {
    type Output = i64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CountFreeInvitesByOwner")]
    async fn process(&self, input: CountFreeInvitesByOwner) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"SELECT count(*) AS "count!" FROM auth.invite
               WHERE owner = $1 AND status = 'free'
                 AND (will_expire_at IS NULL OR will_expire_at > $2)"#,
            input.owner.0,
            input.now
        )
        .fetch_one(self.db())
        .await?;
        Ok(row.count)
    }
}

/// Look up the pending invitation pinned to an invite (the newest, if any).
pub struct FindPendingInvitationByInvite {
    pub invite: InviteId,
}

impl Processor<FindPendingInvitationByInvite> for DatabaseProcessor {
    type Output = Option<PendingInvitationEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindPendingInvitationByInvite")]
    async fn process(
        &self,
        input: FindPendingInvitationByInvite,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            PendingInvitationEntity,
            r#"SELECT id, invite AS "invite: InviteId", email, sent_at, will_release_at,
                      last_status_change, status AS "status: PendingInvitationStatus"
               FROM auth.pending_invitation WHERE invite = $1
               ORDER BY sent_at DESC LIMIT 1"#,
            input.invite.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Regenerate a `Pending` invite's token and extend its expiry (used on resend).
pub struct RefreshPendingInvite {
    pub invite: InviteId,
    pub new_token: String,
    pub will_expire_at: PrimitiveDateTime,
}

impl Processor<RefreshPendingInvite> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:RefreshPendingInvite")]
    async fn process(&self, input: RefreshPendingInvite) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.invite SET invite_token = $2, will_expire_at = $3,
                      last_status_change = now()
               WHERE id = $1 AND status = 'pending'"#,
            input.invite.0,
            input.new_token,
            input.will_expire_at
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Mark an invite's pending invitations expired (used when invalidating).
pub struct ExpirePendingInvitationsByInvite {
    pub invite: InviteId,
}

impl Processor<ExpirePendingInvitationsByInvite> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ExpirePendingInvitationsByInvite")]
    async fn process(
        &self,
        input: ExpirePendingInvitationsByInvite,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.pending_invitation SET status = 'expired', last_status_change = now()
               WHERE invite = $1 AND status = 'pending'"#,
            input.invite.0
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Invalidate an invite when it is `Free` or `Pending`. Returns the prior
/// status when invalidated, or `None` if absent or already terminal.
pub struct InvalidateInvite {
    pub id: InviteId,
}

impl Processor<InvalidateInvite> for DatabaseProcessor {
    type Output = Option<InviteStatus>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:InvalidateInvite")]
    async fn process(&self, input: InvalidateInvite) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"WITH prev AS (SELECT id, status FROM auth.invite WHERE id = $1),
                    upd AS (
                        UPDATE auth.invite SET status = 'invalid', last_status_change = now()
                        WHERE id = $1 AND status IN ('free', 'pending')
                        RETURNING id
                    )
               SELECT p.status AS "status: InviteStatus"
               FROM prev p JOIN upd u ON u.id = p.id"#,
            input.id.0
        )
        .fetch_optional(self.db())
        .await?;
        Ok(row.map(|r| r.status))
    }
}
