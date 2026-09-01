//! API-key authentication for the OpenAPI (axum) surface.
//!
//! [`ApiKeyIdentity`] is an axum extractor: it reads the [`API_KEY_HEADER`],
//! verifies it against the database (via [`IdentityVerifier`]), and yields the
//! authenticated caller. Any handler that takes an `ApiKeyIdentity` argument is
//! therefore guarded — a missing or invalid key rejects the request with
//! `401 Unauthorized` before the handler body runs.
//!
//! [`ApiKeySecurityAddon`] mirrors that scheme into the generated OpenAPI
//! document so the `/api/openapi.json` spec advertises the header requirement.

use crate::entities::db::account::AccountId;
use crate::entities::db::api_key::ApiKey;
use crate::utils::identity::{ApiKeyVerify, IdentityVerifier};
use axum::extract::{FromRef, FromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use utoipa::Modify;
use utoipa::openapi::security::{ApiKey as OpenApiApiKey, ApiKeyValue, SecurityScheme};

/// Header carrying the opaque API key.
pub const API_KEY_HEADER: &str = "X-API-Key";

/// Name of the API-key security scheme in the generated OpenAPI document.
pub const API_KEY_SECURITY_SCHEME: &str = "api_key";

/// A caller authenticated by an API key.
///
/// Use it as a handler argument to require authentication:
///
/// ```ignore
/// async fn handler(identity: ApiKeyIdentity) { /* identity.user is verified */ }
/// ```
#[derive(Debug, Clone)]
pub struct ApiKeyIdentity {
    /// The account the key belongs to.
    pub user: AccountId,
    /// Expiry of the key, if it is time-limited.
    pub valid_until: Option<PrimitiveDateTime>,
    /// Scopes granted to the key.
    pub scopes: Vec<String>,
}

impl<S> FromRequestParts<S> for ApiKeyIdentity
where
    IdentityVerifier: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    #[tracing::instrument(skip_all, name = "ApiKeyIdentity", err)]
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let api_key = parts
            .headers
            .get(API_KEY_HEADER)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let verifier = IdentityVerifier::from_ref(state);
        let verified = verifier
            .process(ApiKeyVerify {
                api_key: ApiKey(api_key.to_string()),
            })
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        Ok(ApiKeyIdentity {
            user: verified.user,
            valid_until: verified.valid_until,
            scopes: verified.scopes,
        })
    }
}

/// utoipa [`Modify`] that registers the API-key security scheme, so the
/// generated document declares the `X-API-Key` header requirement.
pub struct ApiKeySecurityAddon;

impl Modify for ApiKeySecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            API_KEY_SECURITY_SCHEME,
            SecurityScheme::ApiKey(OpenApiApiKey::Header(ApiKeyValue::new(API_KEY_HEADER))),
        );
    }
}
