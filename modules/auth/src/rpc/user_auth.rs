//! gRPC adapter for `airi.auth.UserAuth`.

use crate::entities::db::account::AccountId;
use crate::entities::db::sessions::{SessionEntity, SessionId, SessionSecurityOption};
use crate::rpc::middleware::{CurrentSessionId, UserId, request_ip, request_user_agent};
use crate::services::account::{
    AccountService, ChangePassword, ChangePasswordResult, Register, RegisterResult,
};
use crate::services::login::{Login, LoginResult, LoginService};
use crate::services::session::{
    ListUserSessions, RefreshResult, RefreshSession, SessionService, TerminateAllSessions,
    TerminateSession,
};
use crate::utils::datetime::to_unix;
use app_protobuf::auth::user_auth_server::UserAuth;
use app_protobuf::{auth as pb, shared};
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the public `UserAuth` service.
#[derive(Clone)]
pub struct UserAuthRpc {
    pub account: AccountService,
    pub login: LoginService,
    pub session: SessionService,
}

fn security_option_str(option: SessionSecurityOption) -> &'static str {
    match option {
        SessionSecurityOption::RejectDifferentIp => "reject_different_ip",
        SessionSecurityOption::RejectDifferentIpOrUserAgent => "reject_different_ip_or_user_agent",
        SessionSecurityOption::None => "none",
    }
}

fn session_info(s: SessionEntity) -> pb::SessionInfo {
    pb::SessionInfo {
        session_id: s.session_id.0,
        user_agent: s.user_agent,
        ip_address: s.ip_address,
        created_at: to_unix(s.created_at),
        last_refreshed_at: to_unix(s.last_refreshed_at),
        expired_at: s.expired_at.map(to_unix),
        security_option: security_option_str(s.security_option).to_string(),
    }
}

#[tonic::async_trait]
impl UserAuth for UserAuthRpc {
    async fn register(
        &self,
        request: Request<pb::RegisterRequest>,
    ) -> Result<Response<pb::RegisterReply>, Status> {
        let req = request.into_inner();
        let result = self
            .account
            .process(Register {
                username: req.username,
                email: req.email,
                password: req.password,
                invite_token: req.invite_token,
            })
            .await?;
        let (code, user_id) = match result {
            RegisterResult::Success { user_id } => {
                (pb::RegisterResult::Success, Some(user_id.0.to_string()))
            }
            RegisterResult::EmailTaken => (pb::RegisterResult::EmailTaken, None),
            RegisterResult::UsernameTaken => (pb::RegisterResult::UsernameTaken, None),
            RegisterResult::InvalidInvite => (pb::RegisterResult::InvalidInvite, None),
        };
        Ok(Response::new(pb::RegisterReply {
            result: code as i32,
            user_id,
        }))
    }

    async fn login(
        &self,
        request: Request<pb::LoginRequest>,
    ) -> Result<Response<pb::LoginReply>, Status> {
        let ip = request_ip(&request);
        let user_agent = request_user_agent(&request);
        let req = request.into_inner();
        let result = self
            .login
            .process(Login {
                identifier: req.identifier,
                password: req.password,
                ip,
                user_agent,
            })
            .await?;
        let (code, session_id) = match result {
            LoginResult::Success(sid) => (pb::LoginResult::Success, Some(sid.0)),
            LoginResult::WrongCredential => (pb::LoginResult::WrongCredential, None),
            LoginResult::NotFound => (pb::LoginResult::NotFound, None),
            // The wire contract does not distinguish suspension (anti-enumeration).
            LoginResult::Suspended => (pb::LoginResult::WrongCredential, None),
        };
        Ok(Response::new(pb::LoginReply {
            result: code as i32,
            session_id,
        }))
    }

    async fn logout(
        &self,
        request: Request<pb::LogoutRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let session_id = CurrentSessionId::from_request(&request)?;
        self.session
            .process(TerminateSession {
                session_id: SessionId(session_id.0),
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn refresh_session(
        &self,
        request: Request<pb::RefreshSessionRequest>,
    ) -> Result<Response<pb::RefreshSessionReply>, Status> {
        let session_id = CurrentSessionId::from_request(&request)?;
        let ip = request_ip(&request);
        let user_agent = request_user_agent(&request);
        let result = self
            .session
            .process(RefreshSession {
                session_id: SessionId(session_id.0),
                ip,
                user_agent,
            })
            .await?;
        let code = match result {
            RefreshResult::Refreshed => pb::RefreshResult::Refreshed,
            RefreshResult::NotFound => pb::RefreshResult::NotFound,
        };
        Ok(Response::new(pb::RefreshSessionReply {
            result: code as i32,
        }))
    }

    async fn list_sessions(
        &self,
        request: Request<pb::ListSessionsRequest>,
    ) -> Result<Response<pb::ListSessionsReply>, Status> {
        let user = UserId::from_request(&request)?;
        let sessions = self
            .session
            .process(ListUserSessions {
                user_id: AccountId(user.0),
            })
            .await?;
        Ok(Response::new(pb::ListSessionsReply {
            sessions: sessions.into_iter().map(session_info).collect(),
        }))
    }

    async fn terminate_session(
        &self,
        request: Request<pb::TerminateSessionRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let _user = UserId::from_request(&request)?;
        let req = request.into_inner();
        self.session
            .process(TerminateSession {
                session_id: SessionId(req.session_id),
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn terminate_all_sessions(
        &self,
        request: Request<pb::TerminateAllSessionsRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let user = UserId::from_request(&request)?;
        self.session
            .process(TerminateAllSessions {
                user_id: AccountId(user.0),
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn change_password(
        &self,
        request: Request<pb::ChangePasswordRequest>,
    ) -> Result<Response<pb::ChangePasswordReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let result = self
            .account
            .process(ChangePassword {
                user_id: AccountId(user.0),
                old_password: req.old_password,
                new_password: req.new_password,
            })
            .await?;
        let code = match result {
            ChangePasswordResult::Success => pb::ChangePasswordResult::Success,
            ChangePasswordResult::WrongPassword => pb::ChangePasswordResult::WrongPassword,
        };
        Ok(Response::new(pb::ChangePasswordReply {
            result: code as i32,
        }))
    }
}
