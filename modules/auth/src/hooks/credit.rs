//! Applies external credit-change events to the ledger.

use crate::entities::db::account::AccountId;
use crate::entities::db::credit::ApplyCreditChange;
use base::events::CreditChangeEvent;
use kanau::processor::Processor;
use rust_decimal::Decimal;
use std::str::FromStr;
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::sqlx::DatabaseProcessor;

/// Consumes [`CreditChangeEvent`] and applies the delta to `auth.credit`.
#[derive(Clone)]
pub struct CreditHook {
    pub db: DatabaseProcessor,
}

impl AmqpMessageProcessor<CreditChangeEvent> for CreditHook {
    const QUEUE: &'static str = "auth_credit_change";
}

impl Processor<CreditChangeEvent> for CreditHook {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreditChangeEvent) -> Result<Self::Output, Self::Error> {
        let available_delta =
            Decimal::from_str(&input.available_delta).map_err(|_| wakuwaku::Error::InvalidInput)?;
        let frozen_delta =
            Decimal::from_str(&input.frozen_delta).map_err(|_| wakuwaku::Error::InvalidInput)?;
        self.db
            .process(ApplyCreditChange {
                account: AccountId(input.user_id),
                available_delta,
                frozen_delta,
                reason: input.reason,
            })
            .await?;
        Ok(())
    }
}
