//! gRPC adapter for `airi.messaging.NotificationSettings`.

use crate::services::notification::{
    GetNotificationSettings, NotificationPrefs, NotificationSettingsService, SetNotificationSettings,
};
use app_protobuf::messaging::notification_settings_server::NotificationSettings;
use app_protobuf::{messaging as pb, shared};
use auth::rpc::middleware::UserId;
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the `NotificationSettings` service.
#[derive(Clone)]
pub struct NotificationRpc {
    pub settings: NotificationSettingsService,
}

#[tonic::async_trait]
impl NotificationSettings for NotificationRpc {
    async fn get_notification_settings(
        &self,
        request: Request<pb::GetNotificationSettingsRequest>,
    ) -> Result<Response<pb::GetNotificationSettingsReply>, Status> {
        let user = UserId::from_request(&request)?;
        let prefs = self
            .settings
            .process(GetNotificationSettings { user_id: user.0 })
            .await?;
        Ok(Response::new(pb::GetNotificationSettingsReply {
            settings: Some(pb::NotificationSettingsData {
                send_login_email: prefs.send_login_email,
                send_invitation_email: prefs.send_invitation_email,
                receive_marketing_email: prefs.receive_marketing_email,
            }),
        }))
    }

    async fn set_notification_settings(
        &self,
        request: Request<pb::SetNotificationSettingsRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let data = req
            .settings
            .ok_or_else(|| Status::invalid_argument("settings required"))?;
        self.settings
            .process(SetNotificationSettings {
                user_id: user.0,
                prefs: NotificationPrefs {
                    send_login_email: data.send_login_email,
                    send_invitation_email: data.send_invitation_email,
                    receive_marketing_email: data.receive_marketing_email,
                },
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }
}
