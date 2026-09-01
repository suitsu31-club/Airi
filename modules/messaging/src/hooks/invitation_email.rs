//! Renders and enqueues invitation emails.

use crate::config::MessagingConfig;
use crate::events::MailSendCall;
use askama::Template;
use base::config_provider::find_config_from_redis;
use base::events::InvitationSentEvent;
use kanau::processor::Processor;
use wakuwaku::amqp::{AmqpMessageProcessor, AmqpMessageSend, AmqpPool};
use wakuwaku::redis::RedisConnection;

#[derive(Template)]
#[template(path = "invitation_email.html")]
struct InvitationEmailTemplate {
    site_name: String,
    logo_url: String,
    invite_url: String,
}

/// Consumes [`InvitationSentEvent`] and publishes a [`MailSendCall`].
pub struct InvitationEmailHook {
    pub config_store: RedisConnection,
    pub mq: AmqpPool,
}

impl AmqpMessageProcessor<InvitationSentEvent> for InvitationEmailHook {
    const QUEUE: &'static str = "messaging_invitation_email";
}

impl Processor<InvitationSentEvent> for InvitationEmailHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Hook:InvitationSentEvent")]
    async fn process(&self, input: InvitationSentEvent) -> Result<Self::Output, Self::Error> {
        let cfg = find_config_from_redis::<MessagingConfig>(&mut self.config_store.clone()).await?;
        let invite_url = format!(
            "{}/invite/{}",
            cfg.site.frontend_domain.trim_end_matches('/'),
            input.invite_token
        );
        let template = InvitationEmailTemplate {
            site_name: cfg.site.site_name.clone(),
            logo_url: cfg.site.logo_url.clone(),
            invite_url,
        };
        let body = template
            .render()
            .map_err(|e| wakuwaku::Error::Io(e.into()))?;
        MailSendCall {
            from: None,
            to: input.email,
            subject: format!("You're invited to {}", cfg.site.site_name),
            body,
        }
        .send(&self.mq)
        .await?;
        Ok(())
    }
}
