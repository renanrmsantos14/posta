//! Sincronização IMAP → Store. Uma conexão por chamada; `idle_wait` espera novidades (IMAP IDLE).

use imap::Session;
use mail_parser::{MessageParser, PartKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use imap::types::UnsolicitedResponse;
use imap_proto::NameAttribute;

use crate::attachments;
use crate::oauth::{Provider, XOauth2};
use crate::folders::{archive_name, is_system, new_name, pick, renamed, user_folders, with_children, Folder, Listed, Role};
use crate::session::{Hub, Kind, Live};
use crate::store::{Delta, NewMessage, Store};
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
    /// Como a pasta foi reconciliada: "full", "qresync", "condstore" ou "uidnext".
    pub mode: &'static str,
    /// Mensagens inseridas, alteradas e removidas, por id local.
    #[serde(skip)]
    pub delta: Delta,
}

/// Quantas mensagens (só o início de cada uma) buscar por comando FETCH.
const BATCH: usize = 100;
/// Quantos bytes do início da mensagem baixar na sync: cabeçalhos mais a prévia do texto.
/// Mensagem menor que isso já chega inteira.
const PREVIEW_BYTES: usize = 8192;
/// Quantas mensagens completas buscar por comando FETCH ao baixar corpos.
const BODY_BATCH: usize = 10;
/// Quantas das mensagens mais novas (por UID) a sync incremental sem QRESYNC reconfere (flags e exclusões).
const RECENT_WINDOW: u32 = 500;

/// Servidores cuja resposta a `BODY[]<0.N>` não deu para ler (o Greenmail, por exemplo, omite o espaço
/// antes do literal, contra o RFC 3501). Neles a sync baixa a mensagem inteira, como antes.
static NO_PARTIAL: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn server_key(acc: &ImapAccount) -> String {
    format!("{}:{}", acc.host, acc.port)
}

fn partial_broken(acc: &ImapAccount) -> bool {
    NO_PARTIAL.lock().unwrap_or_else(|p| p.into_inner()).contains(&server_key(acc))
}

pub(crate) type ImapSession = Session<imap::Connection>;

/// A recusa veio da autenticação? (`NO [AUTHENTICATIONFAILED]`; erro de rede não conta.)
pub(crate) fn is_imap_auth_error(e: &crate::Error) -> bool {
    matches!(e, crate::Error::Imap(imap::Error::No(_)))
}

pub(crate) fn connect(acc: &ImapAccount) -> Result<ImapSession> {
    // Contas OAuth: token recusado (revogado ou vencido antes do previsto) = renova e tenta uma vez de novo.
    crate::oauth::with_fresh_token_retry(acc, is_imap_auth_error, connect_once)
}

fn connect_once(acc: &ImapAccount) -> Result<ImapSession> {
    // Conexão própria (crate::net): o `ClientBuilder` do imap não tem timeout de conexão, leitura nem escrita.
    let client = crate::net::connect(&acc.host, acc.port, &acc.security)?;
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
    /// A conta foi descartada (`Hub::drop_account`) durante a espera.
    Cancelled,
}

/// De quanto em quanto tempo `idle_until` confere o cancelamento.
pub const IDLE_SLICE: Duration = Duration::from_secs(2);

/// Bloqueia até o servidor avisar de mudança em `mailbox` ou até `timeout`. Abre e fecha a própria conexão.
pub fn idle_wait(acc: &ImapAccount, mailbox: &str, timeout: Duration) -> Result<IdleEnd> {
    let mut live = Live::open(acc)?;
    let end = idle_on(&mut live, mailbox, timeout);
    live.logout();
    end
}

/// IDLE numa sessão já aberta (a conexão fica selecionada em `mailbox` ao voltar, pronta para `sync_on`).
pub fn idle_on(live: &mut Live, mailbox: &str, timeout: Duration) -> Result<IdleEnd> {
    idle_sliced(live, mailbox, timeout, timeout, &|| false)
}

