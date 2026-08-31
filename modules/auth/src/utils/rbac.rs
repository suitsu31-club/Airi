//! Role-based access control for administrative operations.

use crate::entities::db::membership::AdminRole;

/// An administrative operation governed by RBAC.
///
/// The `AdminService` resolves the caller's [`AdminRole`], checks
/// [`AdminOperation::is_allowed`], performs the operation, and then records an
/// audit-log entry named [`AdminOperation::OPERATION_NAME`] with
/// [`AdminOperation::audit_content`].
pub trait AdminOperation {
    /// Roles permitted to perform this operation.
    const ALLOWED_ROLES: &'static [AdminRole];

    /// Stable operation name recorded in the audit log.
    const OPERATION_NAME: &'static str;

    /// The audited payload describing this operation instance.
    fn audit_content(&self) -> serde_json::Value;

    /// Whether the given role may perform this operation.
    fn is_allowed(role: AdminRole) -> bool {
        Self::ALLOWED_ROLES.contains(&role)
    }
}
