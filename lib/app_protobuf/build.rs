//! Compiles the workspace protobuf definitions with `tonic-prost-build`.
//!
//! Proto files live under the repository `proto/` directory. Every `.proto`
//! listed here is compiled with both the server and client generated, then
//! re-exported from `src/lib.rs` via `tonic::include_proto!`.

const PROTOS: &[&str] = &[
    "../../proto/shared/shared.proto",
    "../../proto/auth/auth.proto",
    "../../proto/auth/profile.proto",
    "../../proto/auth/mfa.proto",
    "../../proto/auth/invitation.proto",
    "../../proto/auth/admin.proto",
    "../../proto/auth/internal.proto",
    "../../proto/messaging/notification.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto");
    for proto in PROTOS {
        println!("cargo:rerun-if-changed={proto}");
    }

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(PROTOS, &["../../proto"])?;

    Ok(())
}