/// IDLE em fatias de `IDLE_SLICE` (refaz só o IDLE, sem novo SELECT) para conferir `cancelled`.
/// Avisos que chegam entre uma fatia e outra contam como `Changed`.
pub fn idle_until(live: &mut Live, mailbox: &str, timeout: Duration, cancelled: &dyn Fn() -> bool) -> Result<IdleEnd> {
    idle_sliced(live, mailbox, timeout, IDLE_SLICE, cancelled)
}

/// Núcleo de `idle_on`/`idle_until`: `slice` é o tempo máximo de cada IDLE antes de conferir `cancelled`
/// (`idle_on` passa o prazo inteiro, evitando refazer IDLE/DONE a cada 2 s numa espera de ~29 min).
fn idle_sliced(live: &mut Live, mailbox: &str, timeout: Duration, slice: Duration, cancelled: &dyn Fn() -> bool) -> Result<IdleEnd> {
    if !live.idle {
        return Ok(IdleEnd::Unsupported);
    }
    live.s.select(mailbox)?;
    let deadline = Instant::now() + timeout;
    loop {
        if cancelled() {
            return Ok(IdleEnd::Cancelled);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(IdleEnd::TimedOut);
        }
        let outcome = live.s.idle().timeout(left.min(slice)).keepalive(false).wait_while(imap::extensions::idle::stop_on_any)?;
        if matches!(outcome, imap::extensions::idle::WaitOutcome::MailboxChanged) || live.s.take_all_unsolicited().len() > 0 {
            return Ok(IdleEnd::Changed);
        }
    }
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
        // References, mais In-Reply-To quando o cliente de origem só mandou ele (serve ao agrupamento em conversas).
        refs: msg
            .as_ref()
            .map(|m| {
                let mut ids: Vec<&str> = m.references().map(|r| r.iter().collect()).unwrap_or_default();
                for id in m.in_reply_to().map(|r| r.iter().collect::<Vec<_>>()).unwrap_or_default() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                ids.join(" ")
            })
            .unwrap_or_default(),
        reply_addr,
        to_addrs: msg.as_ref().map(|m| addresses(m.to())).unwrap_or_default(),
        cc_addrs: msg.as_ref().map(|m| addresses(m.cc())).unwrap_or_default(),
        header_only: false,
        importance: msg.as_ref().map(importance_of).unwrap_or(0),
        unsub: crate::unsubscribe::header_lines(raw),
    }
}

