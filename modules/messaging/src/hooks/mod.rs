//! Background reactors: the mailer and email-composing consumers.

pub mod invitation_accepted_email;
pub mod invitation_email;
pub mod login_email;
pub mod mailer;
pub mod notification_init;

pub use invitation_accepted_email::InvitationAcceptedEmailHook;
pub use invitation_email::InvitationEmailHook;
pub use login_email::LoginEmailHook;
pub use mailer::MailerHook;
pub use notification_init::NotificationInitHook;
