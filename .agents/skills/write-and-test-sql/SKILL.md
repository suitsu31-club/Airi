---
name: write-and-test-sql
description: Write, place, or test SQL in this workspace (sqlx queries, SQL files, migrations); defines "simple SQL", which stays inline and untested.
---

# Write and test SQL

## Simple SQL

A query is **simple** when it meets **all** of these conditions:

- It only does CRUD: no materialized-view refresh, trigger, or other side
  effect.
- It has no join, subquery, type cast, or `unnest`.
- Its filter has at most 3 conditions joined by `AND` or `OR`.
- Its filter does not mix `AND` and `OR`.
- Excluding the `SELECT` column list and the `RETURNING` clause, it is shorter
  than 3 lines.

Anything else is **non-simple**.

## Where SQL goes

- **Simple SQL is always inline**, via `sqlx::query!` / `sqlx::query_as!` in
  the query's `Execute` impl under `entities/db`.
- **Non-simple SQL lives in a `.sql` file** in the module's `sql/` directory
  (`modules/<name>/sql/`), one statement per file, named after its query
  struct, and is loaded with `sqlx::query_file!` / `sqlx::query_file_as!`
  (paths are relative to the crate root, e.g. `"sql/list_pending_mail.sql"`).
- Both forms are checked against the database at compile time. Use
  schema-qualified table names.
- Statements that must commit together are a transaction flow
  (`wakuwaku::entity::sqlx::transaction`), not hand-written BEGIN/COMMIT.

## Migrations

- Every schema change, for every module, is a `.up.sql` / `.down.sql` pair in
  the root `migrations/` directory (no per-module migration directories).
- Files are versioned by logical order with a zero-padded number:
  `0001_<name>.up.sql`, `0002_<name>.up.sql`, …
- An up migration is lossless: it keeps existing data, unless its purpose is
  dropping old data. A down migration may lose data.

## Testing SQL

- Don't test simple SQL; compile-time checking plus reading it is enough.
- Test non-simple SQL against the database, targeting what makes it
  non-simple: join cardinality, `AND`/`OR` precedence, casts, side effects.
- Logic around SQL that can be pulled out as pure Rust (key derivation, row
  mapping with rules) is tested as a unit test instead.