/// Importância declarada: `Importance` (high/low), `X-Priority` (1–2 alta, 4–5 baixa) ou `Priority` (urgent/non-urgent).
pub(crate) fn importance_of(m: &mail_parser::Message) -> i8 {
    let raw = |n: &str| m.headers().value(n).and_then(|v| v.as_text().map(|t| t.trim().to_ascii_lowercase())).unwrap_or_default();
    match raw("Importance").as_str() {
        "high" => return 1,
        "low" => return -1,
        _ => {}
    }
    match raw("X-Priority").split_whitespace().next().and_then(|n| n.parse::<u8>().ok()) {
        Some(1 | 2) => return 1,
        Some(4 | 5) => return -1,
        _ => {}
    }
    match raw("Priority").as_str() {
        "urgent" => 1,
        "non-urgent" => -1,
        _ => 0,
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

/// Traz a pasta para o banco local numa conexão só dela (abre e fecha). Para repetir sync, prefira `Hub` + `sync_on`.
/// `Some(n)`: só baixa as n mais recentes do servidor; as locais mais antigas ficam.
pub fn sync_mailbox(acc: &ImapAccount, store: &mut Store, mailbox: &str, keep: Option<usize>) -> Result<SyncReport> {
    loop {
        let was_broken = partial_broken(acc);
        let mut live = Live::open(acc)?;
        let report = sync_on(&mut live, acc, store, mailbox, keep);
        live.logout();
        // O servidor acabou de se revelar incompatível com a leitura parcial: repete já sem ela.
        if report.is_err() && !was_broken && partial_broken(acc) {
            continue;
        }
        return report;
    }
}

/// Flags (UID, lida, estrela) das mensagens do conjunto `set` que casam com `query`.
fn fetch_flags(s: &mut ImapSession, set: &str, query: &str) -> Result<Vec<(u32, bool, bool)>> {
    Ok(s.uid_fetch(set, query)?.iter().filter_map(|f| Some((f.uid?, is_seen(f), is_flagged(f)))).collect())
}

/// UIDs que o servidor disse ter sumido (`VANISHED`, só com QRESYNC) desde o último comando.
fn vanished(s: &mut ImapSession) -> Vec<u32> {
    s.take_all_unsolicited()
        .filter_map(|u| match u {
            UnsolicitedResponse::Vanished { uids, .. } => Some(uids),
            _ => None,
        })
        .flatten()
        .flatten()
        .collect()
}

/// Sync de uma pasta numa sessão já aberta, no menor custo que o servidor permite:
/// - sem estado anterior (ou UIDVALIDITY nova): lista tudo (`1:*`) e baixa o que falta;
/// - QRESYNC: só o que mudou desde o HIGHESTMODSEQ guardado, inclusive exclusões;
/// - CONDSTORE: o que mudou nas flags, mais a janela recente para ver exclusões;
/// - nada disso: UIDNEXT e a janela recente (`RECENT_WINDOW` mensagens mais novas), sem varrer a pasta toda.
pub fn sync_on(live: &mut Live, acc: &ImapAccount, store: &mut Store, mailbox: &str, keep: Option<usize>) -> Result<SyncReport> {
    live.s.take_all_unsolicited().for_each(drop);
    let sel = live.s.select(mailbox)?;
    let validity = sel.uid_validity.unwrap_or(0);
    let prev = store.sync_state(&acc.id, mailbox)?;
    store.set_uid_validity(&acc.id, mailbox, validity)?;
    // Cópia incompleta (janela anterior): só segue incremental se a janela pedida agora já está preenchida.
    let have = store.count_box(&acc.id, mailbox)? as usize;
    let base = prev.filter(|p| p.uid_validity == validity && p.uid_next > 0 && (p.complete || keep.is_some_and(|n| have >= n)));
    let (highest, uid_next) = (sel.highest_mod_seq.unwrap_or(0), sel.uid_next.unwrap_or(0));
    let mut report = SyncReport { total: sel.exists as usize, ..Default::default() };

    let (missing, complete) = match base {
        None => {
            let server = if sel.exists > 0 { fetch_flags(&mut live.s, "1:*", "(UID FLAGS)")? } else { Vec::new() };
            report.delta.merge(store.apply_server_state(&acc.id, mailbox, &server)?);
            let local: HashSet<u32> = store.local_uids(&acc.id, mailbox)?.into_iter().collect();
            let mut newest: Vec<u32> = server.iter().map(|(u, _, _)| *u).collect();
            newest.sort_unstable_by(|a, b| b.cmp(a));
            if let Some(n) = keep {
                newest.truncate(n);
            }
            report.mode = "full";
            (newest.into_iter().filter(|u| !local.contains(u)).collect::<Vec<u32>>(), keep.is_none())
        }
        Some(b) => {
            let max_local = store.max_uid(&acc.id, mailbox)?;
            let mut fresh: BTreeSet<u32> = BTreeSet::new();
            report.mode = "uidnext";
            if live.condstore && b.highest_modseq > 0 && highest > 0 {
                let modifier = if live.qresync { " VANISHED" } else { "" };
                let q = format!("(UID FLAGS) (CHANGEDSINCE {}{modifier})", b.highest_modseq);
                let changes = fetch_flags(&mut live.s, "1:*", &q)?;
                let gone = if live.qresync { vanished(&mut live.s) } else { Vec::new() };
                let (new, old): (Vec<_>, Vec<_>) = changes.into_iter().partition(|c| c.0 > max_local);
                fresh.extend(new.iter().map(|c| c.0));
                report.delta.changed.extend(store.apply_flag_changes(&acc.id, mailbox, &old)?);
                report.delta.removed.extend(store.remove_uids(&acc.id, mailbox, &gone)?);
                report.mode = if live.qresync { "qresync" } else { "condstore" };
            }
            if !live.qresync {
                // Sem QRESYNC o servidor não conta o que foi excluído: confere a janela recente.
                let lo = store.window_start(&acc.id, mailbox, RECENT_WINDOW)?;
                let server = if sel.exists > 0 { fetch_flags(&mut live.s, &format!("{}:*", lo.max(1)), "(UID FLAGS)")? } else { Vec::new() };
                let d = store.apply_server_state_from(&acc.id, mailbox, lo, &server)?;
                report.delta.changed.extend(d.changed);
                report.delta.removed.extend(d.removed);
                fresh.extend(server.iter().map(|c| c.0).filter(|u| *u > max_local));
            }
            let mut newest: Vec<u32> = fresh.into_iter().rev().collect();
            let truncated = keep.is_some_and(|n| newest.len() > n);
            if let Some(n) = keep {
                newest.truncate(n);
            }
            (newest, b.complete && !truncated)
        }
    };
    report.delta.inserted = download(&mut live.s, acc, store, mailbox, &missing)?;
    report.fetched = report.delta.inserted.len();
    report.delta.changed.sort_unstable();
    report.delta.changed.dedup();
    store.save_sync_state(&acc.id, mailbox, highest, uid_next, complete)?;
    Ok(report)
}

/// Baixa o início de cada mensagem (cabeçalhos e uma prévia do texto) e a estrutura MIME, e grava.
/// Mensagem curta cabe inteira na prévia e já fica com o corpo; as maiores ficam sem corpo
/// (`body_loaded = 0`) até alguém abrir a mensagem ou o prefetch chegar nela. Devolve os ids gravados.
fn download(session: &mut ImapSession, acc: &ImapAccount, store: &mut Store, mailbox: &str, uids: &[u32]) -> Result<Vec<i64>> {
    let mut ids = Vec::new();
    let partial = !partial_broken(acc);
    let query = if partial {
        format!("(UID FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[]<0.{PREVIEW_BYTES}>)")
    } else {
        "(UID FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[])".to_string()
    };
    for chunk in uids.chunks(BATCH) {
        let set = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let fetches = match session.uid_fetch(&set, &query) {
            Ok(f) => f,
            // Resposta ilegível: a conexão ficou dessincronizada. Anota o servidor e derruba a sessão;
            // a nova tentativa (Hub ou sync_mailbox) já baixa sem leitura parcial.
            Err(e) if partial && !matches!(e, imap::Error::Io(_) | imap::Error::ConnectionLost) => {
                NO_PARTIAL.lock().unwrap_or_else(|p| p.into_inner()).push(server_key(acc));
                return Err(imap::Error::ConnectionLost.into());
            }
            Err(e) => return Err(e.into()),
        };
        let msgs: Vec<NewMessage> = fetches
            .iter()
            .filter_map(|f| {
                let date = f.internal_date().map(|d| d.timestamp()).unwrap_or(0);
                let raw = f.body()?;
                let mut m = parse(f.uid?, raw, is_seen(f), is_flagged(f), date, &acc.username);
                if partial && raw.len() >= PREVIEW_BYTES {
                    // Cortada: o HTML pela metade não serve, e a lista de anexos vem da estrutura MIME.
                    m.header_only = true;
                    m.body_html.clear();
                    if let Some(bs) = f.bodystructure() {
                        m.attachments = structure_attachments(bs);
                    }
                }
                Some(m)
            })
            .collect();
        ids.extend(store.insert_messages(&acc.id, mailbox, &msgs)?);
    }
    Ok(ids)
}

fn param<'a>(params: &'a imap_proto::types::BodyParams, key: &str) -> Option<&'a str> {
    params.as_ref()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_ref())
}

