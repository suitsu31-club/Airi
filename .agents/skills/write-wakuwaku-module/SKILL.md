---
name: write-wakuwaku-module
description: Write or change a Rust module under modules/ (entities, services, events, hooks, rpc, config), or add a new module; layer rules and conventions for wakuwaku code.
---

# Write a wakuwaku module

wakuwaku reference: <https://www.wakuwaku.rs/en>. For SQL text, migrations,
and SQL tests, also read the `write-and-test-sql` skill.

## Anatomy

Every module crate mirrors `modules/base`:

```
sql/              # SQL files for non-simple queries (see write-and-test-sql)
src/
├── lib.rs        # declares the modules below; sets crate-wide lints
├── error.rs      # (base) the shared `Error` services and hooks return
├── config.rs     # typed configuration (stored in DB, cached in Redis)
├── utils/        # small, dependency-light helpers
├── entities/
│   ├── db/       # PostgreSQL entities + compile-time-checked queries
│   └── redis/    # Redis collections (Cache, Lock, …) + queries (rkyv bodies)
├── services/     # business logic (stateful Processors)
├── events/       # Apache Iggy events: partition keys + event types
├── hooks/        # background reactors (Iggy event handlers, cron, loggers)
└── rpc/          # gRPC service implementations (transport edge)
```

| You are writing…                                   | Put it in…       |
| -------------------------------------------------- | ---------------- |
| A SQL query or a table row type                    | `entities/db`    |
| A Redis-cached value or ephemeral token            | `entities/redis` |
| A use case that combines queries and rules         | `services`       |
| A message other modules react to                   | `events`         |
| A reaction to an event / a cron job / an audit log | `hooks`          |
| A gRPC endpoint implementation                     | `rpc`            |
| A typed setting an operator can change             | `config`         |
| A pure helper with no runtime deps                 | `utils`          |

## Layer rules

### `entities/db`

- One submodule per table or aggregate.
- Declare the table as a `wakuwaku::entity::Entity` marker type and define a
  `sqlx::FromRow` struct for the row.
- One input struct per query/command, implementing `Query` (its effect:
  `Read<T>` / `Write<T>`, or a tuple of them) and `Execute<PgSource>` (the SQL,
  run on `&mut PgConnection`, failing with `sqlx::Error`).
- Annotate `execute` with `#[tracing::instrument(skip_all, err)]`.
- Statements that must commit together are a transaction flow
  (`wakuwaku::entity::sqlx::transaction`), not hand-written BEGIN/COMMIT.

### `entities/redis`

- One submodule per key kind.
- Prefer the typed collections in `wakuwaku::entity::redis::collection`
  (`Cache`, `HashedCache`, `Lock`, `RwLock`, `TokenBucket`), declared as
  `const` values with a namespace prefixed by the module name.
- Bodies derive the `rkyv` traits plus `RkyvMessageSer` / `RkyvMessageDe`; key
  types implement `wakuwaku::entity::redis::Name`.
- Anything the collections don't cover is a query: `Query` +
  `Execute<RedisSource<C>>`, failing with `redis::RedisError`.

### `services`

- A service is a `Clone` struct owning its dependencies (`Db` handles for
  PostgreSQL/Redis, the Iggy `Publisher`, loaded config, other services).
- Take the narrowest capability: `Db<_, ReadOnly>` when the service only reads.
- One `Processor` impl per operation, failing with `base::Error`; return
  domain types, not protobuf types.
- Encode checked preconditions as `wakuwaku::proof::Proven<T, P>` inputs
  instead of re-validating in every operation.
- Load config with the config-cache helpers, not by re-reading the database on
  every call.
- Services orchestrate entities and publish events. **No transport types.**

### `events`

- Implement `wakuwaku::iggy::PartitionKey` on the key type: the thing whose
  events must stay in order. Its `SUPER_PARTITION_NAME` is the Iggy topic,
  prefixed with the module name (e.g. `base_example`).
- Implement `Event` on the payload: a `TYPE_TAG` unique within the topic
  (never renumbered or reused), `key()`, `atomic_ordering()`, and an `Algebra`
  (`NonAssociative` when in doubt). Derive the `rkyv` traits plus
  `RkyvMessageSer` / `RkyvMessageDe`, and `impl EventBody`.
- Document each event's contract in a doc comment: **who publishes, who
  consumes, its topic and type tag.**
- wakuwaku does not create the stream or main topics; `manage-tool` provisions
  them (wakuwaku creates only `retry_<topic>`).

### `hooks`

- Iggy event handlers implement `EventHandler<E>` (with `MAX_BATCH`). Delivery
  is at least once: handlers must be idempotent.
- One consumer (one `consumer_name`) per topic: register every handler of a
  topic in the same `iggy_consumer` chain.
- Cron jobs implement `Processor<CronTick>` + `CronJob`. Every replica runs
  them, so make them idempotent or take a Redis `Lock`. Read the time from
  `tick.fired_at`.
- Also the home for event loggers.
- Implement `wakuwaku::ServiceCreation` so `app-server` can build the hook from
  its `ServiceBuilder`. Hooks own their dependencies and carry no transport
  logic.

### `rpc`

- Implement the protobuf service trait from `app_protobuf`.
- Handlers are thin adapters: decode request → call a service → encode reply.
  If a handler grows rules, move them into a service.
- Re-export each concrete service type from `rpc/mod.rs` so `app-server` can
  mount it.

### `config`

- A `serde`-(de)serializable struct implementing `Default`, bound to a stable
  string key.
- Stored as JSON in the database, cached in Redis, seeded by `manage-tool`.

## Conventions

- **Errors:** services and hooks return `base::Error`
  (`modules/base/src/error.rs`); it converts into `tonic::Status` for `rpc`
  and into the Iggy `HandleError` (choosing its retry class) for hooks.
  Queries keep their native error (`sqlx::Error`, `RedisError`) and `?` lifts
  it. Add a variant to `base::Error` only when every module needs it;
  otherwise define the module's own `thiserror` enum the same way.
- **Tracing:** instrument entities, services, and RPC handlers with
  `#[tracing::instrument(skip_all, err)]`.
- **Third-party crates:** reach crates in wakuwaku's API through
  `wakuwaku::deps` (`iggy` only that way). `kanau`, `sqlx`, `redis`, and
  `tonic` are also direct workspace dependencies because derive macros and
  generated code name them by path and we enable extra features; keep their
  versions semver-compatible with wakuwaku's so one copy is resolved.

## Adding a new module

1. Copy the `modules/base` layout into `modules/<name>`.
2. Add the crate to the workspace `members` in the root `Cargo.toml`.
3. Define tables in `migrations/` and the API in `proto/` (register it in
   `app_protobuf`).
4. Implement from the inside out: `entities` → `services` → `rpc`/`hooks`.
5. Wire the new services/hooks into `bin/app-server`'s workers.
6. Keep `config` values seedable from `bin/manage-tool`.
