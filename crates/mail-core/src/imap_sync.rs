//! Sincronização IMAP → Store. Uma conexão por chamada; `idle_wait` espera novidades (IMAP IDLE).

use imap::{ClientBuilder, ConnectionMode, Session};
use mail_parser::{MessageParser, PartKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

use imap_proto::NameAttribute;

use crate::attachments;
use crate::oauth::{Provider, XOauth2};
use crate::folders::{archive_name, is_system, new_name, pick, renamed, user_folders, with_children, Folder, Listed, Role};
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

/// Como a conta se autentica. Contas salvas antes do OAuth não têm o campo: valem `Password`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    Password,
    Google,
    Microsoft,
}

impl AuthMode {
    pub fn provider(self) -> Option<Provider> {
        match self {
            AuthMode::Password => None,
            AuthMode::Google => Some(Provider::Google),
            AuthMode::Microsoft => Some(Provider::Microsoft),
        }
    }
}

impl From<Provider> for AuthMode {
    fn from(p: Provider) -> Self {
        match p {
            Provider::Google => AuthMode::Google,
            Provider::Microsoft => AuthMode::Microsoft,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapAccount {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    /// Nome de exibição no De: dos e-mails enviados.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Servidor de envio. Contas antigas sem ele usam o padrão do provedor.
    #[serde(default)]
    pub smtp: Option<SmtpServer>,
    #[serde(default)]
    pub auth: AuthMode,
    /// Senha (modo `Password`) ou access token OAuth já renovado (demais modos).
    /// Nunca vai para disco nem para a UI: senha e refresh token ficam no cofre do sistema.
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
    match acc.auth {
        AuthMode::Password => client.login(&acc.username, &acc.password),
        _ => client.authenticate("XOAUTH2", &XOauth2::new(&acc.username, &acc.password)),
    }
    .map_err(|(e, _)| e.into())
}

/// Como terminou a espera de `idle_wait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleEnd {
    /// O servidor avisou de novidade na caixa (chegada, remoção ou mudança de flag).
    Changed,
    /// Nada mudou dentro do prazo; o servidor pode derrubar o IDLE se passar de ~29 min.
    TimedOut,
    /// O servidor não anuncia IDLE: quem chama deve fazer polling.
    Unsupported,
}

/// Bloqueia até o servidor avisar de mudança em `mailbox` ou até `timeout`. Abre e fecha a própria conexão.
pub fn idle_wait(acc: &ImapAccount, mailbox: &str, timeout: Duration) -> Result<IdleEnd> {
    let mut session = connect(acc)?;
    if !session.capabilities()?.has_str("IDLE") {
        return Ok(IdleEnd::Unsupported);
    }
    session.select(mailbox)?;
    let outcome = session.idle().timeout(timeout).keepalive(false).wait_while(imap::extensions::idle::stop_on_any)?;
    Ok(match outcome {
        imap::extensions::idle::WaitOutcome::MailboxChanged => IdleEnd::Changed,
        imap::extensions::idle::WaitOutcome::TimedOut => IdleEnd::TimedOut,
    })
}

fn parse(uid: u32, raw: &[u8], seen: bool, flagged: bool, fallback_date: i64, own_addr: &str) -> NewMessage {
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
    // Mensagem enviada por mim: responder vai para o destinatário, não para mim.
    let reply_addr = if from_addr.eq_ignore_ascii_case(own_addr) {
        msg.as_ref().and_then(first_to).unwrap_or(reply_addr)
    } else {
        reply_addr
    };
    let html = msg
        .as_ref()
        .map(|m| {
            // `html_body` também traz texto puro convertido; só interessa HTML de verdade.
            let html = m
                .html_body()
                .filter(|p| matches!(p.kind(), PartKind::Html))
                .filter_map(|p| p.text())
                .collect::<Vec<_>>()
                .join("\n");
            attachments::inline_cids(m, html)
        })
        .unwrap_or_default();
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
        body_html: html.clone(),
        attachments: msg.as_ref().map(|m| attachments::list(m, &html)).unwrap_or_default(),
        seen,
        flagged,
        message_id: msg.as_ref().and_then(|m| m.message_id()).unwrap_or_default().to_string(),
        refs: msg
            .as_ref()
            .and_then(|m| m.references())
            .map(|r| r.iter().collect::<Vec<_>>().join(" "))
            .unwrap_or_default(),
        reply_addr,
        to_addrs: msg.as_ref().map(|m| addresses(m.to())).unwrap_or_default(),
        cc_addrs: msg.as_ref().map(|m| addresses(m.cc())).unwrap_or_default(),
    }
}

/// Endereços de um cabeçalho To/Cc, separados por vírgula.
fn addresses(list: Option<mail_parser::AddressList>) -> String {
    list.map(|a| a.mailboxes().filter_map(|x| x.address()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

fn first_to(msg: &mail_parser::Message) -> Option<String> {
    msg.to()?.first()?.address().map(str::to_string)
}

fn is_seen(fetch: &imap::types::Fetch) -> bool {
    fetch.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen))
}

fn is_flagged(fetch: &imap::types::Fetch) -> bool {
    fetch.flags().iter().any(|f| matches!(f, imap::types::Flag::Flagged))
}

/// Traz a pasta para o banco local: novas mensagens (as mais recentes primeiro),
/// exclusões feitas no servidor e mudanças de lida/não lida.
/// `Some(n)`: só baixa o corpo das n mais recentes do servidor; as locais mais antigas ficam.
pub fn sync_mailbox(acc: &ImapAccount, store: &mut Store, mailbox: &str, keep: Option<usize>) -> Result<SyncReport> {
    let mut session = connect(acc)?;
    let selected = session.select(mailbox)?;
    store.set_uid_validity(&acc.id, mailbox, selected.uid_validity.unwrap_or(0))?;

    let mut server: Vec<(u32, bool, bool)> = Vec::new();
    if selected.exists > 0 {
        for f in session.uid_fetch("1:*", "(UID FLAGS)")?.iter() {
            if let Some(uid) = f.uid {
                server.push((uid, is_seen(f), is_flagged(f)));
            }
        }
    }
    store.apply_server_state(&acc.id, mailbox, &server)?;

    let local: HashSet<u32> = store.local_uids(&acc.id, mailbox)?.into_iter().collect();
    let mut newest: Vec<u32> = server.iter().map(|(u, _, _)| *u).collect();
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
                Some(parse(f.uid?, f.body()?, is_seen(f), is_flagged(f), date, &acc.username))
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
        NameAttribute::Drafts => Some("drafts"),
        NameAttribute::Flagged => Some("flagged"),
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

/// Descobre as pastas padrão. Sem Arquivo, tenta criar e assinar um; se o servidor recusar, segue sem Arquivo.
pub fn discover_folders(acc: &ImapAccount) -> Result<Vec<Folder>> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    let mut folders = pick(&listed);
    if !folders.iter().any(|f| f.role == Role::Archive) {
        let name = archive_name(&listed, delimiter.as_deref());
        // Servidor que não deixa criar pasta não pode esconder Enviados/Lixeira já descobertos.
        if session.create(&name).is_ok() {
            session.subscribe(&name).ok();
            folders.push(Folder { role: Role::Archive, name });
            folders.sort_by_key(|f| Role::ALL.iter().position(|r| *r == f.role));
        }
    }
    session.logout().ok();
    Ok(folders)
}

/// Pastas criadas pelo usuário (sem as de sistema) e o separador da hierarquia.
pub fn list_user_folders(acc: &ImapAccount) -> Result<(Vec<String>, Option<String>)> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    session.logout().ok();
    Ok((user_folders(&listed, &pick(&listed)), delimiter))
}

/// Cria a pasta (ou subpasta, com `parent`) e a assina. Devolve o nome completo.
pub fn create_folder(acc: &ImapAccount, parent: Option<&str>, leaf: &str) -> Result<String> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    let name = new_name(&listed, delimiter.as_deref(), parent, leaf)?;
    session.create(&name)?;
    session.subscribe(&name).ok();
    session.logout().ok();
    Ok(name)
}

