//! # `manage-tool`
//!
//! Command-line administration: migrations, config seeding/validation, and admin
//! account management.

use anyhow::{Context, bail};
use auth::config::AuthConfig;
use auth::entities::db::account::{AccountId, CreateAccount, CreateAccountResult};
use auth::entities::db::admin_view::ListAdminUsers;
use auth::entities::db::credit::CreateCreditRow;
use auth::entities::db::membership::{AdminRole, CreateMembership, SetAdminRole};
use auth::utils::password::{Argon2PasswordAlgorithm, PasswordAlgorithm};
use base::config::ConfigJson;
use base::config_provider::{insert_config_if_absent, refresh_config};
use clap::{Parser, Subcommand};
use kanau::processor::Processor;
use messaging::config::MessagingConfig;
use uuid::Uuid;
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

#[derive(Parser)]
#[command(name = "manage-tool", about = "Airi administration CLI")]
struct Cli {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    #[arg(long, env = "REDIS_URL")]
    redis_url: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run pending database migrations.
    Migrate,
    /// Seed default configuration rows and refresh the Redis cache.
    InitConfig,
    /// Validate a JSON config file against its typed schema.
    ConfigValidate {
        /// Config key: `auth` or `messaging`.
        #[arg(long)]
        key: String,
        /// Path to the JSON file.
        #[arg(long)]
        file: String,
    },
    /// Administrator account management.
    #[command(subcommand)]
    Admin(AdminAction),
}

#[derive(Subcommand)]
enum AdminAction {
    /// Create an administrator account.
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
        /// Role: site_owner | maintainer | moderator | assistant.
        #[arg(long)]
        role: String,
    },
    /// Set (or clear) a user's role.
    SetRole {
        #[arg(long)]
        id: String,
        /// Role, or `none` to clear.
        #[arg(long)]
        role: String,
    },
    /// List users.
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = Cli::parse();

    let pool = base::db::connect_pool(&cli.database_url)
        .await
        .context("connecting to postgres")?;

    match cli.command {
        Commands::Migrate => {
            sqlx::migrate!("../../migrations")
                .run(&pool)
                .await
                .context("running migrations")?;
            println!("migrations applied");
        }
        Commands::InitConfig => {
            let mut redis = open_redis(&cli.redis_url).await?;
            seed_config::<AuthConfig>(&pool, &mut redis).await?;
            seed_config::<MessagingConfig>(&pool, &mut redis).await?;
            println!("config seeded and cache refreshed");
        }
        Commands::ConfigValidate { key, file } => {
            let text = std::fs::read_to_string(&file).context("reading config file")?;
            match key.as_str() {
                "auth" => {
                    let _: AuthConfig =
                        serde_json::from_str(&text).context("invalid auth config")?;
                }
                "messaging" => {
                    let _: MessagingConfig =
                        serde_json::from_str(&text).context("invalid messaging config")?;
                }
                other => bail!("unknown config key {other:?}; expected auth|messaging"),
            }
            println!("config for {key:?} is valid");
        }
        Commands::Admin(action) => run_admin(&pool, action).await?,
    }
    Ok(())
}

async fn open_redis(url: &str) -> anyhow::Result<RedisConnection> {
    let client = redis::Client::open(url).context("opening redis client")?;
    client
        .get_multiplexed_async_connection()
        .await
        .context("connecting to redis")
}

async fn seed_config<T: ConfigJson>(
    pool: &sqlx::PgPool,
    redis: &mut RedisConnection,
) -> anyhow::Result<()> {
    let inserted = insert_config_if_absent::<T>(pool, &T::default()).await?;
    refresh_config::<T>(pool, redis).await?;
    println!(
        "config {:?}: {}",
        T::KEY,
        if inserted { "inserted" } else { "already present" }
    );
    Ok(())
}

fn parse_role(role: &str) -> anyhow::Result<AdminRole> {
    AdminRole::parse(role).with_context(|| format!("invalid role {role:?}"))
}

async fn run_admin(pool: &sqlx::PgPool, action: AdminAction) -> anyhow::Result<()> {
    let db = DatabaseProcessor::from_pool(pool.clone());
    match action {
        AdminAction::Create {
            email,
            username,
            password,
            role,
        } => {
            let role = parse_role(&role)?;
            let alg = Argon2PasswordAlgorithm;
            let password_hash = alg
                .hash_password(&password)
                .map_err(|e| anyhow::anyhow!("hashing password: {e}"))?;
            let id = AccountId(Uuid::new_v4());
            match db
                .process(CreateAccount {
                    id,
                    username,
                    email,
                    avatar_url: None,
                    password_hash,
                })
                .await?
            {
                CreateAccountResult::Success => {}
                CreateAccountResult::EmailTaken => bail!("email already taken"),
                CreateAccountResult::UsernameTaken => bail!("username already taken"),
            }
            db.process(CreateMembership {
                account: id,
                level: 0,
                admin_privilege: Some(role),
                invited_by: None,
                available_invitation_count: 0,
            })
            .await?;
            db.process(CreateCreditRow { account: id }).await?;
            println!("created admin {} with role {}", id.0, role.as_str());
        }
        AdminAction::SetRole { id, role } => {
            let account = AccountId(Uuid::parse_str(&id).context("invalid user id")?);
            let role = if role.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(parse_role(&role)?)
            };
            db.process(SetAdminRole { account, role }).await?;
            println!("role updated for {}", account.0);
        }
        AdminAction::List => {
            let users = db
                .process(ListAdminUsers {
                    limit: 100,
                    offset: 0,
                })
                .await?;
            for u in users {
                println!(
                    "{}\t{}\t{}\trole={:?}\tsuspended={}",
                    u.id.0,
                    u.username,
                    u.email,
                    u.admin_role.map(|r| r.as_str()),
                    u.suspended
                );
            }
        }
    }
    Ok(())
}
