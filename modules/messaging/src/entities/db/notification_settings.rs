use kanau::processor::Processor;
use time::PrimitiveDateTime;
use uuid::Uuid;
use wakuwaku::sqlx::DatabaseProcessor;

/// A row of `messaging.notification_settings` (`id` is the user id).
#[derive(Debug, Clone)]
pub struct NotificationSettings {
    pub id: Uuid,
    /// Email the user on every new sign-in.
    pub send_login_email: bool,
    /// Email the user when someone registers using one of their invitations.
    pub send_invitation_email: bool,
    /// Include the user in marketing emails.
    pub receive_marketing_email: bool,
    pub created_at: PrimitiveDateTime,
    pub updated_at: PrimitiveDateTime,
}

/// Look up a user's notification settings.
pub struct FindNotificationSettingsById {
    pub id: Uuid,
}

impl Processor<FindNotificationSettingsById> for DatabaseProcessor {
    type Output = Option<NotificationSettings>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindNotificationSettingsById")]
    async fn process(
        &self,
        input: FindNotificationSettingsById,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            NotificationSettings,
            r#"SELECT id, send_login_email, send_invitation_email, receive_marketing_email,
                      created_at, updated_at
               FROM messaging.notification_settings WHERE id = $1"#,
            input.id
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Initialise default settings for a new user (no-op if already present).
pub struct InitializeNotificationSettings {
    pub user_id: Uuid,
}

impl Processor<InitializeNotificationSettings> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:InitializeNotificationSettings")]
    async fn process(
        &self,
        input: InitializeNotificationSettings,
    ) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO messaging.notification_settings (id) VALUES ($1)
               ON CONFLICT (id) DO NOTHING"#,
            input.user_id
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Insert or replace a user's notification settings.
pub struct UpsertNotificationSettings {
    pub settings: NotificationSettings,
}

impl Processor<UpsertNotificationSettings> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:UpsertNotificationSettings")]
    async fn process(
        &self,
        input: UpsertNotificationSettings,
    ) -> Result<Self::Output, Self::Error> {
        let s = input.settings;
        sqlx::query!(
            r#"INSERT INTO messaging.notification_settings
               (id, send_login_email, send_invitation_email, receive_marketing_email)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (id) DO UPDATE
               SET send_login_email = EXCLUDED.send_login_email,
                   send_invitation_email = EXCLUDED.send_invitation_email,
                   receive_marketing_email = EXCLUDED.receive_marketing_email,
                   updated_at = now()"#,
            s.id,
            s.send_login_email,
            s.send_invitation_email,
            s.receive_marketing_email
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}
