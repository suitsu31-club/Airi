//! Session-auth tower layer and request identity extractors.
//!
//! The [`UserAuthLayer`] is non-rejecting: when a valid `X-Session-Id` header is
//! present it injects [`UserId`] and [`CurrentSessionId`] into the request
//! extensions; absence or failure simply skips injection. Individual RPC
//! handlers enforce authentication via [`UserId::from_request`].

use crate::entities::db::sessions::SessionId;
use crate::utils::identity::{IdentityVerifier, SessionIdVerify};
use futures::future::BoxFuture;
use kanau::processor::Processor;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use uuid::Uuid;

/// Header carrying the opaque session id.
pub const SESSION_ID_HEADER: &str = "x-session-id";

/// Authenticated user id, injected into request extensions.
#[derive(Clone, Copy, Debug)]
pub struct UserId(pub Uuid);

impl UserId {
    /// Extract the authenticated user id, or reject as unauthenticated.
    pub fn from_request<T>(req: &tonic::Request<T>) -> Result<Self, tonic::Status> {
        req.extensions()
            .get::<UserId>()
            .copied()
            .ok_or_else(|| tonic::Status::unauthenticated("authentication required"))
    }
}

/// The current session id, injected into request extensions.
#[derive(Clone, Debug)]
pub struct CurrentSessionId(pub String);

impl CurrentSessionId {
    /// Extract the current session id, or reject as unauthenticated.
    pub fn from_request<T>(req: &tonic::Request<T>) -> Result<Self, tonic::Status> {
        req.extensions()
            .get::<CurrentSessionId>()
            .cloned()
            .ok_or_else(|| tonic::Status::unauthenticated("authentication required"))
    }
}

/// The client IP of an incoming gRPC request.
pub fn request_ip<T>(req: &tonic::Request<T>) -> String {
    req.remote_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_default()
}

/// The user agent of an incoming gRPC request.
pub fn request_user_agent<T>(req: &tonic::Request<T>) -> String {
    req.metadata()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// Non-rejecting session-auth layer.
#[derive(Clone)]
pub struct UserAuthLayer {
    pub verifier: IdentityVerifier,
}

impl<S> Layer<S> for UserAuthLayer {
    type Service = UserAuthMiddleware<S>;
    fn layer(&self, inner: S) -> Self::Service {
        UserAuthMiddleware {
            inner,
            verifier: self.verifier.clone(),
        }
    }
}

/// Service produced by [`UserAuthLayer`].
#[derive(Clone)]
pub struct UserAuthMiddleware<S> {
    inner: S,
    verifier: IdentityVerifier,
}

fn forwarded_ip<B>(req: &http::Request<B>) -> String {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

impl<S, B> Service<http::Request<B>> for UserAuthMiddleware<S>
where
    S: Service<http::Request<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
        let verifier = self.verifier.clone();
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let session_id = req
                .headers()
                .get(SESSION_ID_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            if let Some(session_id) = session_id {
                let ip = forwarded_ip(&req);
                let user_agent = req
                    .headers()
                    .get(http::header::USER_AGENT)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                if let Ok(verified) = verifier
                    .process(SessionIdVerify {
                        session_id: SessionId(session_id.clone()),
                        ip,
                        user_agent,
                    })
                    .await
                {
                    req.extensions_mut().insert(UserId(verified.user.0));
                    req.extensions_mut().insert(CurrentSessionId(session_id));
                }
            }
            inner.call(req).await
        })
    }
}
