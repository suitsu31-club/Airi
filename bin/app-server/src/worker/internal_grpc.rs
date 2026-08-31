//! Internal gRPC worker: the inter-service `Identity` service, no auth layer.
//! Deploy on an internal-only listener.

use super::Deps;
use app_protobuf::internal::identity_server::IdentityServer;
use auth::rpc::IdentityRpc;
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
    let identity = IdentityRpc { verifier, profile };

    tracing::info!(%addr, "internal gRPC listening");
    tonic::transport::Server::builder()
        .add_service(IdentityServer::new(identity))
        .serve(addr)
        .await?;
    Ok(())
}
