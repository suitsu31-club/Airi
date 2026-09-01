//! Notifies an inviter (when opted in) that their invitation was accepted.

use crate::config::MessagingConfig;
use crate::entities::db::notification_settings::FindNotificationSettingsById;
use crate::events::MailSendCall;
use askama::Template;
use auth::entities::db::account::{AccountId, FindAccountById};
use base::config_provider::find_config_from_redis;
use base::events::InvitationAcceptedEvent;
use kanau::processor::Processor;
use wakuwaku::amqp::{AmqpMessageProcessor, AmqpMessageSend, AmqpPool};
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

#[derive(Template)]
#[template(path = "invitation_accepted.html")]
struct InvitationAcceptedTemplate {
    site_name: String,
    logo_url: String,
    site_url: String,
    new_member_username: String,
}

/// Consumes [`InvitationAcceptedEvent`] and, when the inviter opted in,
/// publishes a [`MailSendCall`] addressed to the inviter.
pub struct InvitationAcceptedEmailHook {
    pub db: DatabaseProcessor,
    pub config_store: RedisConnection,
    pub mq: AmqpPool,
}

impl AmqpMessageProcessor<InvitationAcceptedEvent> for InvitationAcceptedEmailHook {
    const QUEUE: &'static str = "messaging_invitation_accepted_email";
}

impl Processor<InvitationAcceptedEvent> for InvitationAcceptedEmailHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Hook:InvitationAcceptedEvent")]
    async fn process(&self, input: InvitationAcceptedEvent) -> Result<Self::Output, Self::Error> {
        let settings = self
            .db
            .process(FindNotificationSettingsById {
                id: input.inviter_id,
            })
            .await?;
        // Default on when the inviter has no settings row yet (mirrors the
        // column default); every registered user is seeded with one anyway.
        if !settings.map(|s| s.send_invitation_email).unwrap_or(true) {
            return Ok(());
        }

        // Resolve the inviter's address via the shared `auth` account query.
        let Some(account) = self
            .db
            .process(FindAccountById {
                id: AccountId(input.inviter_id),
            })
            .await?
        else {
            return Ok(());
        };

        let cfg = find_config_from_redis::<MessagingConfig>(&mut self.config_store.clone()).await?;
        let template = InvitationAcceptedTemplate {
            site_name: cfg.site.site_name.clone(),
            logo_url: cfg.site.logo_url.clone(),
            site_url: cfg.site.frontend_domain.trim_end_matches('/').to_string(),
            new_member_username: input.new_member_username,
        };
        let body = template
            .render()
            .map_err(|e| wakuwaku::Error::Io(e.into()))?;
        MailSendCall {
            from: None,
            to: account.email,
            subject: format!("Your invitation to {} was accepted", cfg.site.site_name),
            body,
        }
        .send(&self.mq)
        .await?;
        Ok(())
    }
}
