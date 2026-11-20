# AGENTS.md

How to organise code in this workspace. Read this before adding anything. The
rules exist so that every module looks the same and both humans and agents can
navigate the codebase mechanically.

## The golden rule

**Everything is a `Processor`.** `kanau::processor::Processor` is *state plus an
async function*:

```text
Processor = State + async fn(Input) -> Result<Output, Error>
```

A `Processor` is a struct that owns its dependencies and implements
`Processor<Input>` once per operation. Model each business operation and each
cron job as an input plus a `Processor` impl. wakuwaku 1.0 builds its own
layers on the same idea — use them, do not invent parallel abstractions:

- a **query** is an input struct implementing `wakuwaku::entity::Query` +
  `Execute<Source>`; a capability-typed `Db<Source, Capability>` handle runs it
  as a `Processor` (`db.process(query)`);
- an **event handler** implements `wakuwaku::iggy::EventHandler<E>`, which
  receives a run of events instead of one input;
- a **cron job** is a `Processor<CronTick>` plus `wakuwaku::CronJob`.

## Workspace layout

```
bin/          # Rust binaries — wiring only, no business logic
  app-server/     # runs modules behind pluggable workers (gRPC, consumer, cron, ...)
  manage-tool/    # CLI: migrations, config seeding, admin tasks
lib/
  app_protobuf/   # generated gRPC/protobuf types + shared conversions (Rust)
modules/          # business logic, one crate per feature
  base/           # foundational + template module
proto/            # protobuf definitions (grouped by module) — the single API source
migrations/       # SQLx migrations (.up.sql / .down.sql)
typescript/       # Bun workspace: all frontend / TypeScript packages
  app-protobuf/   # generated gRPC/protobuf TypeScript code (shared)
package.json      # root of the Bun workspace (workspaces: ["typescript/*"])
```

Rust binaries live under `bin/` — do **not** place binary crates at the
repository root. All TypeScript/JavaScript packages live under `typescript/`
and are managed as a single Bun workspace — do **not** create standalone,
unlinked npm/pnpm projects.

## Anatomy of a module

Every module crate mirrors `modules/base`:

```
src/
├── lib.rs        # declares the modules below; sets crate-wide lints
├── error.rs      # (base) the shared `Error` services and hooks return
├── config.rs     # typed configuration (stored in DB, cached in Redis)
├── utils/        # small, dependency-light helpers
├── entities/     # persistence layer
│   ├── db/       # PostgreSQL entities + compile-time-checked queries
│   └── redis/    # Redis collections (Cache, Lock, …) + queries (rkyv bodies)
├── services/     # business logic (stateful Processors)
├── events/       # Apache Iggy events: partition keys + event types
├── hooks/        # background reactors (Iggy event handlers, cron, loggers)
└── rpc/          # gRPC service implementations (transport edge)
```

### Where does my code go?

| You are writing…                                    | Put it in…      |
| --------------------------------------------------- | --------------- |
| A SQL query or a table row type                     | `entities/db`   |
| A Redis-cached value or ephemeral token             | `entities/redis`|
| A use case that combines queries and rules          | `services`      |
| A message other modules react to                    | `events`        |
| A reaction to an event / a cron job / an audit log  | `hooks`         |
| A gRPC endpoint implementation                       | `rpc`           |
| A typed setting an operator can change               | `config`        |
| A pure helper with no runtime deps                   | `utils`         |

## Layer rules

### `entities/db`

- One submodule per table or aggregate.
- Declare the table as a `wakuwaku::entity::Entity` marker type and define a
  `sqlx::FromRow` struct for the row.
- One input struct per query/command, implementing `Query` (its effect:
  `Read<T>` / `Write<T>`, or a tuple of them) and `Execute<PgSource>` (the SQL,
  run on `&mut PgConnection`, failing with `sqlx::Error`).
- Use `sqlx::query!` / `query_as!` so queries are checked against the database at
  compile time. Prefer schema-qualified table names.
- Annotate `execute` with `#[tracing::instrument(skip_all, err)]`.
- Several statements that must commit together are a transaction flow
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
- Load config with the config-cache helpers, not by re-reading the database on
  every call.
- Services orchestrate entities and publish events. **No transport types here.**

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
- Events are the *only* sanctioned way for modules to communicate
  asynchronously.
- wakuwaku does not create the stream or main topics; `manage-tool` provisions
  them (it creates only `retry_<topic>`).

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
  its `ServiceBuilder`. Like services, hooks own their dependencies and carry
  no transport logic.

