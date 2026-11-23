# Airi

An identity provider for clubs and small teams, and a template for building
your internal services on top of it.

Airi works two ways:

- **As a service.** An unmodified clone runs as the IdP for one organization:
  a member directory, groups and roles, sign-in, and single sign-on to your
  other applications.
- **As a template.** Write your own modules beside the core ones. They key
  their data by member ID, react to membership events, and check permissions
  with the same proof the core modules use.

> **Status:** pre-release. Milestone 0.1 is tracked in
> [#1](https://github.com/suitsu31-club/Airi/issues/1); the binaries do not
> run yet.

## Features (0.1)

- **Directory:** members with lifecycle states, nested groups, roles and
  grants with end dates, invitations, and join requests with approval.
- **Sign-in:** passkeys, password with TOTP and recovery codes, email code and
  magic link, and upstream Google, GitHub, Discord or any OIDC provider.
- **Single sign-on:** OIDC provider, SAML 2.0, and forward-auth for Caddy,
  Traefik and nginx. Per-application access rules.
- **Operations:** service accounts, outbound webhooks, an audit log, and
  notifications by email or in-app inbox. Email is optional: with no mail
  relay configured, an operator passes links on by hand.

Settings that change at run time (applications, users, groups, policies,
mail) live in the database and the admin console. Anything that changes
behaviour, such as profile fields, claims or login steps, is code: you
write a module.

## Modules

| Module | Owns |
|---|---|
| `base` | Errors, configuration, the caller and permission-proof contract |
| `mailer` | Sending rendered email through a relay |
| `notification` | Messages to a person: channels, templates, preferences, inbox |
| `membership` | Directory, groups, roles and grants, access rules, invitations |
| `auth` | Credentials, sessions, the login flow, upstream login, service accounts |
| `connection` | Application registry, OIDC, SAML, forward-auth, webhooks |

## Stack

Rust on [wakuwaku](https://www.wakuwaku.rs/en) (gRPC, PostgreSQL, Redis,
Apache Iggy), with a SvelteKit web front end that calls the backend over gRPC.

```
bin/          app-server (gRPC, consumer and cron workers), manage-tool (CLI)
lib/          app_protobuf: generated Rust gRPC types
modules/      one crate per module
proto/        the API definitions
migrations/   SQL migrations for every module, in order (0001, 0002, ...)
typescript/   Bun workspace: web app and the generated TypeScript client
```

## Extending

Read [`AGENTS.md`](AGENTS.md) before adding code. It describes the module
layout, the layer rules, and how modules may use each other.

## License

[MIT](LICENSE)
