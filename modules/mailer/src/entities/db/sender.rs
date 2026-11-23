use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct SmtpSenderId(pub Uuid);

pub struct SmtpSenderEntity {
    pub id: SmtpSenderId,
    pub enabled: bool,
    pub username: String,
    pub password: String,
    pub address: String,
    pub remark: String,
}