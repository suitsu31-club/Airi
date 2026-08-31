use crate::entities::db::account::AccountId;
use time::PrimitiveDateTime;

pub struct InviteId(pub i64);

pub struct InviteEntity {
    pub id: InviteId,
    pub owner: AccountId,
    pub invite_token: String,
    pub created_at: PrimitiveDateTime,
    pub will_expire_at: Option<PrimitiveDateTime>,
    pub last_status_change: PrimitiveDateTime,
    pub status: InviteStatus,
}

pub enum InviteStatus {
    Accepted,
    Expired,
    Invalid,
    Pending,
    Free,
}

pub struct PendingInvitationEntity {
    pub id: i64,
    pub invite: InviteId,
    pub email: String,
    pub sent_at: PrimitiveDateTime,
    pub will_release_at: PrimitiveDateTime,
    pub last_status_change: PrimitiveDateTime,
}

pub enum PendingInvitationStatus {
    Pending,
    Accepted,
    Expired,
}
