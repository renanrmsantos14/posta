//! Integração contra um servidor IMAP real e local (GreenMail): login, pastas, sync inicial e incremental,
//! UIDVALIDITY, queda de conexão e casos de borda. Rodar: `pnpm test:imap`.
//! Só contas sintéticas no servidor local; sem `POSTA_IMAP_TEST` os testes falham de propósito.

mod common;

use common::{account, raw, session, Proxy};
use mail_core::{imap_sync, Hub, ImapAccount, Kind, Store};
use std::time::{Duration, Instant};

fn subjects(store: &Store, mailbox: &str) -> Vec<String> {
    let mut v: Vec<String> = store.list(mailbox, 20_000, 0).unwrap().into_iter().map(|m| m.subject).collect();
    v.sort();
    v
}

fn sync(acc: &ImapAccount, store: &mut Store, mailbox: &str) -> mail_core::SyncReport {
    imap_sync::sync_mailbox(acc, store, mailbox, None).unwrap()
}

#[test]
fn login_valido_e_servidor_fora_do_ar() {
    let acc = account("login");
    imap_sync::check_login(&acc).unwrap();
    let off = ImapAccount { port: 1, ..acc };
    let err = imap_sync::check_login(&off).unwrap_err();
    assert!(err.is_network(), "{err}");
}

#[test]
fn lista_pastas_de_sistema_e_de_usuario() {
    let acc = account("pastas");
    session(&acc).logout().ok();
    let (user, delim) = imap_sync::list_user_folders(&acc).unwrap();
    assert!(user.is_empty(), "{user:?}");
    assert_eq!(delim.as_deref(), Some("."));
    imap_sync::create_folder(&acc, None, "Clientes").unwrap();
    imap_sync::create_folder(&acc, Some("Clientes"), "Aeroporto").unwrap();
    let (user, _) = imap_sync::list_user_folders(&acc).unwrap();
    assert_eq!(user, vec!["Clientes".to_string(), "Clientes.Aeroporto".to_string()]);
    let sys = imap_sync::discover_folders(&acc).unwrap();
    assert!(sys.iter().any(|f| f.role == mail_core::Role::Inbox), "{sys:?}");
}

#[test]
fn sync_inicial_de_caixa_vazia_e_depois_com_mensagens() {
    let acc = account("vazia");
    let mut store = Store::open_in_memory().unwrap();
    let r = sync(&acc, &mut store, "INBOX");
    assert_eq!((r.fetched, r.total, r.mode), (0, 0, "full"));
    assert!(store.list("INBOX", 10, 0).unwrap().is_empty());
    let mut s = session(&acc);
    for i in 0..30 {
        s.append("INBOX", &raw(i, &format!("Msg {i:02}"))).finish().unwrap();
    }
    let r = sync(&acc, &mut store, "INBOX");
    assert_eq!((r.fetched, r.total), (30, 30));
    assert_eq!(subjects(&store, "INBOX").len(), 30);
    // Sem mudança nenhuma, a segunda sync não baixa nem altera nada.
    let r = sync(&acc, &mut store, "INBOX");
    assert_eq!(r.fetched, 0);
    assert!(r.delta.changed.is_empty() && r.delta.removed.is_empty());
}

