//! Applies external moderation ban/unban events.

use crate::entities::db::account::AccountId;
use crate::entities::db::suspense::{InsertSuspense, SuspenseStatus};
use crate::services::session::{SessionService, TerminateAllSessions};
use base::events::SystemBanEvent;
use kanau::processor::Processor;
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::sqlx::DatabaseProcessor;

/// Consumes [`SystemBanEvent`], recording suspense state and terminating
/// sessions on a ban.
#[derive(Clone)]
pub struct BanHook {
    pub db: DatabaseProcessor,
    pub session: SessionService,
}

impl AmqpMessageProcessor<SystemBanEvent> for BanHook {
    const QUEUE: &'static str = "auth_system_ban";
}

impl Processor<SystemBanEvent> for BanHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: SystemBanEvent) -> Result<Self::Output, Self::Error> {
        let account_id = AccountId(input.user_id);
        let status = if input.banned {
            SuspenseStatus::Suspended
        } else {
            SuspenseStatus::Active
        };
        self.db
            .process(InsertSuspense {
                account_id,
                status,
                reason: input.reason,
                operated_by: input.operated_by.map(AccountId),
            })
            .await?;
        if input.banned {
            self.session
                .process(TerminateAllSessions {
                    user_id: account_id,
                })
                .await?;
        }
        Ok(())
    }
}