/// Renomeia (a subárvore vai junto, no servidor). Devolve o novo nome completo.
pub fn rename_folder(acc: &ImapAccount, old: &str, leaf: &str) -> Result<String> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    if is_system(old, &pick(&listed)) {
        return Err(crate::Error::Invalid("pastas do sistema não podem ser renomeadas".into()));
    }
    let name = renamed(&listed, delimiter.as_deref(), old, leaf)?;
    session.rename(old, &name)?;
    session.unsubscribe(old).ok();
    session.subscribe(&name).ok();
    session.logout().ok();
    Ok(name)
}

/// Exclui a pasta e as subpastas, da mais funda para a mais rasa. Devolve os nomes excluídos.
pub fn delete_folder(acc: &ImapAccount, name: &str) -> Result<Vec<String>> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    let system = pick(&listed);
    if is_system(name, &system) {
        return Err(crate::Error::Invalid("pastas do sistema não podem ser excluídas".into()));
    }
    let doomed = with_children(&listed, delimiter.as_deref(), name);
    if doomed.is_empty() {
        return Err(crate::Error::Invalid(format!("a pasta {name} não existe")));
    }
    for n in &doomed {
        session.unsubscribe(n).ok();
        session.delete(n)?;
    }
    session.logout().ok();
    Ok(doomed)
}

