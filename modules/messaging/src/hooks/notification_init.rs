//! Initialises per-user notification settings on registration.

use crate::entities::db::notification_settings::InitializeNotificationSettings;
use base::events::UserRegisteredEvent;
use kanau::processor::Processor;
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::sqlx::DatabaseProcessor;

/// Consumes [`UserRegisteredEvent`] and creates default notification settings.
pub struct NotificationInitHook {
    pub db: DatabaseProcessor,
}

impl AmqpMessageProcessor<UserRegisteredEvent> for NotificationInitHook {
    const QUEUE: &'static str = "messaging_notification_init";
}

impl Processor<UserRegisteredEvent> for NotificationInitHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Hook:UserRegisteredEvent")]
    async fn process(&self, input: UserRegisteredEvent) -> Result<Self::Output, Self::Error> {
        self.db
            .process(InitializeNotificationSettings {
                user_id: input.user_id,
            })
            .await?;
        Ok(())
    }
}
