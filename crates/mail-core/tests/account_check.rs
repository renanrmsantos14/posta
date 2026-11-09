//! Teste de conexão do assistente de conta: causas distintas para servidor inexistente, porta fechada e senha recusada.
//! Servidor IMAP falso em processo; sem Docker.

use mail_core::setup::{check_step, FailKind, Step};
use mail_core::{ImapAccount, Security};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

fn acc(host: &str, port: u16) -> ImapAccount {
    ImapAccount { id: "t".into(), host: host.into(), port, security: Security::Plaintext, username: "eu@exemplo.com".into(), name: None, smtp: None, auth: mail_core::AuthMode::Password, password: "errada".into() }
}

/// Aceita uma conexão, cumprimenta e recusa o LOGIN como o Gmail faz.
fn recusa_login() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let mut w = conn.try_clone().unwrap();
            w.write_all(b"* OK [CAPABILITY IMAP4rev1] pronto\r\n").unwrap();
            for line in BufReader::new(conn).lines().map_while(Result::ok) {
                let tag = line.split(' ').next().unwrap_or("a").to_string();
                let reply = if line.contains("CAPABILITY") {
                    format!("* CAPABILITY IMAP4rev1\r\n{tag} OK feito\r\n")
                } else {
                    format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials (Failure)\r\n")
                };
                w.write_all(reply.as_bytes()).unwrap();
            }
        }
    });
    port
}

#[test]
fn senha_recusada_e_auth() {
    let a = acc("127.0.0.1", recusa_login());
    check_step(&a, Step::Connect).unwrap();
    let f = check_step(&a, Step::Login).unwrap_err();
    assert_eq!((f.step, f.kind), (Step::Login, FailKind::Auth), "{}", f.detail);
}

#[test]
fn porta_fechada_e_recusada() {
    let porta = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let f = check_step(&acc("127.0.0.1", porta), Step::Connect).unwrap_err();
    assert_eq!((f.step, f.kind), (Step::Connect, FailKind::Refused), "{}", f.detail);
    let f = check_step(&acc("127.0.0.1", porta), Step::Login).unwrap_err();
    assert_eq!((f.step, f.kind), (Step::Connect, FailKind::Refused), "{}", f.detail);
}

#[test]
fn servidor_inexistente_e_dns() {
    let f = check_step(&acc("imap.dominio-que-nao-existe.invalid", 993), Step::Connect).unwrap_err();
    assert_eq!(f.kind, FailKind::Dns, "{}", f.detail);
}
