# Arquivar, excluir e pastas — plano de implementação

> **Para agentes:** use superpowers:subagent-driven-development. Passos com `- [ ]`.

**Objetivo:** arquivar (`e`) e excluir (`#`) com 4 s para desfazer, e navegar por Caixa de entrada, Enviados, Arquivo, Lixeira e Spam.

**Arquitetura:** o mail-core descobre as pastas de cada conta (SPECIAL-USE + nomes comuns) e guarda `papel → nome` no SQLite. A lista é unificada por papel (todas as contas), como a Caixa de entrada já é. Mover/excluir vão ao servidor só quando o aviso de desfazer expira; a linha local é apagada depois do OK do servidor.

**Stack:** Rust (`imap 3.0.0-alpha.15`, `imap-proto 0.16.7`, `rusqlite`), Tauri 2, React 19 + Vite, `@phosphor-icons/react`.

**Spec:** `docs/superpowers/specs/2026-11-08-arquivar-excluir-pastas-design.md`

## Restrições globais

- Textos de UI, comentários e commits em pt-BR, com acentos. Commits detalhados terminando com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Papéis (strings exatas, usadas em Rust serde e TS): `"inbox" | "sent" | "archive" | "trash" | "junk"`.
- Janela das pastas não-Caixa de entrada: `1000` ao abrir, `+500` ao rolar até o fim.
- Desfazer: `4000` ms.
- Não alterar `SCHEMA_VERSION` (a tabela nova usa `CREATE TABLE IF NOT EXISTS`).
- Rodar comandos a partir da raiz da worktree `.worktrees/pastas`.
- Tauri não compila no host (falta webkit): verificar com o Docker da Tarefa 4.

## Mapa de arquivos

| Arquivo | Responsabilidade |
| --- | --- |
| `crates/mail-core/src/folders.rs` (novo) | `Role`, `Folder`, escolha pura de pastas a partir do `LIST`, nome do Arquivo a criar |
| `crates/mail-core/src/store.rs` | tabela `folders`, `set_folders`/`folders`, `list_folder`, `search_folder`, `remove` |
| `crates/mail-core/src/imap_sync.rs` | `discover_folders`, `sync_mailbox(.., keep)`, `move_message`, `delete_forever` |
| `crates/mail-core/src/send.rs` | usa a descoberta de Enviados de `folders.rs` |
| `crates/mail-core/tests/imap_greenmail.rs` | integração: descobrir, mover, apagar, janela |
| `apps/client/src-tauri/src/lib.rs` | comandos `list_folders`, `sync_folder`, `move_message`, `delete_forever`; `list_messages`/`search_messages` por papel |
| `apps/client/src/api.ts`, `mock.ts` | contrato TS e mock em memória |
| `apps/client/src/folders.ts` (novo) | rótulos, ícones e textos por papel |
| `apps/client/src/App.tsx`, `styles.css` | menu de pastas, ações, atalhos, desfazer, confirmação |

---

### Tarefa 1: `folders.rs` — papéis e escolha de pastas (puro)

**Arquivos:** criar `crates/mail-core/src/folders.rs`; modificar `crates/mail-core/src/lib.rs`.

**Produz:**
- `pub enum Role { Inbox, Sent, Archive, Trash, Junk }` — `Serialize/Deserialize` lowercase, `Copy`, `Hash`, `as_str()`, `parse(&str) -> Option<Role>`.
- `pub struct Folder { pub role: Role, pub name: String }` (`Serialize`, `Clone`, `Debug`, `PartialEq`).
- `pub struct Listed { pub name: String, pub special: Option<&'static str> }` — `special` ∈ `"sent" | "archive" | "all" | "trash" | "junk"`.
- `pub fn pick(listed: &[Listed]) -> Vec<Folder>` — sempre inclui `Inbox → "INBOX"`; resto na ordem `Sent, Archive, Trash, Junk`.
- `pub fn archive_name(listed: &[Listed], delimiter: Option<&str>) -> String`.

- [ ] **Passo 1: escrever o módulo com os testes**

