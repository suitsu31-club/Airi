//! OpenAPI (axum) surface: the API-key-authenticated `/api/me` endpoint and the
//! generated `/api/openapi.json` document describing it.

pub mod middleware;

use crate::openapi::middleware::{ApiKeyIdentity, ApiKeySecurityAddon};
use crate::services::profile::{
    GetMyCredit, GetMyInvitationGrouping, GetMyProfile, ProfileService,
};
use crate::utils::datetime::to_unix;
use crate::utils::identity::IdentityVerifier;
use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use kanau::processor::Processor;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

/// Shared state for the `/api/me` router.
///
/// [`FromRef`] lets the [`ApiKeyIdentity`] extractor pull the
/// [`IdentityVerifier`] out of the state automatically.
#[derive(Clone, FromRef)]
pub struct MeState {
    pub verifier: IdentityVerifier,
    pub profile: ProfileService,
}

/// The caller's account profile.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileView {
    /// Account id (UUID).
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    /// Membership level (0 when the caller has no membership row).
    pub level: i32,
    /// Administrative role, if any (snake-case).
    pub admin_role: Option<String>,
    /// Registration time, unix seconds.
    pub registered_at: u64,
}

/// The caller's credit balance. Amounts are decimal strings to avoid
/// floating-point loss.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreditView {
    /// Total balance.
    pub total: String,
    /// Portion currently held/frozen.
    pub frozen: String,
    /// Spendable balance (`total - frozen`).
    pub available: String,
}

/// The caller's invitations counted by lifecycle status.
#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationByStatus {
    pub accepted: u32,
    pub expired: u32,
    pub invalid: u32,
    pub pending: u32,
    pub free: u32,
}

/// The caller's invitation availability and status breakdown.
#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationView {
    /// Number of invitations the caller may still send.
    pub available: i32,
    /// Owned invitations grouped by status.
    pub by_status: InvitationByStatus,
}

/// The `/api/me` response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub profile: ProfileView,
    pub credit: CreditView,
    pub invitation: InvitationView,
}

/// OpenAPI document for the public API surface.
#[derive(OpenApi)]
#[openapi(
    paths(me_handler),
    components(schemas(
        MeResponse,
        ProfileView,
        CreditView,
        InvitationView,
        InvitationByStatus
    )),
    modifiers(&ApiKeySecurityAddon),
    tags((name = "profile", description = "Self-service profile endpoints"))
)]
pub struct ApiDoc;

/// Build the `/api/me` router plus the `/api/openapi.json` document endpoint.
pub fn me_router(state: MeState) -> Router {
    Router::new()
        .route("/api/me", get(me_handler))
        .route("/api/openapi.json", get(openapi_json))
        .with_state(state)
}

fn internal_error(_e: wakuwaku::Error) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

/// Serve the generated OpenAPI document.
async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Return the authenticated caller's profile, credit balance, and invitation
/// grouping.
#[utoipa::path(
    get,
    path = "/api/me",
    tag = "profile",
    responses(
        (status = 200, description = "Profile, credit balance, and invitation grouping", body = MeResponse),
        (status = 401, description = "Missing or invalid API key"),
        (status = 500, description = "Internal error")
    ),
    security(("api_key" = []))
)]
async fn me_handler(
    identity: ApiKeyIdentity,
    State(state): State<MeState>,
) -> Result<Json<MeResponse>, StatusCode> {
    let user_id = identity.user;

    let profile = state
        .profile
        .process(GetMyProfile { user_id })
        .await
        .map_err(internal_error)?;
    let credit = state
        .profile
        .process(GetMyCredit { user_id })
        .await
        .map_err(internal_error)?;
    let invitation = state
        .profile
        .process(GetMyInvitationGrouping { user_id })
        .await
        .map_err(internal_error)?;

    let available = credit.total_amount - credit.frozen_amount;

    let response = MeResponse {
        profile: ProfileView {
            user_id: profile.account.id.0.to_string(),
            username: profile.account.username,
            email: profile.account.email,
            avatar_url: profile.account.avatar_url,
            level: profile.membership.as_ref().map_or(0, |m| m.level),
            admin_role: profile
                .membership
                .as_ref()
                .and_then(|m| m.admin_privilege)
                .map(|r| r.as_str().to_string()),
            registered_at: to_unix(profile.account.registered_at),
        },
        credit: CreditView {
            total: credit.total_amount.to_string(),
            frozen: credit.frozen_amount.to_string(),
            available: available.to_string(),
        },
        invitation: InvitationView {
            available: invitation.available_count,
            by_status: InvitationByStatus {
                accepted: invitation.accepted,
                expired: invitation.expired,
                invalid: invitation.invalid,
                pending: invitation.pending,
                free: invitation.free,
            },
        },
    };

    Ok(Json(response))
}
