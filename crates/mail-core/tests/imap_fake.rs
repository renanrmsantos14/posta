//! Testes da sync contra um servidor IMAP falso (em processo), para exercitar o que o Greenmail não anuncia:
//! CONDSTORE, QRESYNC e a degradação para UIDNEXT, além de queda de conexão e reconexão do `Hub`.
//! Rodam sempre, sem Docker.

use mail_core::{imap_sync, Hub, ImapAccount, Kind, Live, Security, Store};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Msg {
    uid: u32,
    seen: bool,
    modseq: u64,
    raw: Vec<u8>,
}

struct Server {
    caps: String,
    validity: u32,
    modseq: u64,
    next_uid: u32,
    msgs: Vec<Msg>,
    /// Comandos recebidos (já sem a tag), para conferir o que o cliente pediu.
    log: Vec<String>,
    /// Derruba a conexão, sem responder, quando chegar um comando que comece com isto (uma vez).
    drop_on: Option<String>,
    /// Imita o Greenmail: sem espaço entre `BODY[]<0>` e o literal.
    no_space: bool,
    conns: usize,
}

type Shared = Arc<Mutex<Server>>;

fn mail(n: u32, extra: usize) -> Vec<u8> {
    let mut s = format!(
        "From: Ana <ana@exemplo.com>\r\nTo: eu@exemplo.com\r\nSubject: Mensagem {n}\r\n\
         Message-ID: <m{n}@exemplo.com>\r\nDate: Sat, 7 Nov 2026 10:00:00 -0300\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\nCorpo {n}.\r\n"
    );
    s.push_str(&"linha de enchimento do corpo\r\n".repeat(extra));
    s.into_bytes()
}

impl Server {
    fn add(&mut self, extra: usize) -> u32 {
        let uid = self.next_uid;
        self.next_uid += 1;
        self.modseq += 1;
        self.msgs.push(Msg { uid, seen: false, modseq: self.modseq, raw: mail(uid, extra) });
        uid
    }
    fn set_seen(&mut self, uid: u32) {
        self.modseq += 1;
        let m = self.modseq;
        let msg = self.msgs.iter_mut().find(|x| x.uid == uid).unwrap();
        msg.seen = true;
        msg.modseq = m;
    }
    fn remove(&mut self, uid: u32) {
        self.modseq += 1;
        self.msgs.retain(|x| x.uid != uid);
    }
    fn cond(&self) -> bool {
        self.caps.contains("CONDSTORE") || self.caps.contains("QRESYNC")
    }
}

fn start(caps: &str) -> (ImapAccount, Shared, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let srv: Shared = Arc::new(Mutex::new(Server {
        caps: caps.into(),
        validity: 7,
        modseq: 10,
        next_uid: 1,
        msgs: Vec::new(),
        log: Vec::new(),
        drop_on: None,
        no_space: false,
        conns: 0,
    }));
    let count = Arc::new(AtomicUsize::new(0));
    let (s2, c2) = (srv.clone(), count.clone());
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            c2.fetch_add(1, Ordering::SeqCst);
            s2.lock().unwrap().conns += 1;
            let s3 = s2.clone();
            std::thread::spawn(move || serve(conn, s3));
        }
    });
    let acc = ImapAccount {
        id: "fake".into(),
        host: "127.0.0.1".into(),
        port,
        security: Security::Plaintext,
        username: "eu@exemplo.com".into(),
        name: None,
        smtp: None,
        auth: mail_core::AuthMode::Password,
        password: "x".into(),
    };
    (acc, srv, count)
}

/// Expande um conjunto de UIDs ("1:*", "3,4", "5") sobre as mensagens do servidor.
fn select(set: &str, msgs: &[Msg]) -> Vec<Msg> {
    let max = msgs.iter().map(|m| m.uid).max().unwrap_or(0);
    msgs.iter()
        .filter(|m| {
            set.split(',').any(|p| match p.split_once(':') {
                Some((a, b)) => {
                    let lo: u32 = a.parse().unwrap();
                    let hi: u32 = if b == "*" { u32::MAX } else { b.parse().unwrap() };
                    // `lo:*` sempre inclui o maior UID, mesmo que lo seja maior que ele.
                    (m.uid >= lo && m.uid <= hi) || (b == "*" && m.uid == max)
                }
                None => p.parse::<u32>().unwrap() == m.uid,
            })
        })
        .cloned()
        .collect()
}

