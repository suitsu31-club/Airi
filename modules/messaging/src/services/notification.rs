//! Notification preference reads/writes.

use crate::entities::db::notification_settings::{
    FindNotificationSettingsById, NotificationSettings, UpsertNotificationSettings,
};
use kanau::processor::Processor;
use time::{OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;
use wakuwaku::sqlx::DatabaseProcessor;

/// A user's notification preferences.
#[derive(Debug, Clone, Copy)]
pub struct NotificationPrefs {
    /// Email the user on every new sign-in.
    pub send_login_email: bool,
    /// Email the user when someone registers using one of their invitations.
    pub send_invitation_email: bool,
    /// Include the user in marketing emails.
    pub receive_marketing_email: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            send_login_email: false,
            send_invitation_email: true,
            receive_marketing_email: false,
        }
    }
}

fn now_primitive() -> PrimitiveDateTime {
    let now = OffsetDateTime::now_utc();
    PrimitiveDateTime::new(now.date(), now.time())
}

/// Notification settings service.
#[derive(Clone)]
pub struct NotificationSettingsService {
    pub db: DatabaseProcessor,
}

/// Fetch a user's notification preferences (defaults when absent).
pub struct GetNotificationSettings {
    pub user_id: Uuid,
}

impl Processor<GetNotificationSettings> for NotificationSettingsService {
    type Output = NotificationPrefs;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetNotificationSettings")]
    async fn process(&self, input: GetNotificationSettings) -> Result<Self::Output, Self::Error> {
        let settings = self
            .db
            .process(FindNotificationSettingsById { id: input.user_id })
            .await?;
        Ok(
            settings.map_or_else(NotificationPrefs::default, |s| NotificationPrefs {
                send_login_email: s.send_login_email,
                send_invitation_email: s.send_invitation_email,
                receive_marketing_email: s.receive_marketing_email,
            }),
        )
    }
}

/// Replace a user's notification preferences.
pub struct SetNotificationSettings {
    pub user_id: Uuid,
    pub prefs: NotificationPrefs,
}

impl Processor<SetNotificationSettings> for NotificationSettingsService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:SetNotificationSettings")]
    async fn process(&self, input: SetNotificationSettings) -> Result<Self::Output, Self::Error> {
        let now = now_primitive();
        self.db
            .process(UpsertNotificationSettings {
                settings: NotificationSettings {
                    id: input.user_id,
                    send_login_email: input.prefs.send_login_email,
                    send_invitation_email: input.prefs.send_invitation_email,
                    receive_marketing_email: input.prefs.receive_marketing_email,
                    created_at: now,
                    updated_at: now,
                },
            })
            .await?;
        Ok(())
    }
}