```rust
//! Pastas padrão de cada conta: qual nome do servidor faz o papel de Enviados, Arquivo, Lixeira e Spam.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Inbox,
    Sent,
    Archive,
    Trash,
    Junk,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Inbox, Role::Sent, Role::Archive, Role::Trash, Role::Junk];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Inbox => "inbox",
            Role::Sent => "sent",
            Role::Archive => "archive",
            Role::Trash => "trash",
            Role::Junk => "junk",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub role: Role,
    pub name: String,
}

/// Uma linha do LIST, já reduzida ao que importa aqui.
#[derive(Debug, Clone)]
pub struct Listed {
    pub name: String,
    /// Atributo SPECIAL-USE: "sent", "archive", "all", "trash" ou "junk".
    pub special: Option<&'static str>,
}

/// Nomes comuns quando o servidor não marca a pasta (comparação sem maiúsculas; também sob "INBOX.").
const NAMES: [(Role, &[&str]); 4] = [
    (Role::Sent, &["Sent", "Sent Items", "Sent Messages", "Enviados", "Itens Enviados"]),
    (Role::Archive, &["Archive", "Archives", "Arquivo", "Arquivados"]),
    (Role::Trash, &["Trash", "Deleted Items", "Deleted Messages", "Lixeira", "Itens Excluídos"]),
    (Role::Junk, &["Junk", "Junk E-mail", "Spam", "Lixo Eletrônico"]),
];

fn by_name<'a>(listed: &'a [Listed], wanted: &[&str]) -> Option<&'a Listed> {
    wanted.iter().find_map(|w| {
        listed.iter().find(|l| {
            let n = l.name.strip_prefix("INBOX.").or_else(|| l.name.strip_prefix("INBOX/")).unwrap_or(&l.name);
            n.eq_ignore_ascii_case(w) || n.to_lowercase() == w.to_lowercase()
        })
    })
}

/// Escolhe a pasta de cada papel. SPECIAL-USE ganha de nome; para Arquivo, \Archive ganha de \All (Gmail).
pub fn pick(listed: &[Listed]) -> Vec<Folder> {
    let special = |tag: &str| listed.iter().find(|l| l.special == Some(tag));
    let mut out = vec![Folder { role: Role::Inbox, name: "INBOX".into() }];
    for (role, names) in NAMES {
        let hit = match role {
            Role::Archive => special("archive").or_else(|| special("all")),
            _ => special(role.as_str()),
        }
        .or_else(|| by_name(listed, names));
        if let Some(l) = hit {
            out.push(Folder { role, name: l.name.clone() });
        }
    }
    out
}

/// Nome para criar o Arquivo: segue o prefixo "INBOX." quando o servidor organiza tudo sob a Caixa de entrada.
pub fn archive_name(listed: &[Listed], delimiter: Option<&str>) -> String {
    match delimiter {
        Some(d) if listed.iter().any(|l| l.name.starts_with(&format!("INBOX{d}"))) => format!("INBOX{d}Archive"),
        _ => "Archive".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(name: &str, special: Option<&'static str>) -> Listed {
        Listed { name: name.into(), special }
    }

    fn name(f: &[Folder], r: Role) -> Option<&str> {
        f.iter().find(|x| x.role == r).map(|x| x.name.as_str())
    }

    #[test]
    fn gmail_usa_special_use_e_all_como_arquivo() {
        let f = pick(&[
            l("INBOX", None),
            l("[Gmail]/E-mails enviados", Some("sent")),
            l("[Gmail]/Todos os e-mails", Some("all")),
            l("[Gmail]/Lixeira", Some("trash")),
            l("[Gmail]/Spam", Some("junk")),
        ]);
        assert_eq!(name(&f, Role::Inbox), Some("INBOX"));
        assert_eq!(name(&f, Role::Sent), Some("[Gmail]/E-mails enviados"));
        assert_eq!(name(&f, Role::Archive), Some("[Gmail]/Todos os e-mails"));
        assert_eq!(name(&f, Role::Trash), Some("[Gmail]/Lixeira"));
        assert_eq!(name(&f, Role::Junk), Some("[Gmail]/Spam"));
    }

    #[test]
    fn archive_ganha_de_all() {
        let f = pick(&[l("All", Some("all")), l("Arquivo", Some("archive"))]);
        assert_eq!(name(&f, Role::Archive), Some("Arquivo"));
    }

    #[test]
    fn sem_atributo_usa_nomes_comuns_e_prefixo_inbox() {
        let f = pick(&[l("INBOX", None), l("INBOX.Sent", None), l("Itens Excluídos", None), l("junk", None)]);
        assert_eq!(name(&f, Role::Sent), Some("INBOX.Sent"));
        assert_eq!(name(&f, Role::Trash), Some("Itens Excluídos"));
        assert_eq!(name(&f, Role::Junk), Some("junk"));
        assert_eq!(name(&f, Role::Archive), None);
    }

    #[test]
    fn nome_do_arquivo_a_criar() {
        assert_eq!(archive_name(&[l("INBOX", None), l("INBOX.Sent", None)], Some(".")), "INBOX.Archive");
        assert_eq!(archive_name(&[l("INBOX", None), l("Sent", None)], Some("/")), "Archive");
        assert_eq!(archive_name(&[], None), "Archive");
    }

    #[test]
    fn papel_ida_e_volta() {
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("drafts"), None);
    }
}
```

Em `lib.rs`: adicionar `pub mod folders;` e `pub use folders::{Folder, Role};`.

- [ ] **Passo 2:** `cargo test -p mail-core folders` → 5 testes passam.
- [ ] **Passo 3:** commit "Adiciona escolha de pastas padrão por SPECIAL-USE e nomes comuns".

---

### Tarefa 2: Store — tabela de pastas, lista por papel, remover

**Arquivos:** modificar `crates/mail-core/src/store.rs`.

