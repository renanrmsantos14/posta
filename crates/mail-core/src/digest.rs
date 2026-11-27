//! Resumo "Hoje": o que chegou nas últimas 24 h, separado por heurística local (sem IA, sem rede).
//! Só lê o banco local; não muda nenhuma mensagem. Quem decide o que fazer com cada uma é o usuário.

use rusqlite::params;
use serde::Serialize;

use crate::folders::Role;
use crate::store::{row_to_summary, MessageSummary, Store, NOT_SNOOZED, SUMMARY_COLS};
use crate::Result;

/// Janela padrão do resumo.
pub const WINDOW_HOURS: i64 = 24;
/// Teto por consulta: o resumo é de um dia, não de uma caixa de 10 mil mensagens.
const MAX_ROWS: u32 = 500;

/// Grupo do resumo, na ordem em que aparece na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    Important,
    People,
    Notifications,
    Newsletters,
}

/// O que se sabe de uma mensagem para classificá-la (tudo vem do banco local).
#[derive(Debug, Clone, Default)]
pub struct Facts<'a> {
    pub from_name: &'a str,
    pub from_addr: &'a str,
    pub subject: &'a str,
    /// Estrela, fixada ou importância alta pedida pelo remetente.
    pub flagged: bool,
    pub pinned: bool,
    pub high_importance: bool,
    /// Linhas `List-Unsubscribe*` guardadas (`Some` não vazio = é lista/boletim).
    pub has_unsub: bool,
    /// Já existe, na conversa, uma mensagem enviada pelo próprio usuário.
    pub i_wrote_in_thread: bool,
}

/// Minúsculas e sem acento, para comparar "Fatura" com "fatura" e "Notificação" com "notificacao".
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Parte antes do `@`, sem `+etiqueta` (`notificacoes+abc@x.com` → `notificacoes`).
fn local_part(addr: &str) -> String {
    let l = fold(addr.split('@').next().unwrap_or(""));
    l.split('+').next().unwrap_or("").trim().to_string()
}

const NOTIFY_LOCALS: &[&str] = &[
    "notifications", "notification", "notificacao", "notificacoes", "notify", "alerts", "alert", "alerta", "alertas",
    "mailer-daemon", "postmaster", "bounce", "bounces", "security", "seguranca", "receipts", "receipt", "billing",
    "invoice", "invoices", "fatura", "faturas", "cobranca", "pagamentos", "payments", "account", "accounts", "system",
    "sistema", "automated", "auto",
];
const NEWS_LOCALS: &[&str] = &[
    "newsletter", "newsletters", "news", "marketing", "mkt", "promo", "promocao", "promocoes", "ofertas", "offers",
    "deals", "digest", "weekly", "boletim", "campaign", "campanhas", "hello", "info",
];
const NOREPLY_LOCALS: &[&str] = &["noreply", "no-reply", "no_reply", "donotreply", "do-not-reply", "naoresponda", "nao-responda"];
/// Assuntos de aviso transacional (código, fatura, segurança, pedido). Frases específicas: "pedido" sozinho pegaria gente.
const TRANSACTIONAL: &[&str] = &[
    "codigo de verificacao", "verification code", "seu codigo", "your code", "one-time", "codigo de acesso", "fatura",
    "boleto", "recibo", "comprovante", "alerta de seguranca", "security alert", "redefinir senha", "redefinicao de senha",
    "reset your password", "password reset", "novo login", "novo acesso", "sign-in", "sign in attempt", "confirmacao de pedido",
    "order confirmation", "seu pedido", "your order", "pagamento aprovado", "payment received",
];

fn in_list(list: &[&str], local: &str) -> bool {
    list.contains(&local)
}

/// Escolhe o grupo. Ordem (a primeira regra que bate vence):
/// 1. Importante: estrela, fixada, ou importância alta de remetente que não é automático (aviso, boletim, noreply).
/// 2. Importante: resposta numa conversa em que o usuário já escreveu (e o remetente não é automático).
/// 3. Notificações: remetente de aviso (`notifications@`, `alerts@`…) ou assunto transacional.
/// 4. Boletins: tem `List-Unsubscribe`, ou remetente/assunto de newsletter.
/// 5. Notificações: `noreply@` sem lista de cancelamento.
/// 6. Pessoas: o resto.
pub fn classify(f: &Facts) -> Bucket {
    let local = local_part(f.from_addr);
    let subject = fold(f.subject);
    let name = fold(f.from_name);
    let notify_sender = in_list(NOTIFY_LOCALS, &local);
    let news_sender = in_list(NEWS_LOCALS, &local) || name.contains("newsletter") || subject.contains("newsletter");
    let noreply = in_list(NOREPLY_LOCALS, &local);
    let automated = notify_sender || news_sender || noreply || f.has_unsub;

    if f.flagged || f.pinned || (f.high_importance && !automated) {
        return Bucket::Important;
    }
    if f.i_wrote_in_thread && !automated {
        return Bucket::Important;
    }
    if notify_sender || TRANSACTIONAL.iter().any(|t| subject.contains(t)) {
        return Bucket::Notifications;
    }
    if f.has_unsub || news_sender {
        return Bucket::Newsletters;
    }
    if noreply {
        return Bucket::Notifications;
    }
    Bucket::People
}

