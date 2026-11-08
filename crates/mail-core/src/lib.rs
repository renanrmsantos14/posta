//! Núcleo do Posta: sincronização de email e cópia local pesquisável.

pub mod imap_sync;
pub mod store;

pub use imap_sync::{ImapAccount, Security, SyncReport};
pub use store::{MessageDetail, MessageSummary, Store};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IMAP: {0}")]
    Imap(#[from] imap::Error),
    #[error("banco local: {0}")]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
