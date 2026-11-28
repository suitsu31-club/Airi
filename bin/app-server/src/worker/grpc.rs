//! The gRPC worker: serves each module's `rpc` servers and
//! `grpc.health.v1.Health` on one listener.

use super::Services;
use crate::health_check::HealthCheck;
use anyhow::Context;
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::service::Routes;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;

/// Bind `listen` and serve on it until `shutdown` is cancelled; then the
/// server stops accepting and finishes the requests in flight.
pub fn spawn(
    services: &Services,
    listen: SocketAddr,
    health: &HealthCheck,
    shutdown: CancellationToken,
) -> anyhow::Result<JoinHandle<Result<(), tonic::transport::Error>>> {
    let incoming = TcpIncoming::bind(listen)
        .with_context(|| format!("binding the gRPC listener to {listen} (GRPC_LISTEN)"))?;

    let (_, register) = services
        .clone()
        .compose_grpc_service(|_| health.grpc_service())
        // One call per module server:
        // .grpc_service::<ExampleServer<_>, ExampleServiceImpl, _>(ExampleServer::new)
        .into_router_fn();
    let router = register(Server::builder().add_routes(Routes::default()));

    tracing::info!(%listen, "gRPC worker listening");
    Ok(tokio::spawn(router.serve_with_incoming_shutdown(
        incoming,
        shutdown.cancelled_owned(),
    )))
}
