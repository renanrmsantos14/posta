//! Conexão IMAP com prazos. O `ClientBuilder::connect` do crate `imap` abre o TCP sem timeout de conexão,
//! leitura nem escrita: depois de hibernar ou trocar de rede a sessão morre em silêncio e a próxima leitura
//! fica presa por minutos segurando o slot. Aqui a conexão é montada à mão, com prazo em cada etapa.

use imap::extensions::idle::SetReadTimeout;
use imap::{Client, Connection};
use rustls_connector::{rustls::{ClientConfig, RootCertStore}, rustls_native_certs::load_native_certs, RustlsConnector, TlsStream};
use socket2::{SockRef, TcpKeepalive};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::Duration;

use crate::imap_sync::Security;

/// Prazo para abrir a conexão TCP (por endereço resolvido).
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Prazo de cada leitura/escrita no socket. Um IDLE usa fatias menores, restauradas a este valor ao terminar.
pub(crate) const IO_TIMEOUT: Duration = Duration::from_secs(20);

/// Raízes de certificado do sistema, as mesmas que o crate `imap` usa; carregadas uma vez.
fn roots() -> &'static RootCertStore {
    static ROOTS: OnceLock<RootCertStore> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let mut store = RootCertStore::empty();
        for cert in load_native_certs().unwrap_or_default() {
            let _ = store.add(cert);
        }
        store
    })
}

/// Transporte (TCP puro ou TLS) cujo `set_read_timeout(None)` volta ao prazo padrão em vez de desligá-lo:
/// o IDLE do `imap` faz `Some(fatia)` e depois `None`, o que removeria o timeout dali em diante.
enum Stream {
    Plain(TcpStream, Duration),
    Tls(Box<TlsStream<TcpStream>>, Duration),
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Plain(s, _) => s,
            Stream::Tls(s, _) => s.get_ref(),
        }
    }
    fn default_timeout(&self) -> Duration {
        match self {
            Stream::Plain(_, t) | Stream::Tls(_, t) => *t,
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(s, _) => s.read(buf),
            Stream::Tls(s, _) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(s, _) => s.write(buf),
            Stream::Tls(s, _) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Plain(s, _) => s.flush(),
            Stream::Tls(s, _) => s.flush(),
        }
    }
}

impl SetReadTimeout for Stream {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::Result<()> {
        let t = timeout.unwrap_or_else(|| self.default_timeout());
        self.tcp().set_read_timeout(Some(t)).map_err(imap::Error::Io)
    }
}

fn open_tcp(host: &str, port: u16, connect_timeout: Duration, io_timeout: Duration) -> io::Result<TcpStream> {
    let mut last = None;
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, connect_timeout) {
            Ok(tcp) => {
                tcp.set_read_timeout(Some(io_timeout))?;
                tcp.set_write_timeout(Some(io_timeout))?;
                // Keepalive do TCP: detecta o par morto (rede trocada, hibernação) sem depender só do IDLE.
                let ka = TcpKeepalive::new().with_time(Duration::from_secs(60)).with_interval(Duration::from_secs(20));
                let _ = SockRef::from(&tcp).set_tcp_keepalive(&ka);
                return Ok(tcp);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("sem endereço para {host}"))))
}

fn tls(host: &str, tcp: TcpStream, io_timeout: Duration) -> imap::Result<Stream> {
    let config = ClientConfig::builder().with_root_certificates(roots().clone()).with_no_client_auth();
    let conn: RustlsConnector = config.into();
    Ok(Stream::Tls(Box::new(conn.connect(host, tcp)?), io_timeout))
}

/// Lê uma linha (até `\n`) byte a byte: sem buffer, nada da camada TLS que vem depois fica retido.
fn read_line(tcp: &mut TcpStream) -> io::Result<String> {
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    while line.last() != Some(&b'\n') {
        if tcp.read(&mut b)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        line.push(b[0]);
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// Lê as linhas da resposta até a linha com a etiqueta; devolve as não etiquetadas e o status final.
fn run_tagged(tcp: &mut TcpStream, tag: &str, command: &str) -> io::Result<(Vec<String>, String)> {
    tcp.write_all(format!("{tag} {command}\r\n").as_bytes())?;
    let mut untagged = Vec::new();
    loop {
        let line = read_line(tcp)?;
        if let Some(rest) = line.strip_prefix(tag).and_then(|r| r.strip_prefix(' ')) {
            return Ok((untagged, rest.trim().to_string()));
        }
        untagged.push(line);
    }
}

/// Saudação, CAPABILITY e STARTTLS em texto puro, como o `upgrade_tls` do `imap`.
fn starttls(tcp: &mut TcpStream) -> imap::Result<()> {
    read_line(tcp)?;
    let (caps, _) = run_tagged(tcp, "p1", "CAPABILITY")?;
    if !caps.iter().any(|l| l.to_ascii_uppercase().split_whitespace().any(|w| w == "STARTTLS")) {
        return Err(imap::Error::StartTlsNotAvailable);
    }
    let (_, status) = run_tagged(tcp, "p2", "STARTTLS")?;
    if !status.to_ascii_uppercase().starts_with("OK") {
        return Err(imap::Error::StartTlsNotAvailable);
    }
    Ok(())
}

/// Abre a conexão IMAP (TLS direto, STARTTLS ou texto puro) já com a saudação lida.
pub(crate) fn connect(host: &str, port: u16, security: &Security) -> imap::Result<Client<Connection>> {
    connect_with(host, port, security, CONNECT_TIMEOUT, IO_TIMEOUT)
}

pub(crate) fn connect_with(host: &str, port: u16, security: &Security, connect_timeout: Duration, io_timeout: Duration) -> imap::Result<Client<Connection>> {
    let tcp = open_tcp(host, port, connect_timeout, io_timeout).map_err(imap::Error::Io)?;
    let stream: Connection = match security {
        Security::Plaintext => Box::new(Stream::Plain(tcp, io_timeout)),
        Security::Tls => Box::new(tls(host, tcp, io_timeout)?),
        Security::StartTls => {
            let mut tcp = tcp;
            starttls(&mut tcp)?;
            let mut upgraded = Client::new(Box::new(tls(host, tcp, io_timeout)?) as Connection);
            // A saudação já foi lida antes do STARTTLS.
            upgraded.greeting_read = true;
            return Ok(upgraded);
        }
    };
    let mut client = Client::new(stream);
    client.read_greeting()?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Instant;

    /// Servidor que aceita e nunca responde: a conexão tem de falhar perto do prazo, como erro de rede.
    #[test]
    fn servidor_mudo_falha_no_prazo_como_erro_de_rede() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let _mudo = std::thread::spawn(move || {
            let _conn = listener.accept();
            std::thread::sleep(Duration::from_secs(5));
        });
        let t = Instant::now();
        let err = connect_with("127.0.0.1", port, &Security::Plaintext, Duration::from_secs(1), Duration::from_secs(1)).err().expect("devia falhar");
        let gasto = t.elapsed();
        assert!(gasto >= Duration::from_millis(900) && gasto < Duration::from_secs(4), "levou {gasto:?}");
        assert!(crate::Error::Imap(err).is_network());
    }

    /// Porta fechada também falha rápido e como erro de rede.
    #[test]
    fn porta_fechada_e_erro_de_rede() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = connect_with("127.0.0.1", port, &Security::Plaintext, Duration::from_secs(1), Duration::from_secs(1)).err().expect("devia falhar");
        assert!(crate::Error::Imap(err).is_network());
    }
}
