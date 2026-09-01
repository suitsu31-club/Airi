CREATE TABLE auth.totp (
    user_id uuid PRIMARY KEY REFERENCES auth.account (id) ON DELETE CASCADE,
    secret bytea NOT NULL,
    created_at timestamp NOT NULL DEFAULT now()
);

CREATE TABLE auth.totp_recovery_code (
    user_id uuid NOT NULL REFERENCES auth.account (id) ON DELETE CASCADE,
    code_hash bytea NOT NULL,
    created_at timestamp NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, code_hash)
);
CREATE INDEX totp_recovery_code_user_id_idx ON auth.totp_recovery_code (user_id);
