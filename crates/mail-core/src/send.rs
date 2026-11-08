//! Envio por SMTP. Depois de entregue, guarda a cópia em Enviados e marca a original como respondida.

use imap::types::Flag;
use imap_proto::NameAttribute;
use lettre::message::{header, Mailbox, Mailboxes};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};

use crate::imap_sync::{connect, ImapAccount, ImapSession, Security};
use crate::store::ReplyRef;
use crate::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Outgoing {
    /// Um ou mais endereços separados por vírgula ou ponto e vírgula.
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct SendReport {
    pub saved_to_sent: bool,
    /// A mensagem saiu, mas algo depois do envio falhou (ex.: cópia em Enviados).
    pub warning: Option<String>,
}

/// Nomes comuns quando o servidor não marca a pasta com \Sent.
const SENT_NAMES: [&str; 6] = ["Sent", "Enviados", "Itens Enviados", "Sent Items", "Sent Messages", "INBOX.Sent"];

fn recipients(to: &str) -> Result<Mailboxes> {
    let list: Vec<Mailbox> = to
        .split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map_err(|_| Error::Invalid(format!("endereço inválido: {s}"))))
        .collect::<Result<_>>()?;
    if list.is_empty() {
        return Err(Error::Invalid("informe ao menos um destinatário".into()));
    }
    Ok(list.into_iter().collect())
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
    Ok(b.header(header::ContentType::TEXT_PLAIN).body(out.body.clone())?)
}

fn transport(acc: &ImapAccount) -> Result<SmtpTransport> {
    let s = acc.smtp();
    let builder = match s.security {
        Security::Tls => SmtpTransport::relay(&s.host)?,
        Security::StartTls => SmtpTransport::starttls_relay(&s.host)?,
        Security::Plaintext => SmtpTransport::builder_dangerous(&s.host),
    };
    Ok(builder
        .port(s.port)
        .credentials(Credentials::new(acc.username.clone(), acc.password.clone()))
        .build())
}

/// O Gmail já guarda no Enviados o que passa pelo SMTP; copiar duplicaria.
fn server_saves_sent(acc: &ImapAccount) -> bool {
    acc.host.ends_with("gmail.com") || acc.host.ends_with("googlemail.com")
}

fn sent_folder(session: &mut ImapSession) -> Result<String> {
    let names = session.list(None, Some("*"))?;
    if let Some(n) = names.iter().find(|n| n.attributes().contains(&NameAttribute::Sent)) {
        return Ok(n.name().to_string());
    }
    if let Some(n) = SENT_NAMES
        .iter()
        .find_map(|want| names.iter().find(|n| n.name().eq_ignore_ascii_case(want)))
    {
        return Ok(n.name().to_string());
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
            password: String::new(),
        }
    }

    fn out(to: &str) -> Outgoing {
        Outgoing { to: to.into(), subject: "Re: Orçamento".into(), body: "Combinado, até amanhã.".into() }
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

    #[test]
    fn smtp_padrao_vem_do_imap() {
        let s = acc().smtp();
        assert_eq!((s.host.as_str(), s.port), ("smtp.exemplo.com", 465));
    }
}
