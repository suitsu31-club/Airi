//! The retention job deletes old bodies, but never one still to be sent.
//!
//! Needs PostgreSQL (`DATABASE_URL`; each test gets a fresh database) and
//! Redis (`REDIS_URL`, default `redis://127.0.0.1:6379`).

use base::services::config::ConfigLoader;
use kanau::processor::Processor;
use mailer::config::MailerConfig;
use mailer::entities::db::send_box::{
    EmailFormat, FindMailById, InsertMail, InsertMailOutcome, MailContent, MailId, MailStatus,
    MarkMailSent,
};
use mailer::entities::db::sender::{InsertSmtpSender, SmtpSenderId, SmtpTlsMode};
use mailer::hooks::mail_retention::DeleteOldMailContent;
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

async fn has_body(db: &Db<PgSource, ReadWrite>, mail_id: MailId) -> bool {
    db.process(FindMailById { mail_id })
        .await
        .unwrap()
        .unwrap()
        .message_content
        .is_some()
}

#[sqlx::test(migrations = "../../migrations")]
async fn deletes_old_bodies_of_finished_mail_only(pool: PgPool) {
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
    let sent = insert(&db, sender, "sent").await;
    db.process(MarkMailSent { mail_id: sent }).await.unwrap();
    let queued = insert(&db, sender, "queued").await;

    // The default config is stored nowhere, so the job uses it.
    let job = DeleteOldMailContent::new(
        db.clone(),
        ConfigLoader::new(Db::new(PgSource::new(pool)), redis),
    );
    let retention = Duration::from_secs(u64::from(MailerConfig::default().delete_mail_older_than));
    let tick = |run, after| CronTick {
        run,
        fired_at: SystemTime::now() + after,
    };

    job.process(tick(0, Duration::ZERO)).await.unwrap();
    assert!(has_body(&db, sent).await, "a recent body is kept");

    let later = retention + Duration::from_secs(60);
    job.process(tick(1, later)).await.unwrap();
    assert!(!has_body(&db, sent).await, "an old sent body is deleted");
    assert!(
        has_body(&db, queued).await,
        "a queued body is still to be sent"
    );

    // Another replica's run finds nothing left to delete.
    job.process(tick(2, later)).await.unwrap();
    assert!(has_body(&db, queued).await);
}
