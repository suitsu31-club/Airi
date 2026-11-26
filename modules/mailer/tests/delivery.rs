//! Delivery through a real relay: the mail catcher of the dev stack.
//!
//! Needs PostgreSQL (`DATABASE_URL`; each test gets a fresh database), Redis
//! (`REDIS_URL`, default `redis://127.0.0.1:6379`), Iggy (`IGGY_URL`, default
//! `iggy://iggy:iggy@127.0.0.1:8090`) and Mailpit accepting any SMTP
//! credentials (`MAILPIT_SMTP_HOST` / `MAILPIT_SMTP_PORT`, default
//! `127.0.0.1:2525`; `MAILPIT_URL`, default `http://127.0.0.1:8025`).
//!
//! ```sh
//! cargo test -p mailer --features delivery --test delivery
//! ```
#![cfg(feature = "delivery")]

use base::ModuleConfig;
use base::entities::redis::config::MODULE_CONFIG_CACHE;
use base::services::config::{ConfigLoader, ConfigUpdater, UpdateConfig};
use kanau::processor::Processor;
use mailer::config::{MailerConfig, MailerMode};
use mailer::entities::db::send_box::{
    EmailFormat, FindMailById, InsertMail, InsertMailOutcome, MailContent, MailStatus,
};
use mailer::entities::db::sender::{
    InsertSmtpSender, SmtpCredentials, SmtpSecret, SmtpSenderEntity, SmtpSenderId, SmtpTlsMode,
};
use mailer::services::delivery::{DeliverMail, Delivery, MailDeliveryService, TestTransport};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use time::{OffsetDateTime, PrimitiveDateTime};
use type_safe_data::encryption::Key256;
use uuid::Uuid;
use wakuwaku::deps::iggy::prelude::{
    Client, Identifier, IggyClient, StreamClient, TopicClient, TopicCreateOptions,
};
use wakuwaku::entity::redis::RedisSource;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use wakuwaku::iggy::Publisher;

/// The tests publish `EmailSent` here, away from the application's stream.
const STREAM: &str = "airi_mailer_test";
const DELIVERY_TOPIC: &str = "mailer_delivery";
const USERNAME: &str = "relay-user";
const PASSWORD: &str = "relay-password";

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn now_utc() -> PrimitiveDateTime {
    let now = OffsetDateTime::now_utc();
    PrimitiveDateTime::new(now.date(), now.time())
}

/// A fresh token to find this run's messages in the catcher.
fn token() -> String {
    Uuid::now_v7().simple().to_string()
}

struct Harness {
    db: Db<PgSource, ReadWrite>,
    redis: Db<RedisSource, ReadWrite>,
    service: MailDeliveryService,
    key: Key256,
}

impl Harness {
    /// The delivery service on `pool`, in SMTP mode, publishing to [`STREAM`].
    async fn new(pool: PgPool) -> Self {
        let connection = redis::Client::open(env_or("REDIS_URL", "redis://127.0.0.1:6379"))
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        let redis: Db<RedisSource, ReadWrite> = Db::new(RedisSource::new(connection));
        let source = PgSource::new(pool);
        let db: Db<PgSource, ReadWrite> = Db::new(source.clone());
        ConfigUpdater::new(db.clone(), redis.clone())
            .process(UpdateConfig {
                value: MailerConfig {
                    mode: MailerMode::Smtp,
                    ..MailerConfig::default()
                },
            })
            .await
            .unwrap();
        let key = Key256::from([7; 32]);
        let service = MailDeliveryService::new(
            db.clone(),
            publisher().await,
            ConfigLoader::new(Db::new(source), redis.clone()),
            Some(Arc::new(key.clone())),
        );
        Self {
            db,
            redis,
            service,
            key,
        }
    }

    /// A sender with credentials, relaying through `host:port` in plaintext.
    async fn sender(&self, host: String, port: u16) -> SmtpSenderEntity {
        let id = SmtpSenderId::generate();
        let credentials = SmtpCredentials {
            username_encrypted: SmtpSecret::encrypt(
                &self.key,
                USERNAME,
                &id.username_associated_data(),
            )
            .unwrap(),
            password_encrypted: SmtpSecret::encrypt(
                &self.key,
                PASSWORD,
                &id.password_associated_data(),
            )
            .unwrap(),
        };
        self.db
            .process(InsertSmtpSender {
                id,
                name: format!("test-{}", token()),
                address: "noreply@example.com".parse().unwrap(),
                host,
                port: i32::from(port),
                tls_mode: SmtpTlsMode::Plaintext,
                credentials: Some(credentials),
                enabled: true,
                remark: String::new(),
            })
            .await
            .unwrap()
    }

    async fn catcher_sender(&self) -> SmtpSenderEntity {
        let port = env_or("MAILPIT_SMTP_PORT", "2525").parse().unwrap();
        self.sender(env_or("MAILPIT_SMTP_HOST", "127.0.0.1"), port)
            .await
    }

    /// Leave no SMTP mode in the shared cache for an application on the
    /// same Redis.
    async fn finish(self) {
        self.redis
            .process(MODULE_CONFIG_CACHE.remove(&MailerConfig::full_config_key()))
            .await
            .unwrap();
    }
}

