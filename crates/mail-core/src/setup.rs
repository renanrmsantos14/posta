//! Assistente de nova conta: descobre os servidores do domínio e testa a conexão em etapas, com a causa da falha.
//! Só o domínio sai da máquina (autoconfig do Thunderbird e DNS); a senha nunca passa por aqui fora dos testes.

use crate::{imap_sync, send, Error, ImapAccount, Security};
use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

/// Prazo das consultas de descoberta: se a rede ou o serviço demora, o assistente segue com o modo manual.
const DETECT_TIMEOUT: Duration = Duration::from_secs(4);
/// Prazo de cada etapa do teste (conexão TCP, login IMAP, login SMTP).
const STEP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

/// Quem hospeda o email do domínio, pelo registro MX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Hosting {
    Microsoft,
    Google,
    Other,
}

#[derive(Debug, Default, Serialize)]
pub struct Detection {
    pub imap: Option<Server>,
    pub smtp: Option<Server>,
    pub hosting: Option<Hosting>,
}

/// Lê o `clientConfig` do ISPDB do Thunderbird: primeiro servidor IMAP e primeiro SMTP.
pub fn parse_autoconfig(xml: &str, domain: &str) -> Option<(Server, Server)> {
    let imap = server_in(xml, "incomingServer", "imap", domain)?;
    let smtp = server_in(xml, "outgoingServer", "smtp", domain)?;
    Some((imap, smtp))
}

fn server_in(xml: &str, tag: &str, kind: &str, domain: &str) -> Option<Server> {
    let mut rest = xml;
    while let Some(start) = rest.find(&format!("<{tag}")) {
        let after = &rest[start..];
        let end = after.find(&format!("</{tag}>"))?;
        let block = &after[..end];
        rest = &after[end..];
        if !block.split('>').next()?.contains(&format!("\"{kind}\"")) {
            continue;
        }
        let host = text_of(block, "hostname")?.replace("%EMAILDOMAIN%", domain);
        let port: u16 = text_of(block, "port")?.parse().ok()?;
        let security = match text_of(block, "socketType")?.to_ascii_uppercase().as_str() {
            "SSL" => Security::Tls,
            "STARTTLS" => Security::StartTls,
            _ => continue, // sem criptografia: não oferecemos
        };
        if host.is_empty() || host.contains('%') {
            continue;
        }
        return Some(Server { host, port, security });
    }
    None
}

fn text_of(block: &str, tag: &str) -> Option<String> {
    let open = block.find(&format!("<{tag}>"))? + tag.len() + 2;
    let len = block[open..].find(&format!("</{tag}>"))?;
    Some(block[open..open + len].trim().to_string())
}

/// Classifica os servidores MX (`aspmx.l.google.com.`, `x.mail.protection.outlook.com`).
pub fn classify_mx(hosts: &[String]) -> Option<Hosting> {
    let hosts: Vec<String> = hosts.iter().map(|h| h.trim().trim_end_matches('.').to_ascii_lowercase()).collect();
    if hosts.is_empty() {
        return None;
    }
    if hosts.iter().any(|h| h.ends_with(".mail.protection.outlook.com")) {
        Some(Hosting::Microsoft)
    } else if hosts.iter().any(|h| h.ends_with("google.com") || h.ends_with("googlemail.com")) {
        Some(Hosting::Google)
    } else {
        Some(Hosting::Other)
    }
}

fn valid_domain(d: &str) -> bool {
    !d.is_empty() && d.len() <= 253 && d.contains('.') && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(DETECT_TIMEOUT)).build().into()
}

fn fetch_autoconfig(domain: &str) -> Option<(Server, Server)> {
    let mut resp = agent().get(&format!("https://autoconfig.thunderbird.net/v1.1/{domain}")).call().ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    parse_autoconfig(&resp.body_mut().read_to_string().ok()?, domain)
}

/// MX por DNS sobre HTTPS (Cloudflare): não há resolvedor de MX na biblioteca padrão.
fn fetch_mx(domain: &str) -> Option<Hosting> {
    let mut resp = agent()
        .get(&format!("https://cloudflare-dns.com/dns-query?name={domain}&type=MX"))
        .header("accept", "application/dns-json")
        .call()
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&resp.body_mut().read_to_string().ok()?).ok()?;
    let hosts: Vec<String> = v["Answer"]
        .as_array()?
        .iter()
        .filter(|a| a["type"] == 15)
        .filter_map(|a| a["data"].as_str()?.split_whitespace().last().map(str::to_string))
        .collect();
    classify_mx(&hosts)
}

