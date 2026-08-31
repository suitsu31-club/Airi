//! Periodic cleanup reactors for sessions and invitations.

use crate::entities::db::invite::{ExpireInvitesBefore, ReleaseExpiredPending};
use crate::entities::db::sessions::DeleteExpiredSessions;
use crate::events::{InvitationExpiryCleanupSignal, SessionCleanupSignal};
use crate::utils::datetime::now_primitive;
use kanau::processor::Processor;
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::sqlx::DatabaseProcessor;

/// Consumes cron signals and performs expiry cleanup.
#[derive(Clone)]
pub struct AuthCronHook {
    pub db: DatabaseProcessor,
}

impl AmqpMessageProcessor<SessionCleanupSignal> for AuthCronHook {
    const QUEUE: &'static str = "auth_session_cleanup";
}

impl Processor<SessionCleanupSignal> for AuthCronHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, _input: SessionCleanupSignal) -> Result<Self::Output, Self::Error> {
        self.db
            .process(DeleteExpiredSessions {
                now: now_primitive(),
            })
            .await?;
        Ok(())
    }
}

impl AmqpMessageProcessor<InvitationExpiryCleanupSignal> for AuthCronHook {
    const QUEUE: &'static str = "auth_invitation_expiry";
}

impl Processor<InvitationExpiryCleanupSignal> for AuthCronHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(
        &self,
        _input: InvitationExpiryCleanupSignal,
    ) -> Result<Self::Output, Self::Error> {
        let now = now_primitive();
        self.db.process(ExpireInvitesBefore { now }).await?;
        self.db.process(ReleaseExpiredPending { now }).await?;
        Ok(())
    }
}
