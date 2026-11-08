//! Testes contra um servidor IMAP real (Greenmail em Docker).
//! Rodar: `pnpm test:imap` (sobe o container e define POSTA_IMAP_TEST=127.0.0.1:3143).
//! Sem a variável, os testes passam sem fazer nada.

use imap::{ClientBuilder, ConnectionMode};
use mail_core::{imap_sync, ImapAccount, Security, Store};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn account(tag: &str) -> Option<ImapAccount> {
    let addr = std::env::var("POSTA_IMAP_TEST").ok()?;
    let (host, port) = addr.split_once(':')?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let user = format!("{tag}{nanos}");
    Some(ImapAccount {
        id: user.clone(),
        host: host.into(),
        port: port.parse().ok()?,
        security: Security::Plaintext,
        username: user.clone(),
        password: user,
    })
}

fn session(acc: &ImapAccount) -> imap::Session<imap::Connection> {
    ClientBuilder::new(acc.host.as_str(), acc.port)
        .mode(ConnectionMode::Plaintext)
        .connect()
        .unwrap()
        .login(&acc.username, &acc.password)
        .map_err(|(e, _)| e)
        .unwrap()
}

fn raw(i: usize, subject: &str) -> Vec<u8> {
    format!(
        "From: \"Ana Souza\" <ana@exemplo.com>\r\nTo: eu@exemplo.com\r\n\
         Subject: {subject}\r\nDate: Sat, 7 Nov 2026 10:{:02}:00 -0300\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\nCorpo da mensagem {i}, translado Guarulhos.\r\n",
        i % 60
    )
    .into_bytes()
}

#[test]
fn sincroniza_incremental_flags_e_exclusao() {
    let Some(acc) = account("inc") else { return };
    let mut s = session(&acc);
    s.append("INBOX", &raw(1, "=?UTF-8?Q?Cota=C3=A7=C3=A3o?= aeroporto")).finish().unwrap();
    s.append("INBOX", &raw(2, "Reunião")).flag(imap::types::Flag::Seen).finish().unwrap();
    s.append("INBOX", &raw(3, "Terceira")).finish().unwrap();

    let mut store = Store::open_in_memory().unwrap();
    let r = imap_sync::sync_mailbox(&acc, &mut store, "INBOX").unwrap();
    assert_eq!((r.fetched, r.total), (3, 3));
    let list = store.list("INBOX", 50, 0).unwrap();
    assert_eq!(list.len(), 3);
    let cot = &store.search("cotacao", 10).unwrap()[0];
    assert_eq!(cot.subject, "Cotação aeroporto");
    assert_eq!(cot.from_name, "Ana Souza");
    assert!(!cot.seen);
    assert!(list.iter().find(|m| m.subject == "Reunião").unwrap().seen);

    // Segunda sincronização não baixa nada de novo.
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX").unwrap().fetched, 0);

    // Marcar lida no servidor e excluir outra lá; o local acompanha.
    imap_sync::set_seen(&acc, "INBOX", cot.uid, true).unwrap();
    let terceira = list.iter().find(|m| m.subject == "Terceira").unwrap().uid;
    s.select("INBOX").unwrap();
    s.uid_store(terceira.to_string(), "+FLAGS (\\Deleted)").unwrap();
    s.expunge().unwrap();
    s.logout().ok();

    imap_sync::sync_mailbox(&acc, &mut store, "INBOX").unwrap();
    let list = store.list("INBOX", 50, 0).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|m| m.seen));
}

#[test]
fn mil_mensagens_sincronizam_e_buscam_rapido() {
    let Some(acc) = account("mil") else { return };
    let mut s = session(&acc);
    for i in 0..1000 {
        s.append("INBOX", &raw(i, &format!("Mensagem {i}"))).finish().unwrap();
    }
    s.logout().ok();

    let dir = std::env::temp_dir().join(format!("{}.sqlite", acc.id));
    let mut store = Store::open(&dir).unwrap();
    let t = Instant::now();
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX").unwrap().fetched, 1000);
    let sync = t.elapsed();

    let t = Instant::now();
    let page = store.list("INBOX", 100, 0).unwrap();
    let list = t.elapsed();
    let t = Instant::now();
    let hits = store.search("guarul", 50).unwrap();
    let search = t.elapsed();
    eprintln!("sync 1000: {sync:?} | lista 100: {list:?} | busca: {search:?}");

    assert_eq!(page.len(), 100);
    assert_eq!(hits.len(), 50);
    assert!(list.as_millis() < 50 && search.as_millis() < 50);
    drop(store);
    let _ = std::fs::remove_file(&dir);
}
