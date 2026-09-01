//! Credential login and session issuance.

use crate::entities::db::account::{FindAccountByEmail, FindAccountByUsername};
use crate::entities::db::sessions::{SessionId, SessionSecurityOption};
use crate::entities::db::suspense::IsSuspended;
use crate::services::session::{CreateSession, SessionService};
use crate::utils::datetime::{now_primitive, to_unix};
use crate::utils::password::{Argon2PasswordAlgorithm, PasswordAlgorithm};
use base::events::UserLoginEvent;
use kanau::processor::Processor;
use wakuwaku::amqp::{AmqpMessageSend, AmqpPool};
use wakuwaku::sqlx::DatabaseProcessor;

/// Authenticates credentials and issues sessions.
#[derive(Clone)]
pub struct LoginService {
    pub db: DatabaseProcessor,
    pub mq: AmqpPool,
    pub alg: Argon2PasswordAlgorithm,
    pub session: SessionService,
}

/// Authenticate by email/username and password.
pub struct Login {
    pub identifier: String,
    pub password: String,
    pub ip: String,
    pub user_agent: String,
}

/// Outcome of [`Login`].
#[derive(Debug, Clone)]
pub enum LoginResult {
    Success(SessionId),
    WrongCredential,
    NotFound,
    Suspended,
}

impl Processor<Login> for LoginService {
    type Output = LoginResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:Login")]
    async fn process(&self, input: Login) -> Result<Self::Output, Self::Error> {
        let account = self
            .db
            .process(FindAccountByEmail {
                email: input.identifier.clone(),
            })
            .await?;
        let account = match account {
            Some(a) => Some(a),
            None => {
                self.db
                    .process(FindAccountByUsername {
                        username: input.identifier.clone(),
                    })
                    .await?
            }
        };

        let Some(account) = account else {
            // Constant-time dummy verification for a missing account.
            let _ = self.alg.verify_password_or_dummy(&input.password, None);
            return Ok(LoginResult::NotFound);
        };

        if !self
            .alg
            .verify_password(&input.password, &account.password_hash)
        {
            return Ok(LoginResult::WrongCredential);
        }

        if self
            .db
            .process(IsSuspended {
                account_id: account.id,
            })
            .await?
        {
            return Ok(LoginResult::Suspended);
        }

        let session_id = self
            .session
            .process(CreateSession {
                user_id: account.id,
                ip: input.ip.clone(),
                user_agent: input.user_agent.clone(),
                security_option: SessionSecurityOption::None,
            })
            .await?;

        UserLoginEvent {
            user_id: account.id.0,
            ip: input.ip,
            user_agent: input.user_agent,
            at: to_unix(now_primitive()),
        }
        .send(&self.mq)
        .await?;

        Ok(LoginResult::Success(session_id))
    }
}
