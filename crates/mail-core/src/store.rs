//! Cópia local da caixa em SQLite. Toda leitura da UI vem daqui, nunca da rede.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

use crate::Result;

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
  seen INTEGER NOT NULL DEFAULT 0,
  UNIQUE (account, mailbox, uid)
);
CREATE INDEX IF NOT EXISTS messages_by_date ON messages (date DESC);
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
    pub seen: bool,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDetail {
    #[serde(flatten)]
    pub summary: MessageSummary,
    pub body_text: String,
}

pub struct Store {
    conn: Connection,
}

const SUMMARY_COLS: &str =
    "m.id, m.account, m.uid, m.from_name, m.from_addr, m.subject, m.date, m.snippet, m.seen";

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
        conn.execute_batch(SCHEMA)?;
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
                 (account, mailbox, uid, from_name, from_addr, subject, date, snippet, body_text, seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                    m.seen
                ])?;
            }
        }
        Ok(tx.commit()?)
    }

    /// Aplica o estado do servidor: remove o que sumiu e atualiza a flag de lida.
    pub fn apply_server_state(&mut self, account: &str, mailbox: &str, server: &[(u32, bool)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS server_uids (uid INTEGER PRIMARY KEY, seen INTEGER); DELETE FROM server_uids;")?;
        {
            let mut ins = tx.prepare_cached("INSERT OR REPLACE INTO server_uids (uid, seen) VALUES (?1, ?2)")?;
            for (uid, seen) in server {
                ins.execute(params![uid, seen])?;
            }
        }
        tx.execute(
            "DELETE FROM messages WHERE account = ?1 AND mailbox = ?2
             AND uid NOT IN (SELECT uid FROM server_uids)",
            params![account, mailbox],
        )?;
        tx.execute(
            "UPDATE messages SET seen = (SELECT s.seen FROM server_uids s WHERE s.uid = messages.uid)
             WHERE account = ?1 AND mailbox = ?2
             AND seen <> (SELECT s.seen FROM server_uids s WHERE s.uid = messages.uid)",
            params![account, mailbox],
        )?;
        Ok(tx.commit()?)
    }

    pub fn set_seen(&self, id: i64, seen: bool) -> Result<()> {
        self.conn
            .execute("UPDATE messages SET seen = ?2 WHERE id = ?1", params![id, seen])?;
        Ok(())
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
                &format!("SELECT {SUMMARY_COLS}, m.body_text FROM messages m WHERE m.id = ?1"),
                params![id],
                |r| {
                    Ok(MessageDetail {
                        summary: row_to_summary(r)?,
                        body_text: r.get(9)?,
                    })
                },
            )
            .optional()?)
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
            seen: false,
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
        s.apply_server_state("a", "INBOX", &[(2, true)]).unwrap();
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
}