fn serve(conn: TcpStream, srv: Shared) {
    let mut out = conn.try_clone().unwrap();
    let mut input = BufReader::new(conn.try_clone().unwrap());
    out.write_all(b"* OK fake ready\r\n").unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end().to_string();
        let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
        let up = cmd.to_uppercase();
        let mut s = srv.lock().unwrap();
        s.log.push(cmd.to_string());
        if s.drop_on.as_deref().is_some_and(|p| up.starts_with(p)) {
            s.drop_on = None;
            conn.shutdown(Shutdown::Both).ok();
            return;
        }
        let mut resp = String::new();
        let mut bin: Vec<u8> = Vec::new();
        if up == "CAPABILITY" {
            resp += &format!("* CAPABILITY IMAP4rev1 {}\r\n", s.caps);
        } else if up.starts_with("ENABLE") {
            resp += &format!("* ENABLED {}\r\n", &cmd[7..]);
        } else if up.starts_with("SELECT") {
            resp += &format!("* FLAGS (\\Seen \\Flagged)\r\n* {} EXISTS\r\n* 0 RECENT\r\n", s.msgs.len());
            resp += &format!("* OK [UIDVALIDITY {}] ok\r\n* OK [UIDNEXT {}] ok\r\n", s.validity, s.next_uid);
            if s.cond() {
                resp += &format!("* OK [HIGHESTMODSEQ {}] ok\r\n", s.modseq);
            }
        } else if up.starts_with("UID FETCH") {
            let rest = &cmd["UID FETCH ".len()..];
            let (set, query) = rest.split_once(' ').unwrap();
            let changed_since: Option<u64> = query.split("CHANGEDSINCE ").nth(1).and_then(|x| x.split([' ', ')']).next()?.parse().ok());
            let mut list = select(set, &s.msgs);
            if let Some(m) = changed_since {
                list.retain(|x| x.modseq > m);
                if query.contains("VANISHED") {
                    // O servidor falso não guarda histórico: sempre avisa o UID 2 como excluído se faltar.
                    let have: Vec<u32> = s.msgs.iter().map(|x| x.uid).collect();
                    let gone: Vec<String> = (1..s.next_uid).filter(|u| !have.contains(u)).map(|u| u.to_string()).collect();
                    if !gone.is_empty() {
                        resp += &format!("* VANISHED (EARLIER) {}\r\n", gone.join(","));
                    }
                }
            }
            for (i, m) in list.iter().enumerate() {
                let flags = if m.seen { "\\Seen" } else { "" };
                let mut item = format!("UID {} FLAGS ({flags})", m.uid);
                if s.cond() {
                    item += &format!(" MODSEQ ({})", m.modseq);
                }
                if query.contains("BODYSTRUCTURE") {
                    item += &format!(
                        " INTERNALDATE \"07-Nov-2026 10:00:00 -0300\" BODYSTRUCTURE (\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"7BIT\" {} 3)",
                        m.raw.len()
                    );
                }
                let body: Option<(String, Vec<u8>)> = if query.contains("BODY.PEEK[]<0.8192>") {
                    Some(("BODY[]<0>".into(), m.raw[..m.raw.len().min(8192)].to_vec()))
                } else if query.contains("BODY.PEEK[]") {
                    Some(("BODY[]".into(), m.raw.clone()))
                } else {
                    None
                };
                match body {
                    Some((name, data)) => {
                        let sp = if s.no_space && name.contains('<') { "" } else { " " };
                        bin.extend(format!("* {} FETCH ({item} {name}{sp}{{{}}}\r\n", i + 1, data.len()).as_bytes());
                        bin.extend(&data);
                        bin.extend(b")\r\n");
                    }
                    None => bin.extend(format!("* {} FETCH ({item})\r\n", i + 1).as_bytes()),
                }
            }
        } else if up == "LOGOUT" {
            out.write_all(format!("* BYE bye\r\n{tag} OK logout\r\n").as_bytes()).ok();
            return;
        }
        let tail = if up.starts_with("SELECT") { "OK [READ-WRITE] selected" } else { "OK done" };
        out.write_all(resp.as_bytes()).unwrap();
        out.write_all(&bin).unwrap();
        out.write_all(format!("{tag} {tail}\r\n").as_bytes()).unwrap();
    }
}

fn cmds(srv: &Shared) -> Vec<String> {
    srv.lock().unwrap().log.clone()
}