/// Descobre servidores e hospedagem do domínio. Falha de rede ou serviço vira campo vazio, nunca erro.
pub fn detect(domain: &str) -> Detection {
    let domain = domain.trim().to_ascii_lowercase();
    if !valid_domain(&domain) {
        return Detection::default();
    }
    let d2 = domain.clone();
    let mx = std::thread::spawn(move || fetch_mx(&d2));
    let auto = fetch_autoconfig(&domain);
    let hosting = mx.join().ok().flatten();
    let (imap, smtp) = auto.unzip();
    Detection { imap, smtp, hosting }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Connect,
    Login,
    Smtp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailKind {
    /// Nome do servidor não existe.
    Dns,
    /// Sem resposta no prazo.
    Timeout,
    /// Porta fechada.
    Refused,
    /// Certificado ou criptografia.
    Tls,
    /// Usuário ou senha recusados.
    Auth,
    /// A conta não permite IMAP.
    ImapDisabled,
    /// Sem rede no computador.
    Offline,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct CheckFailure {
    pub step: Step,
    pub kind: FailKind,
    /// Mensagem técnica original, para o botão Detalhes.
    pub detail: String,
}

/// Traduz o texto de um erro (minúsculas) em causa. Pura para ser testável.
pub fn classify_text(text: &str) -> FailKind {
    let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
    if has(&["certificate", "certificado", "handshake", "tls", "ssl", "invalid peer", "unknownissuer", "notvalidforname"]) && !has(&["authenticationfailed"]) {
        FailKind::Tls
    } else if text.contains("imap") && has(&["disabled", "not enabled", "desativ", "not allowed", "access denied by admin"]) {
        FailKind::ImapDisabled
    } else if has(&["failed to lookup", "name or service not known", "no such host", "nodename nor servname", "temporary failure in name resolution", "dns", "could not resolve", "no address associated"]) {
        FailKind::Dns
    } else if has(&["timed out", "timeout", "deadline"]) {
        FailKind::Timeout
    } else if has(&["refused", "recusou a conexão"]) {
        FailKind::Refused
    } else if has(&["network is unreachable", "network unreachable", "no route to host"]) {
        FailKind::Offline
    } else if has(&["authenticationfailed", "invalid credentials", "login failed", "authentication failed", "authenticate failed", "invalid login", "535", "username and password not accepted", "no response", "bad credentials", "invalid password", "auth"]) {
        FailKind::Auth
    } else {
        FailKind::Unknown
    }
}

fn classify_io(e: &std::io::Error) -> FailKind {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::TimedOut | K::WouldBlock => FailKind::Timeout,
        K::ConnectionRefused => FailKind::Refused,
        K::NetworkUnreachable | K::HostUnreachable => FailKind::Offline,
        _ => classify_text(&e.to_string().to_ascii_lowercase()),
    }
}

/// Causa de um erro do mail-core: usa o erro de E/S no fim da cadeia quando houver, senão o texto.
pub fn classify(e: &Error) -> FailKind {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(c) = cur {
        if let Some(io) = c.downcast_ref::<std::io::Error>() {
            let k = classify_io(io);
            if k != FailKind::Unknown {
                return k;
            }
        }
        cur = c.source();
    }
    // Recusa do servidor IMAP (`NO`) ou SMTP permanente é senha, a menos que o texto diga outra coisa.
    let text = e.to_string().to_ascii_lowercase();
    match (classify_text(&text), e) {
        (FailKind::Unknown, Error::Imap(imap::Error::No(_))) => FailKind::Auth,
        (FailKind::Unknown, Error::Smtp(s)) if s.is_permanent() => FailKind::Auth,
        (k, _) => k,
    }
}

fn fail(step: Step, e: &Error) -> CheckFailure {
    let kind = classify(e);
    // Falha de criptografia ou de alcance é do servidor, mesmo vista no login.
    let step = if step == Step::Login && !matches!(kind, FailKind::Auth | FailKind::ImapDisabled | FailKind::Unknown) { Step::Connect } else { step };
    CheckFailure { step, kind, detail: e.to_string() }
}

/// Roda a etapa numa thread com prazo: servidor mudo não pode prender o assistente.
fn within<T: Send + 'static>(step: Step, f: impl FnOnce() -> Result<T, Error> + Send + 'static) -> Result<T, CheckFailure> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(STEP_TIMEOUT) {
        Ok(r) => r.map_err(|e| fail(step, &e)),
        Err(_) => Err(CheckFailure { step, kind: FailKind::Timeout, detail: "sem resposta do servidor no prazo".into() }),
    }
}

