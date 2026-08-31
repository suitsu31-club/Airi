//! End-to-end smoke test exercising the auth services, session-auth middleware,
//! and the `/api/me` OpenAPI surface against live Postgres/Redis/RabbitMQ.
//!
//! Requires `DATABASE_URL`, `REDIS_URL`, and `MQ_URL` in the environment.

use auth::config::AuthConfig;
use auth::entities::db::api_key::ApiKey;
use auth::entities::db::membership::{AdminRole, SetAdminRole};
use auth::entities::db::sessions::SessionId;
use auth::hooks::CreditHook;
use auth::openapi::{MeState, me_router};
use auth::rpc::middleware::{UserAuthLayer, UserId};
use auth::services::account::{
    AccountService, ChangePassword, ChangePasswordResult, Register, RegisterResult,
};
use auth::services::admin::{AdminService, BanUser, ListUsers};
use auth::services::api_key::{ApiKeyService, CreateApiKey};
use auth::services::login::{Login, LoginResult, LoginService};
use auth::services::profile::{GetMyCredit, GetMyProfile, ProfileService};
use auth::services::session::SessionService;
use auth::utils::identity::{ApiKeyVerify, IdentityVerifier, SessionIdVerify};
use auth::utils::password::Argon2PasswordAlgorithm;
use base::config_provider::{refresh_config, upsert_config};
use base::events::{CreditChangeEvent, InvitationSentEvent, UserLoginEvent, UserRegisteredEvent};
use kanau::processor::Processor;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::{Layer, Service};
use wakuwaku::amqp::{AmqpPool, AmqpRouting};
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

async fn deps() -> (DatabaseProcessor, RedisConnection, AmqpPool) {
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL");
    let mq_url = std::env::var("MQ_URL").expect("MQ_URL");
    let pool = base::db::connect_pool(&db_url)
        .await
        .expect("connect postgres");
    let db = DatabaseProcessor::from_pool(pool);
    let client = redis::Client::open(redis_url).expect("open redis");
    let redis = client
        .get_multiplexed_async_connection()
        .await
        .expect("connect redis");
    let args =
        amqprs::connection::OpenConnectionArguments::try_from(mq_url.as_str()).expect("mq url");
    let conn = amqprs::connection::Connection::open(&args)
        .await
        .expect("open amqp");
    let mq = AmqpPool::connect(conn).await;
    (db, redis, mq)
}

#[derive(Clone)]
struct RecordingService {
    seen: Arc<Mutex<Option<uuid::Uuid>>>,
}

impl Service<http::Request<axum::body::Body>> for RecordingService {
    type Response = http::Response<axum::body::Body>;
    type Error = std::convert::Infallible;
    type Future =
        std::pin::Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<axum::body::Body>) -> Self::Future {
        let seen = self.seen.clone();
        let uid = req.extensions().get::<UserId>().map(|u| u.0);
        Box::pin(async move {
            *seen.lock().expect("lock") = uid;
            Ok(http::Response::new(axum::body::Body::empty()))
        })
    }
}

