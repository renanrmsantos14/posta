//! Testes contra um servidor IMAP real (Greenmail em Docker).
//! Rodar: `pnpm test:imap` (sobe o container e define POSTA_IMAP_TEST=127.0.0.1:3143).
//! Sem a variável, os testes passam sem fazer nada.

use imap::{ClientBuilder, ConnectionMode};
use mail_core::{imap_sync, send, ImapAccount, OutAttachment, Outgoing, Role, Security, SmtpServer, Store};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn account(tag: &str) -> Option<ImapAccount> {
    let addr = std::env::var("POSTA_IMAP_TEST").ok()?;
    let (host, port) = addr.split_once(':')?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    // Greenmail cria a caixa pelo endereço: login = email.
    let user = format!("{tag}{nanos}@localhost");
    Some(ImapAccount {
        id: user.clone(),
        host: host.into(),
        port: port.parse().ok()?,
        security: Security::Plaintext,
        username: user.clone(),
        name: None,
        smtp: std::env::var("POSTA_SMTP_TEST").ok().and_then(|p| {
            Some(SmtpServer { host: host.into(), port: p.parse().ok()?, security: Security::Plaintext })
        }),
        auth: mail_core::AuthMode::Password,
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
    let r = imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    assert_eq!((r.fetched, r.total), (3, 3));
    let list = store.list("INBOX", 50, 0).unwrap();
    assert_eq!(list.len(), 3);
    let cot = &store.search("cotacao", 10).unwrap()[0];
    assert_eq!(cot.subject, "Cotação aeroporto");
    assert_eq!(cot.from_name, "Ana Souza");
    assert!(!cot.seen);
    assert!(list.iter().find(|m| m.subject == "Reunião").unwrap().seen);

    // Segunda sincronização não baixa nada de novo.
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap().fetched, 0);

    // Marcar lida no servidor e excluir outra lá; o local acompanha.
    imap_sync::set_seen(&acc, "INBOX", cot.uid, true).unwrap();
    let terceira = list.iter().find(|m| m.subject == "Terceira").unwrap().uid;
    s.select("INBOX").unwrap();
    s.uid_store(terceira.to_string(), "+FLAGS (\\Deleted)").unwrap();
    s.expunge().unwrap();
    s.logout().ok();

    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
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
    assert_eq!(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap().fetched, 1000);
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

#[test]
fn responde_por_smtp_guarda_em_enviados_e_marca_respondida() {
    let (Some(eu), Some(ana)) = (account("eu"), account("ana")) else { return };
    if eu.smtp.is_none() {
        return;
    }
    let original = format!(
        "From: {}\r\nTo: {}\r\nSubject: Translado sexta\r\nMessage-ID: <orig@exemplo.com>\r\n\
         Date: Sat, 7 Nov 2026 10:00:00 -0300\r\n\r\nConfirma o horário?\r\n",
        ana.username, eu.username
    );
    session(&eu).append("INBOX", original.as_bytes()).finish().unwrap();
    let mut mine = Store::open_in_memory().unwrap();
    imap_sync::sync_mailbox(&eu, &mut mine, "INBOX", None).unwrap();
    let id = mine.list("INBOX", 1, 0).unwrap()[0].id;
    let reply = mine.reply_ref(id).unwrap().unwrap();
    assert_eq!(mine.get(id).unwrap().unwrap().reply_addr, ana.username);

    let out = Outgoing {
        to: ana.username.clone(),
        subject: "Re: Translado sexta".into(),
        body: "Confirmado: 7h40 no saguão.".into(),
        html: None,
        cc: String::new(),
        bcc: String::new(),
        attachments: vec![],
        from: None,
    };
    let report = send::send(&eu, &out, Some(&reply)).unwrap();
    assert_eq!(report.warning, None);
    assert!(report.saved_to_sent);

    // Ana recebe a resposta encadeada na conversa.
    let mut theirs = Store::open_in_memory().unwrap();
    let mut got = None;
    for _ in 0..20 {
        imap_sync::sync_mailbox(&ana, &mut theirs, "INBOX", None).unwrap();
        got = theirs.search("saguao", 5).unwrap().into_iter().next();
        if got.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let got = got.expect("resposta chegou");
    assert_eq!(got.subject, "Re: Translado sexta");
    assert_eq!(theirs.reply_ref(got.id).unwrap().unwrap().refs, "orig@exemplo.com");

    // Cópia em Enviados e original marcada como respondida.
    let mut s = session(&eu);
    let sent = s.select("Sent").unwrap();
    assert_eq!(sent.exists, 1);
    s.select("INBOX").unwrap();
    let f = s.uid_fetch(reply.uid.to_string(), "FLAGS").unwrap();
    assert!(f.iter().next().unwrap().flags().iter().any(|f| matches!(f, imap::types::Flag::Answered)));
}

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

#[test]
fn fetch_to_traz_o_primeiro_destinatario() {
    let Some(eu) = account("eu") else { return };
    let msg = "From: eu@exemplo.com\r\nTo: \"Ana Souza\" <ana@exemplo.com>, b@exemplo.com\r\n\
               Subject: Enviada\r\n\r\ncorpo\r\n";
    let mut s = session(&eu);
    s.append("INBOX", msg.as_bytes()).finish().unwrap();
    s.select("INBOX").unwrap();
    let uid = *s.uid_search("ALL").unwrap().iter().next().unwrap();
    assert_eq!(imap_sync::fetch_to(&eu, "INBOX", uid).unwrap().as_deref(), Some("ana@exemplo.com"));
    assert_eq!(imap_sync::fetch_to(&eu, "INBOX", uid + 99).unwrap(), None);
}

#[test]
fn idle_acorda_quando_chega_mensagem() {
    use mail_core::IdleEnd;
    use std::time::Duration;
    let Some(acc) = account("idle") else { return };
    let waiter = {
        let acc = acc.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            (imap_sync::idle_wait(&acc, "INBOX", Duration::from_secs(20)).unwrap(), t.elapsed())
        })
    };
    // Dá tempo de o IDLE começar, depois entrega uma mensagem por outra conexão.
    std::thread::sleep(Duration::from_millis(1500));
    session(&acc).append("INBOX", &raw(1, "Chegou")).finish().unwrap();
    let (end, took) = waiter.join().unwrap();
    assert_eq!(end, IdleEnd::Changed);
    assert!(took < Duration::from_secs(10), "voltou só após {took:?}");

    // Sem novidade: expira no prazo.
    let end = imap_sync::idle_wait(&acc, "INBOX", Duration::from_secs(2)).unwrap();
    assert_eq!(end, IdleEnd::TimedOut);
}

#[test]
fn envia_com_anexo_e_cc_e_le_de_volta() {
    use base64::Engine;
    let (Some(eu), Some(ana), Some(bia)) = (account("eu"), account("ana"), account("bia")) else { return };
    if eu.smtp.is_none() {
        return;
    }
    let conteudo: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    let out = Outgoing {
        to: ana.username.clone(),
        subject: "Contrato com anexo".into(),
        body: "Segue o arquivo.".into(),
        html: Some("<p>Segue o <b>arquivo</b>.</p>".into()),
        cc: bia.username.clone(),
        bcc: String::new(),
        attachments: vec![OutAttachment {
            name: "contrato.bin".into(),
            mime: "application/octet-stream".into(),
            data: base64::engine::general_purpose::STANDARD.encode(&conteudo),
        }],
        from: None,
    };
    send::send(&eu, &out, None).unwrap();

    // Destinatário e copiado recebem a mensagem com o anexo listado.
    for quem in [&ana, &bia] {
        let mut store = Store::open_in_memory().unwrap();
        let mut got = None;
        for _ in 0..20 {
            imap_sync::sync_mailbox(quem, &mut store, "INBOX", None).unwrap();
            got = store.search("arquivo", 5).unwrap().into_iter().next();
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let got = got.expect("mensagem chegou");
        let d = store.get(got.id).unwrap().unwrap();
        assert_eq!(d.attachments.len(), 1, "{:?}", d.attachments);
        let a = &d.attachments[0];
        assert_eq!((a.name.as_str(), a.size), ("contrato.bin", 5000));
        // Corpo HTML + texto (multipart/alternative dentro do mixed) chegam juntos.
        assert!(d.body_html.contains("<b>arquivo</b>"), "{:?}", d.body_html);
        assert!(d.body_text.contains("Segue o arquivo."), "{:?}", d.body_text);

        // O conteúdo vem do servidor sob demanda e bate byte a byte.
        let raw = imap_sync::fetch_raw(quem, "INBOX", got.uid).unwrap().unwrap();
        let (_, data) = mail_core::attachments::extract(&raw, a.index).unwrap();
        assert_eq!(data, conteudo);
    }
}

fn server_flags(acc: &ImapAccount, uid: u32) -> Vec<imap::types::Flag<'static>> {
    let mut s = session(acc);
    s.select("INBOX").unwrap();
    let f = s.uid_fetch(uid.to_string(), "FLAGS").unwrap();
    let flags = f.iter().next().unwrap().flags().iter().map(|f| f.clone().into_owned()).collect();
    s.logout().ok();
    flags
}

#[test]
fn marca_lida_e_estrela_no_servidor_e_em_lote() {
    use imap::types::Flag;
    let Some(acc) = account("flg") else { return };
    let mut s = session(&acc);
    s.append("INBOX", &raw(1, "Um")).finish().unwrap();
    s.append("INBOX", &raw(2, "Dois")).finish().unwrap();
    s.append("INBOX", &raw(3, "Tres")).flag(Flag::Flagged).finish().unwrap();
    s.logout().ok();

    let mut store = Store::open_in_memory().unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    let list = store.list("INBOX", 10, 0).unwrap();
    let by = |subject: &str| list.iter().find(|m| m.subject == subject).unwrap().clone();
    // A estrela que já estava no servidor chega ao banco local.
    assert!(by("Tres").flagged && !by("Um").flagged);
    assert_eq!(store.list_starred(10, 0).unwrap().len(), 1);

    // Estrela e lida no servidor, conferidas direto nele e depois pelo sync.
    let (um, dois) = (by("Um").uid, by("Dois").uid);
    imap_sync::set_flag(&acc, "INBOX", &[um], imap_sync::MsgFlag::Flagged, true).unwrap();
    imap_sync::set_seen(&acc, "INBOX", um, true).unwrap();
    let flags = server_flags(&acc, um);
    assert!(flags.contains(&Flag::Flagged) && flags.contains(&Flag::Seen), "{flags:?}");
    assert!(!server_flags(&acc, dois).contains(&Flag::Seen));

    // Lote: marca as duas como lidas de uma vez e tira a estrela da terceira.
    imap_sync::set_flag(&acc, "INBOX", &[um, dois], imap_sync::MsgFlag::Seen, true).unwrap();
    imap_sync::set_flag(&acc, "INBOX", &[by("Tres").uid], imap_sync::MsgFlag::Flagged, false).unwrap();
    imap_sync::set_flag(&acc, "INBOX", &[um], imap_sync::MsgFlag::Seen, false).unwrap();
    assert!(!server_flags(&acc, um).contains(&Flag::Seen));
    assert!(server_flags(&acc, dois).contains(&Flag::Seen));
    assert!(!server_flags(&acc, by("Tres").uid).contains(&Flag::Flagged));

    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    let list = store.list("INBOX", 10, 0).unwrap();
    let by = |subject: &str| list.iter().find(|m| m.subject == subject).unwrap().clone();
    assert!(by("Um").flagged && !by("Um").seen);
    assert!(by("Dois").seen && !by("Tres").flagged);
    assert_eq!(store.list_starred(10, 0).unwrap().len(), 1);
}

#[test]
fn move_e_exclui_varias_de_uma_vez() {
    let Some(acc) = account("lote") else { return };
    let mut s = session(&acc);
    for i in 0..4 {
        s.append("INBOX", &raw(i, &format!("M{i}"))).finish().unwrap();
    }
    s.logout().ok();
    let folders = imap_sync::discover_folders(&acc).unwrap();
    let archive = folders.iter().find(|f| f.role == Role::Archive).unwrap().name.clone();

    let mut store = Store::open_in_memory().unwrap();
    store.set_folders(&acc.id, &folders).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    let uids: Vec<u32> = store.list("INBOX", 10, 0).unwrap().iter().map(|m| m.uid).collect();

    imap_sync::move_messages(&acc, "INBOX", &uids[..3], &archive).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, &archive, Some(1000)).unwrap();
    assert_eq!(store.list_folder(Role::Inbox, 10, 0).unwrap().len(), 1);
    let arquivo = store.list_folder(Role::Archive, 10, 0).unwrap();
    assert_eq!(arquivo.len(), 3);
    assert_eq!(arquivo[0].role.as_deref(), Some("archive"));

    let alvo: Vec<u32> = arquivo.iter().map(|m| m.uid).collect();
    imap_sync::delete_forever_many(&acc, &archive, &alvo).unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, &archive, Some(1000)).unwrap();
    assert!(store.list_folder(Role::Archive, 10, 0).unwrap().is_empty());
}

