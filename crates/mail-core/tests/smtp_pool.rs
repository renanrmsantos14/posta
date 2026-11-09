//! Envio SMTP com conexão reaproveitada (pool do lettre) e reconexão quando a conexão guardada cai.
//! Servidor SMTP falso em processo; roda sempre, sem Docker.

use mail_core::send::{self, Outgoing};
use mail_core::{AuthMode, ImapAccount, Security, SmtpServer};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct Stats {
    conns: AtomicUsize,
    messages: AtomicUsize,
}

/// `hang_up_after_message`: encerra a conexão logo depois de aceitar uma mensagem (como um servidor que
/// derruba conexões ociosas), sem dizer nada ao cliente.
fn start(hang_up_after_message: bool) -> (ImapAccount, Arc<Stats>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stats = Arc::new(Stats::default());
    let st = stats.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            st.conns.fetch_add(1, Ordering::SeqCst);
            let st = st.clone();
            std::thread::spawn(move || serve(conn, st, hang_up_after_message));
        }
    });
    let acc = ImapAccount {
        id: format!("smtp-{port}"),
        host: "127.0.0.1".into(),
        port: 1,
        security: Security::Plaintext,
        username: "eu@exemplo.com".into(),
        name: None,
        smtp: Some(SmtpServer { host: "127.0.0.1".into(), port, security: Security::Plaintext }),
        auth: AuthMode::Password,
        password: "segredo".into(),
    };
    (acc, stats)
}

fn serve(conn: TcpStream, stats: Arc<Stats>, hang_up: bool) {
    let mut out = conn.try_clone().unwrap();
    let mut input = BufReader::new(conn.try_clone().unwrap());
    out.write_all(b"220 fake ESMTP\r\n").unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let up = line.trim().to_uppercase();
        let reply: &[u8] = if up.starts_with("EHLO") {
            b"250-fake\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n"
        } else if up.starts_with("AUTH") {
            b"235 ok\r\n"
        } else if up.starts_with("DATA") {
            out.write_all(b"354 go\r\n").unwrap();
            loop {
                line.clear();
                if input.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                if line == ".\r\n" {
                    break;
                }
            }
            stats.messages.fetch_add(1, Ordering::SeqCst);
            out.write_all(b"250 queued\r\n").unwrap();
            if hang_up {
                conn.shutdown(Shutdown::Both).ok();
                return;
            }
            continue;
        } else if up.starts_with("QUIT") {
            out.write_all(b"221 bye\r\n").ok();
            return;
        } else {
            b"250 ok\r\n"
        };
        out.write_all(reply).unwrap();
    }
}

fn msg(acc: &ImapAccount, n: usize) -> lettre::Message {
    let out = Outgoing {
        to: "ana@exemplo.com".into(),
        subject: format!("Envio {n}"),
        body: "Olá.".into(),
        html: None,
        cc: String::new(),
        bcc: String::new(),
        attachments: vec![],
        from: None,
    };
    send::build(acc, &out, None).unwrap()
}

#[test]
fn varios_envios_reaproveitam_a_mesma_conexao() {
    let (acc, stats) = start(false);
    for n in 0..4 {
        send::deliver(&acc, &msg(&acc, n)).unwrap();
    }
    assert_eq!(stats.messages.load(Ordering::SeqCst), 4);
    assert_eq!(stats.conns.load(Ordering::SeqCst), 1, "cada envio abriu conexão própria");
    send::forget(&acc.id);
}

#[test]
fn conexao_guardada_que_caiu_reconecta_e_o_envio_nao_se_perde() {
    let (acc, stats) = start(true);
    send::deliver(&acc, &msg(&acc, 0)).unwrap();
    // O servidor derrubou a conexão guardada: o próximo envio falha nela, reconecta e passa.
    send::deliver(&acc, &msg(&acc, 1)).unwrap();
    send::deliver(&acc, &msg(&acc, 2)).unwrap();
    assert_eq!(stats.messages.load(Ordering::SeqCst), 3);
    assert_eq!(stats.conns.load(Ordering::SeqCst), 3);
    send::forget(&acc.id);
}

#[test]
fn trocar_a_senha_nao_reaproveita_a_conexao_antiga() {
    let (acc, stats) = start(false);
    send::deliver(&acc, &msg(&acc, 0)).unwrap();
    let nova = ImapAccount { password: "outra".into(), ..acc.clone() };
    send::deliver(&nova, &msg(&nova, 1)).unwrap();
    assert_eq!(stats.conns.load(Ordering::SeqCst), 2);
    send::forget(&acc.id);
}