/// Quantas mensagens há na pasta e nas subpastas (para confirmar a exclusão), direto do servidor.
pub fn count_messages(acc: &ImapAccount, name: &str) -> Result<u32> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    let mut total = 0;
    for n in with_children(&listed, delimiter.as_deref(), name) {
        total += session.status(&n, "(MESSAGES)")?.exists;
    }
    session.logout().ok();
    Ok(total)
}

/// Move pelo UID. Sem MOVE no servidor: copia, marca como apagada e remove.
pub fn move_message(acc: &ImapAccount, from: &str, uid: u32, to: &str) -> Result<()> {
    move_messages(acc, from, &[uid], to)
}

/// Lista de UIDs no formato do IMAP ("1,2,3").
fn uid_set(uids: &[u32]) -> String {
    uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

/// Move vários UIDs da mesma pasta numa só conexão.
pub fn move_messages(acc: &ImapAccount, from: &str, uids: &[u32], to: &str) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    let mut session = connect(acc)?;
    let caps = session.capabilities()?;
    let (has_move, uidplus) = (caps.has_str("MOVE"), caps.has_str("UIDPLUS"));
    drop(caps);
    session.select(from)?;
    let set = uid_set(uids);
    if has_move {
        session.uid_mv(&set, to)?;
    } else {
        session.uid_copy(&set, to)?;
        session.uid_store(&set, "+FLAGS.SILENT (\\Deleted)")?;
        if uidplus { session.uid_expunge(&set)?; } else { session.expunge()?; }
    }
    session.logout().ok();
    Ok(())
}

/// Apaga de vez (usado dentro da Lixeira).
pub fn delete_forever(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<()> {
    delete_forever_many(acc, mailbox, &[uid])
}

pub fn delete_forever_many(acc: &ImapAccount, mailbox: &str, uids: &[u32]) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    let mut session = connect(acc)?;
    let uidplus = session.capabilities()?.has_str("UIDPLUS");
    session.select(mailbox)?;
    let set = uid_set(uids);
    session.uid_store(&set, "+FLAGS.SILENT (\\Deleted)")?;
    if uidplus { session.uid_expunge(&set)?; } else { session.expunge()?; }
    session.logout().ok();
    Ok(())
}

/// Flag que a UI altera no servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgFlag {
    /// `\Seen`: lida.
    Seen,
    /// `\Flagged`: com estrela.
    Flagged,
}

