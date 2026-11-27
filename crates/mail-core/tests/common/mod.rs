//! Apoio dos testes de integração contra um servidor IMAP real (`--features imap-it`).
//! Só contas sintéticas no servidor local (`POSTA_IMAP_TEST=host:porta`, ver `pnpm test:imap`).
#![allow(dead_code)]

use imap::{ClientBuilder, ConnectionMode};
use mail_core::{AuthMode, ImapAccount, Security};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Endereço do servidor de teste. Com a feature ligada e sem servidor, o teste FALHA (não passa em branco).
pub fn server() -> (String, u16) {
    let addr = std::env::var("POSTA_IMAP_TEST")
        .expect("defina POSTA_IMAP_TEST=127.0.0.1:3143 (ou rode `pnpm test:imap`, que sobe o GreenMail)");
    let (host, port) = addr.split_once(':').expect("POSTA_IMAP_TEST deve ser host:porta");
    (host.to_string(), port.parse().expect("porta inválida em POSTA_IMAP_TEST"))
}

/// Conta nova e única no servidor (o GreenMail cria a caixa no primeiro login).
pub fn account(tag: &str) -> ImapAccount {
    let (host, port) = server();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let user = format!("{tag}{nanos}@localhost");
    ImapAccount {
        id: user.clone(),
        host,
        port,
        security: Security::Plaintext,
        username: user.clone(),
        name: None,
        smtp: None,
        auth: AuthMode::Password,
        password: user,
    }
}

/// Sessão IMAP crua (sem passar pelo mail-core) para preparar e alterar o servidor.
pub fn session(acc: &ImapAccount) -> imap::Session<imap::Connection> {
    ClientBuilder::new(acc.host.as_str(), acc.port)
        .mode(ConnectionMode::Plaintext)
        .connect()
        .unwrap()
        .login(&acc.username, &acc.password)
        .map_err(|(e, _)| e)
        .unwrap()
}

/// Mensagem simples em UTF-8; `subject` já vai como escrito (ASCII ou encoded-word).
pub fn raw(i: usize, subject: &str) -> Vec<u8> {
    format!(
        "From: \"Ana Souza\" <ana@exemplo.com>\r\nTo: eu@exemplo.com\r\n\
         Subject: {subject}\r\nDate: Sat, 7 Nov 2026 10:{:02}:00 -0300\r\n\
         Message-ID: <it-{i}-{}@exemplo.com>\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\nCorpo da mensagem {i}, translado Guarulhos.\r\n",
        i % 60,
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    )
    .into_bytes()
}

/// Proxy TCP entre o cliente e o servidor, que corta conexões sob comando: derrubar tudo agora
/// ou depois de N bytes enviados pelo servidor (conexão cortada no meio de um FETCH).
pub struct Proxy {
    pub port: u16,
    conns: Arc<Mutex<Vec<(TcpStream, TcpStream)>>>,
    sent: Arc<AtomicU64>,
    limit: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    validity: Arc<AtomicU64>,
}

impl Proxy {
    pub fn start() -> Proxy {
        let (host, port) = server();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let me = listener.local_addr().unwrap().port();
        let conns: Arc<Mutex<Vec<(TcpStream, TcpStream)>>> = Arc::default();
        let (sent, limit, stop) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(u64::MAX)), Arc::new(AtomicBool::new(false)));
        let validity = Arc::new(AtomicU64::new(0));
        {
            let (conns, sent, limit, stop, validity) = (conns.clone(), sent.clone(), limit.clone(), stop.clone(), validity.clone());
            std::thread::spawn(move || {
                for client in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(client) = client else { continue };
                    let Ok(upstream) = TcpStream::connect((host.as_str(), port)) else { continue };
                    conns.lock().unwrap().push((client.try_clone().unwrap(), upstream.try_clone().unwrap()));
                    let (mut c_in, mut u_out) = (client.try_clone().unwrap(), upstream.try_clone().unwrap());
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 16 * 1024];
                        while let Ok(n) = c_in.read(&mut buf) {
                            if n == 0 || u_out.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                        u_out.shutdown(Shutdown::Both).ok();
                    });
                    let (mut u_in, mut c_out) = (upstream, client);
                    let (sent, limit, validity) = (sent.clone(), limit.clone(), validity.clone());
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 16 * 1024];
                        while let Ok(n) = u_in.read(&mut buf) {
                            if n == 0 {
                                break;
                            }
                            // Passa só o que cabe no orçamento e derruba as duas pontas.
                            let forced = validity.load(Ordering::SeqCst);
                            let data = if forced > 0 { force_validity(&buf[..n], forced) } else { buf[..n].to_vec() };
                            let n = data.len();
                            let buf = &data[..];
                            let before = sent.fetch_add(n as u64, Ordering::SeqCst);
                            let cap = limit.load(Ordering::SeqCst);
                            let pass = if before >= cap { 0 } else { (cap - before).min(n as u64) as usize };
                            if pass > 0 && c_out.write_all(&buf[..pass]).is_err() {
                                break;
                            }
                            if pass < n {
                                break;
                            }
                        }
                        c_out.shutdown(Shutdown::Both).ok();
                        u_in.shutdown(Shutdown::Both).ok();
                    });
                }
            });
        }
        Proxy { port: me, conns, sent, limit, stop, validity }
    }

    /// Conta que passa pelo proxy.
    pub fn via(&self, acc: &ImapAccount) -> ImapAccount {
        ImapAccount { host: "127.0.0.1".into(), port: self.port, ..acc.clone() }
    }

    /// Derruba todas as conexões abertas agora.
    pub fn cut_all(&self) {
        for (c, u) in self.conns.lock().unwrap().drain(..) {
            c.shutdown(Shutdown::Both).ok();
            u.shutdown(Shutdown::Both).ok();
        }
    }

    /// Corta a conexão depois de `n` bytes a mais vindos do servidor.
    pub fn cut_after(&self, n: u64) {
        self.limit.store(self.sent.load(Ordering::SeqCst) + n, Ordering::SeqCst);
    }

    /// Faz o servidor "anunciar" outra UIDVALIDITY nos SELECT (0 desliga). O GreenMail deriva o valor do nome da pasta
    /// e não permite mudá-lo, então é o proxy que simula o servidor que recriou a pasta.
    pub fn force_uidvalidity(&self, n: u64) {
        self.validity.store(n, Ordering::SeqCst);
    }

    /// Volta a deixar passar tudo.
    pub fn heal(&self) {
        self.limit.store(u64::MAX, Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.cut_all();
        TcpStream::connect_timeout(&format!("127.0.0.1:{}", self.port).parse().unwrap(), Duration::from_millis(200)).ok();
    }
}

/// Troca `[UIDVALIDITY n]` por `[UIDVALIDITY novo]` no trecho (linha de texto: o tamanho pode mudar).
fn force_validity(chunk: &[u8], new: u64) -> Vec<u8> {
    const KEY: &[u8] = b"[UIDVALIDITY ";
    let Some(at) = chunk.windows(KEY.len()).position(|w| w.eq_ignore_ascii_case(KEY)) else { return chunk.to_vec() };
    let start = at + KEY.len();
    let digits = chunk[start..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut out = chunk[..start].to_vec();
    out.extend_from_slice(new.to_string().as_bytes());
    out.extend_from_slice(&chunk[start + digits..]);
    out
}
