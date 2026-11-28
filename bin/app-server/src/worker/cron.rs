//! The cron worker: runs each module's `CronJob`s in this process.
//!
//! Every replica in a cron mode runs every job, so jobs are idempotent or
//! take a Redis lock themselves.

use super::Services;
use base::services::config::ConfigLoader;
use mailer::hooks::mail_retention::DeleteOldMailContent;
use mailer::hooks::stale_mail::FailStaleMail;
use wakuwaku::services::cron_job::CronRuntime;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};

/// Start every job. Must be called within the Tokio runtime.
pub fn start(services: &Services) -> CronRuntime {
    let (_, runtime) = services
        .clone()
        // mailer
        .compose_cron_job(|s| {
            DeleteOldMailContent::new(
                s.provide::<Db<PgSource, ReadWrite>, _>().clone(),
                s.provide::<ConfigLoader, _>().clone(),
            )
        })
        .compose_cron_job(|s| {
            FailStaleMail::new(
                s.provide::<Db<PgSource, ReadWrite>, _>().clone(),
                s.provide::<ConfigLoader, _>().clone(),
            )
        })
        .start();
    tracing::info!("cron worker started");
    runtime
}
