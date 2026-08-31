CREATE SCHEMA messaging;

CREATE TABLE messaging.notification_settings (
    id uuid PRIMARY KEY,
    send_login_email boolean NOT NULL DEFAULT false,
    send_invitation_email boolean NOT NULL DEFAULT true,
    receive_marketing_email boolean NOT NULL DEFAULT false,
    created_at timestamp NOT NULL DEFAULT now(),
    updated_at timestamp NOT NULL DEFAULT now()
);
