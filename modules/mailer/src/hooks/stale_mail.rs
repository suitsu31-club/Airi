//! Fails messages left queued with nothing left to deliver them.
//!
//! A queued message can lose its `EmailQueued` in two ways: `SendEmail`
//! stored it but the publish failed and the caller never retried, or the
//! transport hit a retryable failure (relay down, 4xx) until the consumer
//! gave up retrying and dropped the event. Either way the message would stay
//! queued forever, and the retention job would keep its body. This job marks
//! it failed instead of publishing again: a message that late (a sign-in
//! link, say) is no longer worth delivering.

use crate::config::MailerConfig;
use crate::entities::db::send_box::FailStaleQueuedMail;
use crate::utils::time::utc_secs_before;
use base::Error;
use base::services::config::{ConfigLoader, LoadConfig};
use kanau::processor::Processor;
use std::time::Duration;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use wakuwaku::{CronJob, CronJobInterval, CronTick};

/// Every replica runs it; the update only changes messages that are still
/// queued, so concurrent runs fail each message once.
pub struct FailStaleMail {
    db: Db<PgSource, ReadWrite>,
    config: ConfigLoader,
}

impl FailStaleMail {
    pub const fn new(db: Db<PgSource, ReadWrite>, config: ConfigLoader) -> Self {
        Self { db, config }
    }
}

impl Processor<CronTick> for FailStaleMail {
    type Output = ();
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, tick: CronTick) -> Result<(), Error> {
        let config = self
            .config
            .process(LoadConfig::<MailerConfig>::new())
            .await?;
        let Some(last_activity_before) = utc_secs_before(tick.fired_at, config.stale_after_secs)
        else {
            return Ok(());
        };
        let failed = self
            .db
            .process(FailStaleQueuedMail {
                last_activity_before,
                failure_reply: format!(
                    "not delivered within {} seconds of being queued or attempted",
                    config.stale_after_secs
                ),
            })
            .await?;
        if failed > 0 {
            tracing::warn!(failed, "failed queued mail the transport no longer retries");
        }
        Ok(())
    }
}

impl CronJob for FailStaleMail {
    const INTERVAL: CronJobInterval = CronJobInterval::AtLeast(Duration::from_secs(60));
}
