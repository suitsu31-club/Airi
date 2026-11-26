//! The stale job fails queued messages nothing will deliver any more,
//! including ones the transport attempted before the consumer gave up.
//!
//! Needs PostgreSQL (`DATABASE_URL`; each test gets a fresh database) and
//! Redis (`REDIS_URL`, default `redis://127.0.0.1:6379`).

use base::services::config::ConfigLoader;
use kanau::processor::Processor;
use mailer::config::MailerConfig;
use mailer::entities::db::send_box::{
    EmailFormat, FindMailById, InsertMail, InsertMailOutcome, MailContent, MailId, MailStatus,
    MarkMailAttempted,
};
use mailer::entities::db::sender::{InsertSmtpSender, SmtpSenderId, SmtpTlsMode};
use mailer::hooks::stale_mail::FailStaleMail;
use sqlx::PgPool;
use std::time::{Duration, SystemTime};
use time::{OffsetDateTime, PrimitiveDateTime};
use wakuwaku::CronTick;
use wakuwaku::entity::redis::RedisSource;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};

async fn insert(db: &Db<PgSource, ReadWrite>, sender: SmtpSenderId, title: &str) -> MailId {
    let now = OffsetDateTime::now_utc();
    let InsertMailOutcome::Created(mail) = db
        .process(InsertMail {
            requested_at: PrimitiveDateTime::new(now.date(), now.time()),
            send_to_address: "member@example.com".parse().unwrap(),
            send_to_name: String::new(),
            send_as: Some(sender),
            title: title.to_owned(),
            message_content: MailContent("body".to_owned()),
            format: EmailFormat::Plain,
            status: MailStatus::Queued,
            send_before_or_cancel: None,
        })
        .await
        .unwrap()
    else {
        panic!("the request is new");
    };
    mail.mail_id
}

async fn status(db: &Db<PgSource, ReadWrite>, mail_id: MailId) -> MailStatus {
    db.process(FindMailById { mail_id })
        .await
        .unwrap()
        .unwrap()
        .status
}

#[sqlx::test(migrations = "../../migrations")]
async fn fails_queued_mail_whether_or_not_it_was_attempted(pool: PgPool) {
    let connection = redis::Client::open(
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .unwrap()
    .get_multiplexed_async_connection()
    .await
    .unwrap();
    let redis: Db<RedisSource, ReadWrite> = Db::new(RedisSource::new(connection));
    let db: Db<PgSource, ReadWrite> = Db::new(PgSource::new(pool.clone()));
    let sender = SmtpSenderId::generate();
    db.process(InsertSmtpSender {
        id: sender,
        name: "noreply".to_owned(),
        address: "noreply@example.com".parse().unwrap(),
        host: "relay.example.com".to_owned(),
        port: 587,
        tls_mode: SmtpTlsMode::StartTls,
        credentials: None,
        enabled: true,
        remark: String::new(),
    })
    .await
    .unwrap();
    // Its `EmailQueued` was lost before the transport saw it.
    let unattempted = insert(&db, sender, "unattempted").await;
    // The transport tried, failed retryably, and the consumer gave up.
    let abandoned = insert(&db, sender, "abandoned").await;
    assert!(
        db.process(MarkMailAttempted { mail_id: abandoned })
            .await
            .unwrap()
    );

    // The default config is stored nowhere, so the job uses it.
    let job = FailStaleMail::new(
        db.clone(),
        ConfigLoader::new(Db::new(PgSource::new(pool)), redis),
    );
    let stale_after = Duration::from_secs(u64::from(MailerConfig::default().stale_after_secs));
    let tick = |run, after| CronTick {
        run,
        fired_at: SystemTime::now() + after,
    };

    job.process(tick(0, Duration::ZERO)).await.unwrap();
    assert_eq!(status(&db, unattempted).await, MailStatus::Queued);
    assert_eq!(status(&db, abandoned).await, MailStatus::Queued);

    job.process(tick(1, stale_after + Duration::from_secs(60)))
        .await
        .unwrap();
    assert_eq!(status(&db, unattempted).await, MailStatus::Failed);
    assert_eq!(
        status(&db, abandoned).await,
        MailStatus::Failed,
        "an attempted message the consumer no longer retries is failed too"
    );
}