#[test]
fn incremental_flags_exclusao_e_mover_para_outra_pasta() {
    let acc = account("inc");
    let mut s = session(&acc);
    s.create("Arquivo2").unwrap();
    for i in 0..5 {
        s.append("INBOX", &raw(i, &format!("Msg {i}"))).finish().unwrap();
    }
    let mut store = Store::open_in_memory().unwrap();
    sync(&acc, &mut store, "INBOX");
    let list = store.list("INBOX", 50, 0).unwrap();
    let uid = |sub: &str| list.iter().find(|m| m.subject == sub).unwrap().uid;

    s.select("INBOX").unwrap();
    s.uid_store(uid("Msg 0").to_string(), "+FLAGS (\\Seen)").unwrap(); // flag alterada
    s.uid_store(uid("Msg 1").to_string(), "+FLAGS (\\Flagged)").unwrap();
    s.uid_store(uid("Msg 2").to_string(), "+FLAGS (\\Deleted)").unwrap(); // apagada
    s.expunge().unwrap();
    s.uid_mv(uid("Msg 3").to_string(), "Arquivo2").unwrap(); // movida (GreenMail aceita MOVE; senão COPY+EXPUNGE)
    s.append("INBOX", &raw(9, "Nova")).finish().unwrap();

    let r = sync(&acc, &mut store, "INBOX");
    assert_eq!(r.fetched, 1, "{:?}", r.mode);
    let now = store.list("INBOX", 50, 0).unwrap();
    let by = |sub: &str| now.iter().find(|m| m.subject == sub);
    assert!(by("Msg 0").unwrap().seen);
    assert!(by("Msg 1").unwrap().flagged);
    assert!(by("Msg 2").is_none(), "apagada no servidor ainda aparece");
    assert!(by("Msg 3").is_none(), "movida ainda aparece na origem");
    assert!(by("Nova").is_some() && by("Msg 4").is_some());
    assert_eq!(now.len(), 4);

    let r = sync(&acc, &mut store, "Arquivo2");
    assert_eq!(r.fetched, 1);
    assert_eq!(subjects(&store, "Arquivo2"), vec!["Msg 3".to_string()]);
}

/// UIDVALIDITY nova, vista pelo cache. O GreenMail deriva a UIDVALIDITY do nome da pasta (apagar e recriar dá o
/// mesmo valor) e estoura NullPointerException ao apagar pasta já selecionada; então o servidor "recriado" é outra
/// caixa com o mesmo id de conta no banco local (UIDs 1..N repetidos com outro conteúdo), e o proxy troca o valor
/// anunciado no SELECT. É o que a UIDVALIDITY existe para detectar: os UIDs antigos não valem mais.
#[test]
fn uidvalidity_nova_descarta_o_que_era_da_pasta_antiga() {
    let before = account("uidv-a");
    let after = ImapAccount { id: before.id.clone(), ..account("uidv-b") };
    for (acc, subjects) in [(&before, ["Antiga 1", "Antiga 2", "Antiga 3"].as_slice()), (&after, ["Nova A", "Nova B"].as_slice())] {
        let mut s = session(acc);
        s.create("Projetos").unwrap();
        for (i, sub) in subjects.iter().enumerate() {
            s.append("Projetos", &raw(i, sub)).finish().unwrap();
        }
    }
    let proxy = Proxy::start();
    let mut store = Store::open_in_memory().unwrap();
    sync(&proxy.via(&before), &mut store, "Projetos");
    assert_eq!(subjects(&store, "Projetos"), vec!["Antiga 1", "Antiga 2", "Antiga 3"]);

    proxy.force_uidvalidity(777_001);
    let r = sync(&proxy.via(&after), &mut store, "Projetos");
    assert_eq!(r.mode, "full", "UIDVALIDITY nova deve recomeçar do zero");
    assert_eq!(subjects(&store, "Projetos"), vec!["Nova A", "Nova B"]);
    // E continua incremental normal depois da troca.
    session(&after).append("Projetos", &raw(7, "Nova C")).finish().unwrap();
    assert_eq!(sync(&proxy.via(&after), &mut store, "Projetos").fetched, 1);
    assert_eq!(subjects(&store, "Projetos"), vec!["Nova A", "Nova B", "Nova C"]);
}