**Consome:** `Role`, `Folder` (Tarefa 1).
**Produz:**
- `pub fn set_folders(&mut self, account: &str, folders: &[Folder]) -> Result<()>` (substitui as da conta).
- `pub fn folders(&self, account: &str) -> Result<Vec<Folder>>`.
- `pub fn folder_name(&self, account: &str, role: Role) -> Result<Option<String>>` (Inbox sem linha → `Some("INBOX")`).
- `pub fn list_folder(&self, role: Role, limit: u32, offset: u32) -> Result<Vec<MessageSummary>>` — todas as contas.
- `pub fn search_folder(&self, query: &str, role: Role, limit: u32) -> Result<Vec<MessageSummary>>`.
- `pub fn count_folder(&self, role: Role) -> Result<u32>`.
- `pub fn remove(&self, id: i64) -> Result<()>`.
- `list` e `search` existentes ficam iguais (usados pelos testes Greenmail).

- [ ] **Passo 1: testes (no `mod tests` existente)**

```rust
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
```

- [ ] **Passo 2:** `cargo test -p mail-core store` → falha (funções inexistentes).
- [ ] **Passo 3: implementar**

No `SCHEMA` (antes do `CREATE INDEX`):

```sql
CREATE TABLE IF NOT EXISTS folders (
  account TEXT NOT NULL,
  role TEXT NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (account, role)
);
CREATE INDEX IF NOT EXISTS messages_by_box ON messages (account, mailbox, date DESC);
```

Em `use`: `use crate::folders::{Folder, Role};`. Constante e métodos:

```rust
/// Nome da pasta do papel ?1 na conta da linha; Caixa de entrada sem linha em `folders` é "INBOX".
const IN_ROLE: &str = "m.mailbox = COALESCE(
    (SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = ?1),
    CASE ?1 WHEN 'inbox' THEN 'INBOX' END)";

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
```

- [ ] **Passo 4:** `cargo test -p mail-core` → todos passam.
- [ ] **Passo 5:** commit "Guarda pastas por conta e lista mensagens por papel".

---

### Tarefa 3: IMAP — descobrir, janela de sync, mover, excluir de vez

**Arquivos:** modificar `crates/mail-core/src/imap_sync.rs`, `crates/mail-core/src/send.rs`, `crates/mail-core/tests/imap_greenmail.rs`.

**Consome:** `folders::{pick, archive_name, Listed, Folder, Role}`, `Store::set_folders`.
**Produz:**
- `pub fn discover_folders(acc: &ImapAccount) -> Result<Vec<Folder>>` — cria e assina o Arquivo se faltar.
- `pub(crate) fn list_folders(session: &mut ImapSession) -> Result<(Vec<Listed>, Option<String>)>` (nomes + delimitador da INBOX).
- `pub fn sync_mailbox(acc, store, mailbox, keep: Option<usize>) -> Result<SyncReport>` (nova assinatura).
- `pub fn move_message(acc: &ImapAccount, from: &str, uid: u32, to: &str) -> Result<()>`.
- `pub fn delete_forever(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<()>`.

- [ ] **Passo 1: testes Greenmail** — em `imap_greenmail.rs`, trocar toda chamada `sync_mailbox(&x, &mut y, "INBOX")` por `sync_mailbox(&x, &mut y, "INBOX", None)` e acrescentar:

```rust
#[test]
fn descobre_pastas_e_cria_arquivo() {
    let Some(acc) = account("dir") else { return };
    let mut s = session(&acc);
    s.create("Trash").unwrap();
    s.logout().ok();

    let f = imap_sync::discover_folders(&acc).unwrap();
    let name = |r: Role| f.iter().find(|x| x.role == r).map(|x| x.name.clone());
    assert_eq!(name(Role::Inbox).as_deref(), Some("INBOX"));
    assert_eq!(name(Role::Trash).as_deref(), Some("Trash"));
    assert_eq!(name(Role::Archive).as_deref(), Some("Archive"));
    // Segunda vez não tenta criar de novo.
    assert_eq!(imap_sync::discover_folders(&acc).unwrap(), f);
}

#[test]
fn arquiva_e_exclui_de_vez() {
    let Some(acc) = account("mv") else { return };
    let mut s = session(&acc);
    s.append("INBOX", &raw(1, "Arquivar")).finish().unwrap();
    s.append("INBOX", &raw(2, "Ficar")).finish().unwrap();
    s.logout().ok();
    let folders = imap_sync::discover_folders(&acc).unwrap();
    let archive = folders.iter().find(|f| f.role == Role::Archive).unwrap().name.clone();

    let mut store = Store::open_in_memory().unwrap();
    store.set_folders(&acc.id, &folders).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    let alvo = store.list("INBOX", 10, 0).unwrap().into_iter().find(|m| m.subject == "Arquivar").unwrap();

    imap_sync::move_message(&acc, "INBOX", alvo.uid, &archive).unwrap();
    store.remove(alvo.id).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, &archive, Some(1000)).unwrap();
    let arquivo = store.list_folder(Role::Archive, 10, 0).unwrap();
    assert_eq!(arquivo.iter().map(|m| m.subject.as_str()).collect::<Vec<_>>(), ["Arquivar"]);
    assert_eq!(store.list_folder(Role::Inbox, 10, 0).unwrap().len(), 1);

    imap_sync::delete_forever(&acc, &archive, arquivo[0].uid).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, &archive, Some(1000)).unwrap();
    assert!(store.list_folder(Role::Archive, 10, 0).unwrap().is_empty());
}

#[test]
fn janela_baixa_so_as_mais_recentes() {
    let Some(acc) = account("win") else { return };
    let mut s = session(&acc);
    for i in 0..5 {
        s.append("INBOX", &raw(i, &format!("M{i}"))).finish().unwrap();
    }
    s.logout().ok();
    let mut store = Store::open_in_memory().unwrap();
    let r = imap_sync::sync_mailbox(&acc, &mut store, "INBOX", Some(2)).unwrap();
    assert_eq!((r.fetched, r.total), (2, 5));
    let mut uids = store.local_uids(&acc.id, "INBOX").unwrap();
    uids.sort_unstable();
    assert_eq!(uids.len(), 2);
    // Ampliar a janela baixa só o que falta.
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", Some(4)).unwrap().fetched, 2);
}
```

