//! SMTP mailer: consumes [`MailSendCall`] and dispatches email.

use crate::config::{MessagingConfig, SmtpConfig};
use crate::events::MailSendCall;
use base::config_provider::find_config_from_redis;
use kanau::processor::Processor;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::redis::RedisConnection;

/// Builds and holds an SMTP transport; sends emails when `ENABLE_MAIL` is set.
pub struct MailerHook {
    config: MessagingConfig,
    transport: AsyncSmtpTransport<Tokio1Executor>,
    enabled: bool,
}

fn build_transport(smtp: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, wakuwaku::Error> {
    let builder = if smtp.starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
            .map_err(|e| wakuwaku::Error::Io(e.into()))?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host)
    };
    let mut builder = builder.port(smtp.port);
    if !smtp.username.is_empty() {
        builder = builder.credentials(Credentials::new(
            smtp.username.clone(),
            smtp.password.clone(),
        ));
    }
    Ok(builder.build())
}

impl MailerHook {
    /// Load config and build the transport.
    pub async fn new(mut config_store: RedisConnection) -> Result<Self, wakuwaku::Error> {
        let config = find_config_from_redis::<MessagingConfig>(&mut config_store).await?;
        let transport = build_transport(&config.smtp)?;
        let enabled = std::env::var("ENABLE_MAIL").is_ok();
        Ok(Self {
            config,
            transport,
            enabled,
        })
    }
}

impl AmqpMessageProcessor<MailSendCall> for MailerHook {
    const QUEUE: &'static str = "messaging_mail_send";
}

impl Processor<MailSendCall> for MailerHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Hook:MailSendCall")]
    async fn process(&self, input: MailSendCall) -> Result<Self::Output, Self::Error> {
        if !self.enabled {
            tracing::info!(
                to = %input.to,
                subject = %input.subject,
                "ENABLE_MAIL unset; not sending. body:\n{}",
                input.body
            );
            return Ok(());
        }
        let from = input
            .from
            .unwrap_or_else(|| self.config.smtp.sender.clone());
        let message = Message::builder()
            .from(from.parse().map_err(|_| wakuwaku::Error::InvalidInput)?)
            .to(input.to.parse().map_err(|_| wakuwaku::Error::InvalidInput)?)
            .subject(input.subject)
            .header(ContentType::TEXT_HTML)
            .body(input.body)
            .map_err(|e| wakuwaku::Error::Io(e.into()))?;
        self.transport
            .send(message)
            .await
            .map_err(|e| wakuwaku::Error::Io(e.into()))?;
        Ok(())
    }
}
