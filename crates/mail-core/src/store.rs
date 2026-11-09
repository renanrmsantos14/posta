//! Cópia local da caixa em SQLite. Toda leitura da UI vem daqui, nunca da rede.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

use crate::folders::{Folder, Role};
use crate::Result;

/// O banco é só uma cópia do servidor: mudou o esquema, apaga e sincroniza de novo.
const SCHEMA_VERSION: i32 = 4;

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS mailbox_state (
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uid_validity INTEGER NOT NULL,
  PRIMARY KEY (account, mailbox)
);
CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY,
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uid INTEGER NOT NULL,
  from_name TEXT NOT NULL DEFAULT '',
  from_addr TEXT NOT NULL DEFAULT '',
  subject TEXT NOT NULL DEFAULT '',
  date INTEGER NOT NULL,
  snippet TEXT NOT NULL DEFAULT '',
  body_text TEXT NOT NULL DEFAULT '',
  body_html TEXT NOT NULL DEFAULT '',
  seen INTEGER NOT NULL DEFAULT 0,
  flagged INTEGER NOT NULL DEFAULT 0,
  message_id TEXT NOT NULL DEFAULT '',
  refs TEXT NOT NULL DEFAULT '',
  reply_addr TEXT NOT NULL DEFAULT '',
  to_addrs TEXT NOT NULL DEFAULT '',
  cc_addrs TEXT NOT NULL DEFAULT '',
  UNIQUE (account, mailbox, uid)
);
CREATE INDEX IF NOT EXISTS messages_by_date ON messages (date DESC);
CREATE TABLE IF NOT EXISTS folders (
  account TEXT NOT NULL,
  role TEXT NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (account, role)
);
CREATE INDEX IF NOT EXISTS messages_by_box ON messages (account, mailbox, date DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5 (
  subject, from_name, from_addr, body_text,
  content = 'messages', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts (rowid, subject, from_name, from_addr, body_text)
  VALUES (new.id, new.subject, new.from_name, new.from_addr, new.body_text);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts (messages_fts, rowid, subject, from_name, from_addr, body_text)
  VALUES ('delete', old.id, old.subject, old.from_name, old.from_addr, old.body_text);
END;
";

/// Mensagem já interpretada, pronta para gravar.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub uid: u32,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub date: i64,
    pub body_text: String,
    /// HTML original; vazio quando a mensagem só tem texto. A busca continua sobre `body_text`.
    pub body_html: String,
    pub seen: bool,
    /// Estrela (`\Flagged`).
    pub flagged: bool,
    /// Message-ID sem os sinais < >.
    pub message_id: String,
    /// IDs de References, separados por espaço, sem < >.
    pub refs: String,
    /// Para quem a resposta vai: Reply-To, ou o remetente.
    pub reply_addr: String,
    /// Endereços de To e Cc, separados por vírgula (para "Responder a todos").
    pub to_addrs: String,
    pub cc_addrs: String,
}

/// O que é preciso para responder mantendo a conversa agrupada.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyRef {
    pub account: String,
    pub mailbox: String,
    pub uid: u32,
    pub message_id: String,
    pub refs: String,
}

/// Linha da lista de mensagens (sem o corpo).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MessageSummary {
    pub id: i64,
    pub account: String,
    pub uid: u32,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub date: i64,
    pub snippet: String,
    pub seen: bool,
    pub flagged: bool,
    /// Papel da pasta onde a mensagem está (`inbox`, `trash`...); `None` se a pasta não tem papel.
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDetail {
    #[serde(flatten)]
    pub summary: MessageSummary,
    pub body_text: String,
    /// Vazio quando a mensagem só tem texto.
    pub body_html: String,
    pub reply_addr: String,
    pub to_addrs: String,
    pub cc_addrs: String,
}

pub struct Store {
    conn: Connection,
}

/// Nome da pasta do papel ?1 na conta da linha; Caixa de entrada sem linha em `folders` é "INBOX".
const IN_ROLE: &str = "m.mailbox = COALESCE(
    (SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = ?1),
    CASE ?1 WHEN 'inbox' THEN 'INBOX' END)";

