//! In-process consumer test: binds `CreditHook` to RabbitMQ, publishes a
//! `CreditChangeEvent`, and verifies it is applied to `auth.credit`.
//!
//! Requires `DATABASE_URL` and `MQ_URL`.

use auth::entities::db::account::{AccountId, CreateAccount, CreateAccountResult};
use auth::entities::db::credit::{CreateCreditRow, FindCreditByAccount};
use auth::hooks::CreditHook;
use base::events::CreditChangeEvent;
use kanau::processor::Processor;
use std::sync::Arc;
use std::time::Duration;
use wakuwaku::amqp::{AmqpMessageProcessor, AmqpMessageSend, AmqpPool, setup_consumer};
use wakuwaku::sqlx::DatabaseProcessor;

async fn deps() -> (DatabaseProcessor, AmqpPool) {
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let mq_url = std::env::var("MQ_URL").expect("MQ_URL");
    let pool = base::db::connect_pool(&db_url).await.expect("connect postgres");
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
async fn consumer_applies_credit_change() {
    let (db, mq) = deps().await;

    // Bind the credit hook in-process (also declares the exchange + queue).
    let channel = <CreditHook as AmqpMessageProcessor<CreditChangeEvent>>::ensure_queue(&mq)
        .await
        .expect("ensure queue");
    setup_consumer::<CreditChangeEvent, CreditHook>(&channel, Arc::new(CreditHook { db: db.clone() }))
        .await
        .expect("setup consumer");

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let user_id = AccountId(uuid::Uuid::new_v4());
    assert!(matches!(
        db.process(CreateAccount {
            id: user_id,
            username: format!("cons_{tag}"),
            email: format!("{tag}@example.com"),
            avatar_url: None,
            password_hash: "placeholder".into(),
        })
        .await
        .expect("create account"),
        CreateAccountResult::Success
    ));
    db.process(CreateCreditRow { account: user_id })
        .await
        .expect("credit row");

    CreditChangeEvent {
        user_id: user_id.0,
        available_delta: "7".into(),
        frozen_delta: "0".into(),
        reason: "consumer test".into(),
    }
    .send(&mq)
    .await
    .expect("publish credit change");

    let mut applied = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Some(credit) = db
            .process(FindCreditByAccount { account: user_id })
            .await
            .expect("find credit")
            && credit.total_amount.to_string() == "7"
        {
            applied = true;
            break;
        }
    }
    // Keep the channel alive until the assertion (dropping cancels the consumer).
    drop(channel);
    assert!(applied, "consumer did not apply the credit change within 10s");
}
