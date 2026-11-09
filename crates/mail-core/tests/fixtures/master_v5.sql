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