/// Anexos de uma mensagem a partir do BODYSTRUCTURE (sem baixar o corpo). Provisório: ao baixar a mensagem
/// inteira, a lista passa a ser a do mail-parser, que é a que `attachments::extract` usa.
fn structure_attachments(bs: &imap_proto::types::BodyStructure) -> Vec<attachments::Attachment> {
    use imap_proto::types::BodyStructure as B;
    fn walk(bs: &B, out: &mut Vec<attachments::Attachment>) {
        let (common, other, text) = match bs {
            B::Multipart { bodies, .. } => return bodies.iter().for_each(|b| walk(b, out)),
            B::Basic { common, other, .. } => (common, other, false),
            B::Text { common, other, .. } => (common, other, true),
            B::Message { common, other, .. } => (common, other, false),
        };
        let disp = common.disposition.as_ref();
        let name = disp.and_then(|d| param(&d.params, "filename")).or_else(|| param(&common.ty.params, "name"));
        let attached = disp.is_some_and(|d| d.ty.eq_ignore_ascii_case("attachment"));
        let inline = disp.is_some_and(|d| d.ty.eq_ignore_ascii_case("inline"));
        if attached || (!text && !inline && name.is_some()) {
            out.push(attachments::Attachment {
                name: name.unwrap_or("anexo").to_string(),
                mime: format!("{}/{}", common.ty.ty, common.ty.subtype).to_lowercase(),
                size: other.octets as u64,
                index: out.len() as u32,
            });
        }
    }
    let mut out = Vec::new();
    walk(bs, &mut out);
    out
}

