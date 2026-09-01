//! Applies external invitation-slot grants to a user.

use crate::entities::db::account::AccountId;
use crate::entities::db::invite::{CreateInvite, InviteStatus, generate_invite_token};
use crate::utils::datetime::now_primitive;
use base::events::AddInvitationSlotEvent;
use kanau::processor::Processor;
use time::Duration;
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::sqlx::DatabaseProcessor;

/// Consumes [`AddInvitationSlotEvent`] and mints `Free` invite slots for the
/// target user — the external counterpart to an admin invitation grant. Users
/// cannot mint slots themselves; this only reacts to a trusted external event.
#[derive(Clone)]
pub struct InvitationSlotHook {
    pub db: DatabaseProcessor,
}

impl AmqpMessageProcessor<AddInvitationSlotEvent> for InvitationSlotHook {
    const QUEUE: &'static str = "auth_add_invitation_slot";
}

impl Processor<AddInvitationSlotEvent> for InvitationSlotHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Hook:AddInvitationSlotEvent")]
    async fn process(&self, input: AddInvitationSlotEvent) -> Result<Self::Output, Self::Error> {
        if input.count <= 0 {
            return Err(wakuwaku::Error::InvalidInput);
        }
        let owner = AccountId(input.user_id);
        let now = now_primitive();
        let will_expire_at = input
            .expire_in_secs
            .map(|s| now + Duration::seconds(s as i64));
        for _ in 0..input.count {
            self.db
                .process(CreateInvite {
                    owner,
                    invite_token: generate_invite_token(),
                    status: InviteStatus::Free,
                    source: input.source.clone(),
                    will_expire_at,
                })
                .await?;
        }
        Ok(())
    }
}