Atualizar o `use` do teste: `use mail_core::{imap_sync, send, ImapAccount, Outgoing, Role, Security, SmtpServer, Store};`.

- [ ] **Passo 2:** `cargo test -p mail-core` → não compila (assinatura nova). Esperado.
- [ ] **Passo 3: implementar em `imap_sync.rs`**

```rust
use imap_proto::NameAttribute;
use crate::folders::{archive_name, pick, Folder, Listed};
use crate::Error;

fn special(attrs: &[NameAttribute]) -> Option<&'static str> {
    attrs.iter().find_map(|a| match a {
        NameAttribute::Sent => Some("sent"),
        NameAttribute::Archive => Some("archive"),
        NameAttribute::All => Some("all"),
        NameAttribute::Trash => Some("trash"),
        NameAttribute::Junk => Some("junk"),
        _ => None,
    })
}

/// Pastas do servidor e o delimitador da hierarquia (o da INBOX).
pub(crate) fn list_folders(session: &mut ImapSession) -> Result<(Vec<Listed>, Option<String>)> {
    let names = session.list(None, Some("*"))?;
    let delimiter = names
        .iter()
        .find(|n| n.name().eq_ignore_ascii_case("INBOX"))
        .and_then(|n| n.delimiter())
        .map(str::to_string);
    let listed = names
        .iter()
        .filter(|n| !n.attributes().contains(&NameAttribute::NoSelect))
        .map(|n| Listed { name: n.name().to_string(), special: special(n.attributes()) })
        .collect();
    Ok((listed, delimiter))
}

/// Descobre as pastas padrão. Sem Arquivo, cria e assina um.
pub fn discover_folders(acc: &ImapAccount) -> Result<Vec<Folder>> {
    let mut session = connect(acc)?;
    let (listed, delimiter) = list_folders(&mut session)?;
    let mut folders = pick(&listed);
    if !folders.iter().any(|f| f.role == crate::Role::Archive) {
        let name = archive_name(&listed, delimiter.as_deref());
        session.create(&name)?;
        session.subscribe(&name).ok();
        folders.push(Folder { role: crate::Role::Archive, name });
        folders.sort_by_key(|f| crate::Role::ALL.iter().position(|r| *r == f.role));
    }
    session.logout().ok();
    Ok(folders)
}

/// Move pelo UID. Sem MOVE no servidor: copia, marca como apagada e remove.
pub fn move_message(acc: &ImapAccount, from: &str, uid: u32, to: &str) -> Result<()> {
    let mut session = connect(acc)?;
    let caps = session.capabilities()?;
    let (has_move, uidplus) = (caps.has_str("MOVE"), caps.has_str("UIDPLUS"));
    drop(caps);
    session.select(from)?;
    let id = uid.to_string();
    if has_move {
        session.uid_mv(&id, to)?;
    } else {
        session.uid_copy(&id, to)?;
        session.uid_store(&id, "+FLAGS.SILENT (\\Deleted)")?;
        if uidplus { session.uid_expunge(&id)?; } else { session.expunge()?; }
    }
    session.logout().ok();
    Ok(())
}

/// Apaga de vez (usado dentro da Lixeira).
pub fn delete_forever(acc: &ImapAccount, mailbox: &str, uid: u32) -> Result<()> {
    let mut session = connect(acc)?;
    let uidplus = session.capabilities()?.has_str("UIDPLUS");
    session.select(mailbox)?;
    let id = uid.to_string();
    session.uid_store(&id, "+FLAGS.SILENT (\\Deleted)")?;
    if uidplus { session.uid_expunge(&id)?; } else { session.expunge()?; }
    session.logout().ok();
    Ok(())
}
```

Se `Error` não for usado, não importar. Se `drop(caps)` não for necessário para o borrow checker, remover.

Em `sync_mailbox`: adicionar parâmetro `keep: Option<usize>` (doc: "`Some(n)`: só baixa o corpo das n mais recentes do servidor; as locais mais antigas ficam") e trocar o cálculo de `missing`:

```rust
    let mut newest: Vec<u32> = server.iter().map(|(u, _)| *u).collect();
    newest.sort_unstable_by(|a, b| b.cmp(a));
    if let Some(n) = keep {
        newest.truncate(n);
    }
    let missing: Vec<u32> = newest.into_iter().filter(|u| !local.contains(u)).collect();
```

