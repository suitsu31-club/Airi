-- `mailer` module: every message `SendEmail` accepted, and how far its
-- delivery got.
CREATE TABLE "mailer"."send_box" (
    "mail_id"               bigserial PRIMARY KEY,
    -- `IdempotencyKey`: a BLAKE3 hash of the request (when it was made, the
    -- recipient, the sender and the message). A repeated `SendEmail` returns
    -- the stored message instead of storing and sending it again.
    "idempotency_key"       bytea     NOT NULL UNIQUE
        CHECK (length("idempotency_key") = 32),
    "send_to_address"       text      NOT NULL,
    -- Display name of the recipient, '' for none.
    "send_to_name"          text      NOT NULL DEFAULT '',
    -- NULL: no sender to deliver through. The message is stored as `sent`
    -- and never delivered.
    "send_as"               uuid      REFERENCES "mailer"."smtp_sender" ("id"),
    "title"                 text      NOT NULL,
    -- NULL once the retention job deleted it.
    "message_content"       text,
    "format"                text      NOT NULL CHECK ("format" IN ('plain', 'html')),
    "status"                text      NOT NULL
        CHECK ("status" IN ('queued', 'held', 'sent', 'failed', 'cancelled')),
    -- What the relay answered when it rejected the message.
    "failure_reply"         text,
    "created_at"            timestamp NOT NULL DEFAULT (now() AT TIME ZONE 'UTC'),
    "sent_at"               timestamp,
    -- A message still unsent at this time is cancelled instead.
    "send_before_or_cancel" timestamp,
    CONSTRAINT "send_box_sent_at_iff_sent"
        CHECK (("status" = 'sent') = ("sent_at" IS NOT NULL)),
    CONSTRAINT "send_box_failure_reply_iff_failed"
        CHECK (("status" = 'failed') = ("failure_reply" IS NOT NULL)),
    CONSTRAINT "send_box_no_sender_is_sent"
        CHECK ("send_as" IS NOT NULL OR "status" = 'sent')
);

-- Listing by status (held messages for the admin console), newest first.
CREATE INDEX "send_box_status_mail_id" ON "mailer"."send_box" ("status", "mail_id");
