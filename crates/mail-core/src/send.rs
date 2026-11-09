//! Envio por SMTP. Depois de entregue, guarda a cópia em Enviados e marca a original como respondida.

use imap::types::Flag;
use base64::Engine;
use lettre::message::{header, Attachment, Mailbox, Mailboxes, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};

use crate::folders::{pick, Role};
use crate::imap_sync::{connect, AuthMode, ImapAccount, ImapSession, Security};
use crate::store::ReplyRef;
use crate::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Outgoing {
    /// Um ou mais endereços separados por vírgula ou ponto e vírgula.
    pub to: String,
    pub subject: String,
    /// Texto simples (parte `text/plain`); a UI o gera a partir do HTML.
    pub body: String,
    /// Corpo formatado; quando presente a mensagem sai como `multipart/alternative` (texto + HTML).
    #[serde(default)]
    pub html: Option<String>,
    /// Cópia visível (vários endereços, como em `to`).
    #[serde(default)]
    pub cc: String,
    /// Cópia oculta.
    #[serde(default)]
    pub bcc: String,
    #[serde(default)]
    pub attachments: Vec<OutAttachment>,
}

/// Arquivo anexado ao rascunho; o conteúdo vem em base64.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutAttachment {
    pub name: String,
    #[serde(default)]
    pub mime: String,
    pub data: String,
}

/// Limite do total anexado (antes do base64), abaixo do que a maioria dos servidores aceita.
pub const MAX_ATTACHMENTS_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct SendReport {
    pub saved_to_sent: bool,
    /// A mensagem saiu, mas algo depois do envio falhou (ex.: cópia em Enviados).
    pub warning: Option<String>,
}

fn mailboxes(to: &str) -> Result<Vec<Mailbox>> {
    let list: Vec<Mailbox> = to
        .split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map_err(|_| Error::Invalid(format!("endereço inválido: {s}"))))
        .collect::<Result<_>>()?;
    Ok(list)
}

fn recipients(to: &str) -> Result<Mailboxes> {
    let list = mailboxes(to)?;
    if list.is_empty() {
        return Err(Error::Invalid("informe ao menos um destinatário".into()));
    }
    Ok(list.into_iter().collect())
}

fn attachment_part(a: &OutAttachment, data: Vec<u8>) -> Result<SinglePart> {
    let mime = if a.mime.is_empty() { "application/octet-stream" } else { a.mime.as_str() };
    let ct = header::ContentType::parse(mime)
        .or_else(|_| header::ContentType::parse("application/octet-stream"))
        .map_err(|_| Error::Invalid(format!("tipo inválido: {mime}")))?;
    Ok(Attachment::new(crate::attachments::safe_name(&a.name)).body(data, ct))
}

/// Monta a mensagem. Em resposta, In-Reply-To e References mantêm a conversa agrupada.
pub fn build(acc: &ImapAccount, out: &Outgoing, reply: Option<&ReplyRef>) -> Result<Message> {
    let from: Mailbox = acc
        .username
        .parse()
        .map_err(|_| Error::Invalid(format!("remetente inválido: {}", acc.username)))?;
    let mut b = Message::builder().from(from).subject(out.subject.trim()).message_id(None);
    b = b.mailbox(header::To::from(recipients(&out.to)?));
    if let Some(r) = reply.filter(|r| !r.message_id.is_empty()) {
        let parent = format!("<{}>", r.message_id);
        let refs = r
            .refs
            .split_whitespace()
            .map(|id| format!("<{id}>"))
            .chain(std::iter::once(parent.clone()))
            .collect::<Vec<_>>()
            .join(" ");
        b = b.in_reply_to(parent).references(refs);
    }
    let cc = mailboxes(&out.cc)?;
    if !cc.is_empty() {
        b = b.mailbox(header::Cc::from(cc.into_iter().collect::<Mailboxes>()));
    }
    let bcc = mailboxes(&out.bcc)?;
    if !bcc.is_empty() {
        b = b.mailbox(header::Bcc::from(bcc.into_iter().collect::<Mailboxes>()));
    }
    let html = out.html.as_deref().filter(|h| !h.trim().is_empty());
    if out.attachments.is_empty() {
        return Ok(match html {
            Some(h) => b.multipart(MultiPart::alternative_plain_html(out.body.clone(), h.to_string()))?,
            None => b.header(header::ContentType::TEXT_PLAIN).body(out.body.clone())?,
        });
    }
    let mut total = 0;
    let mut multi = match html {
        Some(h) => MultiPart::mixed().multipart(MultiPart::alternative_plain_html(out.body.clone(), h.to_string())),
        None => MultiPart::mixed().singlepart(SinglePart::plain(out.body.clone())),
    };

    for a in &out.attachments {
        let data = base64::engine::general_purpose::STANDARD
            .decode(a.data.trim())
            .map_err(|_| Error::Invalid(format!("anexo corrompido: {}", a.name)))?;
        total += data.len();
        if total > MAX_ATTACHMENTS_BYTES {
            return Err(Error::Invalid("anexos passam de 25 MB no total".into()));
        }
        multi = multi.singlepart(attachment_part(a, data)?);
    }
    Ok(b.multipart(multi)?)
}