#[test]
fn cria_renomeia_e_exclui_pastas_com_subpastas() {
    let Some(acc) = account("pastas") else { return };
    let obra = imap_sync::create_folder(&acc, None, "Obra").unwrap();
    let fase = imap_sync::create_folder(&acc, Some(&obra), "Fase 1").unwrap();
    assert!(imap_sync::create_folder(&acc, None, "obra").is_err(), "nome repetido");
    assert!(imap_sync::create_folder(&acc, None, "a/b").is_err(), "nome inválido");

    let (names, delimiter) = imap_sync::list_user_folders(&acc).unwrap();
    assert!(names.contains(&obra) && names.contains(&fase), "{names:?}");
    assert!(delimiter.is_some());

    // Move uma mensagem para a pasta nova, como no arrastar da lista.
    let mut s = session(&acc);
    s.append("INBOX", &raw(1, "Planta")).finish().unwrap();
    s.logout().ok();
    let mut store = Store::open_in_memory().unwrap();
    imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).unwrap();
    let uid = store.list("INBOX", 10, 0).unwrap()[0].uid;
    imap_sync::move_messages(&acc, "INBOX", &[uid], &fase).unwrap();
    assert_eq!(imap_sync::count_messages(&acc, &obra).unwrap(), 1, "conta as subpastas");

    // Renomear leva a subpasta junto e preserva a mensagem.
    let novo = imap_sync::rename_folder(&acc, &obra, "Projeto").unwrap();
    let (names, _) = imap_sync::list_user_folders(&acc).unwrap();
    let filha = names.iter().find(|n| n.ends_with("Fase 1")).unwrap().clone();
    assert!(names.contains(&novo) && !names.contains(&obra) && filha.starts_with(&novo), "{names:?}");
    assert_eq!(imap_sync::count_messages(&acc, &novo).unwrap(), 1);

    // Pastas do sistema ficam como estão.
    let archive = imap_sync::discover_folders(&acc).unwrap().into_iter().find(|f| f.role == Role::Archive).unwrap().name;
    assert!(imap_sync::rename_folder(&acc, "INBOX", "Outra").is_err());
    assert!(imap_sync::delete_folder(&acc, "INBOX").is_err());
    assert!(imap_sync::delete_folder(&acc, &archive).is_err());

    // Excluir leva as subpastas.
    let gone = imap_sync::delete_folder(&acc, &novo).unwrap();
    assert_eq!(gone, [filha, novo]);
    let (names, _) = imap_sync::list_user_folders(&acc).unwrap();
    assert!(!names.iter().any(|n| n.contains("Projeto")), "{names:?}");
}

#[test]
fn envia_com_alias_como_remetente() {
    let (Some(eu), Some(ana)) = (account("eu"), account("ana")) else { return };
    if eu.smtp.is_none() {
        return;
    }
    let out = Outgoing {
        to: ana.username.clone(),
        subject: "Enviado pelo alias".into(),
        body: "Mensagem vinda do alias.".into(),
        html: None,
        cc: String::new(),
        bcc: String::new(),
        attachments: vec![],
        from: Some("Reservas Betinhos <reservas@betinhos.exemplo>".into()),
    };
    send::send(&eu, &out, None).unwrap();

    let mut store = Store::open_in_memory().unwrap();
    let mut got = None;
    for _ in 0..20 {
        imap_sync::sync_mailbox(&ana, &mut store, "INBOX", None).unwrap();
        got = store.search("alias", 5).unwrap().into_iter().find(|m| m.subject == "Enviado pelo alias");
        if got.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let got = got.expect("mensagem do alias chegou");
    assert_eq!(got.from_addr, "reservas@betinhos.exemplo");
    assert_eq!(got.from_name, "Reservas Betinhos");
}