/// Liga/desliga a flag em vários UIDs da mesma pasta. A UI já atualizou o banco local antes ou depois.
pub fn set_flag(acc: &ImapAccount, mailbox: &str, uids: &[u32], flag: MsgFlag, on: bool) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    let name = match flag {
        MsgFlag::Seen => "\\Seen",
        MsgFlag::Flagged => "\\Flagged",
    };
    let mut session = connect(acc)?;
    session.select(mailbox)?;
    session.uid_store(uid_set(uids), format!("{}FLAGS.SILENT ({name})", if on { "+" } else { "-" }))?;
    session.logout().ok();
    Ok(())
}

/// Marca como lida/não lida no servidor.
pub fn set_seen(acc: &ImapAccount, mailbox: &str, uid: u32, seen: bool) -> Result<()> {
    set_flag(acc, mailbox, &[uid], MsgFlag::Seen, seen)
}

/// Primeiro endereço do cabeçalho To de uma mensagem, buscado só pelo cabeçalho.
pub fn fetch_to(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<Option<String>> {
    let mut session = connect(acc)?;
    session.select(mailbox)?;
    let fetches = session.uid_fetch(uid.to_string(), "BODY.PEEK[HEADER.FIELDS (TO)]")?;
    let to = fetches.iter().find_map(|f| f.header()).and_then(|h| {
        let msg = MessageParser::default().parse(h)?;
        first_to(&msg)
    });
    session.logout().ok();
    Ok(to)
}

/// Testa as credenciais sem sincronizar nada.
/// Mensagem completa (para extrair um anexo sob demanda).
pub fn fetch_raw(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<Option<Vec<u8>>> {
    let mut session = connect(acc)?;
    session.select(mailbox)?;
    let fetches = session.uid_fetch(uid.to_string(), "BODY.PEEK[]")?;
    let raw = fetches.iter().find_map(|f| f.body().map(<[u8]>::to_vec));
    session.logout().ok();
    Ok(raw)
}

pub fn check_login(acc: &ImapAccount) -> Result<()> {
    connect(acc)?.logout().ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarda_o_html_e_o_texto_da_alternativa() {
        let raw = b"From: Ana <ana@exemplo.com>\r\nSubject: Oi\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nol\xc3\xa1 texto\r\n\
--b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>ol\xc3\xa1 <b>html</b></p>\r\n--b--\r\n";
        let m = parse(1, raw, false, false, 0, "");
        assert_eq!(m.body_text.trim(), "olá texto");
        assert!(m.body_html.contains("<b>html</b>"), "{}", m.body_html);
    }

    #[test]
    fn so_html_ainda_gera_texto_para_busca() {
        let raw = b"From: a@b.c\r\nSubject: x\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>so html</p>\r\n";
        let m = parse(1, raw, false, false, 0, "");
        assert!(m.body_html.contains("<p>so html</p>"));
        assert!(m.body_text.contains("so html"));
    }

    #[test]
    fn so_texto_nao_vira_html() {
        let raw = b"From: a@b.c\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\nsimples\r\n";
        let m = parse(1, raw, false, false, 0, "");
        assert_eq!(m.body_html, "");
        assert_eq!(m.body_text.trim(), "simples");
    }

    fn raw(from: &str, to: &str) -> Vec<u8> {
        format!("From: {from}\r\nTo: {to}\r\nSubject: x\r\n\r\ncorpo\r\n").into_bytes()
    }

    #[test]
    fn enviada_por_mim_responde_ao_destinatario() {
        let m = parse(1, &raw("Eu@Exemplo.com", "Ana <ana@exemplo.com>, b@exemplo.com"), true, false, 0, "eu@exemplo.com");
        assert_eq!(m.reply_addr, "ana@exemplo.com");
    }

    #[test]
    fn recebida_responde_ao_remetente() {
        let m = parse(1, &raw("ana@exemplo.com", "eu@exemplo.com"), true, false, 0, "eu@exemplo.com");
        assert_eq!(m.reply_addr, "ana@exemplo.com");
    }
}
