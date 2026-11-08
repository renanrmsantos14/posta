//! Sincronização IMAP → Store. Uma conexão por chamada; IDLE fica para a próxima etapa.

use imap::{ClientBuilder, ConnectionMode, Session};
use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use imap_proto::NameAttribute;

use crate::folders::{archive_name, pick, Folder, Listed, Role};
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
    /// Servidor de envio. Contas antigas sem ele usam o padrão do provedor.
    #[serde(default)]
    pub smtp: Option<SmtpServer>,
    /// Nunca vai para disco nem para a UI: fica no cofre do sistema.
    #[serde(skip_serializing, default)]
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmtpServer {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

impl ImapAccount {
    /// SMTP configurado, ou o palpite usual: imap.x → smtp.x na porta 465.
    pub fn smtp(&self) -> SmtpServer {
        self.smtp.clone().unwrap_or_else(|| SmtpServer {
            host: match self.host.strip_prefix("imap.") {
                Some(rest) => format!("smtp.{rest}"),
                None => self.host.clone(),
            },
            port: 465,
            security: Security::Tls,
        })
    }
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct SyncReport {
    pub fetched: usize,
    pub total: usize,
}

/// Quantas mensagens completas buscar por comando FETCH.
const BATCH: usize = 50;

pub(crate) type ImapSession = Session<imap::Connection>;

pub(crate) fn connect(acc: &ImapAccount) -> Result<ImapSession> {
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
    let from_addr = from.and_then(|f| f.address()).unwrap_or_default().to_string();
    let reply_addr = msg
        .as_ref()
        .and_then(|m| m.reply_to())
        .and_then(|r| r.first())
        .and_then(|r| r.address())
        .map(str::to_string)
        .unwrap_or_else(|| from_addr.clone());
    NewMessage {
        uid,
        from_name: from.and_then(|f| f.name()).unwrap_or_default().to_string(),
        from_addr,
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
        message_id: msg.as_ref().and_then(|m| m.message_id()).unwrap_or_default().to_string(),
        refs: msg
            .as_ref()
            .and_then(|m| m.references())
            .map(|r| r.iter().collect::<Vec<_>>().join(" "))
            .unwrap_or_default(),
        reply_addr,
    }
}

fn is_seen(fetch: &imap::types::Fetch) -> bool {
    fetch.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen))
}

/// Traz a pasta para o banco local: novas mensagens (as mais recentes primeiro),
/// exclusões feitas no servidor e mudanças de lida/não lida.
/// `Some(n)`: só baixa o corpo das n mais recentes do servidor; as locais mais antigas ficam.
pub fn sync_mailbox(acc: &ImapAccount, store: &mut Store, mailbox: &str, keep: Option<usize>) -> Result<SyncReport> {
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
    let mut newest: Vec<u32> = server.iter().map(|(u, _)| *u).collect();
    newest.sort_unstable_by(|a, b| b.cmp(a));
    if let Some(n) = keep {
        newest.truncate(n);
    }
    let missing: Vec<u32> = newest.into_iter().filter(|u| !local.contains(u)).collect();

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

fn special(attrs: &[NameAttribute]) -> Option<&'static str> {
    attrs.iter().find_map(|a| match a {
        NameAttribute::Sent => Some("sent"),
        NameAttribute::Archive => Some("archive"),
        NameAttribute::All => Some("all"),
        NameAttribute::Trash => Some("trash"),
        NameAttribute::Junk => Some("junk"),
        _ => None,
    })
}

/// Pastas do servidor e o delimitador da hierarquia (o da INBOX).
pub(crate) fn list_folders(session: &mut ImapSession) -> Result<(Vec<Listed>, Option<String>)> {
    let names = session.list(None, Some("*"))?;
    let delimiter = names
        .iter()
        .find(|n| n.name().eq_ignore_ascii_case("INBOX"))
        .and_then(|n| n.delimiter())
        .map(str::to_string);
    let listed = names
        .iter()
        .filter(|n| !n.attributes().contains(&NameAttribute::NoSelect))
        .map(|n| Listed { name: n.name().to_string(), special: special(n.attributes()) })
        .collect();
    Ok((listed, delimiter))
}

/// Descobre as pastas padrão. Sem Arquivo, cria e assina um.
pub fn discover_folders(acc: &ImapAccount) -> Result<Vec<Folder>> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    let mut folders = pick(&listed);
    if !folders.iter().any(|f| f.role == Role::Archive) {
        let name = archive_name(&listed, delimiter.as_deref());
        session.create(&name)?;
        session.subscribe(&name).ok();
        folders.push(Folder { role: Role::Archive, name });
        folders.sort_by_key(|f| Role::ALL.iter().position(|r| *r == f.role));
    }
    session.logout().ok();
    Ok(folders)
}

/// Move pelo UID. Sem MOVE no servidor: copia, marca como apagada e remove.
pub fn move_message(acc: &ImapAccount, from: &str, uid: u32, to: &str) -> Result<()> {
    let mut session = connect(acc)?;
    let caps = session.capabilities()?;
    let (has_move, uidplus) = (caps.has_str("MOVE"), caps.has_str("UIDPLUS"));
    drop(caps);
    session.select(from)?;
    let id = uid.to_string();
    if has_move {
        session.uid_mv(&id, to)?;
    } else {
        session.uid_copy(&id, to)?;
        session.uid_store(&id, "+FLAGS.SILENT (\\Deleted)")?;
        if uidplus { session.uid_expunge(&id)?; } else { session.expunge()?; }
    }
    session.logout().ok();
    Ok(())
}

/// Apaga de vez (usado dentro da Lixeira).
pub fn delete_forever(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<()> {
    let mut session = connect(acc)?;
    let uidplus = session.capabilities()?.has_str("UIDPLUS");
    session.select(mailbox)?;
    let id = uid.to_string();
    session.uid_store(&id, "+FLAGS.SILENT (\\Deleted)")?;
    if uidplus { session.uid_expunge(&id)?; } else { session.expunge()?; }
    session.logout().ok();
    Ok(())
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