#[test]
fn qresync_traz_so_o_que_mudou_e_as_exclusoes() {
    let (acc, srv, _) = start("ENABLE IDLE CONDSTORE QRESYNC");
    {
        let mut s = srv.lock().unwrap();
        for _ in 0..3 {
            s.add(0);
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let mut live = Live::open(&acc).unwrap();
    assert!(live.condstore && live.qresync);
    let r = imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert_eq!((r.mode, r.fetched, r.total), ("full", 3, 3));
    let st = store.sync_state(&acc.id, "INBOX").unwrap().unwrap();
    assert_eq!((st.uid_validity, st.highest_modseq, st.uid_next, st.complete), (7, 13, 4, true));

    {
        let mut s = srv.lock().unwrap();
        s.add(0);
        s.set_seen(1);
        s.remove(2);
    }
    srv.lock().unwrap().log.clear();
    let r = imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert_eq!(r.mode, "qresync");
    assert_eq!((r.delta.inserted.len(), r.delta.changed.len(), r.delta.removed.len()), (1, 1, 1), "{:?}", r.delta);
    let log = cmds(&srv);
    assert!(log.iter().any(|c| c.contains("CHANGEDSINCE 13 VANISHED")), "{log:?}");
    // Não varreu a pasta toda de novo.
    assert!(!log.iter().any(|c| c == "UID FETCH 1:* (UID FLAGS)"), "{log:?}");
    let mut uids = store.local_uids(&acc.id, "INBOX").unwrap();
    uids.sort_unstable();
    assert_eq!(uids, vec![1, 3, 4]);
    assert!(store.list("INBOX", 10, 0).unwrap().iter().find(|m| m.uid == 1).unwrap().seen);
    assert_eq!(store.sync_state(&acc.id, "INBOX").unwrap().unwrap().highest_modseq, 16);

    // Nada novo: nenhuma mensagem baixada e o estado não regride.
    let r = imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert!(r.delta.is_empty() && r.fetched == 0, "{:?}", r.delta);
    live.logout();
}

#[test]
fn condstore_sem_qresync_acha_flags_e_confere_exclusoes_na_janela() {
    let (acc, srv, _) = start("ENABLE IDLE CONDSTORE");
    {
        let mut s = srv.lock().unwrap();
        for _ in 0..3 {
            s.add(0);
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let mut live = Live::open(&acc).unwrap();
    assert!(live.condstore && !live.qresync);
    imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    {
        let mut s = srv.lock().unwrap();
        s.set_seen(3);
        s.remove(2);
        s.add(0);
    }
    let r = imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert_eq!(r.mode, "condstore");
    assert_eq!((r.delta.inserted.len(), r.delta.changed.len(), r.delta.removed.len()), (1, 1, 1), "{:?}", r.delta);
    let log = cmds(&srv);
    assert!(log.iter().any(|c| c.contains("ENABLE CONDSTORE")), "{log:?}");
    assert!(!log.iter().any(|c| c.contains("ENABLE QRESYNC")), "{log:?}");
    live.logout();
}

#[test]
fn sem_extensoes_usa_uidnext_e_a_janela_recente() {
    let (acc, srv, _) = start("IDLE");
    {
        let mut s = srv.lock().unwrap();
        for _ in 0..3 {
            s.add(0);
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let mut live = Live::open(&acc).unwrap();
    assert!(!live.condstore && !live.qresync);
    imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    {
        let mut s = srv.lock().unwrap();
        s.set_seen(1);
        s.remove(3);
        s.add(0);
    }
    srv.lock().unwrap().log.clear();
    let r = imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert_eq!(r.mode, "uidnext");
    assert_eq!((r.delta.inserted.len(), r.delta.changed.len(), r.delta.removed.len()), (1, 1, 1), "{:?}", r.delta);
    let log = cmds(&srv);
    assert!(!log.iter().any(|c| c.contains("ENABLE") || c.contains("CHANGEDSINCE")), "{log:?}");
    let st = store.sync_state(&acc.id, "INBOX").unwrap().unwrap();
    assert_eq!((st.highest_modseq, st.uid_next), (0, 5));
    live.logout();
}

#[test]
fn uidvalidity_nova_recomeca_do_zero() {
    let (acc, srv, _) = start("ENABLE CONDSTORE QRESYNC");
    srv.lock().unwrap().add(0);
    let mut store = Store::open_in_memory().unwrap();
    let mut live = Live::open(&acc).unwrap();
    imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    srv.lock().unwrap().validity = 8;
    let r = imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert_eq!(r.mode, "full");
    live.logout();
}

#[test]
fn mensagem_grande_vem_sem_corpo_e_o_corpo_e_baixado_sob_demanda() {
    let (acc, srv, count) = start("IDLE");
    {
        let mut s = srv.lock().unwrap();
        s.add(0); // curta: chega inteira
        s.add(600); // ~17 KB: só o início
    }
    let mut store = Store::open_in_memory().unwrap();
    let hub = Hub::new();
    let r = hub.with(&acc, Kind::Cmd, |l| imap_sync::sync_on(l, &acc, &mut store, "INBOX", None)).unwrap();
    assert_eq!(r.delta.inserted.len(), 2);
    let curta = store.get(r.delta.inserted[0]).unwrap().unwrap();
    let grande = store.get(r.delta.inserted[1]).unwrap().unwrap();
    assert!(curta.body_loaded && curta.body_text.contains("Corpo 1"));
    assert!(!grande.body_loaded);

    // Abrir de novo (cache): sem rede.
    let before = cmds(&srv).len();
    assert!(store.get(grande.summary.id).unwrap().is_some());
    assert_eq!(cmds(&srv).len(), before);

    // Sob demanda: baixa, grava e não repete.
    assert!(imap_sync::load_body(&hub, &acc, &mut store, grande.summary.id).unwrap());
    let g = store.get(grande.summary.id).unwrap().unwrap();
    assert!(g.body_loaded && g.body_text.contains("linha de enchimento"), "{}", g.body_text.len());
    let n = cmds(&srv).len();
    assert!(!imap_sync::load_body(&hub, &acc, &mut store, grande.summary.id).unwrap());
    assert_eq!(cmds(&srv).len(), n);

    // A sessão persistente foi reaproveitada: uma conexão só para tudo isso.
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(hub.is_open(&acc.id, Kind::Cmd));
}

#[test]
fn servidor_que_nao_le_resposta_parcial_cai_para_mensagem_inteira() {
    let (acc, srv, _) = start("IDLE");
    {
        let mut s = srv.lock().unwrap();
        s.no_space = true;
        s.add(0);
        s.add(600);
    }
    let mut store = Store::open_in_memory().unwrap();
    // A primeira tentativa tropeça na resposta; a repetição baixa sem leitura parcial.
    let r = imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    assert_eq!(r.fetched, 2);
    assert!(r.delta.inserted.iter().all(|id| store.get(*id).unwrap().unwrap().body_loaded));
    // Daqui em diante nem tenta a leitura parcial.
    srv.lock().unwrap().add(600);
    srv.lock().unwrap().log.clear();
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap().fetched, 1);
    assert!(!cmds(&srv).iter().any(|c| c.contains("<0.")), "{:?}", cmds(&srv));
}

#[test]
fn janela_menor_que_a_pasta_amplia_baixando_so_o_que_falta() {
    let (acc, srv, _) = start("IDLE CONDSTORE ENABLE");
    {
        let mut s = srv.lock().unwrap();
        for _ in 0..5 {
            s.add(0);
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", Some(2)).unwrap().fetched, 2);
    // Janela já preenchida: segue incremental, sem baixar nada.
    let r = imap_sync::sync_mailbox(&acc, &mut store, "INBOX", Some(2)).unwrap();
    assert_eq!((r.fetched, r.mode), (0, "condstore"));
    // Janela maior: completa só o que falta.
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", Some(4)).unwrap().fetched, 2);
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap().fetched, 1);
}

#[test]
fn prefetch_baixa_os_corpos_que_faltam() {
    let (acc, srv, _) = start("IDLE");
    {
        let mut s = srv.lock().unwrap();
        for _ in 0..4 {
            s.add(600);
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let mut live = Live::open(&acc).unwrap();
    imap_sync::sync_on(&mut live, &acc, &mut store, "INBOX", None).unwrap();
    assert_eq!(store.uids_without_body(&acc.id, "INBOX", 10).unwrap().len(), 4);
    let done = imap_sync::prefetch_bodies_on(&mut live, &acc, &mut store, "INBOX", 3).unwrap();
    assert_eq!(done.len(), 3);
    assert_eq!(store.uids_without_body(&acc.id, "INBOX", 10).unwrap().len(), 1);
    live.logout();
}

#[test]
fn hub_reconecta_quando_a_conexao_cai() {
    let (acc, srv, count) = start("IDLE CONDSTORE ENABLE");
    srv.lock().unwrap().add(0);
    let mut store = Store::open_in_memory().unwrap();
    let hub = Hub::new();
    hub.with(&acc, Kind::Cmd, |l| imap_sync::sync_on(l, &acc, &mut store, "INBOX", None)).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // O servidor derruba a conexão no meio do próximo SELECT; o Hub reabre e repete.
    srv.lock().unwrap().add(0);
    srv.lock().unwrap().drop_on = Some("SELECT".into());
    let r = hub.with(&acc, Kind::Cmd, |l| imap_sync::sync_on(l, &acc, &mut store, "INBOX", None)).unwrap();
    assert_eq!(r.fetched, 1);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(hub.is_open(&acc.id, Kind::Cmd));

    // Servidor fora do ar de vez: erro de rede depois das tentativas, sem travar.
    let acc2 = ImapAccount { port: 1, ..acc.clone() };
    let hub2 = Hub::new();
    let t = std::time::Instant::now();
    let err = hub2.with(&acc2, Kind::Cmd, |_| Ok(())).unwrap_err();
    assert!(err.is_network(), "{err}");
    assert!(t.elapsed() < std::time::Duration::from_secs(5));
    drop(srv);
}
