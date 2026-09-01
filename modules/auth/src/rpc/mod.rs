//! Transport edge: gRPC service implementations.
//!
//! Each handler is a thin adapter that decodes the request, extracts identity
//! (via [`middleware`]), calls into [`crate::services`], and encodes the reply.
//! `wakuwaku::Error` converts to `tonic::Status` via `?`.

pub mod admin;
pub mod internal;
pub mod invitation;
pub mod mfa;
pub mod middleware;
pub mod profile;
pub mod user_auth;

pub use admin::AdminRpc;
pub use internal::IdentityRpc;
pub use invitation::InvitationRpc;
pub use mfa::MfaRpc;
pub use profile::UserProfileRpc;
pub use user_auth::UserAuthRpc;

/// Parse a UUID string, mapping failures to `invalid_argument`.
pub(crate) fn parse_uuid(s: &str) -> Result<uuid::Uuid, tonic::Status> {
    uuid::Uuid::parse_str(s).map_err(|_| tonic::Status::invalid_argument("invalid uuid"))
}
