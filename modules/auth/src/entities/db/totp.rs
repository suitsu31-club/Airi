//! TOTP secret and one-time recovery-code persistence.
//!
//! This submodule owns the whole TOTP aggregate: the per-user shared secret in
//! `auth.totp` and the single-use recovery-code hashes in
//! `auth.totp_recovery_code`. Secrets and hashes are stored as raw `bytea`.

use kanau::processor::Processor;
use uuid::Uuid;
use wakuwaku::sqlx::DatabaseProcessor;

/// A row of `auth.totp`. Holds the raw HMAC secret; never logged.
#[derive(Clone, sqlx::FromRow, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct UserTotp {
    #[zeroize(skip)]
    pub user_id: Uuid,
    pub secret: Vec<u8>,
}

impl core::fmt::Debug for UserTotp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UserTotp")
            .field("user_id", &self.user_id)
            .field("secret", &"***")
            .finish()
    }
}

/// Create (or replace) a user's TOTP secret.
pub struct CreateUserTotp {
    pub user_id: Uuid,
    pub secret: Vec<u8>,
}

impl Processor<CreateUserTotp> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CreateUserTotp")]
    async fn process(&self, input: CreateUserTotp) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO "auth"."totp" (user_id, secret)
               VALUES ($1, $2)
               ON CONFLICT (user_id) DO UPDATE SET secret = $2"#,
            input.user_id,
            input.secret
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Look up a user's TOTP secret.
pub struct FindUserTotpByUserId {
    pub user_id: Uuid,
}

impl Processor<FindUserTotpByUserId> for DatabaseProcessor {
    type Output = Option<UserTotp>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindUserTotpByUserId")]
    async fn process(&self, input: FindUserTotpByUserId) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            UserTotp,
            r#"SELECT user_id, secret FROM "auth"."totp" WHERE user_id = $1 LIMIT 1"#,
            input.user_id
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Delete a user's TOTP secret.
pub struct DeleteUserTotpByUserId {
    pub user_id: Uuid,
}

impl Processor<DeleteUserTotpByUserId> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:DeleteUserTotpByUserId")]
    async fn process(&self, input: DeleteUserTotpByUserId) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"DELETE FROM "auth"."totp" WHERE user_id = $1"#,
            input.user_id
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Store a batch of recovery-code hashes for a user.
pub struct StoreRecoveryCodes {
    pub user_id: Uuid,
    pub code_hashes: Vec<Vec<u8>>,
}

impl Processor<StoreRecoveryCodes> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:StoreRecoveryCodes")]
    async fn process(&self, input: StoreRecoveryCodes) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO "auth"."totp_recovery_code" (user_id, code_hash)
               SELECT $1, unnest($2::bytea[])"#,
            input.user_id,
            &input.code_hashes
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Atomically consume a single recovery code. Returns whether a code was
/// removed (i.e. the presented hash was valid and unused).
pub struct ConsumeRecoveryCode {
    pub user_id: Uuid,
    pub code_hash: Vec<u8>,
}

impl Processor<ConsumeRecoveryCode> for DatabaseProcessor {
    type Output = bool;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ConsumeRecoveryCode")]
    async fn process(&self, input: ConsumeRecoveryCode) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"DELETE FROM "auth"."totp_recovery_code"
               WHERE user_id = $1 AND code_hash = $2
               RETURNING user_id"#,
            input.user_id,
            input.code_hash
        )
        .fetch_optional(self.db())
        .await?;
        Ok(row.is_some())
    }
}

/// Delete all recovery codes belonging to a user.
pub struct DeleteRecoveryCodesByUserId {
    pub user_id: Uuid,
}

impl Processor<DeleteRecoveryCodesByUserId> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:DeleteRecoveryCodesByUserId")]
    async fn process(
        &self,
        input: DeleteRecoveryCodesByUserId,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"DELETE FROM "auth"."totp_recovery_code" WHERE user_id = $1"#,
            input.user_id
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Count a user's remaining recovery codes.
pub struct CountRecoveryCodesByUserId {
    pub user_id: Uuid,
}

impl Processor<CountRecoveryCodesByUserId> for DatabaseProcessor {
    type Output = i64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CountRecoveryCodesByUserId")]
    async fn process(
        &self,
        input: CountRecoveryCodesByUserId,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM "auth"."totp_recovery_code" WHERE user_id = $1"#,
            input.user_id
        )
        .fetch_one(self.db())
        .await
    }
}
