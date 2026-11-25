//! Fails messages whose `EmailQueued` was lost.
//!
//! `SendEmail` stores a message before publishing its `EmailQueued`. If the
//! publish fails and the caller never retries, the transport never sees the
//! message and it would stay queued forever. This job marks such messages
//! failed instead of publishing again: a message that late (a sign-in link,
//! say) is no longer worth delivering.

use crate::config::MailerConfig;
use crate::entities::db::send_box::FailUnattemptedMail;
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
        let Some(queued_before) = utc_secs_before(tick.fired_at, config.stale_after_secs) else {
            return Ok(());
        };
        let failed = self
            .db
            .process(FailUnattemptedMail {
                queued_before,
                failure_reply: format!(
                    "not handed to the transport within {} seconds",
                    config.stale_after_secs
                ),
            })
            .await?;
        if failed > 0 {
            tracing::warn!(failed, "failed queued mail the transport never attempted");
        }
        Ok(())
    }
}

impl CronJob for FailStaleMail {
    const INTERVAL: CronJobInterval = CronJobInterval::AtLeast(Duration::from_secs(60));
}