### `rpc`

- Implement the protobuf service trait from `app_protobuf`.
- Handlers are thin adapters: decode request → call a service → encode reply.
- **No business logic** — if a handler grows rules, move them into a service.
- Re-export each concrete service type from `rpc/mod.rs` so `app-server` can
  mount it.

### `config`

- A `serde`-(de)serializable struct implementing `Default`, bound to a stable
  string key.
- Stored as JSON in the database, cached in Redis, seeded by `manage-tool`.

## Cross-cutting conventions

- **Errors:** wakuwaku 1.0 has no `wakuwaku::Error`. Services and hooks return
  `base::Error` (`modules/base/src/error.rs`); it converts into `tonic::Status`
  for `rpc` and into the Iggy `HandleError` (choosing its retry class) for
  hooks. Queries keep their native error (`sqlx::Error`, `RedisError`) and `?`
  lifts it. Add a variant to `base::Error` only when every module needs it;
  otherwise define the module's own `thiserror` enum the same way.
- **Lints:** keep the crate-level `#![deny(clippy::unwrap_used)]`,
  `expect_used`, and `panic` lints. No panics on the request path.
- **Tracing:** instrument entities, services, and RPC handlers with
  `#[tracing::instrument(skip_all, err)]`.
- **Dependency direction:** `rpc → services → entities/events/config`. A feature
  module may depend on `base` (and on shared modules), but `base` must not depend
  on a feature module, and modules must not depend on each other's internals —
  communicate via gRPC or Iggy events.
- **Third-party crates:** reach crates in wakuwaku's API through
  `wakuwaku::deps` (`iggy` only that way). `kanau`, `sqlx`, `redis`, and `tonic`
  are also direct workspace dependencies because derive macros and generated
  code name them by path and we enable extra features; keep their versions
  semver-compatible with wakuwaku's so one copy is resolved.
- **Protobuf:** `proto/` is the single source of truth for the API. Add `.proto`
  files there, register them in `app_protobuf`'s `build.rs` (Rust side), and
  regenerate the TypeScript side with `bun run generate:proto`. Never hand-edit
  or duplicate generated code.
- **Migrations:** every schema change is a pair of `.up.sql` / `.down.sql` files
  in `migrations/`.

## Adding a new module (checklist)

1. Copy the `modules/base` directory layout into `modules/<name>`.
2. Add the crate to the workspace `members` in the root `Cargo.toml`.
3. Define tables in `migrations/` and the API in `proto/` (register it in
   `app_protobuf`).
4. Implement, from the inside out: `entities` → `services` → `rpc`/`hooks`.
5. Wire the new services/hooks into `bin/app-server`'s workers.
6. Keep `config` values seedable from `bin/manage-tool`.

## Frontend / TypeScript

All TypeScript lives under `typescript/` as one **Bun workspace** (the root
`package.json` declares `workspaces: ["typescript/*"]`). Use **Bun** for
everything — install, scripts, running — not npm or pnpm.

### `app-protobuf` — generated API code, shared once

`typescript/app-protobuf` is the TypeScript counterpart of the `app_protobuf`
Rust crate: it holds the gRPC/protobuf code generated from `proto/`, and
**nothing else**. Every frontend package depends on `app-protobuf` instead of
generating (and duplicating) its own client — this is the whole point of the
workspace.

- Codegen is driven by `typescript/app-protobuf/generate-proto.sh`, exposed as
  the `generate:proto` script. Run it from the repo root:

  ```sh
  bun install            # once, to fetch the toolchain (grpc-tools, ts-proto)
  bun run generate:proto # regenerate after any change to proto/
  ```

- Output lands in `typescript/app-protobuf/src/generated/` (emptied and
  rewritten on every run — never edit it by hand or commit changes into it
  manually). The template ships this directory empty.
- The package exposes generated modules by subpath, mirroring the proto tree:

  ```ts
  import { GreeterDefinition } from "app-protobuf/sample/hello";
  ```

### Adding a frontend package

1. Create it under `typescript/<name>/` with its own `package.json`; the Bun
   workspace picks it up automatically.
2. Add `"app-protobuf": "workspace:*"` to its dependencies and import the
   generated types from there — do **not** re-run protoc inside the package.
3. Keep generated code, gRPC clients, and other shared TypeScript in dedicated
   workspace packages so each concern has exactly one home, just like the Rust
   side.
