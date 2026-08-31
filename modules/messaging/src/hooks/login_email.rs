//! Renders and enqueues login-notification emails (when the user opted in).

use crate::config::MessagingConfig;
use crate::entities::db::notification_settings::FindNotificationSettingsById;
use crate::events::MailSendCall;
use askama::Template;
use auth::entities::db::account::{AccountId, FindAccountById};
use base::config_provider::find_config_from_redis;
use base::events::UserLoginEvent;
use kanau::processor::Processor;
use wakuwaku::amqp::{AmqpMessageProcessor, AmqpMessageSend, AmqpPool};
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

#[derive(Template)]
#[template(path = "login_notification.html")]
struct LoginEmailTemplate {
    site_name: String,
    ip: String,
    user_agent: String,
    time: u64,
}

/// Consumes [`UserLoginEvent`] and, when enabled, publishes a [`MailSendCall`].
pub struct LoginEmailHook {
    pub db: DatabaseProcessor,
    pub config_store: RedisConnection,
    pub mq: AmqpPool,
}

impl AmqpMessageProcessor<UserLoginEvent> for LoginEmailHook {
    const QUEUE: &'static str = "messaging_login_email";
}

impl Processor<UserLoginEvent> for LoginEmailHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: UserLoginEvent) -> Result<Self::Output, Self::Error> {
        let settings = self
            .db
            .process(FindNotificationSettingsById { id: input.user_id })
            .await?;
        // Default off when the user has no settings row yet.
        if !settings.map(|s| s.send_login_email).unwrap_or(false) {
            return Ok(());
        }

        // Resolve the recipient address via the shared `auth` account query.
        let Some(account) = self
            .db
            .process(FindAccountById {
                id: AccountId(input.user_id),
            })
            .await?
        else {
            return Ok(());
        };

        let cfg = find_config_from_redis::<MessagingConfig>(&mut self.config_store.clone()).await?;
        let template = LoginEmailTemplate {
            site_name: cfg.site.site_name.clone(),
            ip: input.ip,
            user_agent: input.user_agent,
            time: input.at,
        };
        let body = template
            .render()
            .map_err(|e| wakuwaku::Error::Io(e.into()))?;
        MailSendCall {
            from: None,
            to: account.email,
            subject: format!("New sign-in to {}", cfg.site.site_name),
            body,
        }
        .send(&self.mq)
        .await?;
        Ok(())
    }
}
