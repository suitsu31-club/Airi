//! Delivers each queued message (feature `delivery`).

use crate::events::outbound::EmailQueued;
use crate::services::delivery::{DeliverMail, MailDeliveryService};
use crate::services::mailer::MailId;
use kanau::processor::Processor;
use std::num::NonZeroUsize;
use wakuwaku::ServiceCreation;
use wakuwaku::iggy::EventHandler;
use wakuwaku::iggy::error::HandleError;

/// Consumer of `mailer_outbound`. Failures are classed by `base::Error`:
/// connection failures and 4xx replies are retried, a 5xx reply marks the
/// message failed and succeeds.
pub struct DeliverQueuedMail {
    service: MailDeliveryService,
}

impl ServiceCreation for DeliverQueuedMail {
    type Dep = MailDeliveryService;

    fn create_service(service: MailDeliveryService) -> Self {
        Self { service }
    }
}

impl EventHandler<EmailQueued> for DeliverQueuedMail {
    const MAX_BATCH: NonZeroUsize = NonZeroUsize::MIN;

    #[tracing::instrument(skip_all, err)]
    async fn handle(&self, run: &[EmailQueued]) -> Result<(), HandleError> {
        for event in run {
            self.service
                .process(DeliverMail {
                    mail_id: MailId(event.mail_id),
                })
                .await?;
        }
        Ok(())
    }
}
