use kanau::processor::Processor;
use time::PrimitiveDateTime;
use uuid::Uuid;
use wakuwaku::sqlx::DatabaseProcessor;

/// Strongly typed account identifier (a transparent wrapper over `Uuid`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct AccountId(pub Uuid);

/// A row of `auth.account`.
#[derive(Debug, Clone)]
pub struct AccountEntity {
    pub id: AccountId,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub password_hash: String,
    pub registered_at: PrimitiveDateTime,
}

/// Look up an account by id.
pub struct FindAccountById {
    pub id: AccountId,
}

impl Processor<FindAccountById> for DatabaseProcessor {
    type Output = Option<AccountEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindAccountById) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AccountEntity,
            r#"SELECT id AS "id: AccountId", username, email, avatar_url, password_hash, registered_at
               FROM auth.account WHERE id = $1"#,
            input.id.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Look up an account by email address.
pub struct FindAccountByEmail {
    pub email: String,
}

impl Processor<FindAccountByEmail> for DatabaseProcessor {
    type Output = Option<AccountEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindAccountByEmail) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AccountEntity,
            r#"SELECT id AS "id: AccountId", username, email, avatar_url, password_hash, registered_at
               FROM auth.account WHERE email = $1"#,
            input.email
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Look up an account by username.
pub struct FindAccountByUsername {
    pub username: String,
}

impl Processor<FindAccountByUsername> for DatabaseProcessor {
    type Output = Option<AccountEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindAccountByUsername) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AccountEntity,
            r#"SELECT id AS "id: AccountId", username, email, avatar_url, password_hash, registered_at
               FROM auth.account WHERE username = $1"#,
            input.username
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Create a new account.
pub struct CreateAccount {
    pub id: AccountId,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub password_hash: String,
}

/// Outcome of [`CreateAccount`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateAccountResult {
    Success,
    EmailTaken,
    UsernameTaken,
}

impl Processor<CreateAccount> for DatabaseProcessor {
    type Output = CreateAccountResult;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreateAccount) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"INSERT INTO auth.account (id, username, email, avatar_url, password_hash)
               VALUES ($1, $2, $3, $4, $5)"#,
            input.id.0,
            input.username,
            input.email,
            input.avatar_url,
            input.password_hash
        )
        .execute(self.db())
        .await;

        match result {
            Ok(_) => Ok(CreateAccountResult::Success),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => match e.constraint() {
                Some("account_email_key") => Ok(CreateAccountResult::EmailTaken),
                Some("account_username_key") => Ok(CreateAccountResult::UsernameTaken),
                _ => Err(sqlx::Error::Database(e)),
            },
            Err(e) => Err(e),
        }
    }
}

/// Update an account's password hash.
pub struct UpdatePasswordHash {
    pub id: AccountId,
    pub password_hash: String,
}

impl Processor<UpdatePasswordHash> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: UpdatePasswordHash) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.account SET password_hash = $2 WHERE id = $1"#,
            input.id.0,
            input.password_hash
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}
