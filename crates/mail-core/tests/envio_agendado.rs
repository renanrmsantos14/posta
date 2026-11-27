//! Envio agendado de ponta a ponta contra um SMTP/IMAP local de teste (Greenmail em Docker).
//! Rodar: `pnpm test:imap` (define POSTA_IMAP_TEST e POSTA_SMTP_TEST). Sem as variáveis, passa sem fazer nada.
//! Nunca usa conta real: todos os endereços são `...@localhost` criados na hora pelo Greenmail.

use mail_core::later::DueOutcome;
use mail_core::{imap_sync, send, AuthMode, ImapAccount, Outgoing, Security, SmtpServer, Store};
use std::time::{SystemTime, UNIX_EPOCH};

fn account(tag: &str, smtp_port: Option<u16>) -> Option<ImapAccount> {
    let addr = std::env::var("POSTA_IMAP_TEST").ok()?;
    let smtp = std::env::var("POSTA_SMTP_TEST").ok()?.parse::<u16>().ok()?;
    let (host, port) = addr.split_once(':')?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let user = format!("{tag}{nanos}@localhost");
    Some(ImapAccount {
        id: user.clone(),
        host: host.into(),
        port: port.parse().ok()?,
        security: Security::Plaintext,
        username: user.clone(),
        name: None,
        smtp: Some(SmtpServer { host: host.into(), port: smtp_port.unwrap_or(smtp), security: Security::Plaintext }),
        auth: AuthMode::Password,
        password: user,
    })
}

fn out(to: &str, subject: &str) -> Outgoing {
    Outgoing { to: to.into(), subject: subject.into(), body: "Agendado para depois.".into(), html: None, cc: String::new(), bcc: String::new(), attachments: vec![], from: None, importance: None }
}

fn chegou(acc: &ImapAccount, subject: &str) -> bool {
    let mut store = Store::open_in_memory().unwrap();
    for _ in 0..20 {
        imap_sync::sync_mailbox(acc, &mut store, "INBOX", None).unwrap();
        if store.search("Agendado", 5).unwrap().iter().any(|m| m.subject == subject) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// O banco em arquivo é fechado antes da hora (app fechado) e reaberto depois dela: a fila envia de verdade e avisa o atraso.
#[test]
fn agendado_com_o_app_fechado_sai_ao_abrir_e_chega_ao_destinatario() {
    let (Some(eu), Some(ana)) = (account("eu", None), account("ana", None)) else { return };
    let path = std::env::temp_dir().join(format!("posta-agendado-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let agora = 1_800_000_000;
    {
        let s = Store::open(&path).unwrap();
        s.schedule_send("a1", &eu.id, None, agora, &out(&ana.username, "Agendado sem app")).unwrap();
    } // app fecha
    let s = Store::open(&path).unwrap(); // app abre 10 minutos depois
    s.requeue_sending().unwrap();
    let rep = s
        .run_due(agora + 600, |x| match send::send(&eu, &x.out, None) {
            Ok(_) => DueOutcome::Sent,
            Err(e) => DueOutcome::Failed(e.to_string()),
        })
        .unwrap();
    assert_eq!((rep.sent, rep.late, rep.failed), (1, 1, 0), "{rep:?}");
    assert!(s.list_scheduled().unwrap().is_empty());
    assert!(chegou(&ana, "Agendado sem app"), "a mensagem não chegou ao destinatário");
    drop(s);
    let _ = std::fs::remove_file(&path);
}

/// Servidor SMTP fora do ar: o envio falha por rede, fica na fila com o erro e sai quando o servidor volta.
#[test]
fn sem_servidor_tenta_de_novo_e_sai_quando_volta() {
    let (Some(boa), Some(ana)) = (account("eu", None), account("ana", None)) else { return };
    // Porta sem ninguém ouvindo.
    let morta = ImapAccount { smtp: Some(SmtpServer { host: "127.0.0.1".into(), port: 1, security: Security::Plaintext }), ..boa.clone() };
    let s = Store::open_in_memory().unwrap();
    s.schedule_send("b1", &boa.id, None, 1000, &out(&ana.username, "Agendado sem rede")).unwrap();

    let tentar = |acc: &ImapAccount, now| {
        s.run_due(now, |x| match send::send(acc, &x.out, None) {
            Ok(_) => DueOutcome::Sent,
            Err(e) => DueOutcome::Failed(e.to_string()),
        })
        .unwrap()
    };
    let r1 = tentar(&morta, 1000);
    assert_eq!((r1.sent, r1.retrying), (0, 1), "{r1:?}");
    let item = &s.list_scheduled().unwrap()[0];
    assert_eq!((item.status.as_str(), item.attempts), ("pending", 1));
    assert!(!item.error.is_empty(), "o erro fica visível na lista");

    let r2 = tentar(&boa, 1000 + 120);
    assert_eq!((r2.sent, r2.late), (1, 0), "{r2:?}");
    assert!(chegou(&ana, "Agendado sem rede"));
}