fn transport(acc: &ImapAccount) -> Result<SmtpTransport> {
    let s = acc.smtp();
    let builder = match s.security {
        Security::Tls => SmtpTransport::relay(&s.host)?,
        Security::StartTls => SmtpTransport::starttls_relay(&s.host)?,
        Security::Plaintext => SmtpTransport::builder_dangerous(&s.host),
    };
    let builder = builder.port(s.port).credentials(Credentials::new(acc.username.clone(), acc.password.clone()));
    Ok(match acc.auth {
        AuthMode::Password => builder,
        _ => builder.authentication(vec![Mechanism::Xoauth2]),
    }
    .build())
}

/// O Gmail já guarda no Enviados o que passa pelo SMTP; copiar duplicaria.
fn server_saves_sent(acc: &ImapAccount) -> bool {
    acc.host.ends_with("gmail.com") || acc.host.ends_with("googlemail.com")
}

fn sent_folder(session: &mut ImapSession) -> Result<String> {
    let (listed, _) = crate::imap_sync::list_folders(session)?;
    if let Some(f) = pick(&listed).into_iter().find(|f| f.role == Role::Sent) {
        return Ok(f.name);
    }
    session.create("Sent")?;
    Ok("Sent".into())
}

fn after_send(acc: &ImapAccount, raw: &[u8], reply: Option<&ReplyRef>) -> Result<bool> {
    let mut session = connect(acc)?;
    let save = !server_saves_sent(acc);
    if save {
        let folder = sent_folder(&mut session)?;
        session.append(&folder, raw).flag(Flag::Seen).finish()?;
    }
    if let Some(r) = reply {
        session.select(&r.mailbox)?;
        session.uid_store(r.uid.to_string(), "+FLAGS.SILENT (\\Answered)")?;
    }
    session.logout().ok();
    Ok(save)
}

