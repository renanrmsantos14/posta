//! Núcleo do Posta: sincronização de email e cópia local pesquisável.

pub mod attachments;
pub mod folders;
pub mod imap_sync;
pub mod later;
pub mod oauth;
pub mod rules;
pub mod session;
pub mod setup;
pub mod search;
pub mod outbox;
pub mod send;
pub mod store;
pub mod threads;
pub mod vacation;

pub use attachments::Attachment;
pub use folders::{Folder, Role};
pub use imap_sync::{AuthMode, IdleEnd, ImapAccount, MsgFlag, Security, SmtpServer, SyncReport};
pub use session::{Hub, Kind, Live};
pub use outbox::{FlushReport, OpKind, OutboxItem, PendingOp};
pub use send::{OutAttachment, Outgoing, SendReport};
pub use store::{Contact, Cursor, Delta, DraftRecord, Page, SyncState, MessageDetail, MessageSummary, ReplyRef, Store, UserFolder, FolderUnread};

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

impl Error {
    /// A falha foi de rede (sem conexão, DNS, TLS, servidor fora do ar)? Quem recebe deve tentar de novo mais tarde.
    /// Recusa do servidor (NO/BAD, 5xx, endereço inválido) não é de rede: repetir daria o mesmo resultado.
    pub fn is_network(&self) -> bool {
        use imap::Error as I;
        match self {
            Error::Imap(e) => matches!(
                e,
                I::Io(_) | I::ConnectionLost | I::RustlsHandshake(_) | I::Bye(_)
            ),
            Error::Smtp(e) => !(e.is_permanent() || e.is_client() || e.is_response()),
            Error::Db(_) | Error::Email(_) | Error::Invalid(_) => false,
        }
    }
}
