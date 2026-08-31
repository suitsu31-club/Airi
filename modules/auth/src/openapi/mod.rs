//! OpenAPI (axum) surface: the API-key-authenticated `/api/me` endpoint.

pub mod middleware;

use crate::entities::db::account::AccountId;
use crate::entities::db::api_key::ApiKey;
use crate::openapi::middleware::API_KEY_HEADER;
use crate::services::profile::{GetMyCredit, GetMyInvitationSummary, GetMyProfile, ProfileService};
use crate::utils::datetime::to_unix;
use crate::utils::identity::{ApiKeyVerify, IdentityVerifier};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use kanau::processor::Processor;
use serde_json::json;

/// Shared state for the `/api/me` router.
#[derive(Clone)]
pub struct MeState {
    pub verifier: IdentityVerifier,
    pub profile: ProfileService,
}

/// Build the `/api/me` router.
pub fn me_router(state: MeState) -> Router {
    Router::new()
        .route("/api/me", get(me_handler))
        .with_state(state)
}

fn internal_error(_e: wakuwaku::Error) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn me_handler(
    State(state): State<MeState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = headers
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let verified = state
        .verifier
        .process(ApiKeyVerify {
            api_key: ApiKey(api_key.to_string()),
        })
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let user_id: AccountId = verified.user;

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
        .process(GetMyInvitationSummary { user_id })
        .await
        .map_err(internal_error)?;
    let available = credit.total_amount - credit.frozen_amount;

    let body = json!({
        "profile": {
            "user_id": profile.account.id.0.to_string(),
            "username": profile.account.username,
            "email": profile.account.email,
            "avatar_url": profile.account.avatar_url,
            "level": profile.membership.as_ref().map_or(0, |m| m.level),
            "admin_role": profile
                .membership
                .as_ref()
                .and_then(|m| m.admin_privilege)
                .map(|r| r.as_str()),
            "registered_at": to_unix(profile.account.registered_at),
        },
        "credit": {
            "total": credit.total_amount.to_string(),
            "frozen": credit.frozen_amount.to_string(),
            "available": available.to_string(),
        },
        "invitation": {
            "available": invitation.available_count,
            "sent": invitation.sent_count,
        },
    });
    Ok(Json(body))
}
