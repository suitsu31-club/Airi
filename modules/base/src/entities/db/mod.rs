//! PostgreSQL entities and queries.
//!
//! Put your database row structs and queries here — one submodule per table or
//! aggregate (e.g. `pub mod user_account;`).
//!
//! The convention (`wakuwaku::entity`):
//!
//! - declare the table as an [`Entity`](wakuwaku::entity::Entity) marker type;
//! - define a `sqlx::FromRow` struct for the row;
//! - model each read or write as an input struct implementing
//!   [`Query`](wakuwaku::entity::Query) (its effect: `Read<T>` / `Write<T>`)
//!   and [`Execute<PgSource>`](wakuwaku::entity::Execute) (how it runs on a
//!   PostgreSQL connection, failing with `sqlx::Error`).
//!
//! A query is run by a capability-typed handle,
//! `Db<PgSource, ReadOnly | ReadWrite>`, which implements `Processor<Q>` for
//! every query its capability covers: `db.process(FindExampleById { id })`.
//! Sending a write query to a `ReadOnly` handle does not compile. Prefer the
//! `sqlx::query!`/`query_as!` macros so queries are checked against the
//! database at compile time.
//!
//! ```ignore
//! use sqlx::PgConnection;
//! use wakuwaku::entity::sqlx::backend::PgSource;
//! use wakuwaku::entity::{Entity, Execute, Query, Read};
//!
//! /// The `base.example` table.
//! pub struct Example;
//! impl Entity for Example {}
//!
//! #[derive(Debug, Clone, sqlx::FromRow)]
//! pub struct ExampleRow {
//!     pub id: uuid::Uuid,
//!     pub name: String,
//! }
//!
//! pub struct FindExampleById {
//!     pub id: uuid::Uuid,
//! }
//!
//! impl Query for FindExampleById {
//!     type Effect = Read<Example>;
//! }
//!
//! impl Execute<PgSource> for FindExampleById {
//!     type Output = Option<ExampleRow>;
//!     type Error = sqlx::Error;
//!
//!     #[tracing::instrument(skip_all, err)]
//!     async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
//!         sqlx::query_as!(
//!             ExampleRow,
//!             r#"SELECT id, name FROM "base"."example" WHERE id = $1"#,
//!             self.id
//!         )
//!         .fetch_optional(conn)
//!         .await
//!     }
//! }
//! ```