const SUMMARY_COLS: &str = "m.id, m.account, m.uid, m.from_name, m.from_addr, m.subject, m.date, m.snippet, m.seen, m.flagged,
    COALESCE((SELECT f.role FROM folders f WHERE f.account = m.account AND f.name = m.mailbox),
    CASE WHEN m.mailbox = 'INBOX' THEN 'inbox' END)";

/// Fora da Lixeira e do Spam.
const NOT_DISCARDED: &str = "COALESCE((SELECT f.role FROM folders f WHERE f.account = m.account AND f.name = m.mailbox), '') NOT IN ('trash', 'junk')";

fn row_to_summary(r: &rusqlite::Row) -> rusqlite::Result<MessageSummary> {
    Ok(MessageSummary {
        id: r.get(0)?,
        account: r.get(1)?,
        uid: r.get(2)?,
        from_name: r.get(3)?,
        from_addr: r.get(4)?,
        subject: r.get(5)?,
        date: r.get(6)?,
        snippet: r.get(7)?,
        seen: r.get(8)?,
        flagged: r.get(9)?,
        role: r.get(10)?,
    })
}

fn snippet(body: &str) -> String {
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

/// Converte o texto digitado em consulta FTS5 segura: cada palavra vira prefixo entre aspas.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|w| format!("\"{}\"*", w.replace('"', "")))
        .filter(|t| t != "\"\"*")
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS messages_fts; DROP TABLE IF EXISTS messages; DROP TABLE IF EXISTS mailbox_state;",
            )?;
        }
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { conn })
    }

    pub fn uid_validity(&self, account: &str, mailbox: &str) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT uid_validity FROM mailbox_state WHERE account = ?1 AND mailbox = ?2",
                params![account, mailbox],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Grava o UIDVALIDITY; se mudou, a cópia local da pasta deixa de valer e é apagada.
    pub fn set_uid_validity(&mut self, account: &str, mailbox: &str, value: u32) -> Result<()> {
        if self.uid_validity(account, mailbox)? == Some(value) {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM messages WHERE account = ?1 AND mailbox = ?2",
            params![account, mailbox],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO mailbox_state (account, mailbox, uid_validity) VALUES (?1, ?2, ?3)",
            params![account, mailbox, value],
        )?;
        Ok(tx.commit()?)
    }

    pub fn local_uids(&self, account: &str, mailbox: &str) -> Result<Vec<u32>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT uid FROM messages WHERE account = ?1 AND mailbox = ?2")?;
        let uids = stmt
            .query_map(params![account, mailbox], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(uids)
    }

    pub fn insert_messages(&mut self, account: &str, mailbox: &str, msgs: &[NewMessage]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO messages
                 (account, mailbox, uid, from_name, from_addr, subject, date, snippet, body_text, body_html, seen,
                  message_id, refs, reply_addr, flagged, to_addrs, cc_addrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            )?;
            for m in msgs {
                stmt.execute(params![
                    account,
                    mailbox,
                    m.uid,
                    m.from_name,
                    m.from_addr,
                    m.subject,
                    m.date,
                    snippet(&m.body_text),
                    m.body_text,
                    m.body_html,
                    m.seen,
                    m.message_id,
                    m.refs,
                    m.reply_addr,
                    m.flagged,
                    m.to_addrs,
                    m.cc_addrs
                ])?;
            }
        }
        Ok(tx.commit()?)
    }

    /// Aplica o estado do servidor: remove o que sumiu e atualiza as flags de lida e estrela.
    /// Cada item é `(uid, lida, estrela)`.
    pub fn apply_server_state(&mut self, account: &str, mailbox: &str, server: &[(u32, bool, bool)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS server_uids (uid INTEGER PRIMARY KEY, seen INTEGER, flagged INTEGER); DELETE FROM server_uids;")?;
        {
            let mut ins = tx.prepare_cached("INSERT OR REPLACE INTO server_uids (uid, seen, flagged) VALUES (?1, ?2, ?3)")?;
            for (uid, seen, flagged) in server {
                ins.execute(params![uid, seen, flagged])?;
            }
        }
        tx.execute(
            "DELETE FROM messages WHERE account = ?1 AND mailbox = ?2
             AND uid NOT IN (SELECT uid FROM server_uids)",
            params![account, mailbox],
        )?;
        tx.execute(
            "UPDATE messages SET seen = s.seen, flagged = s.flagged
             FROM server_uids s
             WHERE s.uid = messages.uid AND messages.account = ?1 AND messages.mailbox = ?2
             AND (messages.seen <> s.seen OR messages.flagged <> s.flagged)",
            params![account, mailbox],
        )?;
        Ok(tx.commit()?)
    }

    pub fn set_seen(&self, id: i64, seen: bool) -> Result<()> {
        self.conn
            .execute("UPDATE messages SET seen = ?2 WHERE id = ?1", params![id, seen])?;
        Ok(())
    }

    pub fn set_flagged(&self, id: i64, flagged: bool) -> Result<()> {
        self.conn
            .execute("UPDATE messages SET flagged = ?2 WHERE id = ?1", params![id, flagged])?;
        Ok(())
    }

    /// Não lidas por papel de pasta, somando as contas (papéis sem pasta ficam em 0).
    pub fn unread_counts(&self) -> Result<Vec<(Role, u32)>> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT COUNT(*) FROM messages m WHERE m.seen = 0 AND {IN_ROLE}"))?;
        Role::ALL
            .iter()
            .map(|r| Ok((*r, stmt.query_row(params![r.as_str()], |row| row.get(0))?)))
            .collect()
    }

    /// Pasta virtual "Com estrela": todas as contas, exceto Lixeira e Spam.
    pub fn list_starred(&self, limit: u32, offset: u32) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.flagged = 1 AND {NOT_DISCARDED}
             ORDER BY m.date DESC, m.id DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt
            .query_map(params![limit, offset], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn search_starred(&self, query: &str, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 AND m.flagged = 1 AND {NOT_DISCARDED} ORDER BY m.date DESC LIMIT ?2"
        ))?;
        let rows = stmt
            .query_map(params![q, limit], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Caixa unificada: todas as contas, mais recentes primeiro.
    pub fn list(&self, mailbox: &str, limit: u32, offset: u32) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.mailbox = ?1
             ORDER BY m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt
            .query_map(params![mailbox, limit, offset], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 ORDER BY m.date DESC LIMIT ?2"
        ))?;
        let rows = stmt
            .query_map(params![q, limit], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn get(&self, id: i64) -> Result<Option<MessageDetail>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SUMMARY_COLS}, m.body_text, m.body_html, m.reply_addr, m.to_addrs, m.cc_addrs FROM messages m WHERE m.id = ?1"),
                params![id],
                |r| {
                    Ok(MessageDetail {
                        summary: row_to_summary(r)?,
                        body_text: r.get(11)?,
                        body_html: r.get(12)?,
                        reply_addr: r.get(13)?,
                        to_addrs: r.get(14)?,
                        cc_addrs: r.get(15)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn reply_ref(&self, id: i64) -> Result<Option<ReplyRef>> {
        Ok(self
            .conn
            .query_row(
                "SELECT account, mailbox, uid, message_id, refs FROM messages WHERE id = ?1",
                params![id],
                |r| {
                    Ok(ReplyRef {
                        account: r.get(0)?,
                        mailbox: r.get(1)?,
                        uid: r.get(2)?,
                        message_id: r.get(3)?,
                        refs: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn set_reply_addr(&self, id: i64, addr: &str) -> Result<()> {
        self.conn.execute("UPDATE messages SET reply_addr = ?1 WHERE id = ?2", params![addr, id])?;
        Ok(())
    }

    pub fn set_folders(&mut self, account: &str, folders: &[Folder]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM folders WHERE account = ?1", params![account])?;
        for f in folders {
            tx.execute(
                "INSERT INTO folders (account, role, name) VALUES (?1, ?2, ?3)",
                params![account, f.role.as_str(), f.name],
            )?;
        }
        Ok(tx.commit()?)
    }

    pub fn folders(&self, account: &str) -> Result<Vec<Folder>> {
        let mut stmt = self.conn.prepare_cached("SELECT role, name FROM folders WHERE account = ?1")?;
        let rows = stmt
            .query_map(params![account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out: Vec<Folder> = rows
            .into_iter()
            .filter_map(|(role, name)| Some(Folder { role: Role::parse(&role)?, name }))
            .collect();
        out.sort_by_key(|f| Role::ALL.iter().position(|r| *r == f.role));
        Ok(out)
    }

    pub fn folder_name(&self, account: &str, role: Role) -> Result<Option<String>> {
        let found: Option<String> = self
            .conn
            .query_row(
                "SELECT name FROM folders WHERE account = ?1 AND role = ?2",
                params![account, role.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.or_else(|| (role == Role::Inbox).then(|| "INBOX".to_string())))
    }

    /// Pasta unificada: o papel em todas as contas, mais recentes primeiro.
    pub fn list_folder(&self, role: Role, limit: u32, offset: u32) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE {IN_ROLE}
             ORDER BY m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt
            .query_map(params![role.as_str(), limit, offset], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn count_folder(&self, role: Role) -> Result<u32> {
        Ok(self.conn.query_row(
            &format!("SELECT COUNT(*) FROM messages m WHERE {IN_ROLE}"),
            params![role.as_str()],
            |r| r.get(0),
        )?)
    }

    pub fn search_folder(&self, query: &str, role: Role, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?2 AND {IN_ROLE} ORDER BY m.date DESC LIMIT ?3"
        ))?;
        let rows = stmt
            .query_map(params![role.as_str(), q, limit], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Apaga a cópia local (o gatilho limpa o FTS). Usado depois que o servidor confirmou mover/excluir.
    pub fn remove(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(uid: u32, subject: &str, body: &str, date: i64) -> NewMessage {
        NewMessage {
            uid,
            from_name: "Ana Souza".into(),
            from_addr: "ana@exemplo.com".into(),
            subject: subject.into(),
            date,
            body_text: body.into(),
            body_html: String::new(),
            seen: false,
            message_id: format!("{uid}@exemplo.com"),
            refs: String::new(),
            reply_addr: "ana@exemplo.com".into(),
            flagged: false,
            to_addrs: "eu@exemplo.com, bia@exemplo.com".into(),
            cc_addrs: String::new(),
        }
    }

    #[test]
    fn lista_ordenada_e_busca_sem_acento() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        s.insert_messages(
            "a",
            "INBOX",
            &[msg(1, "Cotação aeroporto", "Translado   para\nGuarulhos", 100), msg(2, "Reunião", "pauta", 200)],
        )
        .unwrap();
        let list = s.list("INBOX", 50, 0).unwrap();
        assert_eq!(list.iter().map(|m| m.uid).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(list[1].snippet, "Translado para Guarulhos");
        assert_eq!(s.search("cotacao guaru", 10).unwrap()[0].uid, 1);
        assert!(s.search("\"", 10).unwrap().is_empty());
    }

    #[test]
    fn estado_do_servidor_remove_e_marca_lida() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "x", "", 1), msg(2, "y", "", 2)]).unwrap();
        s.apply_server_state("a", "INBOX", &[(2, true, false)]).unwrap();
        let list = s.list("INBOX", 50, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].seen);
        assert!(s.search("x", 10).unwrap().is_empty(), "FTS acompanha a exclusão");
    }

    #[test]
    fn uid_validity_novo_limpa_a_pasta() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "x", "", 1)]).unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        assert_eq!(s.local_uids("a", "INBOX").unwrap(), [1]);
        s.set_uid_validity("a", "INBOX", 2).unwrap();
        assert!(s.local_uids("a", "INBOX").unwrap().is_empty());
    }

    #[test]
    fn referencia_para_resposta() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(7, "x", "", 1)]).unwrap();
        let id = s.list("INBOX", 1, 0).unwrap()[0].id;
        let r = s.reply_ref(id).unwrap().unwrap();
        assert_eq!((r.uid, r.message_id.as_str()), (7, "7@exemplo.com"));
        assert_eq!(s.get(id).unwrap().unwrap().reply_addr, "ana@exemplo.com");
        assert!(s.reply_ref(999).unwrap().is_none());
    }

    #[test]
    fn set_reply_addr_corrige_o_destinatario() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(7, "x", "", 1)]).unwrap();
        let id = s.list("INBOX", 1, 0).unwrap()[0].id;
        s.set_reply_addr(id, "bia@exemplo.com").unwrap();
        assert_eq!(s.get(id).unwrap().unwrap().reply_addr, "bia@exemplo.com");
    }

    fn folders_a() -> Vec<Folder> {
        vec![
            Folder { role: Role::Inbox, name: "INBOX".into() },
            Folder { role: Role::Trash, name: "Lixeira".into() },
        ]
    }

    #[test]
    fn lista_e_busca_por_papel_juntam_contas() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        s.set_folders("b", &[Folder { role: Role::Trash, name: "Trash".into() }]).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "Entrada", "", 1)]).unwrap();
        s.insert_messages("a", "Lixeira", &[msg(2, "Lixo a", "cotacao", 2)]).unwrap();
        s.insert_messages("b", "Trash", &[msg(3, "Lixo b", "cotacao", 3)]).unwrap();
        s.insert_messages("b", "INBOX", &[msg(4, "Entrada b", "", 4)]).unwrap();

        let lixo: Vec<_> = s.list_folder(Role::Trash, 50, 0).unwrap().into_iter().map(|m| m.subject).collect();
        assert_eq!(lixo, ["Lixo b", "Lixo a"]);
        // Conta b não tem linha de Inbox: cai no padrão "INBOX".
        assert_eq!(s.list_folder(Role::Inbox, 50, 0).unwrap().len(), 2);
        assert_eq!(s.count_folder(Role::Trash).unwrap(), 2);
        assert_eq!(s.search_folder("cotacao", Role::Trash, 10).unwrap().len(), 2);
        assert!(s.search_folder("cotacao", Role::Inbox, 10).unwrap().is_empty());
        assert_eq!(s.folder_name("b", Role::Inbox).unwrap().as_deref(), Some("INBOX"));
        assert_eq!(s.folder_name("b", Role::Archive).unwrap(), None);
    }

    #[test]
    fn set_folders_substitui_e_remove_apaga_do_fts() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        s.set_folders("a", &[Folder { role: Role::Sent, name: "Sent".into() }]).unwrap();
        assert_eq!(s.folders("a").unwrap(), [Folder { role: Role::Sent, name: "Sent".into() }]);

        s.insert_messages("a", "INBOX", &[msg(1, "Cotação", "", 1)]).unwrap();
        let id = s.list("INBOX", 1, 0).unwrap()[0].id;
        s.remove(id).unwrap();
        assert!(s.get(id).unwrap().is_none());
        assert!(s.search("cotacao", 10).unwrap().is_empty());
    }

    #[test]
    fn esquema_antigo_e_recriado() {
        let dir = std::env::temp_dir().join(format!("posta-schema-{}.sqlite", std::process::id()));
        {
            let c = Connection::open(&dir).unwrap();
            c.execute_batch("CREATE TABLE messages (id INTEGER PRIMARY KEY, subject TEXT);").unwrap();
        }
        let mut s = Store::open(&dir).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "x", "", 1)]).unwrap();
        assert_eq!(s.list("INBOX", 5, 0).unwrap().len(), 1);
        drop(s);
        for ext in ["", "-wal", "-shm"] {
            std::fs::remove_file(format!("{}{ext}", dir.display())).ok();
        }
    }

    #[test]
    fn html_volta_no_detalhe_e_busca_segue_no_texto() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        let mut m = msg(1, "Promo", "oferta de inverno", 100);
        m.body_html = "<p>oferta de <b>inverno</b></p>".into();
        s.insert_messages("a", "INBOX", &[m]).unwrap();
        let id = s.search("inverno", 10).unwrap()[0].id;
        let d = s.get(id).unwrap().unwrap();
        assert_eq!(d.body_html, "<p>oferta de <b>inverno</b></p>");
        assert_eq!(d.body_text, "oferta de inverno");
        assert!(s.search("strong", 10).unwrap().is_empty());
    }

    #[test]
    fn estrela_e_lida_vem_do_servidor_e_do_usuario() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "x", "", 1), msg(2, "y", "", 2)]).unwrap();
        let id1 = s.list("INBOX", 5, 0).unwrap().iter().find(|m| m.uid == 1).unwrap().id;
        s.set_flagged(id1, true).unwrap();
        s.set_seen(id1, true).unwrap();
        let m = s.list("INBOX", 5, 0).unwrap().into_iter().find(|m| m.uid == 1).unwrap();
        assert!(m.flagged && m.seen);
        assert_eq!(m.role.as_deref(), Some("inbox"));
        // O servidor manda: tirou a estrela da 1 e marcou a 2.
        s.apply_server_state("a", "INBOX", &[(1, true, false), (2, false, true)]).unwrap();
        let list = s.list("INBOX", 5, 0).unwrap();
        assert!(!list.iter().find(|m| m.uid == 1).unwrap().flagged);
        assert!(list.iter().find(|m| m.uid == 2).unwrap().flagged);
    }

    #[test]
    fn nao_lidas_por_pasta_e_pasta_com_estrela() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "a", "", 1), msg(2, "b cotacao", "", 2)]).unwrap();
        s.insert_messages("a", "Lixeira", &[msg(3, "lixo", "", 3)]).unwrap();
        let get = |s: &Store, role| s.unread_counts().unwrap().into_iter().find(|(r, _)| *r == role).unwrap().1;
        assert_eq!((get(&s, Role::Inbox), get(&s, Role::Trash), get(&s, Role::Junk)), (2, 1, 0));
        let ids: Vec<_> = s.list("INBOX", 5, 0).unwrap().iter().map(|m| m.id).collect();
        s.set_seen(ids[0], true).unwrap();
        assert_eq!(get(&s, Role::Inbox), 1);

        // Estrela em mensagem da Entrada e da Lixeira: a Lixeira fica fora da pasta virtual.
        let trash_id = s.list_folder(Role::Trash, 5, 0).unwrap()[0].id;
        s.set_flagged(trash_id, true).unwrap();
        s.set_flagged(ids[0], true).unwrap();
        let starred = s.list_starred(10, 0).unwrap();
        assert_eq!(starred.len(), 1);
        assert_eq!(starred[0].id, ids[0]);
        assert_eq!(s.search_starred("cotacao", 10).unwrap().len(), 1);
        assert!(s.search_starred("lixo", 10).unwrap().is_empty());
        s.set_flagged(ids[0], false).unwrap();
        assert!(s.list_starred(10, 0).unwrap().is_empty());
    }

    #[test]
    fn detalhe_traz_to_e_cc_para_responder_a_todos() {
        let mut s = Store::open_in_memory().unwrap();
        let mut m = msg(1, "x", "", 1);
        m.cc_addrs = "carla@exemplo.com".into();
        s.insert_messages("a", "INBOX", &[m]).unwrap();
        let id = s.list("INBOX", 1, 0).unwrap()[0].id;
        let d = s.get(id).unwrap().unwrap();
        assert_eq!((d.to_addrs.as_str(), d.cc_addrs.as_str()), ("eu@exemplo.com, bia@exemplo.com", "carla@exemplo.com"));
    }
}
