//! Regras automáticas: condição simples sobre remetente, assunto ou destinatário e uma ação.
//! A regra é avaliada aqui; quem aplica as ações (servidor e banco) é o app.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::folders::Role;
use crate::search::plain;
use crate::store::Store;
use crate::Result;

/// Regra guardada na configuração `rules` (JSON), editada em Configurações.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    pub id: String,
    /// `from`, `subject` ou `to`.
    pub field: String,
    pub contains: String,
    /// `move`, `read`, `star`, `category` ou `archive`.
    pub action: String,
    /// Pasta (`move`) ou id da categoria (`category`).
    #[serde(default)]
    pub arg: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// Mensagem da Caixa de entrada vista pelas regras.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: i64,
    pub account: String,
    pub uid: u32,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub to_addrs: String,
    pub cc_addrs: String,
    pub reply_addr: String,
    pub date: i64,
}

/// O que fazer com uma mensagem depois de passar pelas regras.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plan {
    pub read: bool,
    pub star: bool,
    pub category: Option<String>,
    pub archive: bool,
    /// Pasta do usuário de destino. Só a primeira regra de mover vale.
    pub move_to: Option<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        *self == Plan::default()
    }
}

/// Lê as regras do JSON da configuração; JSON inválido vira lista vazia.
pub fn parse_rules(json: Option<&str>) -> Vec<Rule> {
    json.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default()
}

impl Rule {
    pub fn matches(&self, c: &Candidate) -> bool {
        let needle = plain(self.contains.trim());
        if !self.enabled || needle.is_empty() {
            return false;
        }
        let hay = match self.field.as_str() {
            "from" => format!("{} {}", c.from_name, c.from_addr),
            "subject" => c.subject.clone(),
            "to" => format!("{} {}", c.to_addrs, c.cc_addrs),
            _ => return false,
        };
        plain(&hay).contains(&needle)
    }
}

/// Junta as ações de todas as regras que casam.
pub fn plan(rules: &[Rule], c: &Candidate) -> Plan {
    let mut p = Plan::default();
    for r in rules.iter().filter(|r| r.matches(c)) {
        match r.action.as_str() {
            "read" => p.read = true,
            "star" => p.star = true,
            "archive" => p.archive = true,
            "category" if p.category.is_none() && !r.arg.is_empty() => p.category = Some(r.arg.clone()),
            "move" if p.move_to.is_none() && !r.arg.is_empty() => p.move_to = Some(r.arg.clone()),
            _ => {}
        }
    }
    p
}

impl Store {
    /// Maior id local; o app o guarda antes de sincronizar para saber o que é novo depois.
    pub fn max_id(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| r.get(0))?)
    }

    /// Mensagens da Caixa de entrada de `account` com id maior que `after` (`0` = todas).
    pub fn inbox_after(&self, account: &str, after: i64) -> Result<Vec<Candidate>> {
        let mut st = self.conn.prepare(
            "SELECT m.id, m.account, m.uid, m.from_name, m.from_addr, m.subject, m.to_addrs, m.cc_addrs, m.reply_addr, m.date
             FROM messages m
             WHERE m.account = ?1 AND m.id > ?2 AND m.mailbox = COALESCE(
               (SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = ?3), 'INBOX')
             ORDER BY m.id",
        )?;
        let rows = st
            .query_map(params![account, after, Role::Inbox.as_str()], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    uid: r.get(2)?,
                    from_name: r.get(3)?,
                    from_addr: r.get(4)?,
                    subject: r.get(5)?,
                    to_addrs: r.get(6)?,
                    cc_addrs: r.get(7)?,
                    reply_addr: r.get(8)?,
                    date: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewMessage;

    fn rule(field: &str, contains: &str, action: &str, arg: &str) -> Rule {
        Rule { id: format!("{field}{contains}"), field: field.into(), contains: contains.into(), action: action.into(), arg: arg.into(), enabled: true }
    }

    fn cand() -> Candidate {
        Candidate {
            id: 1, account: "a".into(), uid: 1, from_name: "Banco Itaú".into(), from_addr: "avisos@itau.com.br".into(),
            subject: "Fatura de Novembro".into(), to_addrs: "eu@x.com".into(), cc_addrs: "fin@x.com".into(), reply_addr: String::new(), date: 0,
        }
    }

    #[test]
    fn casa_sem_ligar_para_caixa_e_acento() {
        let c = cand();
        assert!(rule("from", "itau", "read", "").matches(&c));
        assert!(rule("from", "ITAÚ", "read", "").matches(&c));
        assert!(rule("subject", "fatura", "read", "").matches(&c));
        assert!(rule("to", "fin@x", "read", "").matches(&c));
        assert!(!rule("subject", "boleto", "read", "").matches(&c));
        assert!(!rule("from", "  ", "read", "").matches(&c));
        let mut off = rule("from", "itau", "read", "");
        off.enabled = false;
        assert!(!off.matches(&c));
        assert!(!rule("campo", "itau", "read", "").matches(&c));
    }

    #[test]
    fn plano_junta_acoes_e_a_primeira_de_mover_vence() {
        let rules = vec![
            rule("from", "itau", "move", "Bancos"),
            rule("subject", "fatura", "move", "Outra"),
            rule("subject", "fatura", "star", ""),
            rule("from", "itau", "category", "c1"),
            rule("from", "itau", "archive", ""),
            rule("subject", "x", "read", ""),
        ];
        let p = plan(&rules, &cand());
        assert_eq!(
            p,
            Plan { read: false, star: true, category: Some("c1".into()), archive: true, move_to: Some("Bancos".into()) }
        );
        assert!(plan(&rules[5..], &cand()).is_empty());
    }

    #[test]
    fn le_regras_do_json() {
        let json = r#"[{"id":"1","field":"from","contains":"a","action":"read"}]"#;
        let r = parse_rules(Some(json));
        assert_eq!((r.len(), r[0].enabled, r[0].arg.as_str()), (1, true, ""));
        assert!(parse_rules(Some("lixo")).is_empty());
        assert!(parse_rules(None).is_empty());
    }

    #[test]
    fn so_traz_novas_da_entrada() {
        let mut s = Store::open_in_memory().unwrap();
        let m = |uid: u32| NewMessage {
            uid, from_name: String::new(), from_addr: "a@b.com".into(), subject: format!("s{uid}"), date: uid as i64,
            body_text: String::new(), body_html: String::new(), attachments: vec![], seen: false, message_id: format!("{uid}@x"),
            refs: String::new(), reply_addr: String::new(), flagged: false, to_addrs: String::new(), cc_addrs: String::new(),
        };
        s.insert_messages("a", "INBOX", &[m(1), m(2)]).unwrap();
        let mark = s.max_id().unwrap();
        s.insert_messages("a", "INBOX", &[m(3)]).unwrap();
        s.insert_messages("a", "Outra", &[m(4)]).unwrap();
        s.insert_messages("b", "INBOX", &[m(5)]).unwrap();
        let new = s.inbox_after("a", mark).unwrap();
        assert_eq!(new.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["s3"]);
        assert_eq!(s.inbox_after("a", 0).unwrap().len(), 3);
    }
}
