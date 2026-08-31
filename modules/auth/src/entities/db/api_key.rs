use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use rand::RngCore;
use sha2::{Digest, Sha256};
use time::PrimitiveDateTime;
use uuid::Uuid;
use wakuwaku::sqlx::DatabaseProcessor;

/// A personal API key value. At rest this holds the SHA-256 hash of the key,
/// never the plaintext.
#[derive(Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct ApiKey(pub String);

impl core::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ApiKey").field(&"***").finish()
    }
}

/// A row of `auth.user_api_key`.
pub struct UserApiKeyEntity {
    pub id: Uuid,
    pub user: AccountId,
    pub key: ApiKey,
    pub remark: String,
    pub created_at: PrimitiveDateTime,
    pub valid_until: Option<PrimitiveDateTime>,
    pub scopes: Vec<String>,
}

/// Hash a plaintext API key for storage/lookup (SHA-256, hex-encoded).
pub fn hash_api_key(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Generate a fresh plaintext API key (`airi_` + URL-safe base32 of 32 bytes).
pub fn generate_api_key() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    format!("airi_{}", fast32::base32::RFC4648_NOPAD.encode(&bytes))
}

/// Create a new API key row (given a precomputed key hash).
pub struct CreateApiKey {
    pub user_id: AccountId,
    pub key_hash: String,
    pub remark: String,
    pub valid_until: Option<PrimitiveDateTime>,
    pub scopes: Vec<String>,
}

impl Processor<CreateApiKey> for DatabaseProcessor {
    type Output = UserApiKeyEntity;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreateApiKey) -> Result<Self::Output, Self::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            UserApiKeyEntity,
            r#"INSERT INTO auth.user_api_key (id, user_id, key_hash, remark, valid_until, scopes)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id, user_id AS "user: AccountId", key_hash AS "key: ApiKey",
                         remark, created_at, valid_until, scopes"#,
            id,
            input.user_id.0,
            input.key_hash,
            input.remark,
            input.valid_until,
            &input.scopes
        )
        .fetch_one(self.db())
        .await
    }
}

/// Look up an API key row by its hash.
pub struct FindApiKeyByHash {
    pub key_hash: String,
}

impl Processor<FindApiKeyByHash> for DatabaseProcessor {
    type Output = Option<UserApiKeyEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindApiKeyByHash) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            UserApiKeyEntity,
            r#"SELECT id, user_id AS "user: AccountId", key_hash AS "key: ApiKey",
                      remark, created_at, valid_until, scopes
               FROM auth.user_api_key WHERE key_hash = $1"#,
            input.key_hash
        )
        .fetch_optional(self.db())
        .await
    }
}

/// List all API keys belonging to a user.
pub struct ListApiKeysByUser {
    pub user_id: AccountId,
}

impl Processor<ListApiKeysByUser> for DatabaseProcessor {
    type Output = Vec<UserApiKeyEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ListApiKeysByUser) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            UserApiKeyEntity,
            r#"SELECT id, user_id AS "user: AccountId", key_hash AS "key: ApiKey",
                      remark, created_at, valid_until, scopes
               FROM auth.user_api_key WHERE user_id = $1 ORDER BY created_at DESC"#,
            input.user_id.0
        )
        .fetch_all(self.db())
        .await
    }
}

/// Delete an API key owned by a user. Returns whether a row was removed.
pub struct DeleteApiKey {
    pub id: Uuid,
    pub user_id: AccountId,
}

impl Processor<DeleteApiKey> for DatabaseProcessor {
    type Output = bool;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: DeleteApiKey) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"DELETE FROM auth.user_api_key WHERE id = $1 AND user_id = $2"#,
            input.id,
            input.user_id.0
        )
        .execute(self.db())
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