/// Baixa o corpo completo de `uids` (os que já estão no banco sem corpo) e grava. Devolve quantos gravou.
fn fill_bodies(session: &mut ImapSession, acc: &ImapAccount, store: &mut Store, mailbox: &str, uids: &[u32]) -> Result<Vec<i64>> {
    let mut done = Vec::new();
    for chunk in uids.chunks(BODY_BATCH) {
        let set = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        for f in session.uid_fetch(&set, "(UID BODY.PEEK[])")?.iter() {
            let (Some(uid), Some(raw)) = (f.uid, f.body()) else { continue };
            let full = parse(uid, raw, false, false, 0, &acc.username);
            if let Some(id) = store.set_body(&acc.id, mailbox, uid, &full)? {
                done.push(id);
            }
        }
    }
    Ok(done)
}

/// Corpo de uma mensagem que só tem o início no banco, buscado do servidor e gravado. `false`: já tinha corpo.
pub fn load_body_on(live: &mut Live, acc: &ImapAccount, store: &mut Store, id: i64) -> Result<bool> {
    let Some((account, mailbox, uid)) = store.body_missing(id)? else { return Ok(false) };
    debug_assert_eq!(account, acc.id);
    live.s.select(&mailbox)?;
    Ok(!fill_bodies(&mut live.s, acc, store, &mailbox, &[uid])?.is_empty())
}

/// Como `load_body_on`, na sessão persistente da conta.
pub fn load_body(hub: &Hub, acc: &ImapAccount, store: &mut Store, id: i64) -> Result<bool> {
    hub.with(acc, Kind::Read, |live| load_body_on(live, acc, store, id))
}

/// Baixa em segundo plano o corpo das `limit` mensagens mais recentes da pasta que ainda não o têm,
/// para abrirem sem rede. Devolve os ids atualizados.
pub fn prefetch_bodies_on(live: &mut Live, acc: &ImapAccount, store: &mut Store, mailbox: &str, limit: u32) -> Result<Vec<i64>> {
    let uids = store.uids_without_body(&acc.id, mailbox, limit)?;
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    live.s.select(mailbox)?;
    fill_bodies(&mut live.s, acc, store, mailbox, &uids)
}