/// Resumo do período, uma lista por grupo (mais recentes primeiro).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Digest {
    /// Início e fim da janela (segundos Unix).
    pub since: i64,
    pub until: i64,
    pub important: Vec<MessageSummary>,
    pub people: Vec<MessageSummary>,
    pub notifications: Vec<MessageSummary>,
    pub newsletters: Vec<MessageSummary>,
}

impl Digest {
    pub fn total(&self) -> usize {
        self.important.len() + self.people.len() + self.notifications.len() + self.newsletters.len()
    }
}

impl Store {
    /// Caixas de entrada de todas as contas, mensagens de `[now - hours, now]` que não estão adiadas,
    /// agrupadas por `classify`. Lê só o banco local.
    pub fn digest(&self, now: i64, hours: i64) -> Result<Digest> {
        let since = now - hours.max(1) * 3600;
        let mut out = Digest { since, until: now, important: vec![], people: vec![], notifications: vec![], newsletters: vec![] };
        for (account, mailbox) in self.role_boxes(Role::Inbox)? {
            let mut st = self.conn.prepare_cached(&format!(
                "SELECT {SUMMARY_COLS},
                   (m.unsub IS NOT NULL AND m.unsub <> ''),
                   (m.thread_id <> 0 AND EXISTS (
                      SELECT 1 FROM messages t JOIN folders f ON f.account = t.account AND f.name = t.mailbox AND f.role = 'sent'
                      WHERE t.account = m.account AND t.thread_id = m.thread_id))
                 FROM messages m
                 WHERE m.account = ?1 AND m.mailbox = ?2 AND m.date >= ?3 AND m.date <= ?4 AND {NOT_SNOOZED}
                 ORDER BY m.date DESC, m.id DESC LIMIT ?5"
            ))?;
            let rows = st.query_map(params![account, mailbox, since, now, MAX_ROWS], |r| {
                Ok((row_to_summary(r)?, r.get::<_, bool>(16)?, r.get::<_, bool>(17)?))
            })?;
            for row in rows {
                let (m, has_unsub, i_wrote) = row?;
                let bucket = classify(&Facts {
                    from_name: &m.from_name,
                    from_addr: &m.from_addr,
                    subject: &m.subject,
                    flagged: m.flagged,
                    pinned: m.pinned,
                    high_importance: m.importance == 1,
                    has_unsub,
                    i_wrote_in_thread: i_wrote,
                });
                match bucket {
                    Bucket::Important => out.important.push(m),
                    Bucket::People => out.people.push(m),
                    Bucket::Notifications => out.notifications.push(m),
                    Bucket::Newsletters => out.newsletters.push(m),
                }
            }
        }
        // Várias contas: cada grupo volta a ficar do mais novo para o mais antigo.
        for list in [&mut out.important, &mut out.people, &mut out.notifications, &mut out.newsletters] {
            list.sort_by(|a, b| (b.date, b.id).cmp(&(a.date, a.id)));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folders::Folder;
    use crate::store::NewMessage;

    fn f<'a>(addr: &'a str, subject: &'a str) -> Facts<'a> {
        Facts { from_name: "", from_addr: addr, subject, ..Default::default() }
    }

    #[test]
    fn pessoa_comum_vai_para_pessoas() {
        assert_eq!(classify(&f("ana@exemplo.com", "Almoço amanhã?")), Bucket::People);
        // "pedido" sozinho não é transacional: é gente pedindo coisa.
        assert_eq!(classify(&f("ana@exemplo.com", "Pedido de reunião")), Bucket::People);
    }

    #[test]
    fn estrela_fixada_e_importancia_alta_sao_importantes() {
        assert_eq!(classify(&Facts { flagged: true, ..f("ana@exemplo.com", "x") }), Bucket::Important);
        assert_eq!(classify(&Facts { pinned: true, ..f("noreply@banco.com", "x") }), Bucket::Important);
        assert_eq!(classify(&Facts { high_importance: true, ..f("ana@exemplo.com", "x") }), Bucket::Important);
        // Boletim que pede "alta importância" não vira Importante.
        assert_eq!(classify(&Facts { high_importance: true, has_unsub: true, ..f("ofertas@loja.com", "x") }), Bucket::Newsletters);
        assert_eq!(classify(&Facts { high_importance: true, ..f("alerts@banco.com", "x") }), Bucket::Notifications);
        assert_eq!(classify(&Facts { high_importance: true, ..f("noreply@banco.com", "x") }), Bucket::Notifications);
    }

    #[test]
    fn resposta_em_conversa_minha_e_importante_mas_so_se_for_gente() {
        assert_eq!(classify(&Facts { i_wrote_in_thread: true, ..f("ana@exemplo.com", "Re: proposta") }), Bucket::Important);
        assert_eq!(classify(&Facts { i_wrote_in_thread: true, ..f("noreply@x.com", "Re: proposta") }), Bucket::Notifications);
        assert_eq!(classify(&Facts { i_wrote_in_thread: true, has_unsub: true, ..f("ana@lista.com", "Re: x") }), Bucket::Newsletters);
    }

    #[test]
    fn notificacoes_por_remetente_e_por_assunto() {
        assert_eq!(classify(&f("notifications@github.com", "[repo] PR #1")), Bucket::Notifications);
        assert_eq!(classify(&f("Notificacoes+abc@banco.com.br", "x")), Bucket::Notifications);
        assert_eq!(classify(&f("MAILER-DAEMON@x.com", "Undelivered")), Bucket::Notifications);
        assert_eq!(classify(&f("contato@loja.com", "Seu código de verificação")), Bucket::Notifications);
        assert_eq!(classify(&f("contato@loja.com", "Sua FATURA chegou")), Bucket::Notifications);
        assert_eq!(classify(&f("x@y.com", "Alerta de segurança na sua conta")), Bucket::Notifications);
    }

    #[test]
    fn notificacao_de_site_com_cancelamento_continua_notificacao() {
        // GitHub manda List-Unsubscribe em tudo; o remetente de aviso vence.
        assert_eq!(classify(&Facts { has_unsub: true, ..f("notifications@github.com", "x") }), Bucket::Notifications);
    }

    #[test]
    fn boletins_por_cancelamento_remetente_ou_assunto() {
        assert_eq!(classify(&Facts { has_unsub: true, ..f("ana@lista.com", "Novidades da semana") }), Bucket::Newsletters);
        assert_eq!(classify(&f("newsletter@site.com", "x")), Bucket::Newsletters);
        assert_eq!(classify(&f("Promoções@loja.com", "x")), Bucket::Newsletters);
        assert_eq!(classify(&f("a@b.com", "Newsletter de novembro")), Bucket::Newsletters);
        // noreply com cancelamento é lista; sem, é aviso.
        assert_eq!(classify(&Facts { has_unsub: true, ..f("noreply@loja.com", "Ofertas") }), Bucket::Newsletters);
        assert_eq!(classify(&f("noreply@loja.com", "Ofertas")), Bucket::Notifications);
    }

    #[test]
    fn caixa_e_acento_nao_importam() {
        assert_eq!(classify(&f("ALERTS@X.COM", "x")), Bucket::Notifications);
        assert_eq!(classify(&f("a@b.com", "CÓDIGO DE VERIFICAÇÃO")), Bucket::Notifications);
        assert_eq!(classify(&f("", "")), Bucket::People);
        assert_eq!(classify(&f("sem-arroba", "x")), Bucket::People);
    }

    // --- Store::digest ---

    const NOW: i64 = 1_800_000_000;
    const H: i64 = 3600;

    fn nm(uid: u32, addr: &str, subject: &str, date: i64) -> NewMessage {
        NewMessage {
            uid,
            from_addr: addr.into(),
            from_name: addr.split('@').next().unwrap_or("").into(),
            subject: subject.into(),
            date,
            message_id: format!("{uid}@t"),
            reply_addr: addr.into(),
            ..Default::default()
        }
    }

    fn store() -> Store {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &[Folder { role: Role::Inbox, name: "INBOX".into() }, Folder { role: Role::Sent, name: "Sent".into() }]).unwrap();
        s
    }

    fn subjects(l: &[MessageSummary]) -> Vec<&str> {
        l.iter().map(|m| m.subject.as_str()).collect()
    }

    #[test]
    fn resumo_vazio_sem_mensagens() {
        let d = store().digest(NOW, 24).unwrap();
        assert_eq!(d.total(), 0);
        assert_eq!((d.since, d.until), (NOW - 24 * H, NOW));
    }

    #[test]
    fn agrupa_so_as_ultimas_24h_e_ordena_do_mais_novo() {
        let mut s = store();
        let mut github = nm(2, "notifications@github.com", "[repo] PR", NOW - 2 * H);
        github.unsub = Some("list-unsubscribe: <https://github.com/u>\r\n".into());
        let mut news = nm(3, "ana@lista.com", "Novidades", NOW - 3 * H);
        news.unsub = Some("list-unsubscribe: <mailto:sair@lista.com>\r\n".into());
        s.insert_messages(
            "a",
            "INBOX",
            &[
                nm(1, "bia@exemplo.com", "Almoço?", NOW - H),
                github,
                news,
                nm(4, "carla@exemplo.com", "Contrato", NOW - 5 * H),
                nm(5, "velha@exemplo.com", "Ontem", NOW - 25 * H),
                nm(6, "noreply@banco.com", "Extrato", NOW - 6 * H),
            ],
        )
        .unwrap();
        // Estrelada vira Importante, mesmo vinda de lista.
        let id = s.list("INBOX", 10, 0).unwrap().into_iter().find(|m| m.subject == "Contrato").unwrap().id;
        s.set_flagged(id, true).unwrap();
        let d = s.digest(NOW, 24).unwrap();
        assert_eq!(subjects(&d.important), ["Contrato"]);
        assert_eq!(subjects(&d.people), ["Almoço?"]);
        assert_eq!(subjects(&d.notifications), ["[repo] PR", "Extrato"]);
        assert_eq!(subjects(&d.newsletters), ["Novidades"]);
        assert_eq!(d.total(), 5, "a de 25 h fica de fora");
    }

    #[test]
    fn resposta_a_conversa_em_que_eu_escrevi_e_importante() {
        let mut s = store();
        let mut sent = nm(1, "eu@exemplo.com", "Proposta", NOW - 10 * H);
        sent.message_id = "p1@t".into();
        s.insert_messages("a", "Sent", &[sent]).unwrap();
        let mut reply = nm(2, "ana@exemplo.com", "Re: Proposta", NOW - H);
        reply.refs = "p1@t".into();
        s.insert_messages("a", "INBOX", &[reply, nm(3, "bia@exemplo.com", "Outra", NOW - 2 * H)]).unwrap();
        let d = s.digest(NOW, 24).unwrap();
        assert_eq!(subjects(&d.important), ["Re: Proposta"]);
        assert_eq!(subjects(&d.people), ["Outra"]);
    }

    #[test]
    fn so_lidas_e_adiadas() {
        let mut s = store();
        let mut lida = nm(1, "ana@exemplo.com", "Lida", NOW - H);
        lida.seen = true;
        s.insert_messages("a", "INBOX", &[lida, nm(2, "bia@exemplo.com", "Adiada", NOW - 2 * H)]).unwrap();
        let adiada = s.list("INBOX", 10, 0).unwrap().into_iter().find(|m| m.subject == "Adiada").unwrap().id;
        // Adiada até muito depois do relógio real: não aparece no resumo.
        s.snooze(&[adiada], 4_102_444_800, NOW).unwrap();
        let d = s.digest(NOW, 24).unwrap();
        assert_eq!(subjects(&d.people), ["Lida"]);
        assert!(d.people[0].seen, "lidas entram: o resumo é do dia, não só do que falta ler");
    }

    #[test]
    fn varias_contas_e_lixeira_fora() {
        let mut s = store();
        s.set_folders("b", &[Folder { role: Role::Inbox, name: "INBOX".into() }, Folder { role: Role::Trash, name: "Trash".into() }]).unwrap();
        s.insert_messages("a", "INBOX", &[nm(1, "ana@exemplo.com", "A", NOW - 3 * H)]).unwrap();
        s.insert_messages("b", "INBOX", &[nm(1, "bia@exemplo.com", "B", NOW - H)]).unwrap();
        s.insert_messages("b", "Trash", &[nm(2, "x@exemplo.com", "Lixo", NOW - H)]).unwrap();
        let d = s.digest(NOW, 24).unwrap();
        assert_eq!(subjects(&d.people), ["B", "A"]);
        assert_eq!(d.total(), 2);
    }
}