/// A publisher on [`STREAM`], creating the stream and its topic if needed.
async fn publisher() -> Publisher {
    let client =
        IggyClient::from_connection_string(&env_or("IGGY_URL", "iggy://iggy:iggy@127.0.0.1:8090"))
            .unwrap();
    client.connect().await.unwrap();
    let stream = Identifier::named(STREAM).unwrap();
    let topic = Identifier::named(DELIVERY_TOPIC).unwrap();
    // Tests run concurrently: a create may lose to another test's, so only
    // the lookup afterwards must succeed.
    if client.get_stream(&stream).await.unwrap().is_none() {
        let _ = client.create_stream(STREAM).await;
    }
    if client.get_topic(&stream, &topic).await.unwrap().is_none() {
        let _ = client
            .create_topic(&stream, DELIVERY_TOPIC, &TopicCreateOptions::default())
            .await;
    }
    assert!(client.get_topic(&stream, &topic).await.unwrap().is_some());
    Publisher::new(Arc::new(client), STREAM).unwrap()
}

/// Every message the catcher holds whose subject contains `token`.
async fn caught(token: &str) -> Vec<Value> {
    let mut url = reqwest::Url::parse(&env_or("MAILPIT_URL", "http://127.0.0.1:8025"))
        .unwrap()
        .join("api/v1/search")
        .unwrap();
    url.query_pairs_mut()
        .append_pair("query", &format!("subject:{token}"));
    let found: Value = reqwest::get(url).await.unwrap().json().await.unwrap();
    found["messages"].as_array().unwrap().clone()
}

/// The full message, with its bodies.
async fn caught_message(id: &str) -> Value {
    let url = reqwest::Url::parse(&env_or("MAILPIT_URL", "http://127.0.0.1:8025"))
        .unwrap()
        .join(&format!("api/v1/message/{id}"))
        .unwrap();
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn queued_mail_reaches_the_relay_once(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let sender = harness.catcher_sender().await;
    let token = token();
    let InsertMailOutcome::Created(mail) = harness
        .db
        .process(InsertMail {
            requested_at: now_utc(),
            send_to_address: "member@example.com".parse().unwrap(),
            send_to_name: "A Member".to_owned(),
            send_as: Some(sender.id),
            title: format!("Sign in {token}"),
            message_content: MailContent("<p>Your link</p>".to_owned()),
            format: EmailFormat::Html,
            status: MailStatus::Queued,
            send_before_or_cancel: None,
        })
        .await
        .unwrap()
    else {
        panic!("the request is new");
    };

    let delivered = harness
        .service
        .process(DeliverMail {
            mail_id: mail.mail_id,
        })
        .await
        .unwrap();
    assert_eq!(delivered, Delivery::Sent);
    let stored = harness
        .db
        .process(FindMailById {
            mail_id: mail.mail_id,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, MailStatus::Sent);
    assert!(stored.attempted_at.is_some());

    let found = caught(&token).await;
    assert_eq!(found.len(), 1, "{found:#?}");
    let summary = &found[0];
    assert_eq!(summary["From"]["Address"], "noreply@example.com");
    assert_eq!(summary["To"][0]["Address"], "member@example.com");
    assert_eq!(summary["To"][0]["Name"], "A Member");
    assert_eq!(
        summary["Username"], USERNAME,
        "decrypted credentials were used"
    );
    let message = caught_message(summary["ID"].as_str().unwrap()).await;
    // SMTP ends the body with a line break.
    assert_eq!(
        message["HTML"].as_str().unwrap().trim_end(),
        "<p>Your link</p>"
    );

    // A redelivered `EmailQueued` finds the message sent and sends nothing.
    let repeated = harness
        .service
        .process(DeliverMail {
            mail_id: mail.mail_id,
        })
        .await
        .unwrap();
    assert_eq!(repeated, Delivery::Sent);
    assert_eq!(caught(&token).await.len(), 1);

    harness.finish().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_transport_returns_the_relay_reply(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let sender = harness.catcher_sender().await;

    let answer = harness
        .service
        .process(TestTransport {
            sender_id: sender.id,
            send_to: "operator@example.com".parse().unwrap(),
        })
        .await
        .unwrap();
    assert!(answer.accepted, "{answer:?}");
    assert!(answer.reply.starts_with("250 "), "{answer:?}");
    // The sender's name is a fresh token, and the subject carries it.
    let found = caught(sender.name.trim_start_matches("test-")).await;
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0]["To"][0]["Address"], "operator@example.com");

    harness.finish().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_transport_reports_an_unreachable_relay(pool: PgPool) {
    let harness = Harness::new(pool).await;
    // A port nothing listens on any more.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let sender = harness.sender("127.0.0.1".to_owned(), port).await;

    let answer = harness
        .service
        .process(TestTransport {
            sender_id: sender.id,
            send_to: "operator@example.com".parse().unwrap(),
        })
        .await
        .unwrap();
    assert!(!answer.accepted, "{answer:?}");
    assert!(!answer.reply.is_empty());

    harness.finish().await;
}
