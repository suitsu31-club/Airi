use crate::entities::db::account::AccountId;
use crate::entities::db::api_key::ApiKey;
use crate::entities::db::sessions::{SessionEntity, SessionId};
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

pub struct IdentityVerifier {
    pub database_processor: DatabaseProcessor,
}

pub struct SessionIdVerify {
    pub session_id: SessionId,
}

impl Processor<SessionIdVerify> for IdentityVerifier {
    type Error = wakuwaku::Error;
    type Output = SessionEntity;
    async fn process(&self, input: SessionIdVerify) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct ApiKeyVerify {
    pub api_key: ApiKey,
}

pub struct ApiKeyVerifyOutput {
    pub user: AccountId,
    pub valid_until: PrimitiveDateTime,
    pub scopes: Vec<String>,
}

impl Processor<ApiKeyVerify> for IdentityVerifier {
    type Error = wakuwaku::Error;
    type Output = ApiKeyVerifyOutput;
    async fn process(&self, input: ApiKeyVerify) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}
