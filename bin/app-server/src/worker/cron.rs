//! Cron worker: periodically refreshes cached config and emits cleanup signals.

use super::Deps;
use auth::config::AuthConfig;
use auth::events::{InvitationExpiryCleanupSignal, SessionCleanupSignal};
use base::config_provider::refresh_config;
use messaging::config::MessagingConfig;
use std::time::Duration;
use time::OffsetDateTime;
use wakuwaku::amqp::AmqpMessageSend;
use wakuwaku::interval_job::IntervalJobExecutionSignal;

pub async fn run(deps: Deps) -> anyhow::Result<()> {
    let Deps { db, redis, mq } = deps;
    let secs: u64 = std::env::var("CRON_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_600);
    let mut interval = tokio::time::interval(Duration::from_secs(secs));
    tracing::info!(interval_secs = secs, "cron worker started");

    loop {
        interval.tick().await;
        if let Err(e) = refresh_config::<AuthConfig>(db.db(), &mut redis.clone()).await {
            tracing::warn!(error = %e, "failed to refresh auth config");
        }
        if let Err(e) = refresh_config::<MessagingConfig>(db.db(), &mut redis.clone()).await {
            tracing::warn!(error = %e, "failed to refresh messaging config");
        }
        let now = OffsetDateTime::now_utc();
        if let Err(e) = SessionCleanupSignal::tick(now).send(&mq).await {
            tracing::warn!(error = %e, "failed to publish session cleanup signal");
        }
        if let Err(e) = InvitationExpiryCleanupSignal::tick(now).send(&mq).await {
            tracing::warn!(error = %e, "failed to publish invitation expiry signal");
        }
    }
}