UID inexistente ao mover (já movida em outro cliente): o servidor responde OK sem fazer nada no `UID MOVE`, então nada especial a fazer.

- [ ] **Passo 4: `send.rs` usa a descoberta** — remover `SENT_NAMES` e o import `NameAttribute`; trocar `sent_folder` por:

```rust
fn sent_folder(session: &mut ImapSession) -> Result<String> {
    let (listed, _) = crate::imap_sync::list_folders(session)?;
    if let Some(f) = pick(&listed).into_iter().find(|f| f.role == Role::Sent) {
        return Ok(f.name);
    }
    session.create("Sent")?;
    Ok("Sent".into())
}
```

com `use crate::folders::{pick, Role};`.

- [ ] **Passo 5:** `cargo test -p mail-core` (unitários) e `pnpm test:imap` (Greenmail, precisa de Docker) → todos passam, inclusive os 3 novos.
- [ ] **Passo 6:** commit "Descobre pastas, sincroniza com janela e move/exclui no IMAP".

---

### Tarefa 4: comandos Tauri

**Arquivos:** modificar `apps/client/src-tauri/src/lib.rs`.

**Consome:** Tarefas 1–3.
**Produz (contrato com a UI):**
- `list_folders() -> Vec<Role>` — papéis presentes em ao menos uma conta (Inbox sempre), ordem `Role::ALL`.
- `sync_inbox() -> SyncResult` — antes de sincronizar cada conta, se `store.folders(id)` vier vazio, roda `discover_folders` e `set_folders` (erro de descoberta vira item em `errors`, não interrompe).
- `sync_folder(role: Role, keep: usize) -> SyncResult` — para cada conta com essa pasta; `SyncResult` ganha `total: usize` (soma dos `report.total`).
- `list_messages(role: Role, limit, offset)`, `search_messages(query, role)`.
- `move_message(id: i64, role: Role) -> ()` e `delete_forever(id: i64) -> ()` — apagam a linha local só depois do OK.
- `open_message` marca como lida na pasta real da mensagem.

- [ ] **Passo 1: implementar** (substituições pontuais; `INBOX` fica só onde ainda faz sentido):

```rust
use mail_core::{imap_sync, send, ImapAccount, MessageDetail, MessageSummary, Outgoing, Role, SendReport, Store};

#[derive(Serialize, Default)]
struct SyncResult {
    fetched: usize,
    /// Mensagens que existem no servidor nas pastas sincronizadas.
    total: usize,
    errors: Vec<String>,
    skipped: bool,
}

impl AppState {
    /// Pastas da conta; descobre no servidor na primeira vez.
    fn ensure_folders(&self, store: &mut Store, acc: &ImapAccount) -> CmdResult<()> {
        if store.folders(&acc.id).map_err(err)?.is_empty() {
            let found = imap_sync::discover_folders(acc).map_err(err)?;
            store.set_folders(&acc.id, &found).map_err(err)?;
        }
        Ok(())
    }

    /// Conta, pasta e UID de uma mensagem local.
    fn locate(&self, id: i64) -> CmdResult<mail_core::ReplyRef> {
        self.store.lock().unwrap().reply_ref(id).map_err(err)?.ok_or_else(|| "mensagem não encontrada".to_string())
    }
}
```

`sync_inbox`: dentro do loop por conta,

```rust
            let r = st.account_with_password(&acc.id).and_then(|a| {
                if let Err(e) = st.ensure_folders(&mut store, &a) {
                    result.errors.push(format!("{}: pastas: {e}", acc.id));
                }
                imap_sync::sync_mailbox(&a, &mut store, "INBOX", None).map_err(err)
            });
            match r {
                Ok(r) => { result.fetched += r.fetched; result.total += r.total; }
                Err(e) => result.errors.push(format!("{}: {e}", acc.id)),
            }
```

(remover a `const INBOX` se ficar sem uso.)

Novos comandos:

