//! TEMPORÁRIO: benchmark de auditoria (não versionar).
use imap::{ClientBuilder, ConnectionMode};
use mail_core::{imap_sync, ImapAccount, Role, Security, Store};
use std::io::{Read, Write};
use std::time::Instant;

fn acc(user: &str, port: u16) -> ImapAccount {
    ImapAccount { id: user.into(), host: "127.0.0.1".into(), port, security: Security::Plaintext, username: user.into(), name: None, smtp: None, auth: mail_core::AuthMode::Password, password: user.into() }
}
fn sess(a: &ImapAccount) -> imap::Session<imap::Connection> {
    ClientBuilder::new(a.host.as_str(), a.port).mode(ConnectionMode::Plaintext).connect().unwrap().login(&a.username, &a.password).map_err(|(e, _)| e).unwrap()
}
fn stats() -> String {
    let mut s = std::net::TcpStream::connect("127.0.0.1:3145").unwrap();
    s.write_all(b"x").unwrap();
    let mut o = String::new();
    s.read_to_string(&mut o).unwrap();
    o.trim().to_string()
}
const WORDS: [&str; 24] = ["translado","aeroporto","Guarulhos","cotação","reunião","contrato","motorista","executivo","reserva","fatura","pagamento","cliente","roteiro","hotel","voo","bagagem","confirmação","orçamento","viagem","veículo","pontualidade","discrição","evento","diretoria"];
fn body(i: usize, n: usize) -> String {
    (0..n).map(|k| WORDS[(i * 7 + k * 13 + k / 5) % 24]).collect::<Vec<_>>().join(" ")
}
fn raw(i: usize) -> Vec<u8> {
    let refs = if i % 5 == 4 { format!("References: <m{}@x>\r\nIn-Reply-To: <m{}@x>\r\n", i - 1, i - 1) } else { String::new() };
    let subj = if i % 5 == 4 { format!("Re: Assunto {} {}", (i - 1) / 5, WORDS[((i - 1) / 5) % 24]) } else { format!("Assunto {} {}", i / 5, WORDS[(i / 5) % 24]) };
    let day = 1 + (i * 7) % 28;
    let mon = 1 + (i / 1500) % 12;
    let (ct, b) = if i % 10 == 0 {
        ("text/html", format!("<html><body>{}</body></html>", (0..60).map(|k| format!("<p style=\"margin:0\"><b>{}</b> {}</p>", k, body(i + k, 12))).collect::<String>()))
    } else {
        ("text/plain", body(i, 150))
    };
    format!(
        "From: \"Pessoa {}\" <p{}@ex{}.com>\r\nTo: eu@exemplo.com\r\nCc: c{}@ex.com\r\nMessage-ID: <m{}@x>\r\n{}Subject: {}\r\nDate: {} {} 2026 10:{:02}:00 -0300\r\nContent-Type: {}; charset=utf-8\r\n\r\n{}\r\n",
        i % 300, i % 300, i % 30, i % 50, i, refs, subj, day,
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][mon - 1],
        i % 60, ct, b
    )
    .into_bytes()
}
fn populate(user: &str, from: usize, to: usize) {
    if std::env::var("PERF_FAKE").is_ok() {
        let mut s = std::net::TcpStream::connect("127.0.0.1:3145").unwrap();
        s.write_all(b"a").unwrap();
        let mut o = String::new();
        s.read_to_string(&mut o).unwrap();
        return;
    }
    let a = acc(user, 3143);
    let threads = 6;
    let per = (to - from + threads - 1) / threads;
    let hs: Vec<_> = (0..threads)
        .map(|t| {
            let a = a.clone();
            std::thread::spawn(move || {
                let mut s = sess(&a);
                let (lo, hi) = (from + t * per, (from + (t + 1) * per).min(to));
                for i in lo..hi {
                    let m = raw(i);
                    if i % 10 < 3 {
                        s.append("INBOX", &m).flag(imap::types::Flag::Seen).finish().unwrap();
                    } else {
                        s.append("INBOX", &m).finish().unwrap();
                    }
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
}
fn t<R>(label: &str, f: impl FnOnce() -> R) -> R {
    let s = Instant::now();
    let r = f();
    println!("PERF {label}: {:.1} ms", s.elapsed().as_secs_f64() * 1000.0);
    r
}
fn q(conn: &rusqlite::Connection, label: &str, sql: &str) {
    println!("PLAN {label}");
    let mut st = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let rows = st.query_map([], |r| r.get::<_, String>(3)).unwrap();
    for r in rows {
        println!("PLAN   {}", r.unwrap());
    }
}

#[test]
fn bench() {
    let Ok(n) = std::env::var("PERF_N") else { return };
    let n: usize = n.parse().unwrap();
    let user = format!("perf{n}@localhost");
    let dir = std::env::var("PERF_DIR").unwrap();
    let db = format!("{dir}/perf{n}.sqlite");
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(format!("{db}-wal"));
    let _ = std::fs::remove_file(format!("{db}-shm"));
    if std::env::var("PERF_SKIP_POP").is_err() && std::env::var("PERF_FAKE").is_err() {
        t(&format!("populate {n} (APPEND, 6 conexoes)"), || populate(&user, 0, n));
    }
    let a = acc(&user, 3144);
    let _ = stats();
    let mut store = Store::open(&db).unwrap();
    let r = t("sync inicial (INBOX, keep=None)", || imap_sync::sync_mailbox(&a, &mut store, "INBOX", None).unwrap());
    println!("PERF sync inicial: fetched={} total={} | IMAP: {}", r.fetched, r.total, stats());
    let r = t("sync incremental sem novidade", || imap_sync::sync_mailbox(&a, &mut store, "INBOX", None).unwrap());
    println!("PERF incremental 0 novas: fetched={} | IMAP: {}", r.fetched, stats());
    populate(&user, n, n + 10);
    let _ = stats();
    let r = t("sync incremental com 10 novas", || imap_sync::sync_mailbox(&a, &mut store, "INBOX", None).unwrap());
    println!("PERF incremental 10 novas: fetched={} | IMAP: {}", r.fetched, stats());
    t("connect+login+logout (check_login)", || imap_sync::check_login(&a).unwrap());
    let _ = stats();
    let r = t("sync com keep=200 (sync_folder)", || imap_sync::sync_mailbox(&a, &mut store, "INBOX", Some(200)).unwrap());
    println!("PERF keep200: fetched={}", r.fetched);
    drop(store);
    println!("PERF db size: {} MB", std::fs::metadata(&db).unwrap().len() / 1_000_000);
    queries(&db, n);
}

fn queries(db: &str, n: usize) {
    let db = db.to_string();
    let store = Store::open(&db).unwrap();
    for i in 0..2 {
        t(&format!("list_folder(Inbox,50,0) #{i}"), || store.list_folder(Role::Inbox, 50, 0).unwrap());
    }
    t("list_folder(Inbox,50,offset 10000)", || store.list_folder(Role::Inbox, 50, 10000.min(n as u32 - 50)).unwrap());
    t("list_folder(Inbox,10000,0)", || store.list_folder(Role::Inbox, 10000, 0).unwrap().len());
    t("list(INBOX,50,0) [sem ORDER_TOP]", || store.list("INBOX", 50, 0).unwrap());
    t("unread_counts", || store.unread_counts().unwrap());
    t("search 'translado'", || store.search("translado", 200).unwrap().len());
    t("search_folder 'translado aeroporto'", || store.search_folder("translado aeroporto", Role::Inbox, 200).unwrap().len());
    t("search_folder 'Assunto 4000' (raro)", || store.search_folder("Assunto 4000", Role::Inbox, 200).unwrap().len());
    let id = store.list_folder(Role::Inbox, 1, 0).unwrap()[0].id;
    t("thread(id)", || store.thread(id).unwrap().len());
    t("get(id) corpo", || store.get(id).unwrap().unwrap().body_text.len());
    t("suggest_contacts('')", || store.suggest_contacts("", 8).unwrap().len());
    t("suggest_contacts('p12')", || store.suggest_contacts("p12", 8).unwrap().len());
    t("suggest_contacts('zzz') [sem match]", || store.suggest_contacts("zzz", 8).unwrap().len());
    t("count_folder", || store.count_folder(Role::Inbox).unwrap());
    t("list_starred", || store.list_starred(50, 0).unwrap().len());
    let conn = rusqlite::Connection::open(&db).unwrap();
    for p in ["journal_mode", "synchronous", "mmap_size", "cache_size", "temp_store", "page_size"] {
        let v = conn.query_row(&format!("PRAGMA {p}"), [], |r| r.get::<_, rusqlite::types::Value>(0).map(|v| format!("{v:?}"))).unwrap();
        println!("PRAGMA {p} = {v} (conexão nova, defaults)");
    }
    let in_role = "m.mailbox = COALESCE((SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = 'inbox'), CASE 'inbox' WHEN 'inbox' THEN 'INBOX' END)";
    let key = "(CASE WHEN m.message_id <> '' THEN m.message_id ELSE m.mailbox || '/' || m.uid END)";
    let ns = "1";
    let ord = "ORDER BY m.date DESC, m.id DESC";
    q(&conn, "list_folder", &format!("SELECT m.id FROM messages m WHERE {in_role} AND {ns} {ord} LIMIT 50 OFFSET 0"));
    q(&conn, "unread_counts", &format!("SELECT COUNT(*) FROM messages m WHERE m.seen = 0 AND {in_role} AND {ns}"));
    q(&conn, "list(INBOX) simples", "SELECT m.id FROM messages m WHERE m.mailbox = 'INBOX' ORDER BY m.date DESC, m.id DESC LIMIT 50");
    q(&conn, "search_folder", &format!("SELECT m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid WHERE messages_fts MATCH '\"translado\"*' AND {in_role} ORDER BY m.date DESC LIMIT 200"));
    q(&conn, "suggest_contacts", "SELECT from_name, from_addr, to_addrs, cc_addrs FROM messages WHERE from_addr LIKE '%p1%' OR from_name LIKE '%p1%' OR to_addrs LIKE '%p1%' OR cc_addrs LIKE '%p1%' ORDER BY date DESC LIMIT 5000");
    q(&conn, "thread", "SELECT m.id FROM messages m, messages o WHERE o.id = 1 AND m.account = o.account AND (m.id = o.id OR (o.thread_id <> 0 AND m.thread_id = o.thread_id)) ORDER BY m.date ASC, m.id ASC");
    q(&conn, "thread_count subquery", "SELECT (SELECT COUNT(*) FROM messages t WHERE t.account = m.account AND t.thread_id = m.thread_id) FROM messages m WHERE m.id = 1");
}

#[test]
fn bench_local() {
    let Ok(n) = std::env::var("PERF_LOCAL") else { return };
    let n: usize = n.parse().unwrap();
    let dir = std::env::var("PERF_DIR").unwrap();
    let db = format!("{dir}/local{n}.sqlite");
    for ext in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{db}{ext}")); }
    // custo de parse (mail-parser) por mensagem
    let raws: Vec<Vec<u8>> = (0..1000).map(raw).collect();
    let tp = Instant::now();
    for r in &raws { let m = mail_parser::MessageParser::default().parse(r).unwrap(); let _ = m.body_text(0); let _ = m.html_body().count(); }
    println!("PERF parse mail-parser: {:.3} ms/msg (1000 msgs)", tp.elapsed().as_secs_f64());
    let mut store = Store::open(&db).unwrap();
    let mk = |i: usize| {
        let r = raw(i);
        let m = mail_parser::MessageParser::default().parse(&r).unwrap();
        mail_core::store::NewMessage {
            uid: i as u32 + 1, from_name: format!("Pessoa {}", i % 300), from_addr: format!("p{}@ex{}.com", i % 300, i % 30),
            subject: m.subject().unwrap_or_default().to_string(), date: 1_780_000_000 + (i as i64) * 60,
            body_text: m.body_text(0).map(|b| b.into_owned()).unwrap_or_default(),
            body_html: m.html_body().filter_map(|p| p.text()).collect::<Vec<_>>().join("\n"),
            attachments: vec![], seen: i % 10 < 3, flagged: false, message_id: format!("m{i}@x"),
            refs: if i % 5 == 4 { format!("m{}@x", i - 1) } else { String::new() },
            reply_addr: format!("p{}@ex{}.com", i % 300, i % 30), to_addrs: "eu@exemplo.com".into(), cc_addrs: format!("c{}@ex.com", i % 50),
        }
    };
    let t0 = Instant::now();
    let mut ins = 0.0; let mut off = 0usize;
    while off < n {
        let hi = (off + 50).min(n);
        let batch: Vec<_> = (off..hi).map(&mk).collect();
        let t = Instant::now();
        store.insert_messages("a", "INBOX", &batch).unwrap();
        ins += t.elapsed().as_secs_f64();
        if off == 0 || hi == n || off % (n / 4 / 50 * 50).max(50) == 0 { println!("PERF   lote {}..{}: insert_messages {:.1} ms", off, hi, t.elapsed().as_secs_f64() * 1000.0); }
        off = hi;
    }
    println!("PERF local {n}: insert_messages total (lotes de 50, inclui rebuild_threads por lote) = {:.0} ms; total com parse = {:.0} ms", ins * 1000.0, t0.elapsed().as_secs_f64() * 1000.0);
    let server: Vec<(u32, bool, bool)> = (0..n).map(|i| (i as u32 + 1, i % 10 < 3, false)).collect();
    t("apply_server_state (todas as UIDs)", || store.apply_server_state("a", "INBOX", &server).unwrap());
    t("local_uids", || store.local_uids("a", "INBOX").unwrap().len());
    drop(store);
    println!("PERF db size: {} MB", std::fs::metadata(&db).unwrap().len() / 1_000_000);
    queries(&db, n);
}