async fn http_get(addr: std::net::SocketAddr, path: &str, api_key: Option<&str>) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let key_header = match api_key {
        Some(k) => format!("X-API-Key: {k}\r\n"),
        None => String::new(),
    };
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{key_header}Connection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_end_to_end() {
    let (db, redis, mq) = deps().await;

    // Declare event exchanges so registration/login publishes succeed.
    UserRegisteredEvent::ensure_exchange(&mq).await.expect("ex1");
    UserLoginEvent::ensure_exchange(&mq).await.expect("ex2");
    InvitationSentEvent::ensure_exchange(&mq).await.expect("ex3");

    // Open registration.
    let cfg = AuthConfig {
        registration_open: true,
        default_invitation_count: 3,
        ..AuthConfig::default()
    };
    upsert_config(db.db(), &cfg).await.expect("upsert config");
    refresh_config::<AuthConfig>(db.db(), &mut redis.clone())
        .await
        .expect("refresh config");

    let alg = Argon2PasswordAlgorithm;
    let session = SessionService {
        db: db.clone(),
        redis: redis.clone(),
    };
    let account = AccountService {
        db: db.clone(),
        redis: redis.clone(),
        mq: mq.clone(),
        alg: alg.clone(),
    };
    let login = LoginService {
        db: db.clone(),
        mq: mq.clone(),
        alg: alg.clone(),
        session: session.clone(),
    };
    let api_key = ApiKeyService { db: db.clone() };
    let profile = ProfileService { db: db.clone() };
    let admin = AdminService {
        db: db.clone(),
        redis: redis.clone(),
        session: session.clone(),
    };
    let verifier = IdentityVerifier {
        db: db.clone(),
        redis: redis.clone(),
    };

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("user_{tag}");
    let email = format!("{tag}@example.com");

    // Register.
    let user_id = match account
        .process(Register {
            username: username.clone(),
            email: email.clone(),
            password: "pw-correct-1".into(),
            invite_token: None,
        })
        .await
        .expect("register")
    {
        RegisterResult::Success { user_id } => user_id,
        other => panic!("expected register success, got {other:?}"),
    };

    // Duplicate email / username.
    assert!(matches!(
        account
            .process(Register {
                username: format!("other_{tag}"),
                email: email.clone(),
                password: "x".into(),
                invite_token: None,
            })
            .await
            .expect("dup email"),
        RegisterResult::EmailTaken
    ));
    assert!(matches!(
        account
            .process(Register {
                username: username.clone(),
                email: format!("o{tag}@example.com"),
                password: "x".into(),
                invite_token: None,
            })
            .await
            .expect("dup username"),
        RegisterResult::UsernameTaken
    ));

    // Login: wrong password, unknown identifier, correct.
    assert!(matches!(
        login
            .process(Login {
                identifier: email.clone(),
                password: "wrong".into(),
                ip: "127.0.0.1".into(),
                user_agent: "test".into(),
            })
            .await
            .expect("login wrong"),
        LoginResult::WrongCredential
    ));
    assert!(matches!(
        login
            .process(Login {
                identifier: "nobody@nowhere".into(),
                password: "x".into(),
                ip: "127.0.0.1".into(),
                user_agent: "test".into(),
            })
            .await
            .expect("login unknown"),
        LoginResult::NotFound
    ));
    let session_id = match login
        .process(Login {
            identifier: email.clone(),
            password: "pw-correct-1".into(),
            ip: "127.0.0.1".into(),
            user_agent: "test".into(),
        })
        .await
        .expect("login ok")
    {
        LoginResult::Success(s) => s,
        other => panic!("expected login success, got {other:?}"),
    };

    // Session verification (valid + bogus).
    let verified = verifier
        .process(SessionIdVerify {
            session_id: session_id.clone(),
            ip: "127.0.0.1".into(),
            user_agent: "test".into(),
        })
        .await
        .expect("verify session");
    assert_eq!(verified.user, user_id);
    assert!(
        verifier
            .process(SessionIdVerify {
                session_id: SessionId("not-a-real-session".into()),
                ip: "127.0.0.1".into(),
                user_agent: "test".into(),
            })
            .await
            .is_err()
    );

    // Profile.
    let p = profile.process(GetMyProfile { user_id }).await.expect("profile");
    assert_eq!(p.account.username, username);

    // API key round-trip.
    let created = api_key
        .process(CreateApiKey {
            user_id,
            remark: "test".into(),
            valid_until: None,
            scopes: vec!["read".into()],
        })
        .await
        .expect("create key");
    let key_verified = verifier
        .process(ApiKeyVerify {
            api_key: ApiKey(created.plaintext.clone()),
        })
        .await
        .expect("verify key");
    assert_eq!(key_verified.user, user_id);
    assert_eq!(key_verified.scopes, vec!["read".to_string()]);

    // Credit hook applies a delta.
    CreditHook { db: db.clone() }
        .process(CreditChangeEvent {
            user_id: user_id.0,
            available_delta: "10.5".into(),
            frozen_delta: "0".into(),
            reason: "test top-up".into(),
        })
        .await
        .expect("credit hook");
    let credit = profile.process(GetMyCredit { user_id }).await.expect("credit");
    assert_eq!(credit.total_amount.to_string(), "10.5");

    // Change password.
    assert!(matches!(
        account
            .process(ChangePassword {
                user_id,
                old_password: "pw-correct-1".into(),
                new_password: "pw-correct-2".into(),
            })
            .await
            .expect("change pw"),
        ChangePasswordResult::Success
    ));
    assert!(matches!(
        account
            .process(ChangePassword {
                user_id,
                old_password: "still-wrong".into(),
                new_password: "x".into(),
            })
            .await
            .expect("change pw wrong"),
        ChangePasswordResult::WrongPassword
    ));

    // Admin: promote a second account, list users, ban the first.
    let admin_tag = uuid::Uuid::new_v4().simple().to_string();
    let admin_email = format!("{admin_tag}@example.com");
    let admin_id = match account
        .process(Register {
            username: format!("admin_{admin_tag}"),
            email: admin_email.clone(),
            password: "adminpw".into(),
            invite_token: None,
        })
        .await
        .expect("register admin")
    {
        RegisterResult::Success { user_id } => user_id,
        other => panic!("expected admin register success, got {other:?}"),
    };
    db.process(SetAdminRole {
        account: admin_id,
        role: Some(AdminRole::SiteOwner),
    })
    .await
    .expect("promote");
    let users = admin
        .process(ListUsers {
            actor: admin_id,
            limit: 10,
            offset: 0,
        })
        .await
        .expect("list users");
    assert!(!users.is_empty());
    admin
        .process(BanUser {
            actor: admin_id,
            target: user_id,
            reason: "spam".into(),
        })
        .await
        .expect("ban");
    assert!(matches!(
        login
            .process(Login {
                identifier: email.clone(),
                password: "pw-correct-2".into(),
                ip: "127.0.0.1".into(),
                user_agent: "test".into(),
            })
            .await
            .expect("login banned"),
        LoginResult::Suspended
    ));

    // Session-auth middleware injects UserId for a valid session.
    let admin_session = match login
        .process(Login {
            identifier: admin_email.clone(),
            password: "adminpw".into(),
            ip: "127.0.0.1".into(),
            user_agent: "test".into(),
        })
        .await
        .expect("admin login")
    {
        LoginResult::Success(s) => s,
        other => panic!("expected admin login success, got {other:?}"),
    };
    let seen = Arc::new(Mutex::new(None));
    let mut mw = UserAuthLayer {
        verifier: verifier.clone(),
    }
    .layer(RecordingService { seen: seen.clone() });
    let req = http::Request::builder()
        .uri("/")
        .header("x-session-id", admin_session.0.clone())
        .body(axum::body::Body::empty())
        .expect("build req");
    std::future::poll_fn(|cx| mw.poll_ready(cx))
        .await
        .expect("poll_ready");
    let _ = mw.call(req).await.expect("mw call");
    assert_eq!(*seen.lock().expect("lock"), Some(admin_id.0));

    // OpenAPI /api/me over real HTTP.
    let me_state = MeState {
        verifier: verifier.clone(),
        profile: profile.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, me_router(me_state)).await;
    });
    let admin_key = api_key
        .process(CreateApiKey {
            user_id: admin_id,
            remark: "portal".into(),
            valid_until: None,
            scopes: vec![],
        })
        .await
        .expect("admin key");
    let ok_resp = http_get(addr, "/api/me", Some(&admin_key.plaintext)).await;
    assert!(ok_resp.contains("200 OK"), "unexpected: {ok_resp}");
    assert!(ok_resp.contains("\"profile\""), "missing profile: {ok_resp}");
    let unauth = http_get(addr, "/api/me", None).await;
    assert!(unauth.contains("401"), "expected 401: {unauth}");
    server.abort();
}
