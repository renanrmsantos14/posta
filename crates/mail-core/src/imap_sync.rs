//! Sincronização IMAP → Store. Uma conexão por chamada; IDLE fica para a próxima etapa.

use imap::{ClientBuilder, ConnectionMode, Session};
use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::store::{NewMessage, Store};
use crate::Result;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// TLS direto (porta 993).
    Tls,
    /// STARTTLS (porta 143).
    StartTls,
    /// Sem criptografia. Só para servidor local de teste.
    Plaintext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapAccount {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    /// Nunca vai para disco nem para a UI: fica no cofre do sistema.
    #[serde(skip_serializing, default)]
    pub password: String,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct SyncReport {
    pub fetched: usize,
    pub total: usize,
}

/// Quantas mensagens completas buscar por comando FETCH.
const BATCH: usize = 50;

type ImapSession = Session<imap::Connection>;

fn connect(acc: &ImapAccount) -> Result<ImapSession> {
    let mode = match acc.security {
        Security::Tls => ConnectionMode::Tls,
        Security::StartTls => ConnectionMode::StartTls,
        Security::Plaintext => ConnectionMode::Plaintext,
    };
    let client = ClientBuilder::new(acc.host.as_str(), acc.port).mode(mode).connect()?;
    client.login(&acc.username, &acc.password).map_err(|(e, _)| e.into())
}

fn parse(uid: u32, raw: &[u8], seen: bool, fallback_date: i64) -> NewMessage {
    let msg = MessageParser::default().parse(raw);
    let from = msg.as_ref().and_then(|m| m.from()).and_then(|f| f.first());
    NewMessage {
        uid,
        from_name: from.and_then(|f| f.name()).unwrap_or_default().to_string(),
        from_addr: from.and_then(|f| f.address()).unwrap_or_default().to_string(),
        subject: msg.as_ref().and_then(|m| m.subject()).unwrap_or_default().to_string(),
        date: msg
            .as_ref()
            .and_then(|m| m.date())
            .map(|d| d.to_timestamp())
            .unwrap_or(fallback_date),
        body_text: msg
            .as_ref()
            .and_then(|m| m.body_text(0))
            .map(|b| b.into_owned())
            .unwrap_or_default(),
        seen,
    }
}

fn is_seen(fetch: &imap::types::Fetch) -> bool {
    fetch.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen))
}

/// Traz a pasta para o banco local: novas mensagens (as mais recentes primeiro),
/// exclusões feitas no servidor e mudanças de lida/não lida.
pub fn sync_mailbox(acc: &ImapAccount, store: &mut Store, mailbox: &str) -> Result<SyncReport> {
    let mut session = connect(acc)?;
    let selected = session.select(mailbox)?;
    store.set_uid_validity(&acc.id, mailbox, selected.uid_validity.unwrap_or(0))?;

    let mut server: Vec<(u32, bool)> = Vec::new();
    if selected.exists > 0 {
        for f in session.uid_fetch("1:*", "(UID FLAGS)")?.iter() {
            if let Some(uid) = f.uid {
                server.push((uid, is_seen(f)));
            }
        }
    }
    store.apply_server_state(&acc.id, mailbox, &server)?;

    let local: HashSet<u32> = store.local_uids(&acc.id, mailbox)?.into_iter().collect();
    let mut missing: Vec<u32> = server.iter().map(|(u, _)| *u).filter(|u| !local.contains(u)).collect();
    missing.sort_unstable_by(|a, b| b.cmp(a));

    let mut report = SyncReport { fetched: 0, total: server.len() };
    for chunk in missing.chunks(BATCH) {
        let set = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let fetches = session.uid_fetch(&set, "(UID FLAGS INTERNALDATE BODY.PEEK[])")?;
        let msgs: Vec<NewMessage> = fetches
            .iter()
            .filter_map(|f| {
                let date = f.internal_date().map(|d| d.timestamp()).unwrap_or(0);
                Some(parse(f.uid?, f.body()?, is_seen(f), date))
            })
            .collect();
        report.fetched += msgs.len();
        store.insert_messages(&acc.id, mailbox, &msgs)?;
    }
    session.logout().ok();
    Ok(report)
}

/// Marca como lida/não lida no servidor. A UI já atualizou o banco local antes.
pub fn set_seen(acc: &ImapAccount, mailbox: &str, uid: u32, seen: bool) -> Result<()> {
    let mut session = connect(acc)?;
    session.select(mailbox)?;
    let op = if seen { "+FLAGS.SILENT (\\Seen)" } else { "-FLAGS.SILENT (\\Seen)" };
    session.uid_store(uid.to_string(), op)?;
    session.logout().ok();
    Ok(())
}

/// Testa as credenciais sem sincronizar nada.
pub fn check_login(acc: &ImapAccount) -> Result<()> {
    connect(acc)?.logout().ok();
    Ok(())
}
