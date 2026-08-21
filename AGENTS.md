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

A `Processor` is a `Clone`-able struct that owns its dependencies and implements
`Processor<Input>` once per operation. Model each database query, each business
operation, and each queue consumer as an input struct plus a `Processor` impl.
Do not invent parallel abstractions.

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
├── config.rs     # typed configuration (stored in DB, cached in Redis)
├── utils/        # small, dependency-light helpers
├── entities/     # persistence layer
│   ├── db/       # PostgreSQL rows + compile-time-checked queries
│   └── redis/    # Redis key/value types (rkyv-encoded)
├── services/     # business logic (stateful Processors)
├── events/       # AMQP payloads + routing
├── hooks/        # background reactors (consumers, cron, loggers)
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
- Define a `sqlx::FromRow` struct for the row.
- Implement `Processor<Input>` for `wakuwaku::sqlx::DatabaseProcessor`, one impl
  per query/command.
- Use `sqlx::query!` / `query_as!` so queries are checked against the database at
  compile time. Prefer schema-qualified table names.
- Annotate impls with `#[tracing::instrument]`.

### `entities/redis`

- One submodule per key kind.
- Define a value type and a key type; derive the `rkyv` traits.
- Implement `KeyValue` + `KeyValueRead` + `KeyValueWrite` from
  `wakuwaku::redis`.

### `services`

- A service is a `Clone` struct owning its dependencies (database, Redis, AMQP,
  loaded config, other services).
- One `Processor` impl per operation; return domain types, not protobuf types.
- Load config with the config-cache helpers, not by re-reading the database on
  every call.
- Services orchestrate entities and publish events. **No transport types here.**

### `events`

- Define the payload, derive the `rkyv` traits, and implement `AmqpRouting`
  (`EXCHANGE`, `EXCHANGE_TYPE`, `ROUTING_KEY`) + `AmqpMessageSend`.
- Document each event's contract in a doc comment: **who publishes, who
  consumes, and the routing key.**
- Events are the *only* sanctioned way for modules to communicate
  asynchronously.

### `hooks`

- AMQP consumers implement `AmqpMessageProcessor<E>` (with a durable `QUEUE`
  name) plus `Processor<E>`.
- Also the home for cron jobs and event loggers.
- Like services, hooks own their dependencies and carry no transport logic.

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

- **Errors:** use `wakuwaku::Error` at the service/hook boundary; `sqlx::Error`
  is fine inside `entities/db`. Define module-specific error enums with
  `thiserror` when a layer needs richer variants.
- **Lints:** keep the crate-level `#![deny(clippy::unwrap_used)]`,
  `expect_used`, and `panic` lints. No panics on the request path.
- **Tracing:** instrument entities, services, and RPC handlers with
  `#[tracing::instrument(skip_all, err)]`.
- **Dependency direction:** `rpc → services → entities/events/config`. A feature
  module may depend on `base` (and on shared modules), but `base` must not depend
  on a feature module, and modules must not depend on each other's internals —
  communicate via gRPC or AMQP events.
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