/// Envia. Só falha se a mensagem não saiu; problemas depois do envio viram aviso.
pub fn send(acc: &ImapAccount, out: &Outgoing, reply: Option<&ReplyRef>) -> Result<SendReport> {
    let msg = build(acc, out, reply)?;
    let raw = msg.formatted();
    transport(acc)?.send(&msg)?;
    Ok(match after_send(acc, &raw, reply) {
        Ok(saved_to_sent) => SendReport { saved_to_sent, warning: None },
        Err(e) => SendReport {
            saved_to_sent: false,
            warning: Some(format!("Enviada, mas não foi possível guardar em Enviados: {e}")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc() -> ImapAccount {
        ImapAccount {
            id: "eu@exemplo.com".into(),
            host: "imap.exemplo.com".into(),
            port: 993,
            security: Security::Tls,
            username: "eu@exemplo.com".into(),
            smtp: None,
            auth: AuthMode::Password,
            password: String::new(),
        }
    }

    fn out(to: &str) -> Outgoing {
        Outgoing { to: to.into(), subject: "Re: Orçamento".into(), body: "Combinado, até amanhã.".into(), html: None, cc: String::new(), bcc: String::new(), attachments: vec![] }
    }

    #[test]
    fn resposta_leva_cabecalhos_da_conversa() {
        let r = ReplyRef {
            account: "eu@exemplo.com".into(),
            mailbox: "INBOX".into(),
            uid: 3,
            message_id: "b@x".into(),
            refs: "a@x".into(),
        };
        let raw = String::from_utf8(build(&acc(), &out("Ana <ana@x.com>; bia@x.com"), Some(&r)).unwrap().formatted()).unwrap();
        assert!(raw.contains("In-Reply-To: <b@x>"), "{raw}");
        assert!(raw.contains("References: <a@x> <b@x>"), "{raw}");
        assert!(raw.contains("ana@x.com") && raw.contains("bia@x.com"));
        assert!(raw.contains("Message-ID: <"));
    }

    #[test]
    fn destinatario_invalido_ou_vazio_falha() {
        assert!(matches!(build(&acc(), &out("sem-arroba"), None), Err(Error::Invalid(_))));
        assert!(matches!(build(&acc(), &out(" , "), None), Err(Error::Invalid(_))));
    }

    fn att(name: &str, mime: &str, bytes: &[u8]) -> OutAttachment {
        OutAttachment { name: name.into(), mime: mime.into(), data: base64::engine::general_purpose::STANDARD.encode(bytes) }
    }

    #[test]
    fn cc_e_bcc_entram_no_envelope_e_so_cc_no_cabecalho() {
        let mut o = out("ana@x.com");
        o.cc = "Bia <bia@x.com>; caio@x.com".into();
        o.bcc = "dan@x.com, ,".into();
        let m = build(&acc(), &o, None).unwrap();
        let to: Vec<String> = m.envelope().to().iter().map(|a| a.to_string()).collect();
        assert_eq!(to, ["ana@x.com", "bia@x.com", "caio@x.com", "dan@x.com"]);
        let raw = String::from_utf8(m.formatted()).unwrap();
        assert!(raw.contains("Cc: ") && raw.contains("bia@x.com"), "{raw}");
        assert!(!raw.contains("dan@x.com"), "Bcc vazou no cabeçalho: {raw}");
        o.cc = "ruim".into();
        assert!(matches!(build(&acc(), &o, None), Err(Error::Invalid(_))));
    }

    #[test]
    fn anexos_viram_multipart_mixed() {
        let mut o = out("ana@x.com");
        o.attachments = vec![att("relatório.pdf", "application/pdf", b"%PDF-1.4 oi"), att("../x.bin", "", &[0, 1, 2, 255])];
        let raw = build(&acc(), &o, None).unwrap().formatted();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("multipart/mixed"), "{text}");
        let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
        let l = crate::attachments::list(&parsed, "");
        assert_eq!(l.len(), 2, "{l:?}");
        assert_eq!((l[0].name.as_str(), l[0].mime.as_str(), l[0].size), ("relatório.pdf", "application/pdf", 11));
        assert_eq!((l[1].name.as_str(), l[1].mime.as_str()), ("x.bin", "application/octet-stream"));
        let (_, data) = crate::attachments::extract(&raw, 1).unwrap();
        assert_eq!(data, [0, 1, 2, 255]);
        assert_eq!(parsed.body_text(0).unwrap().trim(), "Combinado, até amanhã.");
    }

    #[test]
    fn html_vira_multipart_alternative_com_texto_e_html() {
        let mut o = out("ana@x.com");
        o.body = "Olá, *Ana*".into();
        o.html = Some("<p>Olá, <b>Ana</b></p>".into());
        let raw = build(&acc(), &o, None).unwrap().formatted();
        assert!(String::from_utf8_lossy(&raw).contains("multipart/alternative"));
        let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
        assert_eq!(parsed.body_text(0).unwrap().trim(), "Olá, *Ana*");
        assert_eq!(parsed.body_html(0).unwrap().trim(), "<p>Olá, <b>Ana</b></p>");
        // Com anexo: mixed contendo o alternative.
        o.attachments = vec![att("a.pdf", "application/pdf", b"%PDF")];
        let raw = build(&acc(), &o, None).unwrap().formatted();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("multipart/mixed") && text.contains("multipart/alternative"), "{text}");
        let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
        assert_eq!(parsed.body_html(0).unwrap().trim(), "<p>Olá, <b>Ana</b></p>");
        assert_eq!(crate::attachments::list(&parsed, "").len(), 1);
        // HTML em branco = texto simples.
        o.html = Some("  ".into());
        o.attachments.clear();
        assert!(!String::from_utf8_lossy(&build(&acc(), &o, None).unwrap().formatted()).contains("multipart"));
    }

    #[test]
    fn anexos_acima_do_limite_ou_corrompidos_falham() {
        let mut o = out("ana@x.com");
        o.attachments = vec![att("a.bin", "", &vec![0u8; MAX_ATTACHMENTS_BYTES / 2 + 1]), att("b.bin", "", &vec![0u8; MAX_ATTACHMENTS_BYTES / 2 + 1])];
        assert!(matches!(build(&acc(), &o, None), Err(Error::Invalid(m)) if m.contains("25 MB")));
        o.attachments = vec![OutAttachment { name: "a".into(), mime: "".into(), data: "***".into() }];
        assert!(matches!(build(&acc(), &o, None), Err(Error::Invalid(_))));
    }

    #[test]
    fn smtp_padrao_vem_do_imap() {
        let s = acc().smtp();
        assert_eq!((s.host.as_str(), s.port), ("smtp.exemplo.com", 465));
    }
}
