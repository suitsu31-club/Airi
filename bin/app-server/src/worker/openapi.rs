//! OpenAPI worker: serves the axum `/api/me` endpoint.

use super::Deps;
use auth::openapi::{MeState, me_router};
use auth::services::profile::ProfileService;
use auth::utils::identity::IdentityVerifier;
use std::net::SocketAddr;

pub async fn run(deps: Deps, addr: SocketAddr) -> anyhow::Result<()> {
    let Deps { db, redis, mq: _ } = deps;
    let verifier = IdentityVerifier {
        db: db.clone(),
        redis,
    };
    let profile = ProfileService { db };
    let state = MeState { verifier, profile };

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "openapi listening");
    axum::serve(listener, me_router(state)).await?;
    Ok(())
}
