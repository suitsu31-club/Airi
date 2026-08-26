//! PostgreSQL entities and queries.
//!
//! Put your database row structs and query/command processors here — one
//! submodule per table or aggregate (e.g. `pub mod user_account;`).
//!
//! The convention: define a `sqlx::FromRow` struct for the row, then model each
//! read or write as an input struct with a `Processor` implementation on the
//! shared `DatabaseProcessor` from `wakuwaku::sqlx`. Prefer the
//! `sqlx::query!`/`query_as!` macros so queries are checked against the database
//! at compile time.
//!
//! ```ignore
//! use kanau::processor::Processor;
//! use wakuwaku::sqlx::DatabaseProcessor;
//!
//! #[derive(Debug, Clone, sqlx::FromRow)]
//! pub struct Example {
//!     pub id: uuid::Uuid,
//!     pub name: String,
//! }
//!
//! pub struct FindExampleById {
//!     pub id: uuid::Uuid,
//! }
//!
//! impl Processor<FindExampleById> for DatabaseProcessor {
//!     type Output = Option<Example>;
//!     type Error = sqlx::Error;
//!     async fn process(&self, input: FindExampleById) -> Result<Self::Output, Self::Error> {
//!         sqlx::query_as!(
//!             Example,
//!             r#"SELECT id, name FROM "base"."example" WHERE id = $1"#,
//!             input.id
//!         )
//!         .fetch_optional(self.db())
//!         .await
//!     }
//! }
//! ```
