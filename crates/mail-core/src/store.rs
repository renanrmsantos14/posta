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
  importance INTEGER NOT NULL DEFAULT 0,
  unsub TEXT,
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
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts (rowid, subject, from_name, from_addr, body_text)
  VALUES (new.id, new.subject, new.from_name, new.from_addr, new.body_text);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts (messages_fts, rowid, subject, from_name, from_addr, body_text)
  VALUES ('delete', old.id, old.subject, old.from_name, old.from_addr, old.body_text);
  DELETE FROM msg_refs WHERE id = old.id;
END;
-- Corpo baixado sob demanda (ou renomeação de assunto): o índice de busca acompanha.
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF subject, from_name, from_addr, body_text ON messages BEGIN
  INSERT INTO messages_fts (messages_fts, rowid, subject, from_name, from_addr, body_text)
  VALUES ('delete', old.id, old.subject, old.from_name, old.from_addr, old.body_text);
  INSERT INTO messages_fts (rowid, subject, from_name, from_addr, body_text)
  VALUES (new.id, new.subject, new.from_name, new.from_addr, new.body_text);
END;
-- References de cada mensagem, para achar os filhos de uma mensagem que acabou de chegar (fios incrementais).
CREATE TABLE IF NOT EXISTS msg_refs (
  id INTEGER NOT NULL,
  account TEXT NOT NULL,
  ref TEXT NOT NULL,
  PRIMARY KEY (account, ref, id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS msg_refs_by_id ON msg_refs (id);
-- Contatos já vistos (remetentes, Para e Cc), mantidos na gravação: autocompletar sem varrer as mensagens.
CREATE TABLE IF NOT EXISTS contacts (
  key TEXT PRIMARY KEY,
  addr TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  count INTEGER NOT NULL DEFAULT 0,
  last_date INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS contacts_by_count ON contacts (count DESC, last_date DESC);
";

/// Busca de texto. `prefix` acelera as buscas "digitando" (cada palavra vira prefixo).
const FTS_CREATE: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5 (
  subject, from_name, from_addr, body_text,
  content = 'messages', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2'
);";

/// Revisão das colunas/índices/tabelas adicionadas sem recriar o banco (guardada em `application_id`).
/// Banco já na revisão pula todo o esquema ao abrir.
const SCHEMA_REV: i32 = 0x5054_0005;

/// Mensagem já interpretada, pronta para gravar.
#[derive(Debug, Clone, Default)]
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
    /// Só cabeçalhos: `body_text` traz apenas uma prévia curta e o corpo é baixado ao abrir.
    pub header_only: bool,
    /// Importância pedida pelo remetente: 1 alta, -1 baixa, 0 normal.
    pub importance: i8,
    /// Linhas `List-Unsubscribe*` (`unsubscribe::header_lines`); `None` quando o cabeçalho veio cortado.
    pub unsub: Option<String>,
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
    /// 1 alta, -1 baixa, 0 normal (mensagens anteriores ao recurso ficam 0).
    pub importance: i8,
}

/// Pasta criada pelo usuário numa conta.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserFolder {
    pub account: String,
    pub name: String,
    /// Separador da hierarquia no servidor (vazio se não há).
    pub delimiter: String,
}

/// Não lidas de uma pasta do usuário (só as que têm alguma).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FolderUnread {
    pub account: String,
    pub name: String,
    pub unread: u32,
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
    /// `false`: só há a prévia; o corpo completo ainda não foi baixado do servidor.
    pub body_loaded: bool,
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

/// Marcas de sincronização incremental de uma pasta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncState {
    pub uid_validity: u32,
    /// HIGHESTMODSEQ visto na última sync (0 = o servidor não tem CONDSTORE ou nunca sincronizou).
    pub highest_modseq: u64,
    /// UIDNEXT visto na última sync (0 = nunca).
    pub uid_next: u32,
    /// A cópia local tem tudo o que o servidor tinha na última sync (sem janela truncada).
    pub complete: bool,
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
    m.pinned, m.importance"
);

/// Adiada e ainda dentro do prazo: some das listas até a hora marcada. (`snooze_until` é mantida na gravação.)
const NOT_SNOOZED: &str = "m.snooze_until <= CAST(strftime('%s', 'now') AS INTEGER)";

/// Fixadas primeiro; depois a data, ou a hora em que a adiada voltou (o que a leva ao topo).
/// Colunas desnormalizadas + índice `messages_by_top`: a primeira página custa O(limite).
const ORDER_TOP: &str = "ORDER BY m.pinned DESC, m.visible_date DESC, m.id DESC";

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
        importance: r.get(15)?,
    })
}

/// Endereços de uma mensagem para o autocompletar: o remetente (com nome) e os de Para/Cc (sem nome).
fn contact_hits(
    name: &str,
    from: &str,
    to: &str,
    cc: &str,
    date: i64,
    acc: &mut std::collections::HashMap<String, (String, String, u32, i64)>,
) {
    let mut add = |name: &str, addr: &str| {
        let addr = addr.trim();
        if !addr.contains('@') {
            return;
        }
        let e = acc.entry(addr.to_lowercase()).or_insert_with(|| (addr.to_string(), String::new(), 0, 0));
        e.2 += 1;
        e.3 = e.3.max(date);
        if e.1.is_empty() {
            e.1 = name.trim().to_string();
        }
    };
    add(name, from);
    for a in to.split(',').chain(cc.split(',')) {
        add("", a);
    }
}

/// Posição na ordem da lista (fixadas, data visível, id), para paginar por cursor sem OFFSET.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub pinned: bool,
    pub visible_date: i64,
    pub id: i64,
}

impl Cursor {
    pub fn encode(&self) -> String {
        format!("{}:{}:{}", self.pinned as u8, self.visible_date, self.id)
    }

    pub fn decode(s: &str) -> Option<Self> {
        let mut it = s.split(':');
        let pinned = it.next()?.parse::<u8>().ok()? != 0;
        let visible_date = it.next()?.parse().ok()?;
        let id = it.next()?.parse().ok()?;
        it.next().is_none().then_some(Self { pinned, visible_date, id })
    }
}

/// Uma página da lista. `next` é o cursor da página seguinte (`None` = acabou).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Page {
    pub items: Vec<MessageSummary>,
    pub next: Option<String>,
}

/// O que mudou numa aplicação do estado do servidor, por id local (alimenta os eventos delta da UI).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Delta {
    pub inserted: Vec<i64>,
    pub changed: Vec<i64>,
    pub removed: Vec<i64>,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.inserted.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }

    pub fn merge(&mut self, other: Delta) {
        self.inserted.extend(other.inserted);
        self.changed.extend(other.changed);
        self.removed.extend(other.removed);
    }
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

