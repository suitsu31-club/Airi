-- `base` module: typed module configuration, one JSON value per key
-- (`ModuleConfig::full_config_key`, e.g. `@base/site_metadata`).
CREATE SCHEMA IF NOT EXISTS "base";

CREATE TABLE "base"."module_config" (
    "key"             text      PRIMARY KEY,
    "value"           jsonb     NOT NULL,
    -- UTC, matching `time::PrimitiveDateTime` on the Rust side.
    "last_updated_at" timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC')
);