#[test]
fn hub_reconecta_depois_da_queda_e_a_sync_segue() {
    let acc = account("queda");
    let mut s = session(&acc);
    for i in 0..3 {
        s.append("INBOX", &raw(i, &format!("A{i}"))).finish().unwrap();
    }
    let proxy = Proxy::start();
    let via = proxy.via(&acc);
    let hub = Hub::new();
    let mut store = Store::open_in_memory().unwrap();
    let r = hub.with(&via, Kind::Cmd, |l| imap_sync::sync_on(l, &via, &mut store, "INBOX", None)).unwrap();
    assert_eq!(r.fetched, 3);

    s.append("INBOX", &raw(9, "Depois da queda")).finish().unwrap();
    proxy.cut_all(); // a sessão guardada no Hub morre
    let r = hub.with(&via, Kind::Cmd, |l| imap_sync::sync_on(l, &via, &mut store, "INBOX", None)).unwrap();
    assert_eq!(r.fetched, 1);
    assert_eq!(store.list("INBOX", 10, 0).unwrap().len(), 4);
}

#[test]
fn conexao_cortada_no_meio_do_fetch_nao_deixa_lixo_e_a_proxima_sync_completa() {
    let acc = account("corte");
    let mut s = session(&acc);
    for i in 0..60 {
        s.append("INBOX", &raw(i, &format!("Corte {i:02}"))).finish().unwrap();
    }
    let proxy = Proxy::start();
    let via = proxy.via(&acc);
    let mut store = Store::open_in_memory().unwrap();
    // Passa o login, o SELECT e a lista de flags e corta no meio das mensagens.
    proxy.cut_after(9_000);
    let err = imap_sync::sync_mailbox(&via, &mut store, "INBOX", None).unwrap_err();
    assert!(err.is_network(), "esperava erro de rede, veio: {err}");
    let partial = store.list("INBOX", 100, 0).unwrap().len();
    assert!(partial < 60, "cortou tarde demais ({partial})");
    // Nenhuma mensagem gravada pela metade: todas têm assunto e corpo.
    for m in store.list("INBOX", 100, 0).unwrap() {
        assert!(m.subject.starts_with("Corte "), "assunto truncado: {:?}", m.subject);
        assert!(store.get(m.id).unwrap().unwrap().body_text.contains("Corpo da mensagem"));
    }

    proxy.heal();
    sync(&via, &mut store, "INBOX");
    assert_eq!(subjects(&store, "INBOX").len(), 60);
    let r = sync(&via, &mut store, "INBOX");
    assert_eq!(r.fetched, 0, "a sync não ficou idempotente depois do corte");
}

#[test]
fn assunto_com_emoji_e_acento_e_busca_sem_acento() {
    let acc = account("emoji");
    let mut s = session(&acc);
    // "Ação 🚗 Cotação" em encoded-word UTF-8 (base64) e a mesma coisa em Q com acentos.
    s.append("INBOX", &raw(1, "=?UTF-8?B?QcOnw6NvIPCfmpcgQ290YcOnw6Nv?=")).finish().unwrap();
    s.append("INBOX", &raw(2, "=?UTF-8?Q?Reuni=C3=A3o_de_diretoria_=E2=9C=88?=")).finish().unwrap();
    let mut store = Store::open_in_memory().unwrap();
    sync(&acc, &mut store, "INBOX");
    assert_eq!(subjects(&store, "INBOX"), vec!["Ação 🚗 Cotação".to_string(), "Reunião de diretoria ✈".to_string()]);
    assert_eq!(store.search("acao", 10).unwrap().len(), 1);
    assert_eq!(store.search("COTACAO", 10).unwrap().len(), 1);
    assert_eq!(store.search("reuniao dir", 10).unwrap().len(), 1);
}