/// Fios do lote recém-inserido, sem reagrupar a conta inteira.
///
/// Carrega só o que o lote pode ter ligado: pais e cópias (mesmo Message-ID), filhos que já citavam o
/// Message-ID novo (`msg_refs`) e os pares de assunto quando o grupo de assunto passa a valer. Dos que
/// achou, carrega os fios inteiros e roda o mesmo `threads::group` do recálculo completo nesse subconjunto.
fn thread_incremental(conn: &Connection, account: &str, new_ids: &[i64]) -> Result<()> {
    use std::collections::{BTreeMap, HashSet};
    let Some(&min_new) = new_ids.iter().min() else { return Ok(()) };
    let new_set: HashSet<i64> = new_ids.iter().copied().collect();
    type Row = (i64, String, String, String, i64);
    let mut loaded: BTreeMap<i64, Row> = BTreeMap::new();
    let mut touched_threads: HashSet<i64> = HashSet::new();
    let mut touched_msgs: HashSet<i64> = HashSet::new();
    let mut touch = |id: i64, tid: i64| {
        if new_set.contains(&id) {
            return;
        }
        if tid == 0 {
            touched_msgs.insert(id);
        } else {
            touched_threads.insert(tid);
        }
    };
    {
        let mut by_id = conn.prepare_cached(
            "SELECT id, message_id, refs, subject, thread_id, subject_key, subject_re FROM messages WHERE id = ?1",
        )?;
        let mut by_msgid = conn.prepare_cached("SELECT id, thread_id FROM messages WHERE account = ?1 AND message_id = ?2 AND message_id <> '' AND id < ?3")?;
        let mut children = conn.prepare_cached(
            "SELECT m.id, m.thread_id FROM msg_refs r JOIN messages m ON m.id = r.id
             WHERE r.account = ?1 AND r.ref = ?2 AND m.id < ?3",
        )?;
        let mut prefixed = conn.prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE account = ?1 AND subject_key = ?2 AND subject_key <> '' AND subject_re = 1 AND id < ?3)",
        )?;
        let mut peers = conn.prepare_cached("SELECT id, thread_id FROM messages WHERE account = ?1 AND subject_key = ?2 AND subject_key <> '' AND id < ?3")?;
        let mut one_peer =
            conn.prepare_cached("SELECT id, thread_id FROM messages WHERE account = ?1 AND subject_key = ?2 AND subject_key <> '' AND id < ?3 LIMIT 1")?;
        for &id in new_ids {
            let (mid, refs, subject, tid, key, re): (String, String, String, i64, String, bool) =
                by_id.query_row(params![id], |r| Ok((r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?;
            let mut found: Vec<(i64, i64)> = Vec::new();
            let mut look = |st: &mut rusqlite::CachedStatement, key: &str| -> rusqlite::Result<()> {
                let rows = st.query_map(params![account, key, min_new], |r| Ok((r.get(0)?, r.get(1)?)))?;
                for row in rows {
                    found.push(row?);
                }
                Ok(())
            };
            for r in refs.split_whitespace() {
                look(&mut by_msgid, r)?;
            }
            if !mid.is_empty() {
                look(&mut by_msgid, &mid)?;
                look(&mut children, &mid)?;
            }
            if !key.is_empty() {
                if re || prefixed.query_row(params![account, key, min_new], |r| r.get::<_, bool>(0))? {
                    // Grupo de assunto com resposta: todos viram um fio. Se já valia, um representante basta.
                    if re {
                        look(&mut peers, &key)?;
                    } else {
                        look(&mut one_peer, &key)?;
                    }
                }
            }
            for (eid, etid) in found {
                touch(eid, etid);
            }
            loaded.insert(id, (id, mid, refs, subject, tid));
        }
        let mut by_thread = conn.prepare_cached("SELECT id, message_id, refs, subject, thread_id FROM messages WHERE account = ?1 AND thread_id = ?2")?;
        for tid in &touched_threads {
            let rows = by_thread.query_map(params![account, tid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
            for row in rows {
                let row: Row = row?;
                loaded.insert(row.0, row);
            }
        }
        let mut single = conn.prepare_cached("SELECT id, message_id, refs, subject, thread_id FROM messages WHERE id = ?1")?;
        for id in &touched_msgs {
            let row: Row = single.query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
            loaded.insert(row.0, row);
        }
    }
    let rows: Vec<Row> = loaded.into_values().collect();
    let entries: Vec<threads::Entry> = rows
        .iter()
        .map(|(id, message_id, refs, subject, _)| threads::Entry { id: *id, message_id, refs, subject })
        .collect();
    let keys = threads::group(&entries);
    let mut up = conn.prepare_cached("UPDATE messages SET thread_id = ?1 WHERE id = ?2")?;
    for ((id, _, _, _, old), key) in rows.iter().zip(keys) {
        if *old != key {
            up.execute(params![key, id])?;
        }
    }
    Ok(())
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Devolve o WAL ao arquivo principal sem bloquear leitores. O auto-checkpoint só dispara em ~1 GiB (o fsync dele
    /// no meio da carga inicial custava ~1,5 s em 50k mensagens), então quem termina uma sync chama isto.
    pub fn checkpoint(&self) {
        self.conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(())).ok();
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    /// Ajustes por conexão (não persistem no arquivo, exceto o modo WAL): cache e mmap grandes, WAL e fsync relaxado.
    fn tune(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            // busy_timeout primeiro: trocar o journal_mode pode esbarrar na conexão do sync em segundo plano (o Worker
            // do Android) e, sem espera, falha na hora com "database is locked", derrubando a abertura do app.
            "PRAGMA busy_timeout = 5000;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -65536;
             PRAGMA mmap_size = 268435456;
             PRAGMA wal_autocheckpoint = 262144;
             PRAGMA journal_size_limit = 134217728;
             PRAGMA temp_store = MEMORY;",
        )?;
        Ok(())
    }

    fn init(conn: Connection) -> Result<Self> {
        Self::tune(&conn)?;
        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let rev: i32 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        // Caminho rápido: esquema já na revisão atual, nada a criar nem migrar.
        if version == SCHEMA_VERSION && rev == SCHEMA_REV {
            return Ok(Self { conn });
        }
        if version != SCHEMA_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS messages_fts; DROP TABLE IF EXISTS messages; DROP TABLE IF EXISTS mailbox_state;
                 DROP TABLE IF EXISTS msg_refs; DROP TABLE IF EXISTS contacts;",
            )?;
        }
        conn.execute_batch(FTS_CREATE)?;
        // Funde segmentos do índice só quando há 16 deles (padrão 4): a carga inicial paga bem menos merges.
        conn.execute("INSERT INTO messages_fts (messages_fts, rank) VALUES ('automerge', 16)", [])?;
        conn.execute_batch(SCHEMA)?;
        let store = Self { conn };
        store.migrate()?;
        store.conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        store.conn.pragma_update(None, "application_id", SCHEMA_REV)?;
        Ok(store)
    }

    fn has_column(&self, table: &str, col: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_xinfo(?1) WHERE name = ?2",
            params![table, col],
            |r| r.get(0),
        )?)
    }

    /// ALTER só se a coluna não existe (idempotente); devolve `true` se acabou de criar.
    fn add_column(&self, table: &str, col: &str, decl: &str) -> Result<bool> {
        if self.has_column(table, col)? {
            return Ok(false);
        }
        self.conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl}"))?;
        Ok(true)
    }

    /// Leva bancos de qualquer versão anterior (inclusive o esquema v5 do master) ao esquema atual, com backfill.
    fn migrate(&self) -> Result<()> {
        // `drafts` não é recriada com o resto: rascunhos de versões antigas ganham as colunas de Cc/Bcc/HTML.
        for col in ["cc_addr", "bcc_addr", "html"] {
            self.add_column("drafts", col, "TEXT NOT NULL DEFAULT ''")?;
        }
        // Bancos da v5 já têm as mensagens, mas não o fio: ganham a coluna e os fios são calculados abaixo.
        self.add_column("messages", "thread_id", "INTEGER NOT NULL DEFAULT 0")?;
        let new_pinned = self.add_column("messages", "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        let new_snooze = self.add_column("messages", "snooze_until", "INTEGER NOT NULL DEFAULT 0")?;
        // Ordenação da lista: fixadas, depois a data (ou a volta da adiada). Coluna virtual, indexável.
        self.add_column("messages", "visible_date", "INTEGER GENERATED ALWAYS AS (MAX(date, snooze_until)) VIRTUAL")?;
        // Bancos antigos já têm o corpo de todas as mensagens.
        self.add_column("messages", "body_loaded", "INTEGER NOT NULL DEFAULT 1")?;
        let new_subject = self.add_column("messages", "subject_key", "TEXT NOT NULL DEFAULT ''")?;
        self.add_column("messages", "subject_re", "INTEGER NOT NULL DEFAULT 0")?;
        // Só mensagens novas trazem a importância; as anteriores ficam 0 (normal), sem baixar nada.
        self.add_column("messages", "importance", "INTEGER NOT NULL DEFAULT 0")?;
        // NULL = ainda não lido; as anteriores buscam no servidor uma vez, ao abrir.
        self.add_column("messages", "unsub", "TEXT")?;
        self.add_column("mailbox_state", "highest_modseq", "INTEGER NOT NULL DEFAULT 0")?;
        self.add_column("mailbox_state", "uid_next", "INTEGER NOT NULL DEFAULT 0")?;
        // 1 = a cópia local já tem todas as mensagens da pasta (sync incremental vale); 0 = falta histórico.
        self.add_column("mailbox_state", "complete", "INTEGER NOT NULL DEFAULT 0")?;

        // FTS criado sem índice de prefixo (master): recria e reindexa uma única vez.
        let fts_sql: String = self.conn.query_row("SELECT sql FROM sqlite_master WHERE name = 'messages_fts'", [], |r| r.get(0))?;
        if !fts_sql.contains("prefix") {
            self.conn.execute_batch("DROP TABLE messages_fts;")?;
            self.conn.execute_batch(FTS_CREATE)?;
            self.conn.execute_batch("INSERT INTO messages_fts (messages_fts) VALUES ('rebuild');")?;
        }
        if new_pinned || new_snooze {
            self.conn.execute_batch(&format!(
                "UPDATE messages AS m SET
                   pinned = EXISTS (SELECT 1 FROM pins p WHERE p.account = m.account AND p.msg_key = {MSG_KEY}),
                   snooze_until = COALESCE((SELECT z.until FROM snoozes z WHERE z.account = m.account AND z.msg_key = {MSG_KEY}), 0)"
            ))?;
        }
        if new_subject {
            self.backfill_subjects()?;
        }
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS messages_by_thread ON messages (account, thread_id);
             CREATE INDEX IF NOT EXISTS messages_by_msgid ON messages (account, message_id) WHERE message_id <> '';
             CREATE INDEX IF NOT EXISTS messages_by_subject ON messages (account, subject_key) WHERE subject_key <> '';
             CREATE INDEX IF NOT EXISTS messages_by_top ON messages (account, mailbox, pinned DESC, visible_date DESC, id DESC);
             CREATE INDEX IF NOT EXISTS messages_unread ON messages (account, mailbox, snooze_until) WHERE seen = 0;
             CREATE INDEX IF NOT EXISTS messages_starred ON messages (date DESC, id DESC) WHERE flagged = 1;",
        )?;
        let refs_empty: bool = self.conn.query_row("SELECT NOT EXISTS (SELECT 1 FROM msg_refs)", [], |r| r.get(0))?;
        if refs_empty {
            self.backfill_refs()?;
        }
        let contacts_empty: bool = self.conn.query_row("SELECT NOT EXISTS (SELECT 1 FROM contacts)", [], |r| r.get(0))?;
        if contacts_empty {
            self.backfill_contacts()?;
        }
        let pending: Vec<String> = {
            let mut st = self.conn.prepare("SELECT DISTINCT account FROM messages WHERE thread_id = 0")?;
            let rows = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        for account in pending {
            self.rebuild_threads(&account)?;
        }
        Ok(())
    }

    fn backfill_subjects(&self) -> Result<()> {
        let rows: Vec<(i64, String)> = {
            let mut st = self.conn.prepare("SELECT id, subject FROM messages")?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut up = tx.prepare("UPDATE messages SET subject_key = ?1, subject_re = ?2 WHERE id = ?3")?;
            for (id, subject) in rows {
                let (key, re) = threads::normalize_subject(&subject);
                up.execute(params![key, re, id])?;
            }
        }
        Ok(tx.commit()?)
    }

    fn backfill_refs(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut st = tx.prepare("SELECT id, account, refs FROM messages WHERE refs <> ''")?;
            let rows: Vec<(i64, String, String)> =
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
            let mut ins = tx.prepare("INSERT OR IGNORE INTO msg_refs (id, account, ref) VALUES (?1, ?2, ?3)")?;
            for (id, account, refs) in rows {
                for r in refs.split_whitespace() {
                    ins.execute(params![id, account, r])?;
                }
            }
        }
        Ok(tx.commit()?)
    }

    fn backfill_contacts(&self) -> Result<()> {
        let mut acc: std::collections::HashMap<String, (String, String, u32, i64)> = std::collections::HashMap::new();
        {
            let mut st = self.conn.prepare("SELECT from_name, from_addr, to_addrs, cc_addrs, date FROM messages ORDER BY date DESC")?;
            let rows = st.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?))
            })?;
            for row in rows {
                let (name, from, to, cc, date) = row?;
                contact_hits(&name, &from, &to, &cc, date, &mut acc);
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        Self::upsert_contacts(&tx, &acc)?;
        Ok(tx.commit()?)
    }

    fn upsert_contacts(conn: &Connection, hits: &std::collections::HashMap<String, (String, String, u32, i64)>) -> Result<()> {
        if hits.is_empty() {
            return Ok(());
        }
        {
            let mut up = conn.prepare_cached(
                "INSERT INTO contacts (key, addr, name, count, last_date) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (key) DO UPDATE SET
                   name = CASE WHEN contacts.name = '' THEN excluded.name ELSE contacts.name END,
                   count = contacts.count + excluded.count,
                   last_date = MAX(contacts.last_date, excluded.last_date)",
            )?;
            for (key, (addr, name, count, date)) in hits {
                up.execute(params![key, addr, name, count, date])?;
            }
        }
        Ok(())
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

    /// Grava mensagens novas e devolve os ids das que entraram (as já existentes são ignoradas).
    /// Os fios são atualizados de forma incremental: só o que o lote toca é recalculado.
    pub fn insert_messages(&mut self, account: &str, mailbox: &str, msgs: &[NewMessage]) -> Result<Vec<i64>> {
        if msgs.is_empty() {
            return Ok(Vec::new());
        }
        let tx = self.conn.transaction()?;
        let max_before: i64 = tx.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| r.get(0))?;
        let mut ids = Vec::with_capacity(msgs.len());
        let mut contacts = std::collections::HashMap::new();
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO messages
                 (account, mailbox, uid, from_name, from_addr, subject, date, snippet, body_text, body_html, seen,
                  message_id, refs, reply_addr, attachments, flagged, to_addrs, cc_addrs, subject_key, subject_re, body_loaded, importance, unsub)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            )?;
            let mut refs = tx.prepare_cached("INSERT OR IGNORE INTO msg_refs (id, account, ref) VALUES (?1, ?2, ?3)")?;
            for m in msgs {
                let (key, re) = threads::normalize_subject(&m.subject);
                let n = stmt.execute(params![
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
                    m.cc_addrs,
                    key,
                    re,
                    !m.header_only,
                    m.importance,
                    m.unsub
                ])?;
                if n == 0 {
                    continue;
                }
                let id = tx.last_insert_rowid();
                ids.push(id);
                for r in m.refs.split_whitespace() {
                    refs.execute(params![id, account, r])?;
                }
                contact_hits(&m.from_name, &m.from_addr, &m.to_addrs, &m.cc_addrs, m.date, &mut contacts);
            }
        }
        if !ids.is_empty() {
            // Mensagem que volta (ex.: movida de pasta) herda fixada/adiada pela chave de categoria.
            let has_marks: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM pins WHERE account = ?1) OR EXISTS (SELECT 1 FROM snoozes WHERE account = ?1)",
                params![account],
                |r| r.get(0),
            )?;
            if has_marks {
                tx.execute(
                    &format!(
                        "UPDATE messages AS m SET
                           pinned = EXISTS (SELECT 1 FROM pins p WHERE p.account = m.account AND p.msg_key = {MSG_KEY}),
                           snooze_until = COALESCE((SELECT z.until FROM snoozes z WHERE z.account = m.account AND z.msg_key = {MSG_KEY}), 0)
                         WHERE m.account = ?1 AND m.id > ?2"
                    ),
                    params![account, max_before],
                )?;
            }
            thread_incremental(&tx, account, &ids)?;
            Self::upsert_contacts(&tx, &contacts)?;
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Guarda o corpo baixado de uma mensagem que só tinha os cabeçalhos. Devolve o id local. Como veio a mensagem inteira,
    /// grava também as linhas List-Unsubscribe (`m.unsub`) se ainda eram NULL: abrir não precisa ir ao servidor buscá-las.
    pub fn set_body(&mut self, account: &str, mailbox: &str, uid: u32, m: &NewMessage) -> Result<Option<i64>> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM messages WHERE account = ?1 AND mailbox = ?2 AND uid = ?3",
                params![account, mailbox, uid],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(None) };
        self.conn.execute(
            "UPDATE messages SET body_text = ?2, body_html = ?3, snippet = ?4, attachments = ?5, body_loaded = 1,
             unsub = COALESCE(unsub, ?6) WHERE id = ?1",
            params![
                id,
                m.body_text,
                m.body_html,
                snippet(&m.body_text),
                serde_json::to_string(&m.attachments).unwrap_or_else(|_| "[]".into()),
                m.unsub
            ],
        )?;
        Ok(Some(id))
    }

    /// Onde buscar o corpo de uma mensagem que ainda não o tem (`None`: já baixado ou id inexistente).
    pub fn body_missing(&self, id: i64) -> Result<Option<(String, String, u32)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT account, mailbox, uid FROM messages WHERE id = ?1 AND body_loaded = 0",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    /// UIDs das `limit` mensagens mais recentes da pasta ainda sem corpo, para baixar em segundo plano.
    pub fn uids_without_body(&self, account: &str, mailbox: &str, limit: u32) -> Result<Vec<u32>> {
        let mut st = self.conn.prepare_cached(
            "SELECT uid FROM messages WHERE account = ?1 AND mailbox = ?2 AND body_loaded = 0
             ORDER BY date DESC, id DESC LIMIT ?3",
        )?;
        let rows = st.query_map(params![account, mailbox, limit], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Aplica o estado do servidor: remove o que sumiu e atualiza as flags de lida e estrela.
    /// Cada item é `(uid, lida, estrela)`. Devolve o que mudou, por id local.
    pub fn apply_server_state(&mut self, account: &str, mailbox: &str, server: &[(u32, bool, bool)]) -> Result<Delta> {
        self.apply_server_state_from(account, mailbox, 0, server)
    }

    /// Como `apply_server_state`, mas só vale para UIDs a partir de `lo`: o servidor informou a faixa `lo:*`
    /// e o que é mais antigo fica como está. Sync incremental sem CONDSTORE usa isto numa janela recente.
    pub fn apply_server_state_from(&mut self, account: &str, mailbox: &str, lo: u32, server: &[(u32, bool, bool)]) -> Result<Delta> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS server_uids (uid INTEGER PRIMARY KEY, seen INTEGER, flagged INTEGER); DELETE FROM server_uids;")?;
        {
            let mut ins = tx.prepare_cached("INSERT OR REPLACE INTO server_uids (uid, seen, flagged) VALUES (?1, ?2, ?3)")?;
            for (uid, seen, flagged) in server {
                ins.execute(params![uid, seen, flagged])?;
            }
        }
        let ids = |sql: &str| -> rusqlite::Result<Vec<i64>> {
            tx.prepare(sql)?.query_map(params![account, mailbox, lo], |r| r.get(0))?.collect()
        };
        let removed = ids("SELECT id FROM messages WHERE account = ?1 AND mailbox = ?2 AND uid >= ?3 AND uid NOT IN (SELECT uid FROM server_uids)")?;
        let flips: Vec<(i64, bool, bool)> = {
            let mut st = tx.prepare(
                "SELECT m.id, s.seen, s.flagged FROM messages m JOIN server_uids s ON s.uid = m.uid
                 WHERE m.account = ?1 AND m.mailbox = ?2 AND m.uid >= ?3 AND (m.seen <> s.seen OR m.flagged <> s.flagged)",
            )?;
            let rows = st.query_map(params![account, mailbox, lo], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        tx.execute(
            "DELETE FROM messages WHERE account = ?1 AND mailbox = ?2
             AND uid >= ?3 AND uid NOT IN (SELECT uid FROM server_uids)",
            params![account, mailbox, lo],
        )?;
        {
            let mut up = tx.prepare_cached("UPDATE messages SET seen = ?2, flagged = ?3 WHERE id = ?1")?;
            for (id, seen, flagged) in &flips {
                up.execute(params![id, seen, flagged])?;
            }
        }
        let changed = flips.into_iter().map(|f| f.0).collect();
        tx.commit()?;
        Ok(Delta { inserted: Vec::new(), changed, removed })
    }

    /// Sync incremental: aplica só as flags que o servidor reportou como alteradas. Devolve os ids que mudaram de fato.
    pub fn apply_flag_changes(&mut self, account: &str, mailbox: &str, changes: &[(u32, bool, bool)]) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut changed = Vec::new();
        {
            let mut up = tx.prepare_cached(
                "UPDATE messages SET seen = ?4, flagged = ?5
                 WHERE account = ?1 AND mailbox = ?2 AND uid = ?3 AND (seen <> ?4 OR flagged <> ?5) RETURNING id",
            )?;
            for (uid, seen, flagged) in changes {
                let id: Option<i64> = up.query_row(params![account, mailbox, uid, seen, flagged], |r| r.get(0)).optional()?;
                changed.extend(id);
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Sync incremental: remove as UIDs que o servidor disse que sumiram (VANISHED). Devolve os ids removidos.
    pub fn remove_uids(&mut self, account: &str, mailbox: &str, uids: &[u32]) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut removed = Vec::new();
        {
            let mut del = tx.prepare_cached("DELETE FROM messages WHERE account = ?1 AND mailbox = ?2 AND uid = ?3 RETURNING id")?;
            for uid in uids {
                let id: Option<i64> = del.query_row(params![account, mailbox, uid], |r| r.get(0)).optional()?;
                removed.extend(id);
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Marcas de sincronização da pasta (`None` se nunca sincronizou).
    pub fn sync_state(&self, account: &str, mailbox: &str) -> Result<Option<SyncState>> {
        Ok(self
            .conn
            .query_row(
                "SELECT uid_validity, highest_modseq, uid_next, complete FROM mailbox_state WHERE account = ?1 AND mailbox = ?2",
                params![account, mailbox],
                |r| {
                    Ok(SyncState {
                        uid_validity: r.get(0)?,
                        highest_modseq: r.get::<_, i64>(1)? as u64,
                        uid_next: r.get(2)?,
                        complete: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_sync_state(&self, account: &str, mailbox: &str, highest_modseq: u64, uid_next: u32, complete: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE mailbox_state SET highest_modseq = ?3, uid_next = ?4, complete = ?5 WHERE account = ?1 AND mailbox = ?2",
            params![account, mailbox, highest_modseq as i64, uid_next, complete],
        )?;
        Ok(())
    }

    /// Maior UID guardado da pasta (0 se vazia).
    pub fn max_uid(&self, account: &str, mailbox: &str) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(uid), 0) FROM messages WHERE account = ?1 AND mailbox = ?2",
            params![account, mailbox],
            |r| r.get(0),
        )?)
    }

    /// Menor UID entre as `n` mais recentes (por UID) da pasta: início da janela que a sync incremental reconcilia.
    /// `0` quando há menos de `n` mensagens (a janela é a pasta inteira).
    pub fn window_start(&self, account: &str, mailbox: &str, n: u32) -> Result<u32> {
        Ok(self
            .conn
            .query_row(
                "SELECT uid FROM messages WHERE account = ?1 AND mailbox = ?2 ORDER BY uid DESC LIMIT 1 OFFSET ?3",
                params![account, mailbox, n.saturating_sub(1)],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn count_box(&self, account: &str, mailbox: &str) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE account = ?1 AND mailbox = ?2",
            params![account, mailbox],
            |r| r.get(0),
        )?)
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

    /// Pastas (conta, nome) que têm o papel, uma por conta. Caixa de entrada sem linha em `folders` é "INBOX".
    fn role_boxes(&self, role: Role) -> Result<Vec<(String, String)>> {
        let accounts: Vec<String> = {
            // Varredura "solta" no índice: O(contas), não O(mensagens).
            let mut st = self.conn.prepare_cached(
                "WITH RECURSIVE a(x) AS (
                   SELECT MIN(account) FROM messages
                   UNION ALL SELECT (SELECT MIN(account) FROM messages WHERE account > a.x) FROM a WHERE x IS NOT NULL)
                 SELECT x FROM a WHERE x IS NOT NULL UNION SELECT account FROM folders",
            )?;
            let rows = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        let mut out = Vec::new();
        for acc in accounts {
            if let Some(name) = self.folder_name(&acc, role)? {
                out.push((acc, name));
            }
        }
        Ok(out)
    }

    /// Não lidas por (conta, pasta), numa passada pelo índice parcial `messages_unread` (só linhas não lidas), sem tocar na tabela.
    fn unread_per_box(&self) -> Result<std::collections::HashMap<(String, String), u32>> {
        let mut st = self.conn.prepare_cached(
            "SELECT account, mailbox, COUNT(*) FROM messages
             WHERE seen = 0 AND snooze_until <= CAST(strftime('%s', 'now') AS INTEGER)
             GROUP BY account, mailbox",
        )?;
        let rows = st
            .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Não lidas por papel de pasta, somando as contas (papéis sem pasta ficam em 0). Deriva de `unread_by_account`.
    pub fn unread_counts(&self) -> Result<Vec<(Role, u32)>> {
        let by = self.unread_by_account()?;
        Ok(Role::ALL.iter().map(|r| (*r, by.iter().filter(|(_, br, _)| br == r).map(|(_, _, n)| n).sum())).collect())
    }

    /// Não lidas por conta e papel de pasta (só os pares que têm pasta; o resto é 0).
    pub fn unread_by_account(&self) -> Result<Vec<(String, Role, u32)>> {
        let per_box = self.unread_per_box()?;
        let mut out = Vec::new();
        for role in Role::ALL {
            for (account, name) in self.role_boxes(role)? {
                let n = per_box.get(&(account.clone(), name)).copied().unwrap_or(0);
                out.push((account, role, n));
            }
        }
        Ok(out)
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
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.id IN (
               SELECT m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 AND m.flagged = 1 AND {NOT_DISCARDED} ORDER BY m.date DESC LIMIT ?2)
             ORDER BY m.date DESC"
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
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.id IN (
               SELECT m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 ORDER BY m.date DESC LIMIT ?2)
             ORDER BY m.date DESC"
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
                &format!("SELECT {SUMMARY_COLS}, m.body_text, m.body_html, m.reply_addr, m.attachments, m.to_addrs, m.cc_addrs, m.body_loaded FROM messages m WHERE m.id = ?1"),
                params![id],
                |r| {
                    Ok(MessageDetail {
                        summary: row_to_summary(r)?,
                        body_text: r.get(16)?,
                        body_html: r.get(17)?,
                        reply_addr: r.get(18)?,
                        attachments: serde_json::from_str(&r.get::<_, String>(19)?).unwrap_or_default(),
                        to_addrs: r.get(20)?,
                        cc_addrs: r.get(21)?,
                        body_loaded: r.get(22)?,
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

    /// Linhas de cancelamento guardadas; `None` = ainda não lidas (mensagem antiga ou cabeçalho cortado).
    pub fn unsub(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT unsub FROM messages WHERE id = ?1", params![id], |r| r.get(0)).optional()?.flatten())
    }

    pub fn set_unsub(&self, id: i64, lines: &str) -> Result<()> {
        self.conn.execute("UPDATE messages SET unsub = ?1 WHERE id = ?2", params![lines, id])?;
        Ok(())
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

    /// Uma pasta de uma conta na ordem da lista (fixadas, data visível, id), a partir do cursor (exclusivo).
    /// Usa o índice `messages_by_top`: custa O(limite + offset), não O(mensagens da pasta).
    fn box_top(
        &self,
        account: &str,
        mailbox: &str,
        cursor: Option<Cursor>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<(MessageSummary, i64)>> {
        let after = if cursor.is_some() { "AND (m.pinned, m.visible_date, m.id) < (?5, ?6, ?7)" } else { "" };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS}, m.visible_date FROM messages m
             WHERE m.account = ?1 AND m.mailbox = ?2 AND {NOT_SNOOZED} {after}
             {ORDER_TOP} LIMIT ?3 OFFSET ?4"
        ))?;
        let map = |r: &rusqlite::Row| Ok((row_to_summary(r)?, r.get::<_, i64>(16)?));
        let rows = match cursor {
            Some(c) => stmt
                .query_map(params![account, mailbox, limit, offset, c.pinned, c.visible_date, c.id], map)?
                .collect::<rusqlite::Result<_>>()?,
            None => stmt.query_map(params![account, mailbox, limit, offset], map)?.collect::<rusqlite::Result<_>>()?,
        };
        Ok(rows)
    }

    /// Junta as pastas de várias contas na ordem da lista, já com `limit` itens depois do cursor.
    fn merged_top(&self, boxes: &[(String, String)], cursor: Option<Cursor>, limit: u32, offset: u32) -> Result<Vec<(MessageSummary, i64)>> {
        if let [(acc, mb)] = boxes {
            return self.box_top(acc, mb, cursor, limit, offset);
        }
        let mut all = Vec::new();
        for (acc, mb) in boxes {
            all.extend(self.box_top(acc, mb, cursor, limit.saturating_add(offset), 0)?);
        }
        all.sort_by(|a, b| (b.0.pinned, b.1, b.0.id).cmp(&(a.0.pinned, a.1, a.0.id)));
        Ok(all.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    /// Resumos por id local, na ordem da lista (aplicar um `Delta` sem recarregar a pasta). Ids que não existem
    /// mais ou estão adiados ficam de fora: quem pediu trata a ausência como "saiu da lista".
    pub fn summaries(&self, ids: &[i64]) -> Result<Vec<MessageSummary>> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {SUMMARY_COLS} FROM messages m WHERE m.id IN ({marks}) AND {NOT_SNOOZED}"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk), row_to_summary)?;
            for r in rows {
                out.push(r?);
            }
        }
        out.sort_by(|a, b| (b.pinned, b.date, b.id).cmp(&(a.pinned, a.date, a.id)));
        Ok(out)
    }

    /// Pasta unificada: o papel em todas as contas, mais recentes primeiro.
    pub fn list_folder(&self, role: Role, limit: u32, offset: u32) -> Result<Vec<MessageSummary>> {
        let boxes = self.role_boxes(role)?;
        Ok(self.merged_top(&boxes, None, limit, offset)?.into_iter().map(|(m, _)| m).collect())
    }

    /// Página da pasta unificada por cursor (sem OFFSET). `cursor` vem do `next` da página anterior.
    pub fn list_folder_page(&self, role: Role, cursor: Option<&str>, limit: u32) -> Result<Page> {
        let boxes = self.role_boxes(role)?;
        self.page_of(&boxes, cursor, limit)
    }

    /// Página de uma pasta de uma conta por cursor.
    pub fn list_box_page(&self, account: &str, mailbox: &str, cursor: Option<&str>, limit: u32) -> Result<Page> {
        self.page_of(&[(account.to_string(), mailbox.to_string())], cursor, limit)
    }

    fn page_of(&self, boxes: &[(String, String)], cursor: Option<&str>, limit: u32) -> Result<Page> {
        let cursor = match cursor {
            Some(c) => Some(Cursor::decode(c).ok_or_else(|| crate::Error::Invalid("cursor inválido".into()))?),
            None => None,
        };
        let mut rows = self.merged_top(boxes, cursor, limit.saturating_add(1), 0)?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = match (more, rows.last()) {
            (true, Some((m, vd))) => Some(Cursor { pinned: m.pinned, visible_date: *vd, id: m.id }.encode()),
            _ => None,
        };
        Ok(Page { items: rows.into_iter().map(|(m, _)| m).collect(), next })
    }

    pub fn count_folder(&self, role: Role) -> Result<u32> {
        let mut total = 0;
        for (acc, mb) in self.role_boxes(role)? {
            total += self.count_box(&acc, &mb)?;
        }
        Ok(total)
    }

    pub fn search_folder(&self, query: &str, role: Role, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.id IN (
               SELECT m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?2 AND {IN_ROLE} ORDER BY m.date DESC LIMIT ?3)
             ORDER BY m.date DESC"
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
    /// Lê a tabela `contacts` (mantida na gravação): não varre as mensagens.
    pub fn suggest_contacts(&self, query: &str, limit: usize) -> Result<Vec<Contact>> {
        let q = query.trim().to_lowercase();
        let like = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare_cached(
            "SELECT name, addr, count FROM contacts
             WHERE ?1 = '' OR key LIKE ?2 ESCAPE '\\' OR lower(name) LIKE ?2 ESCAPE '\\'
             ORDER BY count DESC, last_date DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![q, like, limit as i64], |r| Ok(Contact { name: r.get(0)?, addr: r.get(1)?, count: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
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

    /// Não lidas por pasta do usuário (mensagens adiadas não contam, como nos papéis).
    pub fn user_folder_unread(&self) -> Result<Vec<FolderUnread>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.account, m.mailbox, COUNT(*) FROM messages m
             JOIN user_folders u ON u.account = m.account AND u.name = m.mailbox
             WHERE m.seen = 0 AND m.snooze_until <= CAST(strftime('%s', 'now') AS INTEGER)
             GROUP BY m.account, m.mailbox ORDER BY m.account, m.mailbox",
        )?;
        let rows = stmt
            .query_map([], |r| Ok(FolderUnread { account: r.get(0)?, name: r.get(1)?, unread: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
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
        Ok(self.box_top(account, mailbox, None, limit, offset)?.into_iter().map(|(m, _)| m).collect())
    }

    pub fn search_box(&self, account: &str, mailbox: &str, query: &str, limit: u32) -> Result<Vec<MessageSummary>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.id IN (
               SELECT m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 AND m.account = ?2 AND m.mailbox = ?3 ORDER BY m.date DESC LIMIT ?4)
             ORDER BY m.date DESC"
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
            "SELECT {SUMMARY_COLS} FROM messages m WHERE m.id IN (
               SELECT m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1
               AND (SELECT c.category FROM message_categories c WHERE c.account = m.account AND c.msg_key = {key}) = ?2
               AND {NOT_DISCARDED}
             ORDER BY m.date DESC LIMIT ?3)
             ORDER BY m.date DESC",
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

    /// Esquema v5 como está no master (v0.1.7), congelado: base das provas de migração.
    const MASTER: &str = include_str!("../tests/fixtures/master_v5.sql");

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
            ..Default::default()
        }
    }

    #[test]
    fn importancia_e_gravada_e_lida_e_banco_antigo_fica_normal() {
        let mut s = Store::open_in_memory().unwrap();
        let mut hi = msg(1, "Urgente", "x", 100);
        hi.importance = 1;
        let mut lo = msg(2, "Sem pressa", "x", 200);
        lo.importance = -1;
        let ids = s.insert_messages("a", "INBOX", &[hi, lo, msg(3, "Normal", "x", 300)]).unwrap();
        let imp = |s: &Store, id: i64| s.summaries(&[id]).unwrap()[0].importance;
        assert_eq!([imp(&s, ids[0]), imp(&s, ids[1]), imp(&s, ids[2])], [1, -1, 0]);
        assert_eq!(s.get(ids[0]).unwrap().unwrap().summary.importance, 1);
        // Banco criado antes da coluna: ela é adicionada e as mensagens antigas ficam normais.
        s.conn.execute_batch("ALTER TABLE messages DROP COLUMN importance").unwrap();
        s.conn.pragma_update(None, "application_id", SCHEMA_REV - 1).unwrap();
        let s = Store::init(s.conn).unwrap();
        assert!(s.summaries(&ids).unwrap().iter().all(|m| m.importance == 0));
    }

    #[test]
    fn cancelamento_guardado_no_sync_e_banco_antigo_fica_desconhecido() {
        let mut s = Store::open_in_memory().unwrap();
        let mut com = msg(1, "Boletim", "x", 100);
        com.unsub = Some("list-unsubscribe: <https://x.com/u>\r\n".into());
        let mut sem = msg(2, "Oi", "x", 200);
        sem.unsub = Some(String::new());
        let ids = s.insert_messages("a", "INBOX", &[com, sem, msg(3, "Cortado", "x", 300)]).unwrap();
        assert_eq!(s.unsub(ids[0]).unwrap().as_deref(), Some("list-unsubscribe: <https://x.com/u>\r\n"));
        assert_eq!(s.unsub(ids[1]).unwrap().as_deref(), Some(""));
        assert_eq!(s.unsub(ids[2]).unwrap(), None);
        s.set_unsub(ids[2], "").unwrap();
        assert_eq!(s.unsub(ids[2]).unwrap().as_deref(), Some(""));
        // Banco criado antes da coluna: ela é adicionada vazia e cada mensagem busca no servidor uma vez.
        s.conn.execute_batch("ALTER TABLE messages DROP COLUMN unsub").unwrap();
        s.conn.pragma_update(None, "application_id", SCHEMA_REV - 1).unwrap();
        let s = Store::init(s.conn).unwrap();
        assert!(ids.iter().all(|&id| s.unsub(id).unwrap().is_none()));
    }

    #[test]
    fn reabrir_banco_com_coluna_gerada_nao_falha() {
        let path = std::env::temp_dir().join(format!("posta-reopen-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let s = Store::open(&path).unwrap();
        // Revisão anterior força o migrate() a rodar de novo sobre a coluna gerada visible_date.
        s.conn.pragma_update(None, "application_id", SCHEMA_REV - 1).unwrap();
        drop(s);
        Store::open(&path).unwrap();
        let _ = std::fs::remove_file(&path);
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
    fn summaries_por_id_na_ordem_da_lista() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_uid_validity("a", "INBOX", 1).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "x", "", 100), msg(2, "y", "", 200), msg(3, "z", "", 300)]).unwrap();
        let ids: Vec<i64> = s.list("INBOX", 50, 0).unwrap().iter().map(|m| m.id).collect();
        let got = s.summaries(&[ids[2], 9999, ids[0]]).unwrap();
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), [ids[0], ids[2]], "ausente fica de fora; mais nova primeiro");
        assert!(s.summaries(&[]).unwrap().is_empty());
    }

    #[test]
    fn summaries_deixa_de_fora_adiadas_e_cursor_sobrevive_a_exclusao() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        let msgs: Vec<_> = (1..=30u32).map(|u| msg(u, &format!("m{u}"), "", 1000 + i64::from(u))).collect();
        s.insert_messages("a", "INBOX", &msgs).unwrap();
        let all: Vec<i64> = s.list_box_page("a", "INBOX", None, 100).unwrap().items.iter().map(|m| m.id).collect();
        s.snooze(&[all[1]], i64::MAX / 2).unwrap();
        let got = s.summaries(&[all[0], all[1]]).unwrap();
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), [all[0]], "adiada sai, como no delta");
        // O cursor guarda posição (fixada, data, id), não a linha: apagar a linha do cursor não invalida a próxima página.
        let first = s.list_box_page("a", "INBOX", None, 10).unwrap();
        let last = first.items.last().unwrap().id;
        s.remove(last).unwrap();
        let second = s.list_box_page("a", "INBOX", first.next.as_deref(), 10).unwrap();
        assert_eq!(second.items.len(), 10);
        assert!(second.items.iter().all(|m| m.id != last && !first.items.iter().any(|f| f.id == m.id)));
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
    fn nao_lidas_por_conta_somam_o_total_por_papel() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        s.set_folders("b", &folders_a()).unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "x", "", 1), msg(2, "y", "", 2)]).unwrap();
        s.insert_messages("b", "INBOX", &[msg(1, "z", "", 3)]).unwrap();
        s.insert_messages("b", "Lixeira", &[msg(2, "w", "", 4)]).unwrap();
        let by = s.unread_by_account().unwrap();
        let get = |acc: &str, role| by.iter().find(|(a, r, _)| a == acc && *r == role).map(|(_, _, n)| *n);
        assert_eq!((get("a", Role::Inbox), get("b", Role::Inbox), get("b", Role::Trash)), (Some(2), Some(1), Some(1)));
        // Conta sem pasta de Spam não aparece; o total por papel continua somando as contas.
        assert_eq!(get("a", Role::Junk), None);
        let total = s.unread_counts().unwrap().into_iter().find(|(r, _)| *r == Role::Inbox).unwrap().1;
        assert_eq!(total, 3);
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
        conn.execute_batch(&MASTER.replace("  thread_id INTEGER NOT NULL DEFAULT 0,\n", "")).unwrap();
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
        let master = MASTER
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
        conn.execute_batch(&cut(MASTER, "-- Produtividade")).unwrap();
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
    fn nao_lidas_por_pasta_do_usuario_ignoram_papeis_e_lidas() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_user_folders("a", &["Clientes".into(), "Clientes/Lumen".into()], "/").unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, "caixa", "x", 1)]).unwrap();
        s.insert_messages("a", "Clientes", &[msg(1, "um", "x", 2), msg(2, "dois", "x", 3)]).unwrap();
        s.insert_messages("a", "Clientes/Lumen", &[msg(1, "tres", "x", 4)]).unwrap();
        let id = s.list_box("a", "Clientes", 10, 0).unwrap()[0].id;
        s.set_seen(id, true).unwrap();
        let got: Vec<_> = s.user_folder_unread().unwrap().into_iter().map(|f| (f.name, f.unread)).collect();
        assert_eq!(got, [("Clientes".to_string(), 1), ("Clientes/Lumen".to_string(), 1)]);
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

    /// Gerador pseudoaleatório fixo (LCG), para provas reproduzíveis sem dependência extra.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }
    }

    fn thread_ids(s: &Store, account: &str) -> Vec<(i64, i64)> {
        let mut st = s.conn.prepare("SELECT id, thread_id FROM messages WHERE account = ?1 ORDER BY id").unwrap();
        let rows = st.query_map(params![account], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        rows
    }

    #[test]
    fn fios_incrementais_batem_com_o_recalculo_completo() {
        // Lotes aleatórios com respostas fora de ordem, cópias do mesmo Message-ID, assuntos repetidos
        // e referências a mensagens que só chegam depois. Depois de cada lote, o fio incremental tem de
        // ser igual ao recálculo completo da conta.
        for seed in 0..12u64 {
            let mut rng = Lcg(seed + 1);
            let mut s = Store::open_in_memory().unwrap();
            let mut uid = 0u32;
            for batch in 0..10 {
                let n = 1 + rng.next(8) as usize;
                let mut msgs = Vec::new();
                for _ in 0..n {
                    uid += 1;
                    let topic = rng.next(6);
                    let mut m = msg(uid, &format!("{}Assunto {topic}", ["", "Re: ", "RES: ", "Fwd: "][rng.next(4) as usize]), "", i64::from(uid));
                    // Message-ID de 0..=40; alguns repetidos entre pastas.
                    m.message_id = if rng.next(10) == 0 { String::new() } else { format!("id{}", rng.next(40)) };
                    let refs: Vec<String> = (0..rng.next(3)).map(|_| format!("id{}", rng.next(40))).collect();
                    m.refs = refs.join(" ");
                    msgs.push(m);
                }
                let mailbox = if batch % 3 == 2 { "Enviados" } else { "INBOX" };
                s.insert_messages("a", mailbox, &msgs).unwrap();
                let incremental = thread_ids(&s, "a");
                s.rebuild_threads("a").unwrap();
                assert_eq!(incremental, thread_ids(&s, "a"), "seed {seed}, lote {batch}");
            }
        }
    }

    #[test]
    fn paginas_por_cursor_percorrem_a_mesma_lista_sem_repetir() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_folders("a", &folders_a()).unwrap();
        let mut rng = Lcg(7);
        // Datas repetidas de propósito: o desempate pelo id é o que mantém o cursor estável.
        let msgs: Vec<_> = (1..=120u32).map(|u| msg(u, &format!("m{u}"), "", 1000 + rng.next(15) as i64)).collect();
        s.insert_messages("a", "INBOX", &msgs).unwrap();
        let ids: Vec<i64> = s.list("INBOX", 500, 0).unwrap().iter().map(|m| m.id).collect();
        s.set_pinned(&[ids[40], ids[99]], true).unwrap();
        s.snooze(&[ids[3], ids[60]], i64::MAX / 2).unwrap();
        let expected: Vec<i64> = s.list_folder(Role::Inbox, 500, 0).unwrap().iter().map(|m| m.id).collect();
        assert_eq!(expected.len(), 118, "os adiados saem da lista");
        assert!(s.list_folder(Role::Inbox, 2, 0).unwrap().iter().all(|m| m.pinned), "fixadas primeiro");

        let mut got = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = s.list_box_page("a", "INBOX", cursor.as_deref(), 17).unwrap();
            assert!(page.items.len() <= 17);
            got.extend(page.items.iter().map(|m| m.id));
            match page.next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(got, expected);
        assert!(s.list_box_page("a", "INBOX", Some("lixo"), 5).is_err());
        // Mensagem que entra no topo entre uma página e outra não desloca o que vem depois do cursor.
        let first = s.list_box_page("a", "INBOX", None, 10).unwrap();
        s.insert_messages("a", "INBOX", &[msg(500, "nova", "", 9999)]).unwrap();
        let second = s.list_box_page("a", "INBOX", first.next.as_deref(), 10).unwrap();
        assert_eq!(second.items.iter().map(|m| m.id).collect::<Vec<_>>(), expected[10..20]);
    }

    #[test]
    fn primeira_pagina_usa_o_indice_de_ordem_sem_ordenar() {
        let s = Store::open_in_memory().unwrap();
        let plan: Vec<String> = {
            let mut st = s
                .conn
                .prepare(
                    "EXPLAIN QUERY PLAN SELECT id FROM messages WHERE account = 'a' AND mailbox = 'INBOX'
                     ORDER BY pinned DESC, visible_date DESC, id DESC LIMIT 50",
                )
                .unwrap();
            let rows = st.query_map([], |r| r.get::<_, String>(3)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
            rows
        };
        let plan = plan.join(" | ");
        assert!(plan.contains("messages_by_top"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }

    #[test]
    fn corpo_que_chega_depois_entra_na_busca_e_nao_vira_duplicata() {
        let mut s = Store::open_in_memory().unwrap();
        let mut m = msg(1, "Proposta", "prévia curta", 10);
        m.header_only = true;
        let ids = s.insert_messages("a", "INBOX", &[m]).unwrap();
        assert!(!s.get(ids[0]).unwrap().unwrap().body_loaded);
        assert!(s.search("zumbaia", 10).unwrap().is_empty());
        assert_eq!(s.body_missing(ids[0]).unwrap(), Some(("a".into(), "INBOX".into(), 1)));
        assert_eq!(s.uids_without_body("a", "INBOX", 10).unwrap(), vec![1]);

        let full = msg(1, "Proposta", "texto completo com zumbaia no meio", 10);
        assert_eq!(s.set_body("a", "INBOX", 1, &full).unwrap(), Some(ids[0]));
        let d = s.get(ids[0]).unwrap().unwrap();
        assert!(d.body_loaded && d.body_text.contains("zumbaia"));
        assert_eq!(s.search("zumbai", 10).unwrap().len(), 1, "prefixo no FTS");
        assert_eq!(s.search("proposta", 10).unwrap().len(), 1);
        assert_eq!(s.body_missing(ids[0]).unwrap(), None);
        assert!(s.uids_without_body("a", "INBOX", 10).unwrap().is_empty());
        // Mensagem que sumiu do banco no meio do caminho: nada a gravar.
        assert_eq!(s.set_body("a", "INBOX", 99, &full).unwrap(), None);
    }

    #[test]
    fn delta_do_estado_do_servidor_traz_ids_alterados_e_removidos() {
        let mut s = Store::open_in_memory().unwrap();
        let ids = s.insert_messages("a", "INBOX", &[msg(1, "a", "", 1), msg(2, "b", "", 2), msg(3, "c", "", 3)]).unwrap();
        // 1 virou lida, 2 sumiu, 3 continua igual, 4 é novo (fica por conta de quem baixa).
        let d = s.apply_server_state("a", "INBOX", &[(1, true, false), (3, false, false), (4, false, false)]).unwrap();
        assert_eq!((d.changed, d.removed, d.inserted), (vec![ids[0]], vec![ids[1]], vec![]));
        assert_eq!(s.apply_server_state("a", "INBOX", &[(1, true, false), (3, false, false)]).unwrap(), Delta::default());
        // Janela: só mexe de `lo` para cima.
        let d = s.apply_server_state_from("a", "INBOX", 3, &[]).unwrap();
        assert_eq!((d.removed, s.local_uids("a", "INBOX").unwrap()), (vec![ids[2]], vec![1]));
    }

    #[test]
    fn migracao_de_banco_grande_do_master_termina_rapido_e_fica_consistente() {
        // 30 mil mensagens no schema do master, com respostas encadeadas e corpos: o backfill (fios, FTS,
        // contatos, referências) roda uma vez, em poucos segundos, e a segunda abertura não refaz nada.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MASTER).unwrap();
        conn.execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 30000)
             INSERT INTO messages (account, mailbox, uid, from_name, from_addr, subject, date, message_id, refs, to_addrs, cc_addrs, seen, body_text)
             SELECT 'a', 'INBOX', i, 'Pessoa ' || (i % 200), 'p' || (i % 200) || '@ex.com',
                    CASE WHEN i % 3 = 0 THEN 'Re: Assunto ' || (i % 500) ELSE 'Assunto ' || (i % 500) END,
                    1000000 + i, 'm' || i, CASE WHEN i % 5 = 0 THEN 'm' || (i - 1) ELSE '' END,
                    'eu@ex.com', 'c' || (i % 50) || '@ex.com', i % 4 = 0, 'texto do corpo ' || i || ' translado'
             FROM n;",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION).unwrap();

        let t = std::time::Instant::now();
        let s = Store::init(conn).unwrap();
        let took = t.elapsed();
        assert!(took < std::time::Duration::from_secs(60), "backfill lento: {took:?}");
        let total: i64 = s.conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 30000);
        assert_eq!(s.search("translado", 10).unwrap().len(), 10, "FTS cobre as mensagens antigas");
        assert!(!s.suggest_contacts("p12", 5).unwrap().is_empty());
        let zero_threads: i64 = s.conn.query_row("SELECT COUNT(*) FROM messages WHERE thread_id = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(zero_threads, 0, "todo mundo tem fio depois do backfill");
        let before = thread_ids(&s, "a");
        s.rebuild_threads("a").unwrap();
        assert_eq!(before, thread_ids(&s, "a"), "fios do backfill = recálculo completo");
        let t = std::time::Instant::now();
        let again = Store::init(s.conn).unwrap();
        assert!(t.elapsed() < std::time::Duration::from_millis(500), "reabrir não pode refazer o backfill");
        assert_eq!(again.list("INBOX", 5, 0).unwrap().len(), 5);
    }

    #[test]
    fn migracao_do_master_preenche_colunas_novas_e_e_idempotente() {
        // Banco do master com dados reais antes de qualquer coluna/índice de desempenho.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MASTER).unwrap();
        conn.execute_batch(
            "INSERT INTO messages (account, mailbox, uid, from_name, from_addr, subject, date, message_id, refs, to_addrs, cc_addrs, seen, body_text)
               VALUES ('a', 'INBOX', 1, 'Ana', 'ana@exemplo.com', 'Pauta', 100, 'p1', '', 'eu@exemplo.com', 'carla@exemplo.com', 0, 'corpo antigo zumbaia'),
                      ('a', 'INBOX', 2, 'Bia', 'bia@exemplo.com', 'Re: Pauta', 200, 'p2', 'p1', 'eu@exemplo.com', '', 1, 'resposta'),
                      ('a', 'INBOX', 3, 'Ana', 'ana@exemplo.com', 'Outra', 300, 'p3', '', 'eu@exemplo.com', '', 0, '');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION).unwrap();

        let s = Store::init(conn).unwrap();
        let list = s.list("INBOX", 10, 0).unwrap();
        assert_eq!(list.iter().map(|m| m.uid).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(list[1].thread_count, 2, "fios calculados na migração");
        assert!(list.iter().all(|m| !m.pinned));
        assert_eq!(s.search("zumbai", 10).unwrap().len(), 1, "FTS refeito, com prefixo");
        assert!(!s.suggest_contacts("carla", 10).unwrap().is_empty(), "contatos preenchidos");
        let unread: u32 = s.unread_counts().unwrap().iter().map(|(_, n)| n).sum();
        assert!(unread <= 2);
        let plan: String = {
            let mut st = s
                .conn
                .prepare("EXPLAIN QUERY PLAN SELECT id FROM messages WHERE account = 'a' AND mailbox = 'INBOX' ORDER BY pinned DESC, visible_date DESC, id DESC LIMIT 5")
                .unwrap();
            let rows: Vec<String> = st.query_map([], |r| r.get(3)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
            rows.join("|")
        };
        assert!(plan.contains("messages_by_top"), "{plan}");

        // Abrir de novo não refaz nem duplica nada.
        let before: i64 = s.conn.query_row("SELECT COUNT(*) FROM msg_refs", [], |r| r.get(0)).unwrap();
        let contacts: i64 = s.conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get(0)).unwrap();
        let again = Store::init(s.conn).unwrap();
        assert_eq!(again.conn.query_row("SELECT COUNT(*) FROM msg_refs", [], |r| r.get::<_, i64>(0)).unwrap(), before);
        assert_eq!(again.conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get::<_, i64>(0)).unwrap(), contacts);
        assert_eq!(again.list("INBOX", 10, 0).unwrap().len(), 3);
        // As colunas de sync ficam zeradas num banco que nunca sincronizou com a versão nova.
        assert_eq!(again.sync_state("a", "INBOX").unwrap().map(|x| x.highest_modseq), None);
    }
}
