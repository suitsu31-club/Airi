//! # `app_protobuf`
//!
//! Generated gRPC/protobuf types shared across the workspace.
//!
//! Protobuf definitions live in the workspace `proto/` directory. Add a
//! `build.rs` to this crate that compiles them with `tonic-prost-build` (already
//! declared as a build dependency in `Cargo.toml`), then re-export each
//! generated package as a module via `tonic::include_proto!`. Both the
//! `app-server` binary and the business modules under `modules/` depend on this
//! crate for their request/reply types and service traits.
//!
//! ## Adding a service
//!
//! 1. Put your `.proto` files under `proto/` (for example
//!    `proto/base/base.proto`) with a package name such as `app.base`.
//! 2. List them in `build.rs` and compile them with `tonic-prost-build`,
//!    building both the server and client.
//! 3. Re-export the generated package here:
//!
//! ```ignore
//! pub mod base {
//!     tonic::include_proto!("app.base");
//! }
//! ```
//!
//! Keep conversions between protobuf types and domain types (`sqlx`, `time`,
//! `uuid`, …) in this crate too, so every consumer shares one implementation.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

#[allow(clippy::enum_variant_names)]
#[allow(clippy::doc_overindented_list_items)]
pub mod shared {
    tonic::include_proto!("airi.shared");
}

#[allow(clippy::enum_variant_names)]
#[allow(clippy::doc_overindented_list_items)]
pub mod auth {
    tonic::include_proto!("airi.auth");
}

#[allow(clippy::enum_variant_names)]
#[allow(clippy::doc_overindented_list_items)]
pub mod admin {
    tonic::include_proto!("airi.admin");
}

#[allow(clippy::enum_variant_names)]
#[allow(clippy::doc_overindented_list_items)]
pub mod internal {
    tonic::include_proto!("airi.internal");
}

#[allow(clippy::enum_variant_names)]
#[allow(clippy::doc_overindented_list_items)]
pub mod messaging {
    tonic::include_proto!("airi.messaging");
}

impl From<()> for shared::Empty {
    fn from((): ()) -> Self {
        shared::Empty {}
    }
}

impl From<shared::Empty> for () {
    fn from(_: shared::Empty) -> Self {}
}
