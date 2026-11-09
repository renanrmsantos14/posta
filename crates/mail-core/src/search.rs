//! Busca avançada: `de:`, `para:`, `assunto:`, `tem:anexo`, `é:não-lida`, `antes:`/`depois:` misturados ao texto livre.
//! O mesmo `Filters` vira SQL sobre a cópia local e consulta IMAP SEARCH no servidor.

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

use crate::folders::Role;
use crate::store::{fts_query, row_to_summary, MessageSummary, Store, NOT_DISCARDED, SUMMARY_COLS};
use crate::Result;

/// Data civil (dia de calendário, sem hora).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ymd(pub i32, pub u32, pub u32);

impl Ymd {
    /// Aceita `2026-11-08` e `08/11/2026`.
    pub(crate) fn parse(s: &str) -> Option<Ymd> {
        let nums = |sep: char| -> Option<Vec<i64>> { s.split(sep).map(|p| p.parse().ok()).collect() };
        let (y, m, d) = match (nums('-'), nums('/')) {
            (Some(v), _) if v.len() == 3 && s.split('-').next()?.len() == 4 => (v[0], v[1], v[2]),
            (_, Some(v)) if v.len() == 3 => (v[2], v[1], v[0]),
            _ => return None,
        };
        let valid = (1970..=2200).contains(&y) && (1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d);
        valid.then_some(Ymd(y as i32, m as u32, d as u32))
    }

    /// Meia-noite em Brasília (UTC-3, sem horário de verão desde 2019), em segundos Unix.
    pub fn unix(self) -> i64 {
        days_from_civil(self.0 as i64, self.1 as i64, self.2 as i64) * 86_400 + 3 * 3600
    }

    /// Formato do IMAP SEARCH: `8-Nov-2026`.
    fn imap(self) -> String {
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        format!("{}-{}-{}", self.2, MONTHS[self.1 as usize - 1], self.0)
    }
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filters {
    /// Texto livre (busca de texto completo).
    pub text: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub subject: Vec<String>,
    pub attachment: bool,
    /// `Some(true)` = só não lidas; `Some(false)` = só lidas.
    pub unread: Option<bool>,
    pub before: Option<Ymd>,
    pub after: Option<Ymd>,
}

impl Filters {
    /// Só texto livre, sem nenhum filtro.
    pub fn is_plain(&self) -> bool {
        *self == Filters { text: self.text.clone(), ..Default::default() }
    }

    /// Sem texto nem filtro algum.
    pub fn is_empty(&self) -> bool {
        self.is_plain() && self.text.trim().is_empty()
    }
}

/// Palavras separadas por espaço; aspas agrupam (`de:"ana souza"`) e saem do resultado.
fn tokens(input: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    for c in input.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub(crate) fn plain(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c {
        'é' | 'ê' => 'e',
        'ã' | 'á' | 'â' => 'a',
        'í' => 'i',
        'ó' | 'ô' => 'o',
        'ú' => 'u',
        c => c,
    }).collect()
}

pub fn parse(input: &str) -> Filters {
    let mut f = Filters::default();
    let mut text = Vec::new();
    for t in tokens(input) {
        let Some((key, val)) = t.split_once(':').filter(|(_, v)| !v.is_empty()) else {
            text.push(t);
            continue;
        };
        let (key, v) = (plain(key), plain(val));
        let ok = match key.as_str() {
            "de" | "from" => { f.from.push(val.to_string()); true }
            "para" | "to" => { f.to.push(val.to_string()); true }
            "assunto" | "subject" => { f.subject.push(val.to_string()); true }
            "tem" | "has" if matches!(v.as_str(), "anexo" | "anexos" | "attachment") => { f.attachment = true; true }
            "e" | "is" if matches!(v.as_str(), "nao-lida" | "naolida" | "unread") => { f.unread = Some(true); true }
            "e" | "is" if matches!(v.as_str(), "lida" | "read") => { f.unread = Some(false); true }
            "antes" | "before" => Ymd::parse(val).map(|d| f.before = Some(d)).is_some(),
            "depois" | "after" | "desde" => Ymd::parse(val).map(|d| f.after = Some(d)).is_some(),
            _ => false,
        };
        if !ok {
            text.push(t);
        }
    }
    f.text = text.join(" ");
    f
}

/// Onde buscar.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    Role { role: Role },
    Starred,
    Box { account: String, mailbox: String },
    Category { id: String },
}

