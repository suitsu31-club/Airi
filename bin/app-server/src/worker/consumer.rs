//! Consumer worker: binds every AMQP hook to its durable queue and parks.

use super::Deps;
use auth::events::{InvitationExpiryCleanupSignal, SessionCleanupSignal};
use auth::hooks::{AuthCronHook, BanHook, CreditHook, InvitationSlotHook};
use auth::services::session::SessionService;
use base::events::{
    AddInvitationSlotEvent, CreditChangeEvent, InvitationAcceptedEvent, InvitationSentEvent,
    SystemBanEvent, UserLoginEvent, UserRegisteredEvent,
};
use messaging::events::MailSendCall;
use messaging::hooks::{
    InvitationAcceptedEmailHook, InvitationEmailHook, LoginEmailHook, MailerHook,
    NotificationInitHook,
};
use std::sync::Arc;
use wakuwaku::amqp::{AmqpMessageProcessor, setup_consumer};

pub async fn run(deps: Deps) -> anyhow::Result<()> {
    let Deps { db, redis, mq } = deps;
    let session = SessionService {
        db: db.clone(),
        redis: redis.clone(),
    };
    let mut channels = Vec::new();

    // auth: external credit changes
    {
        let hook = CreditHook { db: db.clone() };
        let ch = <CreditHook as AmqpMessageProcessor<CreditChangeEvent>>::ensure_queue(&mq).await?;
        setup_consumer::<CreditChangeEvent, CreditHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // auth: external moderation bans
    {
        let hook = BanHook {
            db: db.clone(),
            session: session.clone(),
        };
        let ch = <BanHook as AmqpMessageProcessor<SystemBanEvent>>::ensure_queue(&mq).await?;
        setup_consumer::<SystemBanEvent, BanHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // auth: external invitation-slot grants
    {
        let hook = InvitationSlotHook { db: db.clone() };
        let ch = <InvitationSlotHook as AmqpMessageProcessor<AddInvitationSlotEvent>>::ensure_queue(
            &mq,
        )
        .await?;
        setup_consumer::<AddInvitationSlotEvent, InvitationSlotHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // auth: cron cleanup (two signals share one hook)
    {
        let hook = Arc::new(AuthCronHook { db: db.clone() });
        let ch =
            <AuthCronHook as AmqpMessageProcessor<SessionCleanupSignal>>::ensure_queue(&mq).await?;
        setup_consumer::<SessionCleanupSignal, AuthCronHook>(&ch, hook.clone()).await?;
        channels.push(ch);
        let ch =
            <AuthCronHook as AmqpMessageProcessor<InvitationExpiryCleanupSignal>>::ensure_queue(
                &mq,
            )
            .await?;
        setup_consumer::<InvitationExpiryCleanupSignal, AuthCronHook>(&ch, hook.clone()).await?;
        channels.push(ch);
    }

    // messaging: mailer sink
    {
        let hook = MailerHook::new(redis.clone()).await?;
        let ch = <MailerHook as AmqpMessageProcessor<MailSendCall>>::ensure_queue(&mq).await?;
        setup_consumer::<MailSendCall, MailerHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // messaging: invitation email composer
    {
        let hook = InvitationEmailHook {
            config_store: redis.clone(),
            mq: mq.clone(),
        };
        let ch =
            <InvitationEmailHook as AmqpMessageProcessor<InvitationSentEvent>>::ensure_queue(&mq)
                .await?;
        setup_consumer::<InvitationSentEvent, InvitationEmailHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // messaging: login email composer
    {
        let hook = LoginEmailHook {
            db: db.clone(),
            config_store: redis.clone(),
            mq: mq.clone(),
        };
        let ch =
            <LoginEmailHook as AmqpMessageProcessor<UserLoginEvent>>::ensure_queue(&mq).await?;
        setup_consumer::<UserLoginEvent, LoginEmailHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // messaging: notification settings initialiser
    {
        let hook = NotificationInitHook { db: db.clone() };
        let ch =
            <NotificationInitHook as AmqpMessageProcessor<UserRegisteredEvent>>::ensure_queue(&mq)
                .await?;
        setup_consumer::<UserRegisteredEvent, NotificationInitHook>(&ch, Arc::new(hook)).await?;
        channels.push(ch);
    }

    // messaging: invitation-accepted notifier (emails the inviter)
    {
        let hook = InvitationAcceptedEmailHook {
            db: db.clone(),
            config_store: redis.clone(),
            mq: mq.clone(),
        };
        let ch = <InvitationAcceptedEmailHook as AmqpMessageProcessor<
            InvitationAcceptedEvent,
        >>::ensure_queue(&mq)
        .await?;
        setup_consumer::<InvitationAcceptedEvent, InvitationAcceptedEmailHook>(&ch, Arc::new(hook))
            .await?;
        channels.push(ch);
    }

    tracing::info!(consumers = channels.len(), "consumers ready");
    // Retain the channels (dropping cancels the consumers) and park forever.
    std::future::pending::<()>().await;
    drop(channels);
    Ok(())
}
