use crate::entities::db::account::AccountId;
use rust_decimal::Decimal;
use time::PrimitiveDateTime;

pub struct CreditEntity {
    pub account: AccountId,
    pub total_amount: Decimal,
    pub frozen_amount: Decimal,
}

pub struct CreditChangeHistoryEntity {
    pub id: i64,
    pub account: AccountId,
    pub available_amount_change: Decimal,
    pub frozen_amount_change: Decimal,
    pub reason: String,
    pub created_at: PrimitiveDateTime,
}
