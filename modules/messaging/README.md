# `base` — foundational module

`base` is the reference and shared module of the workspace. It serves two roles:

1. **Shared library.** It holds the entities, config primitives, and utilities
   that more than one module needs, so feature modules depend on `base` instead
   of duplicating them.
2. **Template.** Its directory layout is the layout **every** module follows.
   To add a feature, copy this structure into a new `modules/<name>` crate.

## Layout

```
src/
├── lib.rs          # crate root: declares the modules below
├── config.rs       # strongly typed module configuration (DB-backed, Redis-cached)
├── utils/          # small, dependency-light helpers
├── entities/       # persistence layer
│   ├── db/         # PostgreSQL rows + compile-time-checked queries
│   └── redis/      # Redis key/value types (rkyv-encoded)
├── services/       # business logic (stateful Processors)
├── events/         # AMQP message payloads + routing
├── hooks/          # background reactors: consumers, cron, event loggers
└── rpc/            # gRPC service implementations (the transport edge)
```

## The `Processor` pattern

The whole stack is built on `kanau`'s `Processor` trait — *state + an async
function*:

```text
Processor = State + async fn(Input) -> Result<Output, Error>
```

A processor is a `Clone`-able struct that owns its dependencies and implements
`Processor<Input>` once per operation. The same abstraction is used everywhere:

- **Entities** implement `Processor` on `wakuwaku::sqlx::DatabaseProcessor` (for
  database work) or on their own type (for Redis).
- **Services** implement `Processor` on a service struct that owns the database,
  Redis, message queue, and any collaborating services.
- **Hooks** implement `Processor` (plus `AmqpMessageProcessor`) to consume
  events off the queue.

This keeps each unit small, individually testable, and trivially composable.

## Data flow

```
gRPC request ──► rpc ──► services ──► entities ──► PostgreSQL / Redis
                            │
                            └─► events ──► AMQP ──► hooks (this or another module)
```

## Dependency direction

- `rpc` depends on `services` (and `app_protobuf`).
- `services` depend on `entities`, `events`, and `config`.
- `entities`, `events`, `config`, and `utils` have no intra-module upward deps.
- Feature modules depend on `base`; `base` never depends on a feature module.

See the module-level doc comments (`cargo doc --open -p base`) for copy-pasteable
examples in each layer, and `AGENTS.md` at the workspace root for the full
authoring guide.
