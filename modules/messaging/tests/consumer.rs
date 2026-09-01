//! In-process consumer test: binds `NotificationInitHook`, publishes a
//! `UserRegisteredEvent`, and verifies default notification settings are created.
//!
//! Requires `DATABASE_URL` and `MQ_URL`.

use auth::entities::db::account::{AccountId, CreateAccount};
use base::events::UserRegisteredEvent;
use kanau::processor::Processor;
use messaging::hooks::NotificationInitHook;
use std::sync::Arc;
use std::time::Duration;
use wakuwaku::amqp::{AmqpMessageProcessor, AmqpMessageSend, AmqpPool, setup_consumer};
use wakuwaku::sqlx::DatabaseProcessor;

async fn deps() -> (DatabaseProcessor, AmqpPool) {
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let mq_url = std::env::var("MQ_URL").expect("MQ_URL");
    let pool = base::db::connect_pool(&db_url)
        .await
        .expect("connect postgres");
    let db = DatabaseProcessor::from_pool(pool);
    let args =
        amqprs::connection::OpenConnectionArguments::try_from(mq_url.as_str()).expect("mq url");
    let conn = amqprs::connection::Connection::open(&args)
        .await
        .expect("open amqp");
    let mq = AmqpPool::connect(conn).await;
    (db, mq)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consumer_initialises_notification_settings() {
    let (db, mq) = deps().await;

    let channel =
        <NotificationInitHook as AmqpMessageProcessor<UserRegisteredEvent>>::ensure_queue(&mq)
            .await
            .expect("ensure queue");
    setup_consumer::<UserRegisteredEvent, NotificationInitHook>(
        &channel,
        Arc::new(NotificationInitHook { db: db.clone() }),
    )
    .await
    .expect("setup consumer");

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let user_id = AccountId(uuid::Uuid::new_v4());
    db.process(CreateAccount {
        id: user_id,
        username: format!("notif_{tag}"),
        email: format!("{tag}@example.com"),
        avatar_url: None,
        password_hash: "placeholder".into(),
    })
    .await
    .expect("create account");

    UserRegisteredEvent {
        user_id: user_id.0,
        email: format!("{tag}@example.com"),
        invited_by: None,
        registered_at: 0,
    }
    .send(&mq)
    .await
    .expect("publish user registered");

    let mut created = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let row = sqlx::query_scalar!(
            r#"SELECT id FROM messaging.notification_settings WHERE id = $1"#,
            user_id.0
        )
        .fetch_optional(db.db())
        .await
        .expect("query notification settings");
        if row.is_some() {
            created = true;
            break;
        }
    }
    drop(channel);
    assert!(
        created,
        "consumer did not initialise notification settings within 10s"
    );
}