```rust
#[tauri::command]
fn list_folders(state: State<'_, AppState>) -> CmdResult<Vec<Role>> {
    let store = state.store.lock().unwrap();
    let mut present = vec![Role::Inbox];
    for acc in state.accounts()? {
        for f in store.folders(&acc.id).map_err(err)? {
            if !present.contains(&f.role) { present.push(f.role); }
        }
    }
    present.sort_by_key(|r| Role::ALL.iter().position(|x| x == r));
    Ok(present)
}

#[tauri::command]
async fn sync_folder(app: AppHandle, role: Role, keep: usize) -> CmdResult<SyncResult> {
    blocking(app, move |st| {
        let Ok(_guard) = st.syncing.try_lock() else {
            return Ok(SyncResult { skipped: true, ..Default::default() });
        };
        let mut store = Store::open(st.db_path()).map_err(err)?;
        let mut result = SyncResult::default();
        for acc in st.accounts()? {
            let Some(name) = store.folder_name(&acc.id, role).map_err(err)? else { continue };
            match st
                .account_with_password(&acc.id)
                .and_then(|a| imap_sync::sync_mailbox(&a, &mut store, &name, Some(keep)).map_err(err))
            {
                Ok(r) => { result.fetched += r.fetched; result.total += r.total; }
                Err(e) => result.errors.push(format!("{}: {e}", acc.id)),
            }
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
fn list_messages(state: State<'_, AppState>, role: Role, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().list_folder(role, limit, offset).map_err(err)
}

#[tauri::command]
fn search_messages(state: State<'_, AppState>, query: String, role: Role) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().search_folder(&query, role, 200).map_err(err)
}

/// Move para a pasta do papel na mesma conta. A cópia local sai só depois do OK do servidor.
#[tauri::command]
async fn move_message(app: AppHandle, id: i64, role: Role) -> CmdResult<()> {
    blocking(app, move |st| {
        let loc = st.locate(id)?;
        let to = st
            .store.lock().unwrap()
            .folder_name(&loc.account, role).map_err(err)?
            .ok_or_else(|| "esta conta não tem essa pasta".to_string())?;
        let acc = st.account_with_password(&loc.account)?;
        imap_sync::move_message(&acc, &loc.mailbox, loc.uid, &to).map_err(err)?;
        st.store.lock().unwrap().remove(id).map_err(err)
    })
    .await
}

#[tauri::command]
async fn delete_forever(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(app, move |st| {
        let loc = st.locate(id)?;
        let acc = st.account_with_password(&loc.account)?;
        imap_sync::delete_forever(&acc, &loc.mailbox, loc.uid).map_err(err)?;
        st.store.lock().unwrap().remove(id).map_err(err)
    })
    .await
}
```

`open_message`: trocar `(account, uid)` por `let loc = store.reply_ref(id)…` dentro do bloco (antes de soltar o lock) e chamar `imap_sync::set_seen(&a, &loc.mailbox, loc.uid, true)`.

`add_imap_account`: depois de salvar a conta, `if let Ok(f) = imap_sync::discover_folders(&account) { st.store.lock().unwrap().set_folders(&account.id, &f).ok(); }` (falha não impede adicionar; o próximo sync tenta de novo).

Registrar `list_folders, sync_folder, move_message, delete_forever` no `generate_handler!`.

- [ ] **Passo 2: verificar no Docker** (a partir da raiz da worktree):

```bash
docker run --rm -v "$PWD":/src -v $HOME/.cargo/registry:/usr/local/cargo/registry -w /src -e CARGO_TARGET_DIR=/src/target/docker rust:1-bookworm sh -c "apt-get update -qq >/dev/null && apt-get install -y -qq libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev libdbus-1-dev pkg-config >/dev/null 2>&1 && mkdir -p apps/client/dist && cargo check -p posta 2>&1 | grep -E '^(error|warning)|-->|Finished' | head -30"
```

Esperado: `Finished`, sem `error`/`warning`. Leva alguns minutos (rodar em segundo plano, timeout 600 s).
- [ ] **Passo 3:** commit "Expõe pastas, mover e excluir de vez para a UI".

---

### Tarefa 5: contrato TS e mock

**Arquivos:** modificar `apps/client/src/api.ts`, `apps/client/src/mock.ts`; criar `apps/client/src/folders.ts`.

**Produz:**
- `export type Role = "inbox" | "sent" | "archive" | "trash" | "junk"`; `SyncResult.total: number`.
- `api.folders(): Promise<Role[]>`, `api.syncFolder(role, keep)`, `api.list(role, limit, offset)`, `api.search(query, role)`, `api.move(id, role)`, `api.deleteForever(id)`.
- `folders.ts`: `FOLDERS: Record<Role, { label: string; icon: Icon; empty: string; moved?: string }>`.

- [ ] **Passo 1: `api.ts`**

```ts
export type Role = "inbox" | "sent" | "archive" | "trash" | "junk";
// SyncResult: acrescentar
//   /** Mensagens no servidor nas pastas sincronizadas. */
//   total: number;
```

No objeto Tauri:

```ts
      folders: () => invoke<Role[]>("list_folders"),
      sync: () => invoke<SyncResult>("sync_inbox"),
      syncFolder: (role: Role, keep: number) => invoke<SyncResult>("sync_folder", { role, keep }),
      list: (role: Role, limit: number, offset: number) =>
        invoke<MessageSummary[]>("list_messages", { role, limit, offset }),
      search: (query: string, role: Role) => invoke<MessageSummary[]>("search_messages", { query, role }),
      move: (id: number, role: Role) => invoke<void>("move_message", { id, role }),
      deleteForever: (id: number) => invoke<void>("delete_forever", { id }),
```

- [ ] **Passo 2: `mock.ts`** — cada mensagem ganha `role: Role` interno (não exposto em `summary`). As 60 de exemplo ficam em `inbox`; acrescentar 3 em `sent` (de "Renan" para Marina/Rafael/Juliana) e 2 em `trash`. `sync` retorna `total: messages.length`. Funções:

