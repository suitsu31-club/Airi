//! The error type shared by the workspace's services and hooks.
//!
//! wakuwaku 0.3 shipped one `wakuwaku::Error` for everything. 1.0 removed it:
//! each wakuwaku layer now fails with the error of what it wraps
//! (`sqlx::Error` for PostgreSQL queries, `redis::RedisError` for Redis,
//! [`wakuwaku::iggy::error::Error`] for publishing, `HandleError` for event
//! handlers). [`Error`] is this workspace's replacement: services return it,
//! and it converts at the two edges:
//!
//! - into [`tonic::Status`] in [`rpc`](crate::rpc) handlers (`?` just works);
//! - into [`HandleError`] in [`hooks`](crate::hooks), choosing the
//!   [`ErrorClass`](wakuwaku::iggy::error::ErrorClass) that decides how the
//!   Iggy consumer retries.
//!
//! Queries in [`entities`](crate::entities) keep their native error
//! (`sqlx::Error`, `RedisError`); `?` lifts them into [`Error`] at the
//! service boundary. Feature modules depend on `base` and reuse this type;
//! add a variant here only when it is meaningful to every module, otherwise
//! define the feature module's own `thiserror` enum the same way.

use wakuwaku::iggy::error::HandleError;

/// Unified error of services and hooks.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A message body failed to serialize (kanau).
    #[error("{0}")]
    Serialize(#[from] kanau::message::SerializeError),

    /// A message body failed to deserialize (kanau).
    #[error("{0}")]
    Deserialize(#[from] kanau::message::DeserializeError),

    /// PostgreSQL failed.
    #[error("{0}")]
    Database(#[from] sqlx::Error),

    /// Redis failed.
    #[error("{0}")]
    Redis(#[from] redis::RedisError),

    /// Publishing an event to Iggy failed.
    #[error("{0}")]
    Event(#[from] wakuwaku::iggy::error::Error),

    /// A business rule failed. Retrying cannot fix it.
    #[error("{0}")]
    BusinessPanic(anyhow::Error),

    /// An I/O failure inside business logic. Retrying may fix it.
    #[error("{0}")]
    Io(anyhow::Error),

    /// The caller lacks the permission the operation requires.
    #[error("Permission is not enough")]
    PermissionsDenied,

    /// The input payload or arguments are invalid.
    #[error("Invalid input")]
    InvalidInput,

    /// The requested resource does not exist.
    #[error("Trying to access a resource that does not exist")]
    NotFound,
}

/// Result alias for [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<&Error> for tonic::Status {
    fn from(value: &Error) -> Self {
        match value {
            Error::Serialize(_) | Error::Deserialize(_) => {
                tonic::Status::invalid_argument(value.to_string())
            }
            Error::Database(_) => tonic::Status::internal("Database error"),
            Error::Redis(_) | Error::Event(_) | Error::BusinessPanic(_) | Error::Io(_) => {
                tonic::Status::internal("Internal server error")
            }
            Error::PermissionsDenied => tonic::Status::permission_denied("Permission denied"),
            Error::InvalidInput => tonic::Status::invalid_argument("Invalid input"),
            Error::NotFound => tonic::Status::not_found("Not found"),
        }
    }
}

impl From<Error> for tonic::Status {
    fn from(value: Error) -> Self {
        (&value).into()
    }
}

/// Infrastructure and I/O failures are `Retryable`: the key moves to the retry
/// topic while other keys continue. Everything else can never succeed on
/// retry, so it is `Unrecoverable` and the chunk is dropped and logged. A hook
/// that knows a whole dependency is down (and should stall the partition
/// instead) returns `HandleError::transient` explicitly.
impl From<Error> for HandleError {
    fn from(value: Error) -> Self {
        match value {
            Error::Database(_) | Error::Redis(_) | Error::Event(_) | Error::Io(_) => {
                HandleError::retryable(value)
            }
            Error::Serialize(_)
            | Error::Deserialize(_)
            | Error::BusinessPanic(_)
            | Error::PermissionsDenied
            | Error::InvalidInput
            | Error::NotFound => HandleError::unrecoverable(value),
        }
    }
}
