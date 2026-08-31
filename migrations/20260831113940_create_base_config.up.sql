CREATE SCHEMA IF NOT EXISTS base;

CREATE TABLE base.application_config (
    id serial PRIMARY KEY,
    key text NOT NULL UNIQUE,
    content jsonb NOT NULL DEFAULT '{}'::jsonb,
    last_updated_at timestamp NOT NULL DEFAULT now()
);
