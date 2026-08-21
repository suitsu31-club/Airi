# `manage-tool`

Command-line administration tool for operating the application outside the
request path.

## Typical commands

- **Migrations** — run and validate the SQL migrations in
  [`migrations/`](../../migrations).
- **Configuration** — seed default config values into the database and refresh
  the Redis cache; validate config JSON.
- **Admin accounts** — create, list, and manage administrator accounts.
- **Maintenance** — one-off data fixes and operational chores.

## Why a separate binary

Keeping administrative actions out of [`app-server`](../app-server) means the
server image stays focused on serving traffic, while destructive or
infrequent operations live in a tool run deliberately by an operator (or a
deployment job). It reuses the same modules and entities, so commands share the
exact types and queries the server uses.

## Conventions

- Build subcommands with a CLI parser (e.g. `clap`).
- Read connection settings (`DATABASE_URL`, `REDIS_URL`) from flags or the
  environment.
- Reuse module `entities`/`services` rather than issuing ad-hoc SQL, so the tool
  and the server never drift apart.
