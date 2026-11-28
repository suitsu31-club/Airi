# AGENTS.md

A Rust backend built on [wakuwaku](https://www.wakuwaku.rs/en), plus a Bun
workspace for the frontend. Every module looks the same, so code can be
navigated mechanically.

## Core idea

**Everything is a `Processor`**: state plus an async function.

```text
Processor = State + async fn(Input) -> Result<Output, Error>
```

Each business operation, query, event handler, and cron job is an input type
plus a `Processor` impl on a struct that owns its dependencies. Use wakuwaku's
layers built on this idea; do not invent parallel abstractions. The wakuwaku
docs at <https://www.wakuwaku.rs/en> cover the details.

## Workspace layout

```
bin/              # Rust binaries: wiring only, no business logic
  app-server/     # runs modules behind pluggable workers (gRPC, consumer, cron, ...)
  manage-tool/    # CLI: migrations, config seeding, admin tasks
lib/
  app_protobuf/   # generated gRPC/protobuf types + shared conversions (Rust)
modules/          # business logic, one crate per feature; `base` is the template
proto/            # protobuf definitions, the single source of truth for the API
migrations/       # SQLx migrations for every module
typescript/       # Bun workspace: all TypeScript packages
  app-protobuf/   # generated gRPC/protobuf TypeScript code (shared)
```

- Binary crates live under `bin/`, never at the repository root.
- Inside a module: `rpc → services → entities/events/config`.
- Across modules, a layer uses only the **same layer** of another module
  (`services` ↔ `services`, `entities/db` ↔ `entities/db`, cross-module
  foreign keys allowed). `hooks` may consume any module's `events`; events are
  the only asynchronous channel between modules. `base` is usable from every
  layer and depends on no feature module.
- Keep the crate-level `clippy::unwrap_used`, `expect_used`, and `panic`
  denies. No panics on the request path.
- Never hand-edit generated code. After changing `proto/`, register new files
  in `app_protobuf`'s `build.rs` and run `bun run generate:proto`.

Check the `write-wakuwaku-module` skill when you write or change code in a
module, and the `write-and-test-sql` skill when you need to do something with
SQL (queries, migrations, or their tests).

## Test guideline

- Prefer unit tests over integration tests.
- Test small units. Prove large parts correct by construction with traits
  and/or `wakuwaku::proof` (`Proven<T, P>`), then test the unit that grants the
  proof, not every caller that relies on it.
- Don't test code whose correctness is evident from its semantics: plain
  delegation, wiring, field mapping, derived impls.
- Don't test simple SQL (defined in the `write-and-test-sql` skill).

## Frontend / TypeScript

- All TypeScript lives under `typescript/` as one Bun workspace. Use Bun for
  everything (install, scripts, running), never npm or pnpm, and never create
  standalone projects.
- `typescript/app-protobuf` holds only the code generated from `proto/` into
  `src/generated/` (rewritten on every run). Packages depend on it with
  `"app-protobuf": "workspace:*"` and import by subpath mirroring the proto
  tree (`import { GreeterDefinition } from "app-protobuf/sample/hello";`);
  they never run protoc themselves.
- Shared TypeScript (clients, helpers) goes in its own workspace package, so
  each concern has one home.