/// Busca no servidor (IMAP SEARCH) na pasta. Mensagens que ainda não estão no banco local são baixadas,
/// para a lista poder mostrá-las. Devolve os UIDs encontrados, em ordem decrescente.
pub fn search_server(acc: &ImapAccount, store: &mut Store, mailbox: &str, filters: &crate::search::Filters) -> Result<Vec<u32>> {
    let Some(query) = filters.imap_query() else {
        return Ok(Vec::new());
    };
    let mut session = connect(acc)?;
    session.select(mailbox)?;
    let mut found: Vec<u32> = session.uid_search(&query)?.into_iter().collect();
    found.sort_unstable_by(|a, b| b.cmp(a));
    let local: HashSet<u32> = store.local_uids(&acc.id, mailbox)?.into_iter().collect();
    let missing: Vec<u32> = found.iter().copied().filter(|u| !local.contains(u)).collect();
    download(&mut session, acc, store, mailbox, &missing)?;
    session.logout().ok();
    Ok(found)
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
    Live::once(acc, |l| move_messages_on(l, from, uids, to))
}

/// UIDs de `uids` que ainda existem na pasta selecionada, cada um com o Message-ID (se tiver).
fn present(s: &mut ImapSession, uids: &[u32]) -> Result<Vec<(u32, Option<String>)>> {
    let fetched = s.uid_fetch(uid_set(uids), "(UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")?;
    let mut found: Vec<(u32, Option<String>)> = fetched
        .iter()
        .filter_map(|f| {
            let id = f.header().and_then(|h| MessageParser::default().parse(h)?.message_id().map(str::to_string));
            Some((f.uid?, id))
        })
        .filter(|(u, _)| uids.contains(u))
        .collect();
    found.sort_unstable_by_key(|(u, _)| *u);
    found.dedup_by_key(|(u, _)| *u);
    Ok(found)
}

/// `move_messages` numa sessão já aberta. Idempotente: repetir depois de uma queda de rede não duplica nem falha.
/// UID que não está mais na origem conta como já movido; sem MOVE, o que já existe no destino (mesmo Message-ID)
/// não é copiado de novo, só removido da origem.
pub fn move_messages_on(live: &mut Live, from: &str, uids: &[u32], to: &str) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    let (has_move, uidplus) = (live.has_move, live.uidplus);
    live.s.select(from)?;
    let here = present(&mut live.s, uids)?;
    if here.is_empty() {
        return Ok(());
    }
    let left: Vec<u32> = here.iter().map(|(u, _)| *u).collect();
    let set = uid_set(&left);
    if has_move {
        return match live.s.uid_mv(&set, to) {
            Ok(_) => Ok(()),
            // O servidor recusou (ex.: UID inexistente por corrida): se nada sobrou na origem, a mensagem já foi.
            Err(e) => match present(&mut live.s, &left) {
                Ok(rest) if rest.is_empty() => Ok(()),
                _ => Err(e.into()),
            },
        };
    }
    // Sem MOVE: a cópia pode ter ido numa tentativa anterior que caiu antes do EXPUNGE.
    let mut to_copy: Vec<u32> = Vec::new();
    live.s.select(to)?;
    for (uid, id) in &here {
        let known = match id {
            Some(id) => !live.s.uid_search(format!("HEADER Message-ID \"{}\"", id.replace(['\\', '"'], "")))?.is_empty(),
            None => false,
        };
        if !known {
            to_copy.push(*uid);
        }
    }
    live.s.select(from)?;
    if !to_copy.is_empty() {
        live.s.uid_copy(uid_set(&to_copy), to)?;
    }
    live.s.uid_store(&set, "+FLAGS.SILENT (\\Deleted)")?;
    if uidplus { live.s.uid_expunge(&set)?; } else { live.s.expunge()?; }
    Ok(())
}

