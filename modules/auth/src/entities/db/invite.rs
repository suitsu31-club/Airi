use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use rand::RngCore;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// Strongly typed invite identifier (a transparent wrapper over `i64`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct InviteId(pub i64);

/// A row of `auth.invite`.
pub struct InviteEntity {
    pub id: InviteId,
    pub owner: AccountId,
    pub invite_token: String,
    pub created_at: PrimitiveDateTime,
    pub will_expire_at: Option<PrimitiveDateTime>,
    pub last_status_change: PrimitiveDateTime,
    pub status: InviteStatus,
    pub source: String,
}

/// Lifecycle status of an invite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "invite_status", rename_all = "snake_case")]
pub enum InviteStatus {
    Accepted,
    Expired,
    Invalid,
    Pending,
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

/// A row of `auth.pending_invitation`.
pub struct PendingInvitationEntity {
    pub id: i64,
    pub invite: InviteId,
    pub email: String,
    pub sent_at: PrimitiveDateTime,
    pub will_release_at: PrimitiveDateTime,
    pub last_status_change: PrimitiveDateTime,
    pub status: PendingInvitationStatus,
}

/// Lifecycle status of a pending invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "pending_invitation_status", rename_all = "snake_case")]
pub enum PendingInvitationStatus {
    Pending,
    Accepted,
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

/// Create a pending invitation record.
pub struct CreatePendingInvitation {
    pub invite: InviteId,
    pub email: String,
    pub will_release_at: PrimitiveDateTime,
}

impl Processor<CreatePendingInvitation> for DatabaseProcessor {
    type Output = PendingInvitationEntity;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CreatePendingInvitation")]
    async fn process(&self, input: CreatePendingInvitation) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            PendingInvitationEntity,
            r#"INSERT INTO auth.pending_invitation (invite, email, will_release_at)
               VALUES ($1, $2, $3)
               RETURNING id, invite AS "invite: InviteId", email, sent_at, will_release_at,
                         last_status_change, status AS "status: PendingInvitationStatus""#,
            input.invite.0,
            input.email,
            input.will_release_at
        )
        .fetch_one(self.db())
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

/// Expire pending invites whose absolute expiry has passed.
pub struct ExpireInvitesBefore {
    pub now: PrimitiveDateTime,
}

impl Processor<ExpireInvitesBefore> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ExpireInvitesBefore")]
    async fn process(&self, input: ExpireInvitesBefore) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"UPDATE auth.invite SET status = 'expired', last_status_change = now()
               WHERE status = 'pending' AND will_expire_at IS NOT NULL AND will_expire_at < $1"#,
            input.now
        )
        .execute(self.db())
        .await?;
        Ok(result.rows_affected())
    }
}

/// Mark all pending invitations for an invite as accepted (used on registration
/// so they are not later released and refunded).
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

/// Release pending invitations whose hold has elapsed: mark them expired, expire
/// the underlying invite, and refund the owner's invitation count.
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
            sqlx::query!(
                r#"UPDATE auth.membership m
                   SET available_invitation_count = m.available_invitation_count + sub.cnt
                   FROM (
                       SELECT i.owner AS owner, count(*)::int AS cnt
                       FROM auth.invite i WHERE i.id = ANY($1) GROUP BY i.owner
                   ) sub
                   WHERE m.account = sub.owner"#,
                &invite_ids
            )
            .execute(&mut *tx)
            .await?;

            sqlx::query!(
                r#"UPDATE auth.invite SET status = 'expired', last_status_change = now()
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
