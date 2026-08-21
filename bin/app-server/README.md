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
| AMQP consumer  | Drive each module's `hooks` from the message queue.         |
| Cron executor  | Run scheduled/periodic jobs.                                |
| REST / webhook | Expose HTTP endpoints for third-party callbacks.            |

Running the same image in different modes keeps build and deployment uniform.

## Responsibilities

- Load configuration and construct shared dependencies (PostgreSQL pool, Redis
  connection, AMQP pool).
- Construct each module's services and hooks, injecting those dependencies.
- Mount the selected worker and run until a shutdown signal is received.
- Set up observability (tracing / OpenTelemetry) and health checks.

## What does *not* belong here

Business logic. The binary is wiring only: it selects a worker, builds
dependencies, and hands control to the modules. Keep domain behaviour in the
modules' `services`, `entities`, and `hooks`.
