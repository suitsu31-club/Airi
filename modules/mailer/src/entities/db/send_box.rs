use crate::entities::db::sender::SmtpSenderId;
use time::PrimitiveDateTime;
use type_safe_data::mail::EmailAddress;

#[derive(Debug, Clone, Copy)]
pub struct MailId(pub i64);

#[derive(Debug, Clone)]
pub struct SendBoxEntity {
    pub mail_id: MailId,
    pub send_to_address: EmailAddress,
    pub send_to_name: String,
    pub send_as: SmtpSenderId,
    
    pub title: String,
    pub message_content: String,
    pub format: EmailFormat,
    
    pub created_at: PrimitiveDateTime,
    pub sent_at: Option<PrimitiveDateTime>,

    /// The mail must be sent before this time, or it will be canceled.
    pub send_before_or_cancel: Option<PrimitiveDateTime>,

    pub status: MailStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailFormat {
    Plain,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailStatus {
    Sent,
    Pending,
    Cancelled,
}

impl wakuwaku::entity::Entity for SendBoxEntity {}