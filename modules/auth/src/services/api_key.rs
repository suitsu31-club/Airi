//! Personal API key management.

use crate::entities::db::account::AccountId;
use crate::entities::db::api_key::{
    self as db_api_key, UserApiKeyEntity, generate_api_key, hash_api_key,
};
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use uuid::Uuid;
use wakuwaku::sqlx::DatabaseProcessor;

/// Creates, lists, and revokes personal API keys.
#[derive(Clone)]
pub struct ApiKeyService {
    pub db: DatabaseProcessor,
}

/// Create a new API key. The plaintext is returned exactly once.
pub struct CreateApiKey {
    pub user_id: AccountId,
    pub remark: String,
    pub valid_until: Option<PrimitiveDateTime>,
    pub scopes: Vec<String>,
}

/// A freshly created API key.
pub struct CreatedApiKey {
    pub id: Uuid,
    pub plaintext: String,
}

impl Processor<CreateApiKey> for ApiKeyService {
    type Output = CreatedApiKey;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:CreateApiKey")]
    async fn process(&self, input: CreateApiKey) -> Result<Self::Output, Self::Error> {
        let plaintext = generate_api_key();
        let key_hash = hash_api_key(&plaintext);
        let entity = self
            .db
            .process(db_api_key::CreateApiKey {
                user_id: input.user_id,
                key_hash,
                remark: input.remark,
                valid_until: input.valid_until,
                scopes: input.scopes,
            })
            .await?;
        Ok(CreatedApiKey {
            id: entity.id,
            plaintext,
        })
    }
}

/// List a user's API keys.
pub struct ListApiKeys {
    pub user_id: AccountId,
}

impl Processor<ListApiKeys> for ApiKeyService {
    type Output = Vec<UserApiKeyEntity>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:ListApiKeys")]
    async fn process(&self, input: ListApiKeys) -> Result<Self::Output, Self::Error> {
        Ok(self
            .db
            .process(db_api_key::ListApiKeysByUser {
                user_id: input.user_id,
            })
            .await?)
    }
}

/// Revoke one of a user's API keys.
pub struct RevokeApiKey {
    pub user_id: AccountId,
    pub id: Uuid,
}

impl Processor<RevokeApiKey> for ApiKeyService {
    type Output = bool;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:RevokeApiKey")]
    async fn process(&self, input: RevokeApiKey) -> Result<Self::Output, Self::Error> {
        Ok(self
            .db
            .process(db_api_key::DeleteApiKey {
                id: input.id,
                user_id: input.user_id,
            })
            .await?)
    }
}