/// Executa uma etapa do teste sem salvar nada e sem enviar email.
pub fn check_step(acc: &ImapAccount, step: Step) -> Result<(), CheckFailure> {
    let acc = acc.clone();
    match step {
        Step::Connect => within(step, move || {
            let io = |e| Error::Imap(imap::Error::Io(e));
            let addr = (acc.host.as_str(), acc.port).to_socket_addrs().map_err(io)?.next().ok_or_else(|| Error::Invalid("failed to lookup address".into()))?;
            TcpStream::connect_timeout(&addr, STEP_TIMEOUT).map(drop).map_err(io)
        }),
        Step::Login => within(step, move || imap_sync::check_login(&acc)),
        Step::Smtp => within(step, move || send::check_smtp(&acc)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GMAIL_LIKE: &str = r#"<?xml version="1.0"?>
<clientConfig version="1.1"><emailProvider id="exemplo.org"><domain>exemplo.org</domain>
<incomingServer type="pop3"><hostname>pop.exemplo.org</hostname><port>995</port><socketType>SSL</socketType></incomingServer>
<incomingServer type="imap"><hostname>imap.%EMAILDOMAIN%</hostname><port>143</port><socketType>plain</socketType></incomingServer>
<incomingServer type="imap"><hostname>mail.exemplo.org</hostname><port>143</port><socketType>STARTTLS</socketType><username>%EMAILADDRESS%</username></incomingServer>
<outgoingServer type="smtp"><hostname>smtp.exemplo.org</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>
</emailProvider></clientConfig>"#;

    #[test]
    fn ispdb_pega_o_primeiro_imap_criptografado_e_o_smtp() {
        let (imap, smtp) = parse_autoconfig(GMAIL_LIKE, "exemplo.org").unwrap();
        assert_eq!(imap, Server { host: "mail.exemplo.org".into(), port: 143, security: Security::StartTls });
        assert_eq!(smtp, Server { host: "smtp.exemplo.org".into(), port: 465, security: Security::Tls });
    }

    #[test]
    fn ispdb_troca_emaildomain_e_ignora_lixo() {
        let xml = r#"<incomingServer type="imap"><hostname>imap.%EMAILDOMAIN%</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
<outgoingServer type="smtp"><hostname>smtp.%EMAILDOMAIN%</hostname><port>587</port><socketType>STARTTLS</socketType></outgoingServer>"#;
        let (imap, smtp) = parse_autoconfig(xml, "x.com.br").unwrap();
        assert_eq!(imap.host, "imap.x.com.br");
        assert_eq!(smtp.security, Security::StartTls);
        assert!(parse_autoconfig("<html>404</html>", "x.com").is_none());
        assert!(parse_autoconfig(r#"<incomingServer type="imap"><hostname>a</hostname><port>x</port></incomingServer>"#, "x.com").is_none());
    }

    fn mx(l: &[&str]) -> Option<Hosting> {
        classify_mx(&l.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn mx_reconhece_microsoft_google_e_outros() {
        assert_eq!(mx(&["empresa-com-br.mail.protection.outlook.com."]), Some(Hosting::Microsoft));
        assert_eq!(mx(&["aspmx.l.google.com.", "alt1.aspmx.l.google.com."]), Some(Hosting::Google));
        assert_eq!(mx(&["smtp.google.com"]), Some(Hosting::Google));
        assert_eq!(mx(&["ASPMX.GOOGLEMAIL.COM"]), Some(Hosting::Google));
        assert_eq!(mx(&["mx.locaweb.com.br."]), Some(Hosting::Other));
        assert_eq!(mx(&[]), None);
    }

    #[test]
    fn dominio_invalido_nao_sai_da_maquina() {
        assert!(!valid_domain("a b.com") && !valid_domain("x/../y.com") && !valid_domain("semponto"));
        assert!(valid_domain("minha-empresa.com.br"));
        assert!(detect("x/y").imap.is_none());
    }

    #[test]
    fn traduz_erros_em_causas() {
        for (text, kind) in [
            ("failed to lookup address information: name or service not known", FailKind::Dns),
            ("connection refused (os error 111)", FailKind::Refused),
            ("connection timed out", FailKind::Timeout),
            ("invalid peer certificate: unknownissuer", FailKind::Tls),
            ("no response: [authenticationfailed] invalid credentials (failure)", FailKind::Auth),
            ("no response: login failed.", FailKind::Auth),
            ("no response: [alert] your account is not enabled for imap use.", FailKind::ImapDisabled),
            ("network is unreachable (os error 101)", FailKind::Offline),
            ("algo inesperado", FailKind::Unknown),
        ] {
            assert_eq!(classify_text(text), kind, "{text}");
        }
    }
}