#[test]
#[ignore = "BUG conhecido: corpo 8-bit sem charset vira mojibake (corrigido no PR de charset, #70)"]
fn corpo_sem_charset_declarado() {
    let acc = account("semcs");
    let mut s = session(&acc);
    // 8-bit sem charset: "ação" em Latin-1 (E7 E3), como clientes antigos mandam.
    let mut m = b"From: a@exemplo.com\r\nTo: b@exemplo.com\r\nSubject: Sem charset\r\nMessage-ID: <semcs@x>\r\nContent-Type: text/plain\r\n\r\nA\xE7\xE3o combinada.\r\n".to_vec();
    s.append("INBOX", &m).finish().unwrap();
    m.clear();
    let mut store = Store::open_in_memory().unwrap();
    sync(&acc, &mut store, "INBOX");
    let id = store.list("INBOX", 10, 0).unwrap()[0].id;
    let body = store.get(id).unwrap().unwrap().body_text;
    assert!(body.contains("Ação combinada"), "corpo Latin-1 sem charset veio assim: {body:?}");
}

#[test]
#[ignore = "lento: o GreenMail leva minutos com 10 mil; o sync inicial passou de 15 min no GreenMail (debug); rode com --ignored para medir"]
fn dez_mil_mensagens_sincronizam_e_o_incremental_e_rapido() {
    let acc = account("dezmil");
    let mut s = session(&acc);
    let t = Instant::now();
    for i in 0..10_000 {
        s.append("INBOX", &raw(i, &format!("Mensagem {i:05}"))).finish().unwrap();
    }
    eprintln!("append de 10.000 no servidor: {:?}", t.elapsed());
    let mut store = Store::open_in_memory().unwrap();
    let t = Instant::now();
    let r = sync(&acc, &mut store, "INBOX");
    let first = t.elapsed();
    assert_eq!((r.fetched, r.total), (10_000, 10_000));
    eprintln!("sync inicial de 10.000: {first:?}");
    s.append("INBOX", &raw(10_001, "Chegou")).finish().unwrap();
    let t = Instant::now();
    let r = sync(&acc, &mut store, "INBOX");
    let inc = t.elapsed();
    eprintln!("sync incremental (1 nova em 10.000): {inc:?}");
    assert_eq!(r.fetched, 1);
    assert!(first < Duration::from_secs(900), "sync inicial lenta: {first:?}");
    assert!(inc < Duration::from_secs(5), "sync incremental lenta: {inc:?}");
    let t = Instant::now();
    assert_eq!(store.search("Guarulhos", 50).unwrap().len(), 50);
    assert!(t.elapsed() < Duration::from_millis(500));
}

#[test]
fn mensagem_de_20_mb_sincroniza_e_o_corpo_abre_sob_demanda() {
    let acc = account("grande");
    let mut s = session(&acc);
    let filler = "linha de enchimento com acentuação para ocupar espaço\r\n".repeat(20 * 1024 * 1024 / 56);
    let m = format!(
        "From: a@exemplo.com\r\nTo: b@exemplo.com\r\nSubject: Gigante\r\nMessage-ID: <gig@x>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{filler}FIM-DO-CORPO\r\n"
    );
    assert!(m.len() > 20 * 1024 * 1024);
    s.append("INBOX", m.as_bytes()).finish().unwrap();
    let hub = Hub::new();
    let mut store = Store::open_in_memory().unwrap();
    let t = Instant::now();
    let r = hub.with(&acc, Kind::Cmd, |l| imap_sync::sync_on(l, &acc, &mut store, "INBOX", None)).unwrap();
    eprintln!("sync de 1 mensagem de {} MB: {:?}", m.len() / 1_048_576, t.elapsed());
    assert_eq!(r.fetched, 1);
    let id = store.list("INBOX", 5, 0).unwrap()[0].id;
    imap_sync::load_body(&hub, &acc, &mut store, id).unwrap();
    assert!(store.get(id).unwrap().unwrap().body_text.contains("FIM-DO-CORPO"));
}

