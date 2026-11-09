//! OAuth sem internet: troca de código e renovação contra um servidor OAuth falso, e IMAP/SMTP com XOAUTH2
//! contra servidores falsos que só aceitam o token "bom". Rodam sempre, sem Docker.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use mail_core::oauth::{self, Client, Provider, RefreshError};
use mail_core::{imap_sync, send, AuthMode, ImapAccount, Security, SmtpServer};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ---------- servidor OAuth falso ----------

#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
}

/// Responde `replies` em ordem (a última se repete) e guarda os formulários recebidos.
fn fake_token_server(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<HashMap<String, String>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/token", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let form: HashMap<String, String> =
                url::form_urlencoded::parse(&body).map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
            log.lock().unwrap().push(form);
            let r = replies[n.min(replies.len() - 1)].clone();
            write!(
                stream,
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                r.status,
                r.body.len(),
                r.body
            )
            .unwrap();
        }
    });
    (url, seen)
}

fn client(url: &str) -> Client {
    Client { id: "meu-client-id".into(), secret: Some("meu-secret".into()), token_url: Some(url.into()) }
}

fn ok(body: &str) -> Reply {
    Reply { status: 200, body: body.into() }
}

fn err(status: u16, body: &str) -> Reply {
    Reply { status, body: body.into() }
}

#[test]
fn troca_de_codigo_envia_pkce_e_le_email_e_refresh_token() {
    let id_token = format!("h.{}.s", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"email":"ana@gmail.com"}"#));
    let (url, seen) = fake_token_server(vec![ok(&format!(
        r#"{{"access_token":"AT1","refresh_token":"RT1","expires_in":1800,"id_token":"{id_token}"}}"#
    ))]);
    let t = oauth::exchange_code(Provider::Google, &client(&url), "CODIGO", "http://127.0.0.1:5000", "VERIFICADOR").unwrap();
    assert_eq!(t.access_token, "AT1");
    assert_eq!(t.refresh_token.as_deref(), Some("RT1"));
    assert_eq!(t.email.as_deref(), Some("ana@gmail.com"));
    assert_eq!(t.expires_in.as_secs(), 1800);
    let f = &seen.lock().unwrap()[0];
    assert_eq!(f["grant_type"], "authorization_code");
    assert_eq!(f["code"], "CODIGO");
    assert_eq!(f["code_verifier"], "VERIFICADOR");
    assert_eq!(f["redirect_uri"], "http://127.0.0.1:5000");
    assert_eq!(f["client_id"], "meu-client-id");
    assert_eq!(f["client_secret"], "meu-secret");
}

