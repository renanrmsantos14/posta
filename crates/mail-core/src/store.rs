//! Cópia local da caixa em SQLite. Toda leitura da UI vem daqui, nunca da rede.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::attachments::Attachment;
use crate::folders::{Folder, Role};
use crate::threads;
use crate::Result;

/// O banco é só uma cópia do servidor: mudou o esquema, apaga e sincroniza de novo.
const SCHEMA_VERSION: i32 = 5;

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
  attachments TEXT NOT NULL DEFAULT '[]',
  to_addrs TEXT NOT NULL DEFAULT '',
  cc_addrs TEXT NOT NULL DEFAULT '',
  thread_id INTEGER NOT NULL DEFAULT 0,
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
-- Dados do usuário (não são cópia do servidor): sobrevivem à recriação do esquema acima.
CREATE TABLE IF NOT EXISTS drafts (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  reply_to INTEGER,
  to_addr TEXT NOT NULL DEFAULT '',
  cc_addr TEXT NOT NULL DEFAULT '',
  bcc_addr TEXT NOT NULL DEFAULT '',
  subject TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL DEFAULT '',
  html TEXT NOT NULL DEFAULT '',
  updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- Pastas criadas pelo usuário (lista copiada do servidor) e categorias coloridas (só locais).
CREATE TABLE IF NOT EXISTS user_folders (
  account TEXT NOT NULL,
  name TEXT NOT NULL,
  delimiter TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (account, name)
);
CREATE TABLE IF NOT EXISTS message_categories (
  account TEXT NOT NULL,
  msg_key TEXT NOT NULL,
  category TEXT NOT NULL,
  PRIMARY KEY (account, msg_key)
);
-- Produtividade (só locais, chaveados como as categorias): adiadas, fixadas no topo, envios agendados e respostas de férias.
CREATE TABLE IF NOT EXISTS snoozes (
  account TEXT NOT NULL,
  msg_key TEXT NOT NULL,
  until INTEGER NOT NULL,
  woken INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (account, msg_key)
);
CREATE TABLE IF NOT EXISTS pins (
  account TEXT NOT NULL,
  msg_key TEXT NOT NULL,
  PRIMARY KEY (account, msg_key)
);
CREATE TABLE IF NOT EXISTS scheduled (
  id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  reply_to INTEGER,
  send_at INTEGER NOT NULL,
  payload TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  attempts INTEGER NOT NULL DEFAULT 0,
  error TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS vacation_replied (
  account TEXT NOT NULL,
  addr TEXT NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (account, addr)
);
-- Fila offline (dados do usuário): ações aguardando o servidor e envios aguardando rede.
CREATE TABLE IF NOT EXISTS pending_ops (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uid INTEGER NOT NULL,
  kind TEXT NOT NULL,
  arg TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE TABLE IF NOT EXISTS outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account TEXT NOT NULL,
  to_addr TEXT NOT NULL DEFAULT '',
  subject TEXT NOT NULL DEFAULT '',
  payload TEXT NOT NULL,
  created INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  error TEXT NOT NULL DEFAULT ''
);
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
    pub attachments: Vec<Attachment>,
    /// Endereços de To e Cc, separados por vírgula (para "Responder a todos").
    pub to_addrs: String,
    pub cc_addrs: String,
}

/// O que é preciso para responder mantendo a conversa agrupada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Fio (conversa) da conta a que a mensagem pertence; 0 = ainda sem fio.
    pub thread_id: i64,
    /// Mensagens do fio, em qualquer pasta.
    pub thread_count: u32,
    /// Id da categoria colorida aplicada (definidas nas configurações); `None` sem categoria.
    pub category: Option<String>,
    /// Fixada no topo da lista.
    pub pinned: bool,
}

/// Pasta criada pelo usuário numa conta.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserFolder {
    pub account: String,
    pub name: String,
    /// Separador da hierarquia no servidor (vazio se não há).
    pub delimiter: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDetail {
    #[serde(flatten)]
    pub summary: MessageSummary,
    pub body_text: String,
    /// Vazio quando a mensagem só tem texto.
    pub body_html: String,
    pub reply_addr: String,
    pub attachments: Vec<Attachment>,
    pub to_addrs: String,
    pub cc_addrs: String,
}

/// Pessoa já vista nas mensagens, para autocompletar destinatários.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Contact {
    /// Nome de exibição, quando o contato já enviou mensagem (vazio se só apareceu em Para/Cc).
    pub name: String,
    pub addr: String,
    /// Quantas vezes apareceu.
    pub count: u32,
}

/// Rascunho guardado só localmente (o Composer salva sozinho).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftRecord {
    /// Gerado pela UI; o mesmo id sobrescreve o rascunho anterior.
    pub id: String,
    pub account: String,
    /// Id local da mensagem respondida.
    pub reply_to: Option<i64>,
    pub to: String,
    #[serde(default)]
    pub cc: String,
    #[serde(default)]
    pub bcc: String,
    pub subject: String,
    pub body: String,
    /// Corpo formatado; vazio em rascunhos só de texto (versões antigas).
    #[serde(default)]
    pub html: String,
    /// Unix, em segundos; preenchido ao salvar.
    #[serde(default)]
    pub updated: i64,
}