/// Processo-filho do teste abaixo: sincroniza outra pasta da mesma conta no MESMO arquivo SQLite do pai.
#[test]
fn filho_wal_sincroniza_no_banco_compartilhado() {
    let (Ok(path), Ok(user)) = (std::env::var("POSTA_WAL_DB"), std::env::var("POSTA_WAL_USER")) else { return };
    let (host, port) = common::server();
    let acc = ImapAccount { id: user.clone(), host, port, username: user.clone(), password: user, ..account("filho") };
    let mut store = Store::open(&path).unwrap();
    let r = sync(&acc, &mut store, "Outra");
    assert_eq!(r.fetched, 150);
}

#[test]
#[ignore = "BUG conhecido: 'database is locked' com dois processos no mesmo banco (corrigido no PR do WAL, #72)"]
fn banco_aberto_por_dois_processos_com_wal() {
    let dir = std::env::temp_dir().join(format!("posta-wal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mail.db");
    let acc = account("wal");
    let mut s = session(&acc);
    s.create("Outra").unwrap();
    for i in 0..200 {
        s.append("INBOX", &raw(i, &format!("Wal {i:03}"))).finish().unwrap();
    }
    for i in 0..150 {
        s.append("Outra", &raw(i, &format!("Outra {i:03}"))).finish().unwrap();
    }
    let mut store = Store::open(&path).unwrap();
    // Dois processos (o app e o worker do Android) gravando no mesmo arquivo ao mesmo tempo.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "filho_wal_sincroniza_no_banco_compartilhado", "--nocapture"])
        .env("POSTA_WAL_DB", &path)
        .env("POSTA_WAL_USER", &acc.username)
        .spawn()
        .unwrap();
    let r = sync(&acc, &mut store, "INBOX");
    assert_eq!(r.fetched, 200);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "filho falhou: {}", String::from_utf8_lossy(&out.stdout));
    // O pai, que abriu o banco antes, enxerga o que o filho gravou, e um leitor novo enxerga tudo.
    assert_eq!(store.count_box(&acc.id, "Outra").unwrap(), 150);
    let reader = Store::open(&path).unwrap();
    assert_eq!(reader.count_box(&acc.id, "INBOX").unwrap(), 200);
    assert_eq!(reader.count_box(&acc.id, "Outra").unwrap(), 150);
    drop((store, reader));
    std::fs::remove_dir_all(dir).ok();
}

/// Nome de pasta no fio IMAP (UTF-7 modificado, RFC 3501 §5.1.3), escrito à mão para não depender do código testado.
const PRECOS_UTF7: &str = "Pre&AOc-os e Cota&AOcA9Q-es"; // "Preços e Cotações"

#[test]
#[ignore = "BUG conhecido: nomes de pasta sem UTF-7 modificado (corrigido no PR #75)"]
fn pasta_com_acento_cria_lista_e_sincroniza_com_o_nome_legivel() {
    let acc = account("utf7");
    let leaf = "Preços e Cotações";
    let name = imap_sync::create_folder(&acc, None, leaf).unwrap();
    assert_eq!(name, leaf, "o nome devolvido ao app deve ser o legível");
    // No servidor a pasta existe com o nome em UTF-7 modificado, não em UTF-8 cru.
    let mut s = session(&acc);
    let on_wire: Vec<String> = s.list(None, Some("*")).unwrap().iter().map(|n| n.name().to_string()).collect();
    assert!(on_wire.contains(&PRECOS_UTF7.to_string()), "no servidor: {on_wire:?}");
    s.append(PRECOS_UTF7, &raw(1, "Dentro da pasta acentuada")).finish().unwrap();

    let (user, _) = imap_sync::list_user_folders(&acc).unwrap();
    assert_eq!(user, vec![leaf.to_string()]);
    let mut store = Store::open_in_memory().unwrap();
    let r = sync(&acc, &mut store, leaf);
    assert_eq!(r.fetched, 1);
    assert_eq!(subjects(&store, leaf), vec!["Dentro da pasta acentuada"]);
    assert_eq!(imap_sync::count_messages(&acc, leaf).unwrap(), 1);
}
