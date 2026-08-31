CREATE SCHEMA auth;

CREATE TYPE auth.admin_role AS ENUM ('site_owner', 'maintainer', 'moderator', 'assistant');
CREATE TYPE auth.invite_status AS ENUM ('accepted', 'expired', 'invalid', 'pending', 'free');
CREATE TYPE auth.pending_invitation_status AS ENUM ('pending', 'accepted', 'expired');
CREATE TYPE auth.suspense_status AS ENUM ('active', 'suspended');
CREATE TYPE auth.session_security_option AS ENUM ('reject_different_ip', 'reject_different_ip_or_user_agent', 'none');

CREATE TABLE auth.account (
    id uuid PRIMARY KEY,
    username text NOT NULL UNIQUE,
    email text NOT NULL UNIQUE,
    avatar_url text,
    password_hash text NOT NULL,
    registered_at timestamp NOT NULL DEFAULT now()
);

CREATE TABLE auth.session (
    session_id text PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES auth.account (id),
    user_agent text NOT NULL,
    ip_address text NOT NULL,
    created_at timestamp NOT NULL DEFAULT now(),
    last_refreshed_at timestamp NOT NULL DEFAULT now(),
    lifespan interval NOT NULL,
    security_option auth.session_security_option NOT NULL DEFAULT 'none',
    expired_at timestamp
);
CREATE INDEX session_user_id_idx ON auth.session (user_id);

CREATE TABLE auth.user_api_key (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES auth.account (id),
    key_hash text NOT NULL UNIQUE,
    remark text NOT NULL DEFAULT '',
    created_at timestamp NOT NULL DEFAULT now(),
    valid_until timestamp,
    scopes text[] NOT NULL DEFAULT '{}'
);
CREATE INDEX user_api_key_user_id_idx ON auth.user_api_key (user_id);

CREATE TABLE auth.invite (
    id bigserial PRIMARY KEY,
    owner uuid NOT NULL REFERENCES auth.account (id),
    invite_token text NOT NULL UNIQUE,
    created_at timestamp NOT NULL DEFAULT now(),
    will_expire_at timestamp,
    last_status_change timestamp NOT NULL DEFAULT now(),
    status auth.invite_status NOT NULL,
    source text NOT NULL DEFAULT ''
);

CREATE TABLE auth.membership (
    account uuid PRIMARY KEY REFERENCES auth.account (id),
    level int NOT NULL DEFAULT 0,
    admin_privilege auth.admin_role,
    invited_by bigint REFERENCES auth.invite (id),
    available_invitation_count int NOT NULL DEFAULT 0
);

CREATE TABLE auth.pending_invitation (
    id bigserial PRIMARY KEY,
    invite bigint NOT NULL REFERENCES auth.invite (id),
    email text NOT NULL,
    sent_at timestamp NOT NULL DEFAULT now(),
    will_release_at timestamp NOT NULL,
    last_status_change timestamp NOT NULL DEFAULT now(),
    status auth.pending_invitation_status NOT NULL DEFAULT 'pending'
);

CREATE TABLE auth.credit (
    account uuid PRIMARY KEY REFERENCES auth.account (id),
    total_amount numeric NOT NULL DEFAULT 0,
    frozen_amount numeric NOT NULL DEFAULT 0
);

CREATE TABLE auth.credit_change_history (
    id bigserial PRIMARY KEY,
    account uuid NOT NULL REFERENCES auth.account (id),
    available_amount_change numeric NOT NULL,
    frozen_amount_change numeric NOT NULL,
    reason text NOT NULL,
    created_at timestamp NOT NULL DEFAULT now()
);
CREATE INDEX credit_change_history_account_idx ON auth.credit_change_history (account);

CREATE TABLE auth.account_suspense (
    id bigserial PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES auth.account (id),
    status auth.suspense_status NOT NULL,
    created_at timestamp NOT NULL DEFAULT now(),
    reason text NOT NULL DEFAULT '',
    operated_by uuid REFERENCES auth.account (id)
);
CREATE INDEX account_suspense_account_id_created_at_idx ON auth.account_suspense (account_id, created_at DESC);

CREATE TABLE auth.admin_operation_log (
    id bigserial PRIMARY KEY,
    admin uuid NOT NULL REFERENCES auth.account (id),
    operation_name text NOT NULL,
    operation_content jsonb NOT NULL,
    created_at timestamp NOT NULL DEFAULT now()
);
CREATE INDEX admin_operation_log_admin_idx ON auth.admin_operation_log (admin);