pub struct Store {
    pub(crate) conn: Connection,
}

/// Nome da pasta do papel ?1 na conta da linha; Caixa de entrada sem linha em `folders` é "INBOX".
const IN_ROLE: &str = "m.mailbox = COALESCE(
    (SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = ?1),
    CASE ?1 WHEN 'inbox' THEN 'INBOX' END)";

/// Chave da categoria: o Message-ID (sobrevive a mover entre pastas, que muda o UID); sem ele, pasta/UID.
macro_rules! msg_key {
    () => {
        "CASE WHEN m.message_id <> '' THEN m.message_id ELSE m.mailbox || '/' || m.uid END"
    };
}
pub(crate) const MSG_KEY: &str = msg_key!();

pub(crate) const SUMMARY_COLS: &str = concat!(
    "m.id, m.account, m.uid, m.from_name, m.from_addr, m.subject, m.date, m.snippet, m.seen, m.flagged,
    COALESCE((SELECT f.role FROM folders f WHERE f.account = m.account AND f.name = m.mailbox),
    CASE WHEN m.mailbox = 'INBOX' THEN 'inbox' END),
    m.thread_id,
    CASE WHEN m.thread_id = 0 THEN 1 ELSE (SELECT COUNT(*) FROM messages t WHERE t.account = m.account AND t.thread_id = m.thread_id) END,
    (SELECT c.category FROM message_categories c WHERE c.account = m.account AND c.msg_key = ",
    msg_key!(),
    "),
    EXISTS (SELECT 1 FROM pins p WHERE p.account = m.account AND p.msg_key = ",
    msg_key!(),
    ")"
);

/// Adiada e ainda dentro do prazo: some das listas até a hora marcada.
const NOT_SNOOZED: &str = concat!(
    "NOT EXISTS (SELECT 1 FROM snoozes z WHERE z.account = m.account AND z.msg_key = ",
    msg_key!(),
    " AND z.until > CAST(strftime('%s', 'now') AS INTEGER))"
);

/// Fixadas primeiro; depois a data, ou a hora em que a adiada voltou (o que a leva ao topo).
const ORDER_TOP: &str = concat!(
    "ORDER BY EXISTS (SELECT 1 FROM pins p WHERE p.account = m.account AND p.msg_key = ",
    msg_key!(),
    ") DESC, MAX(m.date, COALESCE((SELECT z.until FROM snoozes z WHERE z.account = m.account AND z.msg_key = ",
    msg_key!(),
    "), 0)) DESC, m.id DESC"
);

/// Fora da Lixeira e do Spam.
pub(crate) const NOT_DISCARDED: &str = "COALESCE((SELECT f.role FROM folders f WHERE f.account = m.account AND f.name = m.mailbox), '') NOT IN ('trash', 'junk')";

pub(crate) fn row_to_summary(r: &rusqlite::Row) -> rusqlite::Result<MessageSummary> {
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
        thread_id: r.get(11)?,
        thread_count: r.get(12)?,
        category: r.get(13)?,
        pinned: r.get(14)?,
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
pub(crate) fn fts_query(input: &str) -> Option<String> {
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
        // `drafts` não é recriada com o resto: rascunhos de versões antigas ganham as colunas de Cc/Bcc/HTML.
        for col in ["cc_addr", "bcc_addr", "html"] {
            let has: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('drafts') WHERE name = ?1",
                params![col],
                |r| r.get(0),
            )?;
            if !has {
                conn.execute_batch(&format!("ALTER TABLE drafts ADD COLUMN {col} TEXT NOT NULL DEFAULT ''"))?;
            }
        }
        // Bancos da v5 já têm as mensagens, mas não o fio: ganham a coluna e os fios são calculados aqui.
        let has_thread: bool =
            conn.query_row("SELECT COUNT(*) > 0 FROM pragma_table_info('messages') WHERE name = 'thread_id'", [], |r| r.get(0))?;
        if !has_thread {
            conn.execute_batch("ALTER TABLE messages ADD COLUMN thread_id INTEGER NOT NULL DEFAULT 0")?;
        }
        conn.execute_batch("CREATE INDEX IF NOT EXISTS messages_by_thread ON messages (account, thread_id)")?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        let store = Self { conn };
        let pending: Vec<String> = {
            let mut st = store.conn.prepare("SELECT DISTINCT account FROM messages WHERE thread_id = 0")?;
            let rows = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        for account in pending {
            store.rebuild_threads(&account)?;
        }
        Ok(store)
    }

    /// Recalcula os fios da conta e grava só os que mudaram.
    fn rebuild_threads(&self, account: &str) -> Result<()> {
        let rows: Vec<(i64, String, String, String, i64)> = {
            let mut st = self
                .conn
                .prepare_cached("SELECT id, message_id, refs, subject, thread_id FROM messages WHERE account = ?1 ORDER BY id")?;
            let rows = st
                .query_map(params![account], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        let entries: Vec<threads::Entry> = rows
            .iter()
            .map(|(id, message_id, refs, subject, _)| threads::Entry { id: *id, message_id, refs, subject })
            .collect();
        let keys = threads::group(&entries);
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut up = tx.prepare_cached("UPDATE messages SET thread_id = ?1 WHERE id = ?2")?;
            for ((id, _, _, _, old), key) in rows.iter().zip(keys) {
                if *old != key {
                    up.execute(params![key, id])?;
                }
            }
        }
        Ok(tx.commit()?)
    }

    /// Mensagens do mesmo fio (qualquer pasta, fora Lixeira/Spam), da mais antiga à mais nova. Sempre inclui `id`.
    pub fn thread(&self, id: i64) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m, messages o
             WHERE o.id = ?1 AND m.account = o.account
               AND (m.id = o.id OR (o.thread_id <> 0 AND m.thread_id = o.thread_id AND {NOT_DISCARDED}))
             ORDER BY m.date ASC, m.id ASC"
        ))?;
        let rows = stmt.query_map(params![id], row_to_summary)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
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
                  message_id, refs, reply_addr, attachments, flagged, to_addrs, cc_addrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
                    serde_json::to_string(&m.attachments).unwrap_or_else(|_| "[]".into()),
                    m.flagged,
                    m.to_addrs,
                    m.cc_addrs
                ])?;
            }
        }
        tx.commit()?;
        if !msgs.is_empty() {
            self.rebuild_threads(account)?;
        }
        Ok(())
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
            .prepare_cached(&format!("SELECT COUNT(*) FROM messages m WHERE m.seen = 0 AND {IN_ROLE} AND {NOT_SNOOZED}"))?;
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
                &format!("SELECT {SUMMARY_COLS}, m.body_text, m.body_html, m.reply_addr, m.attachments, m.to_addrs, m.cc_addrs FROM messages m WHERE m.id = ?1"),
                params![id],
                |r| {
                    Ok(MessageDetail {
                        summary: row_to_summary(r)?,
                        body_text: r.get(15)?,
                        body_html: r.get(16)?,
                        reply_addr: r.get(17)?,
                        attachments: serde_json::from_str(&r.get::<_, String>(18)?).unwrap_or_default(),
                        to_addrs: r.get(19)?,
                        cc_addrs: r.get(20)?,
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
            "SELECT {SUMMARY_COLS} FROM messages m WHERE {IN_ROLE} AND {NOT_SNOOZED}
             {ORDER_TOP} LIMIT ?2 OFFSET ?3"
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

    /// Mensagens novas (não lidas) mais recentes de uma pasta de uma conta, para a notificação.
    pub fn newest_unseen(&self, account: &str, mailbox: &str, limit: u32) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.account = ?1 AND m.mailbox = ?2 AND m.seen = 0
             ORDER BY m.date DESC, m.id DESC LIMIT ?3"
        ))?;
        let rows = stmt
            .query_map(params![account, mailbox, limit], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Grava (ou atualiza) o rascunho; `updated` vem do relógio local.
    pub fn save_draft(&self, d: &DraftRecord) -> Result<()> {
        if d.id.trim().is_empty() {
            return Err(crate::Error::Invalid("rascunho sem id".into()));
        }
        self.conn.execute(
            "INSERT INTO drafts (id, account, reply_to, to_addr, cc_addr, bcc_addr, subject, body, html, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, strftime('%s','now'))
             ON CONFLICT (id) DO UPDATE SET account = ?2, reply_to = ?3, to_addr = ?4, cc_addr = ?5, bcc_addr = ?6,
               subject = ?7, body = ?8, html = ?9, updated = strftime('%s','now')",
            params![d.id, d.account, d.reply_to, d.to, d.cc, d.bcc, d.subject, d.body, d.html],
        )?;
        Ok(())
    }

    /// Rascunhos, o mais recente primeiro.
    pub fn list_drafts(&self) -> Result<Vec<DraftRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, account, reply_to, to_addr, cc_addr, bcc_addr, subject, body, html, updated FROM drafts ORDER BY updated DESC, rowid DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DraftRecord {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    reply_to: r.get(2)?,
                    to: r.get(3)?,
                    cc: r.get(4)?,
                    bcc: r.get(5)?,
                    subject: r.get(6)?,
                    body: r.get(7)?,
                    html: r.get(8)?,
                    updated: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Contatos que já apareceram nas mensagens (remetentes, Para e Cc) cujo nome ou endereço contém `query`.
    /// Mais frequentes primeiro; empate pelo mais recente. Vazio = os mais frequentes.
    pub fn suggest_contacts(&self, query: &str, limit: usize) -> Result<Vec<Contact>> {
        let q = query.trim().to_lowercase();
        let like = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare_cached(
            "SELECT from_name, from_addr, to_addrs, cc_addrs FROM messages
             WHERE ?1 = '%%' OR from_addr LIKE ?1 ESCAPE '\\' OR from_name LIKE ?1 ESCAPE '\\'
                OR to_addrs LIKE ?1 ESCAPE '\\' OR cc_addrs LIKE ?1 ESCAPE '\\'
             ORDER BY date DESC LIMIT 5000",
        )?;
        let rows = stmt.query_map(params![like], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        // Linhas vêm da mais nova para a mais antiga: a ordem de entrada serve de recência.
        let mut found: std::collections::HashMap<String, (Contact, usize)> = std::collections::HashMap::new();
        let mut order = 0usize;
        let mut seen = |name: &str, addr: &str| {
            let addr = addr.trim();
            if !addr.contains('@') {
                return;
            }
            let e = found.entry(addr.to_lowercase()).or_insert_with(|| {
                order += 1;
                (Contact { name: String::new(), addr: addr.to_string(), count: 0 }, order)
            });
            e.0.count += 1;
            if e.0.name.is_empty() {
                e.0.name = name.trim().to_string();
            }
        };
        for row in rows {
            let (name, from, to, cc) = row?;
            seen(&name, &from);
            for a in to.split(',').chain(cc.split(',')) {
                seen("", a);
            }
        }
        let mut list: Vec<_> = found
            .into_values()
            .filter(|(c, _)| q.is_empty() || c.addr.to_lowercase().contains(&q) || c.name.to_lowercase().contains(&q))
            .collect();
        list.sort_by(|a, b| b.0.count.cmp(&a.0.count).then(a.1.cmp(&b.1)));
        Ok(list.into_iter().take(limit).map(|(c, _)| c).collect())
    }

    pub fn delete_draft(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM drafts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare_cached("SELECT key, value FROM settings")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// Troca a lista de pastas do usuário de uma conta (o que o servidor acabou de listar).
    pub fn set_user_folders(&mut self, account: &str, names: &[String], delimiter: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM user_folders WHERE account = ?1", params![account])?;
        for n in names {
            tx.execute("INSERT INTO user_folders (account, name, delimiter) VALUES (?1, ?2, ?3)", params![account, n, delimiter])?;
        }
        Ok(tx.commit()?)
    }

    pub fn user_folders(&self) -> Result<Vec<UserFolder>> {
        let mut stmt = self.conn.prepare_cached("SELECT account, name, delimiter FROM user_folders ORDER BY account, name")?;
        let rows = stmt
            .query_map([], |r| Ok(UserFolder { account: r.get(0)?, name: r.get(1)?, delimiter: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Depois de renomear no servidor: a pasta e as subpastas passam a ter o novo nome, com as mensagens.
    pub fn rename_box(&mut self, account: &str, old: &str, new: &str, delimiter: &str) -> Result<()> {
        // Sem separador não há subpastas: o prefixo impossível não casa com nada.
        let prefix = if delimiter.is_empty() { "\u{0}".to_string() } else { format!("{old}{delimiter}") };
        let tx = self.conn.transaction()?;
        for (table, col) in [("messages", "mailbox"), ("mailbox_state", "mailbox"), ("user_folders", "name")] {
            tx.execute(
                &format!(
                    "UPDATE {table} SET {col} = ?3 || substr({col}, length(?2) + 1)
                     WHERE account = ?1 AND ({col} = ?2 OR substr({col}, 1, length(?4)) = ?4)"
                ),
                params![account, old, new, prefix],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Depois de excluir no servidor: apaga a cópia local das pastas e das mensagens delas.
    pub fn drop_boxes(&mut self, account: &str, names: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for n in names {
            tx.execute("DELETE FROM messages WHERE account = ?1 AND mailbox = ?2", params![account, n])?;
            tx.execute("DELETE FROM mailbox_state WHERE account = ?1 AND mailbox = ?2", params![account, n])?;
            tx.execute("DELETE FROM user_folders WHERE account = ?1 AND name = ?2", params![account, n])?;
        }
        Ok(tx.commit()?)
    }

    /// Mensagens de uma pasta de uma conta (as pastas do usuário não se juntam entre contas).
    pub fn list_box(&self, account: &str, mailbox: &str, limit: u32, offset: u32) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.account = ?1 AND m.mailbox = ?2 AND {NOT_SNOOZED}
             {ORDER_TOP} LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = stmt
            .query_map(params![account, mailbox, limit, offset], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn search_box(&self, account: &str, mailbox: &str, query: &str, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 AND m.account = ?2 AND m.mailbox = ?3 ORDER BY m.date DESC LIMIT ?4"
        ))?;
        let rows = stmt
            .query_map(params![q, account, mailbox, limit], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Aplica a categoria (ou tira, com `None`) às mensagens. Só local: não muda nada no servidor.
    pub fn set_category(&mut self, ids: &[i64], category: Option<&str>) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            let found: Option<(String, String)> = tx
                .query_row(
                    concat!("SELECT m.account, ", msg_key!(), " FROM messages m WHERE m.id = ?1"),
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((account, key)) = found else { continue };
            match category {
                Some(c) => tx.execute(
                    "INSERT INTO message_categories (account, msg_key, category) VALUES (?1, ?2, ?3)
                     ON CONFLICT (account, msg_key) DO UPDATE SET category = excluded.category",
                    params![account, key, c],
                )?,
                None => tx.execute("DELETE FROM message_categories WHERE account = ?1 AND msg_key = ?2", params![account, key])?,
            };
        }
        Ok(tx.commit()?)
    }

    /// Mensagens de uma categoria, em todas as contas, exceto Lixeira e Spam.
    pub fn list_category(&self, category: &str, limit: u32, offset: u32) -> Result<Vec<MessageSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m
             WHERE (SELECT c.category FROM message_categories c WHERE c.account = m.account AND c.msg_key = {key}) = ?1
               AND {NOT_DISCARDED}
             ORDER BY m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3",
            key = msg_key!()
        ))?;
        let rows = stmt
            .query_map(params![category, limit, offset], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn search_category(&self, category: &str, query: &str, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1
               AND (SELECT c.category FROM message_categories c WHERE c.account = m.account AND c.msg_key = {key}) = ?2
               AND {NOT_DISCARDED}
             ORDER BY m.date DESC LIMIT ?3",
            key = msg_key!()
        ))?;
        let rows = stmt
            .query_map(params![q, category, limit], row_to_summary)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Apaga tudo o que é local de uma conta: mensagens (e FTS), estado das pastas, pastas, rascunhos e assinatura.
    pub fn remove_account(&mut self, account: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        for sql in [
            "DELETE FROM messages WHERE account = ?1",
            "DELETE FROM mailbox_state WHERE account = ?1",
            "DELETE FROM folders WHERE account = ?1",
            "DELETE FROM drafts WHERE account = ?1",
            "DELETE FROM pending_ops WHERE account = ?1",
            "DELETE FROM outbox WHERE account = ?1",
            "DELETE FROM user_folders WHERE account = ?1",
            "DELETE FROM message_categories WHERE account = ?1",
            "DELETE FROM snoozes WHERE account = ?1",
            "DELETE FROM pins WHERE account = ?1",
            "DELETE FROM scheduled WHERE account = ?1",
            "DELETE FROM vacation_replied WHERE account = ?1",
            "DELETE FROM settings WHERE key = 'signature:' || ?1",
        ] {
            tx.execute(sql, params![account])?;
        }
        Ok(tx.commit()?)
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
            attachments: vec![],
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

    fn draft(id: &str, account: &str, body: &str) -> DraftRecord {
        DraftRecord { id: id.into(), account: account.into(), reply_to: None, to: "bia@exemplo.com".into(), cc: "carla@exemplo.com".into(), bcc: "dan@exemplo.com".into(), subject: "Oi".into(), body: body.into(), html: String::new(), updated: 0 }
    }

    #[test]
    fn rascunho_salva_atualiza_lista_e_apaga() {
        let s = Store::open_in_memory().unwrap();
        s.save_draft(&draft("d1", "a", "primeira versão")).unwrap();
        s.save_draft(&draft("d2", "a", "outro")).unwrap();
        // Mesmo id sobrescreve; reply_to é preservado.
        let mut d1 = draft("d1", "a", "segunda versão");
        d1.reply_to = Some(7);
        s.save_draft(&d1).unwrap();
        let all = s.list_drafts().unwrap();
        assert_eq!(all.len(), 2);
        let got = all.iter().find(|d| d.id == "d1").unwrap();
        assert_eq!((got.body.as_str(), got.reply_to), ("segunda versão", Some(7)));
        assert_eq!((got.cc.as_str(), got.bcc.as_str()), ("carla@exemplo.com", "dan@exemplo.com"));
        assert!(got.updated > 0);
        s.delete_draft("d1").unwrap();
        assert_eq!(s.list_drafts().unwrap().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["d2"]);
        assert!(s.save_draft(&draft(" ", "a", "x")).is_err());
    }

    #[test]
    fn rascunho_sobrevive_a_recriacao_do_esquema() {
        let dir = std::env::temp_dir().join(format!("posta-draft-{}.sqlite", std::process::id()));
        {
            let s = Store::open(&dir).unwrap();
            s.save_draft(&draft("d1", "a", "texto")).unwrap();
            s.conn.pragma_update(None, "user_version", SCHEMA_VERSION - 1).unwrap();
        }
        let s = Store::open(&dir).unwrap();
        assert_eq!(s.list_drafts().unwrap().len(), 1);
        drop(s);
        for ext in ["", "-wal", "-shm"] {
            std::fs::remove_file(format!("{}{ext}", dir.display())).ok();
        }
    }

    #[test]
    fn contatos_sugeridos_por_frequencia_e_recencia() {
        let mut s = Store::open_in_memory().unwrap();
        let mut a = msg(1, "a", "", 10);
        a.cc_addrs = "carla@exemplo.com".into();
        let mut b = msg(2, "b", "", 20);
        b.from_name = "Carla Dias".into();
        b.from_addr = "carla@exemplo.com".into();
        b.to_addrs = "eu@exemplo.com".into();
        let c = msg(3, "c", "", 30);
        s.insert_messages("a", "INBOX", &[a, b, c]).unwrap();
        let all = s.suggest_contacts("", 10).unwrap();
        let addrs: Vec<_> = all.iter().map(|c| c.addr.as_str()).collect();
        assert_eq!(addrs[0], "eu@exemplo.com");
        assert!(addrs.contains(&"bia@exemplo.com") && addrs.contains(&"carla@exemplo.com"));
        let r = s.suggest_contacts("CARLA", 10).unwrap();
        assert_eq!(r, vec![Contact { name: "Carla Dias".into(), addr: "carla@exemplo.com".into(), count: 2 }]);
        assert_eq!(s.suggest_contacts("ana s", 10).unwrap().len(), 1);
        assert!(s.suggest_contacts("50%_", 10).unwrap().is_empty());
        assert_eq!(s.suggest_contacts("", 2).unwrap().len(), 2);
    }

    #[test]
    fn rascunho_antigo_ganha_colunas_de_cc_e_bcc() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE drafts (id TEXT PRIMARY KEY, account TEXT NOT NULL, reply_to INTEGER, to_addr TEXT NOT NULL DEFAULT '',
               subject TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '', updated INTEGER NOT NULL);
             INSERT INTO drafts (id, account, to_addr, updated) VALUES ('velho', 'a', 'x@y.z', 1);",
        )
        .unwrap();
        let s = Store::init(conn).unwrap();
        let d = &s.list_drafts().unwrap()[0];
        assert_eq!((d.id.as_str(), d.to.as_str(), d.cc.as_str(), d.bcc.as_str()), ("velho", "x@y.z", "", ""));
    }

    #[test]
    fn remover_conta_apaga_so_os_dados_dela() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        s.set_folders("b", &folders_a()).unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        s.set_uid_validity("b", "INBOX", 1).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "Cotação a", "", 1)]).unwrap();
        s.insert_messages("b", "INBOX", &[msg(1, "Cotação b", "", 2)]).unwrap();
        s.save_draft(&draft("da", "a", "x")).unwrap();
        s.save_draft(&draft("db", "b", "y")).unwrap();
        s.set_setting("signature:a", "Ana").unwrap();
        s.set_setting("signature:b", "Beto").unwrap();
        s.set_setting("notify", "0").unwrap();

        s.remove_account("a").unwrap();

        assert!(s.local_uids("a", "INBOX").unwrap().is_empty());
        assert!(s.folders("a").unwrap().is_empty());
        assert_eq!(s.uid_validity("a", "INBOX").unwrap(), None);
        assert_eq!(s.list("INBOX", 10, 0).unwrap().len(), 1);
        assert_eq!(s.search("cotacao", 10).unwrap()[0].subject, "Cotação b", "FTS limpo só da conta removida");
        assert_eq!(s.list_drafts().unwrap()[0].id, "db");
        assert_eq!(s.setting("signature:a").unwrap(), None);
        assert_eq!(s.setting("signature:b").unwrap().as_deref(), Some("Beto"));
        assert_eq!(s.setting("notify").unwrap().as_deref(), Some("0"));
    }

    #[test]
    fn configuracoes_e_novas_nao_lidas() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_setting("k", "1").unwrap();
        s.set_setting("k", "2").unwrap();
        assert_eq!(s.settings().unwrap().get("k").map(String::as_str), Some("2"));
        let mut lida = msg(3, "lida", "", 30);
        lida.seen = true;
        s.insert_messages("a", "INBOX", &[msg(1, "velha", "", 10), msg(2, "nova", "", 20), lida]).unwrap();
        let n = s.newest_unseen("a", "INBOX", 5).unwrap();
        assert_eq!(n.iter().map(|m| m.subject.as_str()).collect::<Vec<_>>(), ["nova", "velha"]);
        assert_eq!(s.newest_unseen("a", "INBOX", 1).unwrap().len(), 1);
    }

    fn reply_to(mut m: NewMessage, parent_uid: u32, subject: &str) -> NewMessage {
        m.refs = format!("{parent_uid}@exemplo.com");
        m.subject = subject.into();
        m
    }

    #[test]
    fn conversa_junta_entrada_e_enviados_e_deixa_a_lixeira_de_fora() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "Contrato", "", 10), msg(9, "Outro assunto", "", 5)]).unwrap();
        s.insert_messages("a", "Enviados", &[reply_to(msg(2, "", "", 20), 1, "Re: Contrato")]).unwrap();
        // Chega depois e cita só a resposta; ao entrar, o fio é recalculado.
        s.insert_messages("a", "INBOX", &[reply_to(msg(3, "", "", 30), 2, "RES: Contrato")]).unwrap();
        s.insert_messages("a", "Lixeira", &[reply_to(msg(4, "", "", 40), 3, "Re: Contrato")]).unwrap();
        // Outra conta com o mesmo assunto não se mistura.
        s.insert_messages("b", "INBOX", &[msg(1, "Contrato", "", 11), reply_to(msg(2, "", "", 12), 1, "Re: Contrato")]).unwrap();

        let inbox = s.list_folder(Role::Inbox, 10, 0).unwrap();
        let last = inbox.iter().find(|m| m.account == "a" && m.uid == 3).unwrap();
        assert_eq!(last.thread_count, 4);
        let ids: Vec<_> = s.thread(last.id).unwrap().iter().map(|m| m.uid).collect();
        assert_eq!(ids, [1, 2, 3]);
        let outro = inbox.iter().find(|m| m.uid == 9).unwrap();
        assert_ne!(outro.thread_id, last.thread_id);
        assert_eq!(s.thread(outro.id).unwrap().len(), 1);
        let b = inbox.iter().find(|m| m.account == "b" && m.uid == 2).unwrap();
        assert_eq!(s.thread(b.id).unwrap().len(), 2);
    }

    #[test]
    fn banco_da_v5_ganha_coluna_e_fios_calculados() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&SCHEMA.replace("  thread_id INTEGER NOT NULL DEFAULT 0,\n", "")).unwrap();
        conn.execute_batch(
            "INSERT INTO messages (account, mailbox, uid, subject, date, message_id, refs) VALUES
               ('a', 'INBOX', 1, 'Pauta', 1, 'p1', ''),
               ('a', 'INBOX', 2, 'Re: Pauta', 2, 'p2', 'p1');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION).unwrap();
        let s = Store::init(conn).unwrap();
        let list = s.list("INBOX", 5, 0).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].thread_id != 0 && list[0].thread_id == list[1].thread_id);
        assert_eq!(list[0].thread_count, 2);
    }

    #[test]
    fn banco_do_master_abre_e_migra_com_tudo_o_que_veio_das_branches() {
        // Esquema da v5 do master: sem thread_id em messages, sem html em drafts, sem user_folders/message_categories.
        let conn = Connection::open_in_memory().unwrap();
        let master = SCHEMA
            .replace("  thread_id INTEGER NOT NULL DEFAULT 0,\n", "")
            .replace("  html TEXT NOT NULL DEFAULT '',\n", "");
        let master = master.split("-- Pastas criadas pelo usuário").next().unwrap().to_string()
            + &master[master.find("CREATE VIRTUAL TABLE").unwrap()..];
        conn.execute_batch(&master).unwrap();
        assert!(conn.prepare("SELECT thread_id FROM messages").is_err());
        assert!(conn.prepare("SELECT html FROM drafts").is_err());
        assert!(conn.prepare("SELECT * FROM user_folders").is_err());
        conn.execute_batch(
            "INSERT INTO messages (account, mailbox, uid, subject, date, message_id, refs) VALUES
               ('a', 'INBOX', 1, 'Pauta', 1, 'p1', ''),
               ('a', 'INBOX', 2, 'Re: Pauta', 2, 'p2', 'p1');
             INSERT INTO drafts (id, account, subject, updated) VALUES ('d1', 'a', 'rascunho', 1);",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION).unwrap();
        let mut s = Store::init(conn).unwrap();
        let list = s.list("INBOX", 5, 0).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].thread_count, 2);
        assert!(list[0].category.is_none());
        s.set_category(&[list[0].id], Some("azul")).unwrap();
        s.set_user_folders("a", &["X".into()], "/").unwrap();
        assert_eq!(s.user_folders().unwrap().len(), 1);
        assert_eq!(s.list("INBOX", 5, 0).unwrap()[0].category.as_deref(), Some("azul"));
        // Abrir de novo (já migrado) não falha nem refaz nada.
        let conn = s.conn;
        let again = Store::init(conn).unwrap();
        assert_eq!(again.list("INBOX", 5, 0).unwrap().len(), 2);
    }

    #[test]
    fn banco_da_integracao_outlook_ganha_as_tabelas_dos_tres_pacotes() {
        // Esquema do feat/integracao-outlook: já tem pastas e categorias, mas não as tabelas de produtividade nem da fila offline.
        let conn = Connection::open_in_memory().unwrap();
        let cut = |t: &str, from: &str| -> String {
            let a = t.find(from).unwrap();
            let b = t[a..].find("CREATE VIRTUAL TABLE").unwrap() + a;
            format!("{}{}", &t[..a], &t[b..])
        };
        conn.execute_batch(&cut(SCHEMA, "-- Produtividade")).unwrap();
        for t in ["snoozes", "pins", "scheduled", "vacation_replied", "pending_ops", "outbox"] {
            assert!(conn.prepare(&format!("SELECT * FROM {t}")).is_err(), "{t} não deveria existir ainda");
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION).unwrap();
        let s = Store::init(conn).unwrap();
        for t in ["snoozes", "pins", "scheduled", "vacation_replied", "pending_ops", "outbox"] {
            s.conn.prepare(&format!("SELECT * FROM {t}")).unwrap_or_else(|e| panic!("{t}: {e}"));
        }
        let again = Store::init(s.conn).unwrap();
        assert_eq!((again.pending_count().unwrap(), again.outbox_count().unwrap()), (0, 0));
    }

    #[test]
    fn pastas_do_usuario_listam_renomeiam_e_somem_com_as_mensagens() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX.Obra", 7).unwrap();
        s.insert_messages("a", "INBOX.Obra", &[msg(1, "planta", "x", 10)]).unwrap();
        s.insert_messages("a", "INBOX.Obra.Fase1", &[msg(2, "fase", "x", 20)]).unwrap();
        s.insert_messages("a", "INBOX.ObraX", &[msg(3, "outra", "x", 30)]).unwrap();
        s.insert_messages("b", "INBOX.Obra", &[msg(4, "da conta b", "x", 40)]).unwrap();
        let names = ["INBOX.Obra".to_string(), "INBOX.Obra.Fase1".to_string(), "INBOX.ObraX".to_string()];
        s.set_user_folders("a", &names, ".").unwrap();
        assert_eq!(s.user_folders().unwrap().len(), 3);
        assert_eq!(s.list_box("a", "INBOX.Obra", 10, 0).unwrap().len(), 1, "só a conta e a pasta pedidas");
        assert_eq!(s.search_box("a", "INBOX.Obra", "planta", 10).unwrap().len(), 1);
        assert!(s.search_box("a", "INBOX.Obra", "fase", 10).unwrap().is_empty());

        s.rename_box("a", "INBOX.Obra", "INBOX.Projeto", ".").unwrap();
        assert_eq!(s.list_box("a", "INBOX.Projeto", 10, 0).unwrap()[0].subject, "planta");
        assert_eq!(s.list_box("a", "INBOX.Projeto.Fase1", 10, 0).unwrap()[0].subject, "fase");
        assert_eq!(s.list_box("a", "INBOX.ObraX", 10, 0).unwrap().len(), 1, "irmã com o mesmo começo não muda");
        assert_eq!(s.list_box("b", "INBOX.Obra", 10, 0).unwrap().len(), 1, "outra conta não muda");
        assert_eq!(s.uid_validity("a", "INBOX.Projeto").unwrap(), Some(7));
        let after: Vec<_> = s.user_folders().unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(after, ["INBOX.ObraX", "INBOX.Projeto", "INBOX.Projeto.Fase1"]);

        s.drop_boxes("a", &["INBOX.Projeto.Fase1".into(), "INBOX.Projeto".into()]).unwrap();
        assert!(s.list_box("a", "INBOX.Projeto", 10, 0).unwrap().is_empty());
        assert!(s.search_box("a", "INBOX.Projeto.Fase1", "fase", 10).unwrap().is_empty(), "FTS limpo");
        assert_eq!(s.uid_validity("a", "INBOX.Projeto").unwrap(), None);
        assert_eq!(s.user_folders().unwrap().len(), 1);
    }

    #[test]
    fn categoria_aplica_filtra_tira_e_sobrevive_a_mover() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "contrato", "x", 10), msg(2, "fatura", "x", 20)]).unwrap();
        let id = |s: &Store, uid| s.list("INBOX", 10, 0).unwrap().into_iter().find(|m| m.uid == uid).unwrap().id;
        let (one, two) = (id(&s, 1), id(&s, 2));
        s.set_category(&[one, two], Some("azul")).unwrap();
        s.set_category(&[two], Some("verde")).unwrap();
        assert_eq!(s.list_category("azul", 10, 0).unwrap().iter().map(|m| m.uid).collect::<Vec<_>>(), [1]);
        assert_eq!(s.search_category("verde", "fatura", 10).unwrap().len(), 1);
        assert!(s.search_category("azul", "fatura", 10).unwrap().is_empty());
        assert_eq!(s.list("INBOX", 10, 0).unwrap().iter().find(|m| m.uid == 1).unwrap().category.as_deref(), Some("azul"));

        // Mover muda pasta e UID no servidor: a cópia local some e a nova entra com o mesmo Message-ID.
        s.remove(one).unwrap();
        let mut moved = msg(9, "contrato", "x", 10);
        moved.message_id = "1@exemplo.com".into();
        s.insert_messages("a", "INBOX.Obra", &[moved]).unwrap();
        assert_eq!(s.list_box("a", "INBOX.Obra", 10, 0).unwrap()[0].category.as_deref(), Some("azul"));

        s.set_category(&[two], None).unwrap();
        assert!(s.list_category("verde", 10, 0).unwrap().is_empty());
        s.set_category(&[999], Some("azul")).unwrap();
    }

    #[test]
    fn categoria_nao_lista_lixeira_e_remover_conta_apaga() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &[Folder { role: Role::Trash, name: "Trash".into() }]).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "ok", "x", 10)]).unwrap();
        s.insert_messages("a", "Trash", &[msg(2, "lixo", "x", 20)]).unwrap();
        let ids: Vec<i64> = s.list("INBOX", 10, 0).unwrap().iter().chain(&s.list("Trash", 10, 0).unwrap()).map(|m| m.id).collect();
        s.set_category(&ids, Some("azul")).unwrap();
        assert_eq!(s.list_category("azul", 10, 0).unwrap().len(), 1);
        s.set_user_folders("a", &["X".into()], "/").unwrap();
        s.remove_account("a").unwrap();
        assert!(s.user_folders().unwrap().is_empty());
        assert!(s.conn.query_row("SELECT COUNT(*) FROM message_categories", [], |r| r.get::<_, i64>(0)).unwrap() == 0);
    }
}
