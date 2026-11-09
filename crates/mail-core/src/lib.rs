//! Núcleo do Posta: sincronização de email e cópia local pesquisável.

pub mod folders;
pub mod imap_sync;
pub mod oauth;
pub mod send;
pub mod store;

pub use folders::{Folder, Role};
pub use imap_sync::{AuthMode, IdleEnd, ImapAccount, MsgFlag, Security, SmtpServer, SyncReport};
pub use send::{Outgoing, SendReport};
pub use store::{MessageDetail, MessageSummary, ReplyRef, Store};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IMAP: {0}")]
    Imap(#[from] imap::Error),
    #[error("banco local: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("SMTP: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),
    #[error("mensagem: {0}")]
    Email(#[from] lettre::error::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