fn like(s: &str) -> Value {
    Value::Text(format!("%{}%", s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
}

impl Filters {
    /// Condições SQL (juntas por AND) sobre `m`, com os parâmetros em `p` numerados a partir do tamanho atual.
    fn conditions(&self, p: &mut Vec<Value>) -> Vec<String> {
        let mut c = Vec::new();
        fn add(c: &mut Vec<String>, sql: &str, v: Value, p: &mut Vec<Value>) {
            p.push(v);
            c.push(sql.replace('?', &format!("?{}", p.len())));
        }
        for x in &self.from {
            add(&mut c, "(m.from_name LIKE ? ESCAPE '\\' OR m.from_addr LIKE ? ESCAPE '\\')", like(x), p);
        }
        for x in &self.to {
            add(&mut c, "(m.to_addrs LIKE ? ESCAPE '\\' OR m.cc_addrs LIKE ? ESCAPE '\\')", like(x), p);
        }
        for x in &self.subject {
            add(&mut c, "m.subject LIKE ? ESCAPE '\\'", like(x), p);
        }
        if self.attachment {
            c.push("m.attachments <> '[]'".into());
        }
        match self.unread {
            Some(true) => c.push("m.seen = 0".into()),
            Some(false) => c.push("m.seen = 1".into()),
            None => {}
        }
        if let Some(d) = self.before {
            add(&mut c, "m.date < ?", Value::Integer(d.unix()), p);
        }
        if let Some(d) = self.after {
            add(&mut c, "m.date >= ?", Value::Integer(d.unix()), p);
        }
        c
    }

    /// Consulta IMAP SEARCH (sem o prefixo `UID SEARCH`); `None` quando não há critério.
    /// `tem:anexo` vira multipart/mixed no Content-Type, que é o melhor que o IMAP oferece.
    pub fn imap_query(&self) -> Option<String> {
        let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let mut k: Vec<String> = Vec::new();
        k.extend(self.from.iter().map(|x| format!("FROM {}", q(x))));
        k.extend(self.to.iter().map(|x| format!("OR TO {} CC {}", q(x), q(x))));
        k.extend(self.subject.iter().map(|x| format!("SUBJECT {}", q(x))));
        k.extend(self.text.split_whitespace().map(|x| format!("TEXT {}", q(x))));
        if self.attachment {
            k.push("HEADER Content-Type \"multipart/mixed\"".into());
        }
        match self.unread {
            Some(true) => k.push("UNSEEN".into()),
            Some(false) => k.push("SEEN".into()),
            None => {}
        }
        if let Some(d) = self.before {
            k.push(format!("BEFORE {}", d.imap()));
        }
        if let Some(d) = self.after {
            k.push(format!("SINCE {}", d.imap()));
        }
        if k.is_empty() {
            return None;
        }
        let all = k.join(" ");
        Some(if all.is_ascii() { all } else { format!("CHARSET UTF-8 {all}") })
    }
}

impl Store {
    /// Busca na cópia local com texto livre e filtros, mais recentes primeiro.
    pub fn search_filtered(&self, scope: &Scope, f: &Filters, limit: u32) -> Result<Vec<MessageSummary>> {
        let mut p: Vec<Value> = Vec::new();
        let mut cond = f.conditions(&mut p);
        let mut from = "messages m".to_string();
        if let Some(q) = fts_query(&f.text) {
            p.push(Value::Text(q));
            from = "messages_fts f JOIN messages m ON m.id = f.rowid".to_string();
            cond.push(format!("messages_fts MATCH ?{}", p.len()));
        } else if !f.text.trim().is_empty() {
            return Ok(Vec::new());
        }
        match scope {
            Scope::Role { role } => {
                p.push(Value::Text(role.as_str().into()));
                let n = p.len();
                cond.push(format!(
                    "m.mailbox = COALESCE((SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = ?{n}), CASE ?{n} WHEN 'inbox' THEN 'INBOX' END)"
                ));
            }
            Scope::Starred => {
                cond.push("m.flagged = 1".into());
                cond.push(NOT_DISCARDED.into());
            }
            Scope::Box { account, mailbox } => {
                p.push(Value::Text(account.clone()));
                p.push(Value::Text(mailbox.clone()));
                cond.push(format!("m.account = ?{} AND m.mailbox = ?{}", p.len() - 1, p.len()));
            }
            Scope::Category { id } => {
                p.push(Value::Text(id.clone()));
                cond.push(format!(
                    "(SELECT c.category FROM message_categories c WHERE c.account = m.account AND c.msg_key = {}) = ?{}",
                    crate::store::MSG_KEY,
                    p.len()
                ));
                cond.push(NOT_DISCARDED.into());
            }
        }
        p.push(Value::Integer(limit.into()));
        let sql = format!(
            "SELECT {SUMMARY_COLS} FROM {from} WHERE {} ORDER BY m.date DESC, m.id DESC LIMIT ?{}",
            cond.join(" AND "),
            p.len()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p), row_to_summary)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Cópias locais de uma pasta pelos UIDs (resultado da busca no servidor), mais recentes primeiro.
    pub fn by_uids(&self, account: &str, mailbox: &str, uids: &[u32]) -> Result<Vec<MessageSummary>> {
        let list = uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.account = ?1 AND m.mailbox = ?2 AND m.uid IN ({list})
             ORDER BY m.date DESC, m.id DESC"
        ))?;
        let rows = stmt.query_map(rusqlite::params![account, mailbox], row_to_summary)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewMessage;

    #[test]
    fn analisa_filtros_aspas_e_texto_livre() {
        let f = parse(r#"orçamento de:"ana souza" para:bia assunto:fatura tem:anexo é:não-lida antes:2026-11-08 depois:01/10/2026 unknown:x"#);
        assert_eq!(f.text, "orçamento unknown:x");
        assert_eq!((f.from.as_slice(), f.to.as_slice(), f.subject.as_slice()), (&["ana souza".to_string()][..], &["bia".to_string()][..], &["fatura".to_string()][..]));
        assert!(f.attachment);
        assert_eq!(f.unread, Some(true));
        assert_eq!(f.before, Some(Ymd(2026, 11, 8)));
        assert_eq!(f.after, Some(Ymd(2026, 10, 1)));
        assert!(!f.is_plain());
        assert!(parse("só texto").is_plain());
        // Data inválida e filtro sem valor ficam como texto.
        assert_eq!(parse("antes:2026-13-40 de:").text, "antes:2026-13-40 de:");
    }

    #[test]
    fn data_vira_meia_noite_de_brasilia_e_formato_imap() {
        assert_eq!(Ymd(1970, 1, 1).unix(), 3 * 3600);
        assert_eq!(Ymd(2026, 11, 8).unix(), 1794106800);
        assert_eq!(Ymd(2026, 11, 8).imap(), "8-Nov-2026");
    }

    #[test]
    fn consulta_imap() {
        assert_eq!(parse("tem:anexo").imap_query().unwrap(), "HEADER Content-Type \"multipart/mixed\"");
        assert_eq!(parse("de:ana é:não-lida boleto antes:2026-11-08").imap_query().unwrap(), "FROM \"ana\" TEXT \"boleto\" UNSEEN BEFORE 8-Nov-2026");
        assert!(parse("ação").imap_query().unwrap().starts_with("CHARSET UTF-8 "));
        assert_eq!(parse("").imap_query(), None);
    }

    fn msg(uid: u32, from: &str, subject: &str, date: i64, seen: bool, att: bool) -> NewMessage {
        NewMessage {
            uid, from_name: from.into(), from_addr: format!("{}@x.com", from.to_lowercase()), subject: subject.into(), date,
            body_text: "corpo do translado".into(), body_html: String::new(),
            attachments: if att { vec![crate::Attachment { name: "a.pdf".into(), mime: "application/pdf".into(), size: 1, index: 1 }] } else { vec![] },
            seen, message_id: format!("{uid}@x"), refs: String::new(), reply_addr: String::new(), flagged: false,
            to_addrs: "eu@y.com".into(), cc_addrs: String::new(),
        }
    }

    #[test]
    fn filtros_no_sql_combinam_com_texto_e_escopo() {
        let mut s = Store::open_in_memory().unwrap();
        let d = Ymd(2026, 11, 1).unix();
        s.insert_messages("a", "INBOX", &[
            msg(1, "Ana", "Fatura de novembro", d + 100, false, true),
            msg(2, "Ana", "Reunião", d - 86_400 * 10, true, false),
            msg(3, "Bia", "Fatura 100%", d + 200, false, false),
        ]).unwrap();
        let inbox = Scope::Role { role: Role::Inbox };
        let ids = |q: &str| s.search_filtered(&inbox, &parse(q), 50).unwrap().into_iter().map(|m| m.uid).collect::<Vec<_>>();
        assert_eq!(ids("de:ana"), [1, 2]);
        assert_eq!(ids("de:ana é:não-lida"), [1]);
        assert_eq!(ids("tem:anexo"), [1]);
        assert_eq!(ids("assunto:fatura depois:2026-11-01"), [3, 1]);
        assert_eq!(ids("antes:2026-11-01"), [2]);
        assert_eq!(ids("assunto:100%"), [3]);
        assert_eq!(ids("assunto:fatura translado"), [3, 1]);
        assert_eq!(ids("para:eu@y.com"), [3, 1, 2]);
        assert!(s.search_filtered(&Scope::Box { account: "a".into(), mailbox: "Outra".into() }, &parse("de:ana"), 50).unwrap().is_empty());
        assert_eq!(s.by_uids("a", "INBOX", &[1, 3]).unwrap().len(), 2);
    }
}