/// Copia para `to` mantendo a original. Idempotente: o que já existe no destino (mesmo Message-ID)
/// não é copiado de novo, então repetir depois de uma queda não duplica.
pub fn copy_messages_on(live: &mut Live, from: &str, uids: &[u32], to: &str) -> Result<()> {
    if uids.is_empty() || from == to {
        return Ok(());
    }
    live.s.select(from)?;
    let here = present(&mut live.s, uids)?;
    if here.is_empty() {
        return Ok(());
    }
    live.s.select(to)?;
    let mut to_copy: Vec<u32> = Vec::new();
    for (uid, id) in &here {
        let known = match id {
            Some(id) => !live.s.uid_search(format!("HEADER Message-ID \"{}\"", id.replace(['\\', '"'], "")))?.is_empty(),
            None => false,
        };
        if !known {
            to_copy.push(*uid);
        }
    }
    if !to_copy.is_empty() {
        live.s.select(from)?;
        live.s.uid_copy(uid_set(&to_copy), to)?;
    }
    Ok(())
}

/// Apaga de vez (usado dentro da Lixeira).
pub fn delete_forever(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<()> {
    delete_forever_many(acc, mailbox, &[uid])
}

pub fn delete_forever_many(acc: &ImapAccount, mailbox: &str, uids: &[u32]) -> Result<()> {
    Live::once(acc, |l| delete_forever_on(l, mailbox, uids))
}

/// `delete_forever_many` numa sessão já aberta. Idempotente: UID que já não existe conta como apagado.
pub fn delete_forever_on(live: &mut Live, mailbox: &str, uids: &[u32]) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    let uidplus = live.uidplus;
    live.s.select(mailbox)?;
    let left: Vec<u32> = present(&mut live.s, uids)?.into_iter().map(|(u, _)| u).collect();
    if left.is_empty() {
        return Ok(());
    }
    let set = uid_set(&left);
    live.s.uid_store(&set, "+FLAGS.SILENT (\\Deleted)")?;
    if uidplus { live.s.uid_expunge(&set)?; } else { live.s.expunge()?; }
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
    Live::once(acc, |l| set_flag_on(l, mailbox, uids, flag, on))
}

/// `set_flag` numa sessão já aberta.
pub fn set_flag_on(live: &mut Live, mailbox: &str, uids: &[u32], flag: MsgFlag, on: bool) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    let name = match flag {
        MsgFlag::Seen => "\\Seen",
        MsgFlag::Flagged => "\\Flagged",
    };
    live.s.select(mailbox)?;
    live.s.uid_store(uid_set(uids), format!("{}FLAGS.SILENT ({name})", if on { "+" } else { "-" }))?;
    Ok(())
}

/// Marca como lida/não lida no servidor.
pub fn set_seen(acc: &ImapAccount, mailbox: &str, uid: u32, seen: bool) -> Result<()> {
    set_flag(acc, mailbox, &[uid], MsgFlag::Seen, seen)
}

/// Primeiro endereço do cabeçalho To de uma mensagem, buscado só pelo cabeçalho.
pub fn fetch_to(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<Option<String>> {
    Live::once(acc, |l| fetch_to_on(l, mailbox, uid))
}

/// `fetch_to` numa sessão já aberta.
pub fn fetch_to_on(live: &mut Live, mailbox: &str, uid: u32) -> Result<Option<String>> {
    live.s.select(mailbox)?;
    let fetches = live.s.uid_fetch(uid.to_string(), "BODY.PEEK[HEADER.FIELDS (TO)]")?;
    Ok(fetches.iter().find_map(|f| f.header()).and_then(|h| {
        let msg = MessageParser::default().parse(h)?;
        first_to(&msg)
    }))
}

/// Mensagem completa (para extrair um anexo sob demanda).
pub fn fetch_raw(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<Option<Vec<u8>>> {
    Live::once(acc, |l| fetch_raw_on(l, mailbox, uid))
}

/// `fetch_raw` numa sessão já aberta.
pub fn fetch_raw_on(live: &mut Live, mailbox: &str, uid: u32) -> Result<Option<Vec<u8>>> {
    live.s.select(mailbox)?;
    let fetches = live.s.uid_fetch(uid.to_string(), "BODY.PEEK[]")?;
    Ok(fetches.iter().find_map(|f| f.body().map(<[u8]>::to_vec)))
}

/// Testa as credenciais sem sincronizar nada.
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
