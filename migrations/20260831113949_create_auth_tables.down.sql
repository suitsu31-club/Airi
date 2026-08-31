DROP TABLE IF EXISTS auth.admin_operation_log;
DROP TABLE IF EXISTS auth.account_suspense;
DROP TABLE IF EXISTS auth.credit_change_history;
DROP TABLE IF EXISTS auth.credit;
DROP TABLE IF EXISTS auth.pending_invitation;
DROP TABLE IF EXISTS auth.membership;
DROP TABLE IF EXISTS auth.invite;
DROP TABLE IF EXISTS auth.user_api_key;
DROP TABLE IF EXISTS auth.session;
DROP TABLE IF EXISTS auth.account;

DROP TYPE IF EXISTS auth.session_security_option;
DROP TYPE IF EXISTS auth.suspense_status;
DROP TYPE IF EXISTS auth.pending_invitation_status;
DROP TYPE IF EXISTS auth.invite_status;
DROP TYPE IF EXISTS auth.admin_role;

DROP SCHEMA IF EXISTS auth;