```ts
  async folders(): Promise<Role[]> {
    return ["inbox", "sent", "archive", "trash", "junk"];
  },
  async syncFolder(role: Role, _keep: number): Promise<SyncResult> {
    await wait(500);
    return { fetched: 0, total: messages.filter((m) => m.role === role).length, errors: [], skipped: false };
  },
  async list(role: Role, limit: number, offset: number): Promise<MessageSummary[]> {
    return messages.filter((m) => m.role === role).sort((a, b) => b.date - a.date).slice(offset, offset + limit).map(summary);
  },
  async search(query: string, role: Role): Promise<MessageSummary[]> { /* igual ao atual, filtrando m.role === role */ },
  /** Para testar a falha: assunto com "Fatura" não move. */
  async move(id: number, role: Role): Promise<void> {
    await wait(400);
    const m = messages.find((x) => x.id === id);
    if (!m) throw "mensagem não encontrada";
    if (m.subject.includes("fatura")) throw "IMAP: servidor recusou o MOVE";
    m.role = role;
  },
  async deleteForever(id: number): Promise<void> {
    await wait(400);
    messages = messages.filter((x) => x.id !== id);
  },
```

`summary` passa a descartar também `role`. Tipo interno: `type MockMessage = MessageDetail & { role: Role }`.

- [ ] **Passo 3: `folders.ts`**

```ts
// Rótulo, ícone e textos de cada pasta padrão.
import { Archive, PaperPlaneTilt, Trash, Tray, Warning, type Icon } from "@phosphor-icons/react";
import type { Role } from "./api";

export const FOLDERS: Record<Role, { label: string; icon: Icon; empty: string }> = {
  inbox: { label: "Caixa de entrada", icon: Tray, empty: "Caixa de entrada vazia" },
  sent: { label: "Enviados", icon: PaperPlaneTilt, empty: "Nada enviado ainda" },
  archive: { label: "Arquivo", icon: Archive, empty: "Nada arquivado" },
  trash: { label: "Lixeira", icon: Trash, empty: "Nada na Lixeira" },
  junk: { label: "Spam", icon: Warning, empty: "Nenhum spam" },
};
```

- [ ] **Passo 4:** `pnpm -C apps/client exec tsc --noEmit` — erros esperados só em `App.tsx` (chamadas antigas), corrigidos na Tarefa 6.
- [ ] **Passo 5:** commit junto com a Tarefa 6 (o app não compila sem ela).

---

### Tarefa 6: UI — pastas, arquivar, excluir, desfazer

**Arquivos:** modificar `apps/client/src/App.tsx`, `apps/client/src/styles.css`.

**Consome:** `api.*` e `FOLDERS` (Tarefa 5), `toast(text, kind, { action, duration, onClose })` de `toast.tsx` (`onClose(reason)`: `"action" | "timeout" | "click"`).

Comportamento exigido:

1. Estado `folder: Role` (inicial `"inbox"`), `folderList: Role[]` (de `api.folders()`, recarregado após cada `sync` e ao adicionar conta), `keep: number` (janela atual da pasta, `1000`), `serverTotal: number | null`.
2. `reload` e `loadMore` usam `api.list(folder, …)`; a busca usa `api.search(query, folder)` e é refeita ao trocar de pasta. `unread` (contador do menu) só para a Caixa de entrada: guardar `inboxUnread` calculado quando `folder === "inbox"`.
3. Trocar de pasta: limpa seleção/detalhe/busca, `setIntro(false)`, `reload()` com o banco local; se `folder !== "inbox"`, chama `api.syncFolder(folder, 1000)` (mostrando o ícone de sincronizar ocupado), depois `reload()` e guarda `serverTotal = r.total`. Skeleton atual não existe: enquanto a lista local está vazia e sincroniza, mostrar `<div className="empty quiet"><p>Carregando…</p></div>`; vazia após sync → `FOLDERS[folder].empty`.
4. `loadMore` fora da Caixa de entrada: quando acabou a lista local (`!hasMore`) e `serverTotal > keep`, `keep += 500`, `syncFolder(folder, keep)`, `reload(messages.length + PAGE)`. Uma chamada por vez (ref `loadingMore`).
5. Botão de sincronizar e `u`: na Caixa de entrada chama `sync(true)`; nas outras, `syncFolder(folder, keep)`.
6. Ações na mensagem aberta (`.actions`, ao lado de Responder):
   - Arquivar (`Archive`, `<kbd>e</kbd>`): visível se `folder !== "archive"` e `folderList` inclui `"archive"`.
   - Excluir (`Trash`, `<kbd>#</kbd>`): se `folder === "trash"` → rótulo "Excluir de vez" e abre confirmação; senão move para `"trash"`; desabilitado com `title="Esta conta não tem Lixeira"` se `folderList` não inclui `"trash"`.
   - Na Lixeira e no Spam, Arquivar vira "Mover para a Caixa de entrada" (`Tray`, move para `"inbox"`), mesmo atalho `e`.
7. Atalhos no `onKey` existente (mesma guarda de `typing`/diálogos): `e` → arquivar/mover para entrada; `#` → excluir (na Lixeira abre a confirmação).
8. Fluxo de mover (abordagem A):

