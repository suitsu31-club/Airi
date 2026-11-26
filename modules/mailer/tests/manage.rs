//! Sender management: credentials stay write-only and bound to their sender,
//! and key rotation can be rerun.
//!
//! Needs PostgreSQL (`DATABASE_URL`; each test gets a fresh database).

use base::Error;
use kanau::processor::Processor;
use mailer::entities::db::sender::{FindSmtpSenderById, SmtpSenderEntity, SmtpSenderId};
use mailer::services::manage::{
    CreateSender, KeyRotation, ListSenders, MailerManageService, RotateSecretKey, SenderSettings,
    SetSenderEnabled, SetSenderLogin, SmtpLogin, SmtpTlsMode, UpdateSender, VALID_SENDER_SETTINGS,
    VALID_SMTP_LOGIN,
};
use sqlx::PgPool;
use std::sync::Arc;
use type_safe_data::encryption::Key256;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use zeroize::Zeroizing;

fn settings(name: &str) -> SenderSettings {
    SenderSettings {
        name: name.to_owned(),
        address: format!("{name}@example.com").parse().unwrap(),
        host: "smtp.example.com".to_owned(),
        port: 587,
        tls_mode: SmtpTlsMode::StartTls,
        remark: String::new(),
    }
}

fn login(username: &str, password: &str) -> SmtpLogin {
    SmtpLogin {
        username: Zeroizing::new(username.to_owned()),
        password: Zeroizing::new(password.to_owned()),
    }
}

async fn create(service: &MailerManageService, name: &str, username: &str) -> SmtpSenderId {
    service
        .process(CreateSender {
            settings: VALID_SENDER_SETTINGS.prove(settings(name)).unwrap(),
            login: Some(VALID_SMTP_LOGIN.prove(login(username, "secret")).unwrap()),
            enabled: true,
        })
        .await
        .unwrap()
        .id
}

async fn row(db: &Db<PgSource, ReadWrite>, id: SmtpSenderId) -> SmtpSenderEntity {
    db.process(FindSmtpSenderById { id })
        .await
        .unwrap()
        .unwrap()
}

/// The username stored for `id`, decrypted with `key`; `None` if it doesn't
/// decrypt.
async fn stored_username(
    db: &Db<PgSource, ReadWrite>,
    id: SmtpSenderId,
    key: &Key256,
) -> Option<String> {
    row(db, id)
        .await
        .username_encrypted
        .unwrap()
        .decrypt(key, &id.username_associated_data())
        .ok()
        .map(|username| String::clone(&username))
}

#[sqlx::test(migrations = "../../migrations")]
async fn credentials_are_encrypted_for_their_sender_and_survive_other_changes(pool: PgPool) {
    let db: Db<PgSource, ReadWrite> = Db::new(PgSource::new(pool));
    let key = Key256::from([7; 32]);
    let service = MailerManageService::new(db.clone(), Some(Arc::new(key.clone())));

    let id = create(&service, "noreply", "relay-user").await;
    assert_eq!(
        stored_username(&db, id, &key).await.as_deref(),
        Some("relay-user")
    );
    // Bound to the sender: another sender's associated data fails.
    let stored = row(&db, id).await.username_encrypted.unwrap();
    assert!(
        stored
            .decrypt(&key, &SmtpSenderId::generate().username_associated_data())
            .is_err()
    );

    let renamed = service
        .process(UpdateSender {
            id,
            settings: VALID_SENDER_SETTINGS.prove(settings("no-reply")).unwrap(),
        })
        .await
        .unwrap();
    let disabled = service
        .process(SetSenderEnabled { id, enabled: false })
        .await
        .unwrap();
    assert_eq!(renamed.name, "no-reply");
    assert!(renamed.enabled && !disabled.enabled);
    assert!(disabled.has_credentials);
    assert_eq!(
        stored_username(&db, id, &key).await.as_deref(),
        Some("relay-user")
    );

    service
        .process(SetSenderLogin { id, login: None })
        .await
        .unwrap();
    let listed = service.process(ListSenders).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].has_credentials);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_taken_name_and_an_unknown_sender_are_reported(pool: PgPool) {
    let db: Db<PgSource, ReadWrite> = Db::new(PgSource::new(pool));
    let service = MailerManageService::new(db, Some(Arc::new(Key256::from([7; 32]))));
    create(&service, "noreply", "relay-user").await;

    let taken = service
        .process(CreateSender {
            settings: VALID_SENDER_SETTINGS.prove(settings("noreply")).unwrap(),
            login: None,
            enabled: true,
        })
        .await;
    assert!(matches!(taken, Err(Error::InvalidInput)), "{taken:?}");

    let unknown = service
        .process(SetSenderEnabled {
            id: SmtpSenderId::generate(),
            enabled: false,
        })
        .await;
    assert!(matches!(unknown, Err(Error::NotFound)), "{unknown:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn rotation_moves_every_credential_to_the_new_key_and_can_rerun(pool: PgPool) {
    let db: Db<PgSource, ReadWrite> = Db::new(PgSource::new(pool));
    let old = Key256::from([1; 32]);
    let new = Key256::from([2; 32]);
    let under_old = MailerManageService::new(db.clone(), Some(Arc::new(old.clone())));
    let under_new = MailerManageService::new(db.clone(), Some(Arc::new(new.clone())));

    let first = create(&under_old, "first", "first-user").await;
    // Written by a replica that already runs with the new key.
    let second = create(&under_new, "second", "second-user").await;
    under_old
        .process(CreateSender {
            settings: VALID_SENDER_SETTINGS.prove(settings("open-relay")).unwrap(),
            login: None,
            enabled: true,
        })
        .await
        .unwrap();

    let rotate = || RotateSecretKey {
        old: old.clone(),
        new: new.clone(),
    };
    assert_eq!(
        under_old.process(rotate()).await.unwrap(),
        KeyRotation {
            rotated: 1,
            already_rotated: 1,
            conflicted: 0
        }
    );
    assert_eq!(
        stored_username(&db, first, &new).await.as_deref(),
        Some("first-user")
    );
    assert_eq!(
        stored_username(&db, second, &new).await.as_deref(),
        Some("second-user")
    );

    assert_eq!(
        under_old.process(rotate()).await.unwrap(),
        KeyRotation {
            rotated: 0,
            already_rotated: 2,
            conflicted: 0
        }
    );

    // Credentials under neither key stop the run before it writes anything.
    let third = create(
        &MailerManageService::new(db.clone(), Some(Arc::new(Key256::from([3; 32])))),
        "third",
        "third-user",
    )
    .await;
    let before = row(&db, first).await.username_encrypted.unwrap();
    let stranded = under_old
        .process(RotateSecretKey {
            old: new.clone(),
            new: old.clone(),
        })
        .await;
    assert!(
        matches!(stranded, Err(Error::BusinessPanic(_))),
        "{stranded:?}"
    );
    assert_eq!(
        row(&db, first)
            .await
            .username_encrypted
            .unwrap()
            .ciphertext(),
        before.ciphertext()
    );
    assert_eq!(stored_username(&db, third, &new).await, None);
}
