//! Public gRPC worker: mounts every user-facing service behind the session-auth
//! layer.

use super::Deps;
use app_protobuf::admin::admin_manage_server::AdminManageServer;
use app_protobuf::auth::invitation_server::InvitationServer;
use app_protobuf::auth::user_auth_server::UserAuthServer;
use app_protobuf::auth::user_profile_server::UserProfileServer;
use app_protobuf::messaging::notification_settings_server::NotificationSettingsServer;
use auth::rpc::middleware::UserAuthLayer;
use auth::rpc::{AdminRpc, InvitationRpc, UserAuthRpc, UserProfileRpc};
use auth::services::account::AccountService;
use auth::services::admin::AdminService;
use auth::services::api_key::ApiKeyService;
use auth::services::invitation::InvitationService;
use auth::services::login::LoginService;
use auth::services::profile::ProfileService;
use auth::services::session::SessionService;
use auth::utils::identity::IdentityVerifier;
use auth::utils::password::Argon2PasswordAlgorithm;
use messaging::rpc::NotificationRpc;
use messaging::services::notification::NotificationSettingsService;
use std::net::SocketAddr;

pub async fn run(deps: Deps, addr: SocketAddr) -> anyhow::Result<()> {
    let Deps { db, redis, mq } = deps;
    let alg = Argon2PasswordAlgorithm;

    let session = SessionService {
        db: db.clone(),
        redis: redis.clone(),
    };
    let account = AccountService {
        db: db.clone(),
        redis: redis.clone(),
        mq: mq.clone(),
        alg: alg.clone(),
    };
    let login = LoginService {
        db: db.clone(),
        mq: mq.clone(),
        alg: alg.clone(),
        session: session.clone(),
    };
    let api_key = ApiKeyService { db: db.clone() };
    let invitation = InvitationService {
        db: db.clone(),
        redis: redis.clone(),
        mq: mq.clone(),
    };
    let profile = ProfileService { db: db.clone() };
    let admin = AdminService {
        db: db.clone(),
        redis: redis.clone(),
        session: session.clone(),
    };
    let verifier = IdentityVerifier {
        db: db.clone(),
        redis: redis.clone(),
    };
    let notification = NotificationSettingsService { db: db.clone() };

    let user_auth = UserAuthRpc {
        account,
        login,
        session: session.clone(),
    };
    let user_profile = UserProfileRpc { profile, api_key };
    let invitation_rpc = InvitationRpc { invitation };
    let admin_rpc = AdminRpc { admin };
    let notification_rpc = NotificationRpc {
        settings: notification,
    };

    tracing::info!(%addr, "public gRPC listening");
    tonic::transport::Server::builder()
        .layer(UserAuthLayer { verifier })
        .add_service(UserAuthServer::new(user_auth))
        .add_service(UserProfileServer::new(user_profile))
        .add_service(InvitationServer::new(invitation_rpc))
        .add_service(AdminManageServer::new(admin_rpc))
        .add_service(NotificationSettingsServer::new(notification_rpc))
        .serve(addr)
        .await?;
    Ok(())
}
