//! Self-service profile, credit, and invitation summary reads.

use crate::entities::db::account::{AccountEntity, AccountId, FindAccountById};
use crate::entities::db::credit::{
    CreditChangeHistoryEntity, CreditEntity, FindCreditByAccount, ListCreditHistory,
};
use crate::entities::db::invite::{InviteStatus, ListInvitesByOwner};
use crate::entities::db::membership::{FindMembershipByAccount, MembershipEntity};
use kanau::processor::Processor;
use rust_decimal::Decimal;
use wakuwaku::sqlx::DatabaseProcessor;

/// Read-only profile queries.
#[derive(Clone)]
pub struct ProfileService {
    pub db: DatabaseProcessor,
}

/// An account together with its membership (if any).
pub struct ProfileData {
    pub account: AccountEntity,
    pub membership: Option<MembershipEntity>,
}

/// A user's invitation availability and usage summary.
pub struct InvitationSummary {
    pub available_count: i32,
    pub sent_count: u32,
}

impl ProfileService {
    async fn load_profile(&self, user_id: AccountId) -> Result<ProfileData, wakuwaku::Error> {
        let account = self
            .db
            .process(FindAccountById { id: user_id })
            .await?
            .ok_or(wakuwaku::Error::NotFound)?;
        let membership = self
            .db
            .process(FindMembershipByAccount { account: user_id })
            .await?;
        Ok(ProfileData {
            account,
            membership,
        })
    }
}

/// Fetch the caller's own profile.
pub struct GetMyProfile {
    pub user_id: AccountId,
}

impl Processor<GetMyProfile> for ProfileService {
    type Output = ProfileData;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetMyProfile")]
    async fn process(&self, input: GetMyProfile) -> Result<Self::Output, Self::Error> {
        self.load_profile(input.user_id).await
    }
}

/// Fetch another user's public profile.
pub struct GetPublicProfile {
    pub user_id: AccountId,
}

impl Processor<GetPublicProfile> for ProfileService {
    type Output = ProfileData;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetPublicProfile")]
    async fn process(&self, input: GetPublicProfile) -> Result<Self::Output, Self::Error> {
        self.load_profile(input.user_id).await
    }
}

/// Fetch the caller's credit balance (zeroed when absent).
pub struct GetMyCredit {
    pub user_id: AccountId,
}

impl Processor<GetMyCredit> for ProfileService {
    type Output = CreditEntity;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetMyCredit")]
    async fn process(&self, input: GetMyCredit) -> Result<Self::Output, Self::Error> {
        let credit = self
            .db
            .process(FindCreditByAccount {
                account: input.user_id,
            })
            .await?;
        Ok(credit.unwrap_or(CreditEntity {
            account: input.user_id,
            total_amount: Decimal::ZERO,
            frozen_amount: Decimal::ZERO,
        }))
    }
}

/// Fetch a page of the caller's credit history.
pub struct GetMyCreditLog {
    pub user_id: AccountId,
    pub limit: i64,
    pub offset: i64,
}

impl Processor<GetMyCreditLog> for ProfileService {
    type Output = Vec<CreditChangeHistoryEntity>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetMyCreditLog")]
    async fn process(&self, input: GetMyCreditLog) -> Result<Self::Output, Self::Error> {
        Ok(self
            .db
            .process(ListCreditHistory {
                account: input.user_id,
                limit: input.limit,
                offset: input.offset,
            })
            .await?)
    }
}

/// Fetch the caller's invitation availability/usage summary.
pub struct GetMyInvitationSummary {
    pub user_id: AccountId,
}

impl Processor<GetMyInvitationSummary> for ProfileService {
    type Output = InvitationSummary;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetMyInvitationSummary")]
    async fn process(&self, input: GetMyInvitationSummary) -> Result<Self::Output, Self::Error> {
        let membership = self
            .db
            .process(FindMembershipByAccount {
                account: input.user_id,
            })
            .await?;
        let available_count = membership.map_or(0, |m| m.available_invitation_count);
        let invites = self
            .db
            .process(ListInvitesByOwner {
                owner: input.user_id,
            })
            .await?;
        let sent_count = invites
            .iter()
            .filter(|i| i.status != InviteStatus::Free)
            .count() as u32;
        Ok(InvitationSummary {
            available_count,
            sent_count,
        })
    }
}

/// A user's invitations grouped by lifecycle status, alongside the number of
/// invitations they may still send.
#[derive(Debug, Clone, Default)]
pub struct InvitationGrouping {
    pub available_count: i32,
    pub accepted: u32,
    pub expired: u32,
    pub invalid: u32,
    pub pending: u32,
    pub free: u32,
}

/// Fetch the caller's invitations grouped by their lifecycle status.
pub struct GetMyInvitationGrouping {
    pub user_id: AccountId,
}

impl Processor<GetMyInvitationGrouping> for ProfileService {
    type Output = InvitationGrouping;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetMyInvitationGrouping")]
    async fn process(&self, input: GetMyInvitationGrouping) -> Result<Self::Output, Self::Error> {
        let membership = self
            .db
            .process(FindMembershipByAccount {
                account: input.user_id,
            })
            .await?;
        let available_count = membership.map_or(0, |m| m.available_invitation_count);
        let invites = self
            .db
            .process(ListInvitesByOwner {
                owner: input.user_id,
            })
            .await?;
        let mut grouping = InvitationGrouping {
            available_count,
            ..Default::default()
        };
        for invite in &invites {
            let bucket = match invite.status {
                InviteStatus::Accepted => &mut grouping.accepted,
                InviteStatus::Expired => &mut grouping.expired,
                InviteStatus::Invalid => &mut grouping.invalid,
                InviteStatus::Pending => &mut grouping.pending,
                InviteStatus::Free => &mut grouping.free,
            };
            *bucket = bucket.saturating_add(1);
        }
        Ok(grouping)
    }
}
