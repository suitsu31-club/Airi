# `app-server`

The main application server binary. It composes the business modules under
[`modules/`](../../modules) and runs them behind one or more **workers**.

## Workers

A worker is a run mode chosen at startup with `APP_MODE`. One binary, several
modes: pick the mode per deployment so each responsibility scales
independently.

| `APP_MODE`      | Responsibility                                              |
| --------------- | ----------------------------------------------------------- |
| `grpc`          | Serve each module's `rpc` services and `grpc.health.v1.Health` over HTTP/2. |
| `consumer`      | Drive each module's event `hooks` from Apache Iggy (one consumer per topic, explicit partition list). |
| `cron`          | Run each module's `CronJob`s in-process.                    |
| `all` (default) | All of the above in one process, for small deployments.    |

Running the same image in different modes keeps build and deployment uniform.

## Settings

Every setting is a flag or an environment variable (`app-server --help`).

| Variable | Default | Meaning |
| --- | --- | --- |
| `APP_MODE` | `all` | Workers to run (see above). |
| `DATABASE_URL` | required | PostgreSQL primary. |
| `DATABASE_READ_URL` | primary | PostgreSQL read replica for read-only handles. |
| `DATABASE_MAX_CONNECTIONS` | `10` | Connections per PostgreSQL pool. |
| `REDIS_URL` | required | Redis. |
| `IGGY_URL` | required | Iggy connection string with credentials, e.g. `iggy://iggy:iggy@127.0.0.1:8090`. |
| `IGGY_STREAM` | `airi` | Iggy stream holding every topic. |
| `CONSUMER_SHARD`, `CONSUMER_SHARDS` | `0`, `1` | Consumer replica `i` of `n` consumes the partitions whose ID modulo `n` is `i`. |
| `GRPC_LISTEN` | `0.0.0.0:50051` | gRPC listener. |
| `HEALTH_LISTEN` | `0.0.0.0:8080` | HTTP probes, served in every mode. |
| `SHUTDOWN_TIMEOUT_SECS` | `30` | Time the workers get to drain after SIGTERM. |
| `LOG_FORMAT` | `text` | `text` or `json`; the filter is `RUST_LOG` (default `info`). |
| `MAILER_SECRET_KEY` | unset | Read by the `mailer` module: the key of the SMTP credentials. |

`app-server` neither migrates the database nor creates the Iggy stream and
topics. `manage-tool` will provision them (#6); until then, apply
`migrations/*.up.sql` and create the topics with the `iggy` CLI. A consumer
whose topic is missing stops the startup.

## Health

- `GET /livez` on `HEALTH_LISTEN`: 200 while the process answers.
- `GET /readyz` on `HEALTH_LISTEN`: 200 once the workers are started and
  PostgreSQL (primary and replica), Redis and Iggy answered their last probe
  (every 5 s); 503 otherwise, and from the moment shutdown starts.
- In `grpc` and `all` modes, the overall (`""`) status of
  `grpc.health.v1.Health` on `GRPC_LISTEN` follows `/readyz`.

## Responsibilities

- Load settings and construct the shared dependencies: PostgreSQL pools and
  the Redis connection wrapped in `Db` handles, and a connected
  `Arc<IggyClient>` plus the `Publisher` for the app's Iggy stream
  (`src/dependencies.rs`).
- Register them and each module's services in a `wakuwaku::ServiceBuilder`
  (`worker::services`), and build each worker's gRPC servers
  (`grpc_service`), event handlers (`iggy_consumer`) and cron jobs
  (`cron_job`) from it, one file per worker under `src/worker/`; a missing
  dependency fails to compile.
- Run the selected workers until SIGTERM or Ctrl-C, then stop the gRPC
  server and `shutdown()` the consumer and cron runtimes, bounded by
  `SHUTDOWN_TIMEOUT_SECS`.
- Set up logging (`src/observability.rs`) and health checks
  (`src/health_check.rs`).

## Adding a module

1. Register its services in `worker::services` and mirror them in the
   `Services` type, both in `src/worker/mod.rs`.
2. Add one registration call per component to the worker that runs it:
   `.grpc_service::<XxxServer<_>, Impl, _>(XxxServer::new)` in `grpc.rs`,
   an `iggy_consumer` chain per topic in `consumer.rs`, `.cron_job` /
   `.compose_cron_job` in `cron.rs`.

## What does *not* belong here

Business logic. The binary is wiring only: it selects a worker, builds
dependencies, and hands control to the modules. Keep domain behaviour in the
modules' `services`, `entities`, and `hooks`.