```tsx
  /** Tira da lista na hora; o servidor só recebe quando o "Desfazer" expira. */
  function moveOut(m: MessageSummary, to: Role) {
    const idx = visible.findIndex((x) => x.id === m.id);
    const next = visible[idx + 1] ?? visible[idx - 1];
    const drop = (l: MessageSummary[]) => l.filter((x) => x.id !== m.id);
    setMessages(drop);
    setResults((r) => r && drop(r));
    if (next) select(next.id, true); else { setSelectedId(null); setDetail(null); setReaderOpen(false); }
    const restore = () => {
      const put = (l: MessageSummary[]) => (l.some((x) => x.id === m.id) ? l : [...l.slice(0, idx), m, ...l.slice(idx)]);
      setMessages(put);
      setResults((r) => r && put(r));
      select(m.id, true);
    };
    const text = to === "trash" ? "Movida para a Lixeira" : to === "inbox" ? "Movida para a Caixa de entrada" : "Arquivada";
    toast(text, "info", {
      action: { label: "Desfazer", onClick: restore },
      duration: 4000,
      onClose: async (reason) => {
        if (reason === "action") return;
        try {
          await api.move(m.id, to);
        } catch (e) {
          restore();
          toast(`Não foi possível mover: ${e}`, "error");
        }
      },
    });
  }
```

   `restore` usa `idx` da lista no momento da ação; se a lista mudou (reload), `put` reinsere na posição limitada pelo tamanho atual — aceitável. Ao final (`onClose` com sucesso) não recarregar a lista.
9. Excluir de vez: estado `confirmDelete: boolean`. Renderizar dentro de `<article className="message">` o mesmo padrão de `Composer` (`.confirm` com `role="alertdialog"`): título "Excluir de vez?", texto curto "Não dá para desfazer.", botões "Cancelar" (autoFocus) e "Excluir" (`btn danger`). Esc fecha a confirmação; clique fora dela (no overlay `.confirm`) também. Durante a chamada (`deleting`), botões desabilitados e Esc ignorado. Sucesso → tira da lista (como `drop` acima, seleciona a próxima) e `toast("Excluída de vez", "success")`; falha → `toast(\`Não foi possível excluir: ${e}\`, "error")` e a mensagem fica. Bloquear os atalhos globais enquanto `confirmDelete`.
10. Menu lateral: trocar o `<nav>` fixo por `folderList.map` com `FOLDERS[r].icon` e `label`, `aria-current={r === folder ? "page" : undefined}`, contador só na Caixa de entrada. O botão "voltar" do leitor mostra `FOLDERS[folder].label`.
11. Celular (≤720 px, onde o `.rail` some): faixa `.folder-tabs.mobile-only` logo abaixo da `.toolbar`, com um botão por pasta (ícone + rótulo, rolagem horizontal, sem barra visível), mesmo `aria-current`.
12. Lista de atalhos do menu (`.shortcuts`): acrescentar `<span><kbd>e</kbd> arquivar</span>` e `<span><kbd>#</kbd> excluir</span>`.

CSS (`styles.css`, seção nova "Pastas e ações"): reaproveitar `.nav-item` para o menu; `.folder-tabs` (`display:flex; gap:4px; overflow-x:auto; scrollbar-width:none; padding: 0 16px 8px`), botões com o mesmo visual de `.btn.ghost.small` e estado `[aria-current=page]` igual ao `.nav-item[aria-current]`; `.message .confirm` reaproveita `.confirm` (verificar posicionamento: `.message` precisa de `position: relative` se `.confirm` usa `position:absolute`). Botão `danger` já existe em `Composer`.

- [ ] **Passo 1:** implementar 1–12.
- [ ] **Passo 2:** `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build` → sem erros.
- [ ] **Passo 3: testar no navegador (modo mock)**, vite em `apps/client` com `npx vite --port 36480 --strictPort` (antes: `ss -ltnp | grep 36480`; se ocupada, conferir `readlink /proc/PID/cwd` e encerrar só se for desta worktree): trocar para Enviados/Lixeira; arquivar com `e`, desfazer, arquivar e esperar 4 s, abrir Arquivo e achar a mensagem; `#` na Caixa de entrada → Lixeira; na Lixeira `#` → confirmação → Excluir; mensagem com "fatura" no assunto → volta com erro. Sem erros no console.
- [ ] **Passo 4:** commit (Tarefas 5 e 6) "Adiciona pastas, arquivar e excluir com desfazer na interface".

---

### Tarefa 7: verificação e PR

- [ ] Prints desktop (1280×800) e celular (390×844): Lixeira aberta com mensagem e botão "Excluir de vez"; aviso "Arquivada · Desfazer"; confirmação "Excluir de vez?". Copiar para `/home/renan/.t3/userdata/attachments/` e embutir no chat.
- [ ] Skill `ux-audit` modo Rápido no fluxo arquivar/desfazer/excluir.
- [ ] Skill `qualidade-antes-do-pr`; depois `verificacao-manter`.
- [ ] Atualizar `README.md` (atalhos `e` e `#`, pastas) se houver lista de atalhos lá.
- [ ] `git push -u origin feat/pastas`, PR para `master`, `link_pull_request`.
