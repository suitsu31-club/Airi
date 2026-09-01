//! gRPC adapter for `airi.auth.Mfa`.

use crate::entities::db::account::AccountId;
use crate::rpc::middleware::UserId;
use crate::services::mfa::{
    DisableMfa, DisableMfaResult, FinishTotpEnrollment, FinishTotpEnrollmentResult, GetMfaStatus,
    MfaService, StartTotpEnrollment, VerifyMfaLogin, VerifyMfaLoginResult,
};
use app_protobuf::auth as pb;
use app_protobuf::auth::mfa_server::Mfa;
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the public `Mfa` service.
#[derive(Clone)]
pub struct MfaRpc {
    pub mfa: MfaService,
}

#[tonic::async_trait]
impl Mfa for MfaRpc {
    async fn start_totp_enrollment(
        &self,
        request: Request<pb::StartTotpEnrollmentRequest>,
    ) -> Result<Response<pb::StartTotpEnrollmentReply>, Status> {
        let user_id = UserId::from_request(&request)?;
        let start = self
            .mfa
            .process(StartTotpEnrollment {
                user_id: AccountId(user_id.0),
            })
            .await?;
        Ok(Response::new(pb::StartTotpEnrollmentReply {
            secret_base32: start.secret_base32,
            otpauth_uri: start.otpauth_uri,
            qr_png_base64: start.qr_png_base64,
        }))
    }

    async fn finish_totp_enrollment(
        &self,
        request: Request<pb::FinishTotpEnrollmentRequest>,
    ) -> Result<Response<pb::FinishTotpEnrollmentReply>, Status> {
        let user_id = UserId::from_request(&request)?;
        let req = request.into_inner();
        let result = self
            .mfa
            .process(FinishTotpEnrollment {
                user_id: AccountId(user_id.0),
                code: req.code,
            })
            .await?;
        let (code, recovery_codes) = match result {
            FinishTotpEnrollmentResult::Success(codes) => {
                (pb::FinishTotpEnrollmentResult::Success, codes)
            }
            FinishTotpEnrollmentResult::InvalidCode => {
                (pb::FinishTotpEnrollmentResult::InvalidCode, Vec::new())
            }
            FinishTotpEnrollmentResult::NoPending => {
                (pb::FinishTotpEnrollmentResult::NoPending, Vec::new())
            }
            FinishTotpEnrollmentResult::AlreadyEnabled => {
                (pb::FinishTotpEnrollmentResult::AlreadyEnabled, Vec::new())
            }
        };
        Ok(Response::new(pb::FinishTotpEnrollmentReply {
            result: code as i32,
            recovery_codes,
        }))
    }

    async fn disable_mfa(
        &self,
        request: Request<pb::DisableMfaRequest>,
    ) -> Result<Response<pb::DisableMfaReply>, Status> {
        let user_id = UserId::from_request(&request)?;
        let req = request.into_inner();
        let result = self
            .mfa
            .process(DisableMfa {
                user_id: AccountId(user_id.0),
                code: req.code,
            })
            .await?;
        let code = match result {
            DisableMfaResult::Success => pb::DisableMfaResult::Success,
            DisableMfaResult::InvalidCode => pb::DisableMfaResult::InvalidCode,
            DisableMfaResult::NotEnabled => pb::DisableMfaResult::NotEnabled,
        };
        Ok(Response::new(pb::DisableMfaReply {
            result: code as i32,
        }))
    }

    async fn get_mfa_status(
        &self,
        request: Request<pb::GetMfaStatusRequest>,
    ) -> Result<Response<pb::GetMfaStatusReply>, Status> {
        let user_id = UserId::from_request(&request)?;
        let status = self
            .mfa
            .process(GetMfaStatus {
                user_id: AccountId(user_id.0),
            })
            .await?;
        Ok(Response::new(pb::GetMfaStatusReply {
            totp_enabled: status.totp_enabled,
            remaining_recovery_codes: status.remaining_recovery_codes,
        }))
    }

    async fn verify_mfa_login(
        &self,
        request: Request<pb::VerifyMfaLoginRequest>,
    ) -> Result<Response<pb::VerifyMfaLoginReply>, Status> {
        // No session auth: the caller is mid-login and holds only the MFA token.
        let req = request.into_inner();
        let mfa_token: [u8; 32] = req
            .mfa_token
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid mfa token length"))?;
        let result = self
            .mfa
            .process(VerifyMfaLogin {
                mfa_token,
                code: req.code,
            })
            .await?;
        let (code, session_id) = match result {
            VerifyMfaLoginResult::Success(sid) => {
                (pb::VerifyMfaLoginResult::Success, Some(sid.0))
            }
            VerifyMfaLoginResult::InvalidToken => (pb::VerifyMfaLoginResult::InvalidToken, None),
            VerifyMfaLoginResult::InvalidCode => (pb::VerifyMfaLoginResult::InvalidCode, None),
        };
        Ok(Response::new(pb::VerifyMfaLoginReply {
            result: code as i32,
            session_id,
        }))
    }
}