#[test]
fn renovacao_usa_o_refresh_token_e_a_microsoft_manda_o_escopo() {
    let (url, seen) = fake_token_server(vec![ok(r#"{"access_token":"AT2","refresh_token":"RT-NOVO","expires_in":3599}"#)]);
    let t = oauth::refresh(Provider::Microsoft, &client(&url), "RT-VELHO").unwrap();
    assert_eq!((t.access_token.as_str(), t.refresh_token.as_deref()), ("AT2", Some("RT-NOVO")));
    let f = &seen.lock().unwrap()[0];
    assert_eq!(f["grant_type"], "refresh_token");
    assert_eq!(f["refresh_token"], "RT-VELHO");
    assert!(f["scope"].contains("IMAP.AccessAsUser.All") && f["scope"].contains("offline_access"));

    // O Google não gira o refresh token e não recebe escopo na renovação.
    let (url, seen) = fake_token_server(vec![ok(r#"{"access_token":"AT3","expires_in":3599}"#)]);
    let t = oauth::refresh(Provider::Google, &client(&url), "RT").unwrap();
    assert_eq!(t.refresh_token, None);
    assert!(!seen.lock().unwrap()[0].contains_key("scope"));
}

#[test]
fn renovacao_recusada_explica_e_pede_novo_login() {
    let (url, _) = fake_token_server(vec![err(400, r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#)]);
    match oauth::refresh(Provider::Google, &client(&url), "RT").unwrap_err() {
        RefreshError::NeedsLogin(m) => assert!(m.contains("7 dias") && m.contains("Entre de novo"), "{m}"),
        e => panic!("esperava NeedsLogin, veio {e}"),
    }

    let (url, _) = fake_token_server(vec![err(
        400,
        r#"{"error":"invalid_grant","error_description":"AADSTS65001: The user or administrator has not consented to use the application."}"#,
    )]);
    match oauth::refresh(Provider::Microsoft, &client(&url), "RT").unwrap_err() {
        RefreshError::NeedsLogin(m) => assert!(m.contains("administrador"), "{m}"),
        e => panic!("esperava NeedsLogin, veio {e}"),
    }

    let (url, _) = fake_token_server(vec![err(400, r#"{"error":"invalid_grant"}"#)]);
    match oauth::refresh(Provider::Microsoft, &client(&url), "RT").unwrap_err() {
        RefreshError::NeedsLogin(m) => assert!(m.contains("novo login"), "{m}"),
        e => panic!("esperava NeedsLogin, veio {e}"),
    }
}

#[test]
fn falha_do_servidor_ou_de_configuracao_nao_pede_novo_login() {
    let (url, _) = fake_token_server(vec![err(503, "indisponível")]);
    assert!(matches!(oauth::refresh(Provider::Google, &client(&url), "RT").unwrap_err(), RefreshError::Failed(_)));
    let (url, _) = fake_token_server(vec![err(401, r#"{"error":"invalid_client"}"#)]);
    let e = oauth::refresh(Provider::Google, &client(&url), "RT").unwrap_err();
    assert!(matches!(&e, RefreshError::Failed(_)) && e.to_string().contains("invalid_client"), "{e}");
    // Servidor fora do ar: erro de rede, não "entre de novo".
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://127.0.0.1:{}/token", l.local_addr().unwrap().port());
    drop(l);
    assert!(matches!(oauth::refresh(Provider::Google, &client(&dead), "RT").unwrap_err(), RefreshError::Failed(_)));
}

#[test]
fn erro_nunca_ecoa_o_corpo_nem_o_refresh_token() {
    let (url, _) = fake_token_server(vec![err(400, r#"{"error":"invalid_request","error_description":"refresh_token=SEGREDO"}"#)]);
    let e = oauth::refresh(Provider::Google, &client(&url), "SEGREDO").unwrap_err().to_string();
    assert!(!e.contains("SEGREDO"), "{e}");
}

// ---------- IMAP e SMTP falsos com XOAUTH2 ----------

const BOM: &str = "token-bom";

fn token_of(b64: &str) -> String {
    let raw = String::from_utf8(STANDARD.decode(b64.trim()).unwrap()).unwrap();
    raw.split('\u{1}').find_map(|p| p.strip_prefix("auth=Bearer ")).unwrap_or("").to_string()
}

/// IMAP mínimo: só CAPABILITY, AUTHENTICATE XOAUTH2 e LOGOUT.
fn fake_imap(auths: Arc<AtomicUsize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let auths = auths.clone();
            std::thread::spawn(move || imap_conn(stream, auths));
        }
    });
    port
}

fn imap_conn(stream: TcpStream, auths: Arc<AtomicUsize>) {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    w.write_all(b"* OK [CAPABILITY IMAP4rev1 AUTH=XOAUTH2] pronto\r\n").unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line).unwrap() == 0 {
            return;
        }
        let mut parts = line.trim_end().splitn(3, ' ');
        let (tag, cmd) = (parts.next().unwrap().to_string(), parts.next().unwrap_or("").to_ascii_uppercase());
        match cmd.as_str() {
            "CAPABILITY" => write!(w, "* CAPABILITY IMAP4rev1 AUTH=XOAUTH2\r\n{tag} OK feito\r\n").unwrap(),
            "AUTHENTICATE" => {
                auths.fetch_add(1, Ordering::SeqCst);
                w.write_all(b"+ \r\n").unwrap();
                line.clear();
                r.read_line(&mut line).unwrap();
                if token_of(&line) == BOM {
                    write!(w, "{tag} OK autenticado\r\n").unwrap();
                } else {
                    // Desafio de erro do XOAUTH2: o cliente responde vazio e o servidor recusa.
                    w.write_all(b"+ eyJzdGF0dXMiOiI0MDEifQ==\r\n").unwrap();
                    line.clear();
                    r.read_line(&mut line).unwrap();
                    write!(w, "{tag} NO [AUTHENTICATIONFAILED] AUTHENTICATE failed.\r\n").unwrap();
                }
            }
            "LOGOUT" => {
                write!(w, "* BYE ate logo\r\n{tag} OK feito\r\n").unwrap();
                return;
            }
            _ => write!(w, "{tag} BAD comando desconhecido\r\n").unwrap(),
        }
    }
}

/// SMTP mínimo: EHLO e AUTH XOAUTH2.
fn fake_smtp(auths: Arc<AtomicUsize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let auths = auths.clone();
            std::thread::spawn(move || {
                let mut w = stream.try_clone().unwrap();
                let mut r = BufReader::new(stream);
                w.write_all(b"220 fake ESMTP\r\n").unwrap();
                let mut line = String::new();
                loop {
                    line.clear();
                    if r.read_line(&mut line).unwrap() == 0 {
                        return;
                    }
                    let up = line.to_ascii_uppercase();
                    if up.starts_with("EHLO") {
                        w.write_all(b"250-fake\r\n250 AUTH XOAUTH2\r\n").unwrap();
                    } else if let Some(b64) = up.strip_prefix("AUTH XOAUTH2 ").and(line.trim_end().get(13..)) {
                        auths.fetch_add(1, Ordering::SeqCst);
                        if token_of(b64) == BOM {
                            w.write_all(b"235 2.7.0 aceito\r\n").unwrap();
                        } else {
                            w.write_all(b"535 5.7.8 credenciais recusadas\r\n").unwrap();
                        }
                    } else if up.starts_with("QUIT") {
                        w.write_all(b"221 tchau\r\n").unwrap();
                        return;
                    } else {
                        w.write_all(b"250 ok\r\n").unwrap();
                    }
                }
            });
        }
    });
    port
}

fn account(imap: u16, smtp: u16, token: &str) -> ImapAccount {
    ImapAccount {
        id: "ana@gmail.com@imap.gmail.com".into(),
        host: "127.0.0.1".into(),
        port: imap,
        security: Security::Plaintext,
        username: "ana@gmail.com".into(),
        name: None,
        smtp: Some(SmtpServer { host: "127.0.0.1".into(), port: smtp, security: Security::Plaintext }),
        auth: AuthMode::Google,
        password: token.into(),
    }
}

static REFRESHES: AtomicUsize = AtomicUsize::new(0);
static RENEWAL: Mutex<Option<Result<String, String>>> = Mutex::new(None);

/// O refresher é global do processo (registrado uma vez); cada teste decide a resposta em `RENEWAL`.
/// Os testes que usam o refresher não rodam juntos: o `GATE` os serializa.
static GATE: Mutex<()> = Mutex::new(());

fn install_refresher() {
    oauth::set_refresher(|id| {
        assert_eq!(id, "ana@gmail.com@imap.gmail.com");
        REFRESHES.fetch_add(1, Ordering::SeqCst);
        RENEWAL.lock().unwrap().clone().expect("teste não definiu a renovação")
    });
}

#[test]
fn imap_xoauth2_entra_com_o_token_bom_sem_renovar() {
    let _g = GATE.lock().unwrap_or_else(|p| p.into_inner());
    install_refresher();
    let (auths, before) = (Arc::new(AtomicUsize::new(0)), REFRESHES.load(Ordering::SeqCst));
    let acc = account(fake_imap(auths.clone()), 1, BOM);
    imap_sync::check_login(&acc).unwrap();
    assert_eq!(auths.load(Ordering::SeqCst), 1);
    assert_eq!(REFRESHES.load(Ordering::SeqCst), before);
}

#[test]
fn imap_recusado_renova_o_token_e_tenta_de_novo() {
    let _g = GATE.lock().unwrap_or_else(|p| p.into_inner());
    install_refresher();
    *RENEWAL.lock().unwrap() = Some(Ok(BOM.into()));
    let (auths, before) = (Arc::new(AtomicUsize::new(0)), REFRESHES.load(Ordering::SeqCst));
    let acc = account(fake_imap(auths.clone()), 1, "token-vencido");
    imap_sync::check_login(&acc).unwrap();
    assert_eq!(auths.load(Ordering::SeqCst), 2, "uma tentativa recusada e uma aceita");
    assert_eq!(REFRESHES.load(Ordering::SeqCst), before + 1);
}

#[test]
fn imap_com_renovacao_recusada_devolve_a_mensagem_de_novo_login() {
    let _g = GATE.lock().unwrap_or_else(|p| p.into_inner());
    install_refresher();
    *RENEWAL.lock().unwrap() = Some(Err("O Google encerrou o acesso. Entre de novo.".into()));
    let acc = account(fake_imap(Arc::new(AtomicUsize::new(0))), 1, "token-vencido");
    let e = imap_sync::check_login(&acc).unwrap_err().to_string();
    assert!(e.contains("Entre de novo"), "{e}");
}

#[test]
fn imap_que_recusa_mesmo_com_token_novo_nao_entra_em_laco() {
    let _g = GATE.lock().unwrap_or_else(|p| p.into_inner());
    install_refresher();
    *RENEWAL.lock().unwrap() = Some(Ok("outro-token-ruim".into()));
    let (auths, before) = (Arc::new(AtomicUsize::new(0)), REFRESHES.load(Ordering::SeqCst));
    let acc = account(fake_imap(auths.clone()), 1, "token-vencido");
    assert!(imap_sync::check_login(&acc).is_err());
    assert_eq!(auths.load(Ordering::SeqCst), 2);
    assert_eq!(REFRESHES.load(Ordering::SeqCst), before + 1);
}

#[test]
fn smtp_xoauth2_renova_no_535_e_tenta_de_novo() {
    let _g = GATE.lock().unwrap_or_else(|p| p.into_inner());
    install_refresher();
    *RENEWAL.lock().unwrap() = Some(Ok(BOM.into()));
    let (auths, before) = (Arc::new(AtomicUsize::new(0)), REFRESHES.load(Ordering::SeqCst));
    let acc = account(1, fake_smtp(auths.clone()), "token-vencido");
    send::check_smtp(&acc).unwrap();
    assert_eq!(auths.load(Ordering::SeqCst), 2);
    assert_eq!(REFRESHES.load(Ordering::SeqCst), before + 1);
}

#[test]
fn erro_de_rede_nao_dispara_renovacao() {
    let _g = GATE.lock().unwrap_or_else(|p| p.into_inner());
    install_refresher();
    let before = REFRESHES.load(Ordering::SeqCst);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    assert!(imap_sync::check_login(&account(port, 1, BOM)).is_err());
    assert_eq!(REFRESHES.load(Ordering::SeqCst), before);
}
