# `app-server`

The main application server binary. It composes the business modules under
[`modules/`](../../modules) and runs them behind one or more **workers**.

## Workers

A worker is a single run mode chosen at startup (typically from environment
variables). One binary, several modes — pick the mode per deployment so each
responsibility scales independently:

| Worker         | Responsibility                                              |
| -------------- | ----------------------------------------------------------- |
| gRPC           | Serve each module's `rpc` services over HTTP/2.             |
| Iggy consumer  | Drive each module's event `hooks` from Apache Iggy (one consumer per topic, explicit partition list). |
| Cron executor  | Run each module's `CronJob`s in-process.                    |
| REST / webhook | Expose HTTP endpoints for third-party callbacks.            |

Running the same image in different modes keeps build and deployment uniform.

## Responsibilities

- Load configuration and construct shared dependencies: PostgreSQL pool and
  Redis connection wrapped in `Db` handles (run migrations on the pool before
  wrapping it), and a connected `Arc<IggyClient>` plus the `Publisher` for the
  app's Iggy stream.
- Register them in a `wakuwaku::ServiceBuilder` and build each module's
  services, gRPC servers (`grpc_service`), event handlers (`iggy_consumer`),
  and cron jobs (`cron_job`) from it; a missing dependency fails to compile.
- Mount the selected worker and run until a shutdown signal is received, then
  `shutdown()` the consumer and cron runtimes (bounded by a timeout).
- Set up observability (tracing / OpenTelemetry) and health checks.

## What does *not* belong here

Business logic. The binary is wiring only: it selects a worker, builds
dependencies, and hands control to the modules. Keep domain behaviour in the
modules' `services`, `entities`, and `hooks`.
