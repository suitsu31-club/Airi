//! Deletes the bodies of old messages.
//!
//! Bodies carry sign-in links and personal text, so they are kept only for
//! `delete_mail_older_than` seconds after the message was created. The rest
//! of the row (recipient, title, status) stays as the delivery record.

use crate::config::MailerConfig;
use crate::entities::db::send_box::DeleteMailContentBefore;
use crate::utils::time::utc_secs_before;
use base::Error;
use base::services::config::{ConfigLoader, LoadConfig};
use kanau::processor::Processor;
use std::time::Duration;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use wakuwaku::{CronJob, CronJobInterval, CronTick};

/// Every replica runs it; deleting a body that is already gone changes
/// nothing, so concurrent runs are harmless.
pub struct DeleteOldMailContent {
    db: Db<PgSource, ReadWrite>,
    config: ConfigLoader,
}

impl DeleteOldMailContent {
    pub const fn new(db: Db<PgSource, ReadWrite>, config: ConfigLoader) -> Self {
        Self { db, config }
    }
}

impl Processor<CronTick> for DeleteOldMailContent {
    type Output = ();
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, tick: CronTick) -> Result<(), Error> {
        let config = self
            .config
            .process(LoadConfig::<MailerConfig>::new())
            .await?;
        let Some(created_before) = utc_secs_before(tick.fired_at, config.delete_mail_older_than)
        else {
            return Ok(());
        };
        let deleted = self
            .db
            .process(DeleteMailContentBefore { created_before })
            .await?;
        if deleted > 0 {
            tracing::info!(deleted, "deleted the bodies of old messages");
        }
        Ok(())
    }
}

impl CronJob for DeleteOldMailContent {
    const INTERVAL: CronJobInterval = CronJobInterval::AtLeast(Duration::from_secs(60 * 60));
}
