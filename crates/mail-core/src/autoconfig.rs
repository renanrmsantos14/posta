//! Autodetecção de servidores a partir do email e teste de conexão com erros em pt-BR.
//! Ordem: tabela embutida, Mozilla ISPDB, MX do domínio (DNS-over-HTTPS), palpite imap./smtp.

use serde::Serialize;
use std::time::Duration;

use crate::imap_sync::{ImapAccount, Security, SmtpServer};

/// Dica ao usuário (ex.: senha de app) com link oficial quando houver.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hint {
    pub text: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Detected {
    /// Nome para mostrar ("Gmail"); vazio quando foi só um palpite.
    pub provider: String,
    /// De onde veio: "tabela", "ispdb", "mx" ou "palpite".
    pub source: String,
    pub imap: SmtpServer,
    pub smtp: SmtpServer,
    /// Login a usar: o email inteiro ou só a parte local, conforme o provedor.
    pub username: String,
    pub hint: Option<Hint>,
}

struct Preset {
    name: &'static str,
    imap: (&'static str, u16),
    smtp: (&'static str, u16, Security),
    hint: Option<(&'static str, Option<&'static str>)>,
}

// Hosts com `{d}` usam o domínio do email.
const GMAIL: Preset = Preset { name: "Gmail", imap: ("imap.gmail.com", 993), smtp: ("smtp.gmail.com", 465, Security::Tls),
    hint: Some(("O Gmail exige uma senha de app (precisa da verificação em duas etapas). Crie uma e cole aqui.", Some("https://myaccount.google.com/apppasswords"))) };
const OUTLOOK: Preset = Preset { name: "Outlook / Hotmail", imap: ("outlook.office365.com", 993), smtp: ("smtp-mail.outlook.com", 587, Security::StartTls),
    hint: Some(("Contas pessoais da Microsoft costumam exigir senha de app (ou use Entrar com Microsoft).", Some("https://account.live.com/proofs/AppPassword"))) };
const M365: Preset = Preset { name: "Microsoft 365", imap: ("outlook.office365.com", 993), smtp: ("smtp.office365.com", 587, Security::StartTls),
    hint: Some(("Se a empresa usa autenticação em duas etapas ou desativou o login básico, use Entrar com Microsoft.", None)) };
const YAHOO: Preset = Preset { name: "Yahoo", imap: ("imap.mail.yahoo.com", 993), smtp: ("smtp.mail.yahoo.com", 465, Security::Tls),
    hint: Some(("O Yahoo exige uma senha de app. Gere uma em Segurança da conta.", Some("https://login.yahoo.com/account/security/app-passwords"))) };
const ICLOUD: Preset = Preset { name: "iCloud", imap: ("imap.mail.me.com", 993), smtp: ("smtp.mail.me.com", 587, Security::StartTls),
    hint: Some(("O iCloud exige uma senha específica de app (Entrar e segurança, no ID Apple).", Some("https://account.apple.com/account/manage"))) };
const ZOHO: Preset = Preset { name: "Zoho", imap: ("imap.zoho.com", 993), smtp: ("smtp.zoho.com", 465, Security::Tls),
    hint: Some(("Com verificação em duas etapas, o Zoho exige senha de app. Verifique também se o IMAP está ativo.", Some("https://accounts.zoho.com/home#security/app_password"))) };
const UOL: Preset = Preset { name: "UOL", imap: ("imap.uol.com.br", 993), smtp: ("smtps.uol.com.br", 465, Security::Tls), hint: None };
const BOL: Preset = Preset { name: "BOL", imap: ("imap.bol.com.br", 993), smtp: ("smtps.bol.com.br", 465, Security::Tls), hint: None };
const TERRA: Preset = Preset { name: "Terra", imap: ("imap.terra.com.br", 993), smtp: ("smtp.terra.com.br", 587, Security::StartTls), hint: None };
const AOL: Preset = Preset { name: "AOL", imap: ("imap.aol.com", 993), smtp: ("smtp.aol.com", 465, Security::Tls),
    hint: Some(("O AOL exige uma senha de app.", Some("https://login.aol.com/account/security"))) };
const GMX: Preset = Preset { name: "GMX", imap: ("imap.gmx.com", 993), smtp: ("mail.gmx.com", 465, Security::Tls),
    hint: Some(("Ative o acesso por IMAP/POP3 nas configurações do GMX.", None)) };
const YANDEX: Preset = Preset { name: "Yandex", imap: ("imap.yandex.com", 993), smtp: ("smtp.yandex.com", 465, Security::Tls),
    hint: Some(("O Yandex exige senha de app e IMAP ativo nas configurações.", Some("https://id.yandex.com/security/app-passwords"))) };
const FASTMAIL: Preset = Preset { name: "Fastmail", imap: ("imap.fastmail.com", 993), smtp: ("smtp.fastmail.com", 465, Security::Tls),
    hint: Some(("O Fastmail exige uma senha de app.", Some("https://app.fastmail.com/settings/security/devicekeys/new"))) };
const PROTON: Preset = Preset { name: "Proton Mail", imap: ("127.0.0.1", 1143), smtp: ("127.0.0.1", 1025, Security::StartTls),
    hint: Some(("O Proton Mail só funciona pelo Proton Mail Bridge, que precisa estar instalado e aberto neste computador. Use o usuário e a senha que o Bridge mostra, não a senha da conta.", Some("https://proton.me/mail/bridge"))) };
// Hospedagem de domínio próprio (provedor identificado pelo MX).
const LOCAWEB: Preset = Preset { name: "Locaweb", imap: ("email-ssl.com.br", 993), smtp: ("email-ssl.com.br", 465, Security::Tls), hint: None };
const HOSTINGER: Preset = Preset { name: "Hostinger", imap: ("imap.hostinger.com", 993), smtp: ("smtp.hostinger.com", 465, Security::Tls), hint: None };
const KINGHOST: Preset = Preset { name: "KingHost", imap: ("imap.kinghost.net", 993), smtp: ("smtp.kinghost.net", 587, Security::StartTls), hint: None };
const UMBLER: Preset = Preset { name: "Umbler", imap: ("imap.umbler.com", 993), smtp: ("smtp.umbler.com", 587, Security::StartTls), hint: None };
const HOSTGATOR: Preset = Preset { name: "HostGator", imap: ("mail.{d}", 993), smtp: ("mail.{d}", 465, Security::Tls), hint: None };
const TITAN: Preset = Preset { name: "Titan", imap: ("imap.titan.email", 993), smtp: ("smtp.titan.email", 465, Security::Tls), hint: None };

fn by_domain(d: &str) -> Option<&'static Preset> {
    Some(match d {
        "gmail.com" | "googlemail.com" => &GMAIL,
        "outlook.com" | "outlook.com.br" | "hotmail.com" | "hotmail.com.br" | "live.com" | "msn.com" => &OUTLOOK,
        "office365.com" => &M365,
        "yahoo.com" | "yahoo.com.br" | "ymail.com" => &YAHOO,
        "icloud.com" | "me.com" | "mac.com" => &ICLOUD,
        "zoho.com" | "zohomail.com" => &ZOHO,
        "uol.com.br" => &UOL,
        "bol.com.br" => &BOL,
        "terra.com.br" => &TERRA,
        "aol.com" => &AOL,
        "gmx.com" | "gmx.net" | "gmx.de" => &GMX,
        "yandex.com" | "yandex.ru" => &YANDEX,
        "fastmail.com" | "fastmail.fm" => &FASTMAIL,
        "proton.me" | "protonmail.com" | "pm.me" => &PROTON,
        _ => return None,
    })
}

/// Provedor pelo host do MX.
fn by_mx(host: &str) -> Option<&'static Preset> {
    let h = host.trim_end_matches('.').to_lowercase();
    let has = |s: &str| h == s || h.ends_with(&format!(".{s}"));
    Some(if has("google.com") || has("googlemail.com") {
        &GMAIL
    } else if has("outlook.com") {
        &M365
    } else if has("locaweb.com.br") || has("email-ssl.com.br") {
        &LOCAWEB
    } else if has("hostinger.com") {
        &HOSTINGER
    } else if has("kinghost.net") {
        &KINGHOST
    } else if has("umbler.com") {
        &UMBLER
    } else if has("hostgator.com.br") || has("hostgator.com") {
        &HOSTGATOR
    } else if has("titan.email") {
        &TITAN
    } else if has("zoho.com") {
        &ZOHO
    } else if has("yahoodns.net") {
        &YAHOO
    } else if has("icloud.com") {
        &ICLOUD
    } else if has("fastmail.com") || has("messagingengine.com") {
        &FASTMAIL
    } else {
        return None;
    })
}

fn split(email: &str) -> Option<(&str, String)> {
    let (local, d) = email.trim().rsplit_once('@')?;
    let d = d.trim().to_lowercase();
    (!local.is_empty() && d.contains('.') && !d.contains(' ')).then_some((local, d))
}

fn from_preset(p: &Preset, source: &str, email: &str, domain: &str) -> Detected {
    let sub = |s: &str| s.replace("{d}", domain);
    Detected {
        provider: p.name.into(),
        source: source.into(),
        imap: SmtpServer { host: sub(p.imap.0), port: p.imap.1, security: if p.imap.1 == 993 { Security::Tls } else { Security::StartTls } },
        smtp: SmtpServer { host: sub(p.smtp.0), port: p.smtp.1, security: p.smtp.2 },
        username: email.trim().into(),
        hint: p.hint.map(|(t, u)| Hint { text: t.into(), url: u.map(Into::into) }),
    }
}

fn get_text(url: &str) -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(5))).build().into();
    agent.get(url).call().ok()?.body_mut().read_to_string().ok()
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].trim())
}

/// Bloco `<incomingServer type="imap">…</incomingServer>` (ou outgoing/smtp).
fn server_block<'a>(xml: &'a str, kind: &str, proto: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{kind}Server type=\"{proto}\""))?;
    let end = xml[start..].find(&format!("</{kind}Server>"))? + start;
    Some(&xml[start..end])
}

fn socket(s: &str) -> Security {
    match s.to_uppercase().as_str() {
        "SSL" | "TLS" => Security::Tls,
        "STARTTLS" => Security::StartTls,
        _ => Security::Plaintext,
    }
}

/// Lê a configuração no formato Mozilla (ISPDB / autoconfig do próprio domínio).
pub fn parse_ispdb(xml: &str, email: &str, local: &str, domain: &str) -> Option<Detected> {
    let sub = |s: &str| s.replace("%EMAILDOMAIN%", domain).replace("%EMAILADDRESS%", email).replace("%EMAILLOCALPART%", local);
    let read = |kind: &str, proto: &str| -> Option<(SmtpServer, String)> {
        let b = server_block(xml, kind, proto)?;
        let srv = SmtpServer { host: sub(tag(b, "hostname")?), port: tag(b, "port")?.parse().ok()?, security: socket(tag(b, "socketType")?) };
        Some((srv, sub(tag(b, "username").unwrap_or("%EMAILADDRESS%"))))
    };
    let (imap, username) = read("incoming", "imap")?;
    let (smtp, _) = read("outgoing", "smtp")?;
    let provider = tag(xml, "displayName").filter(|n| !n.contains('%')).unwrap_or("").to_string();
    Some(Detected { provider, source: "ispdb".into(), imap, smtp, username, hint: None })
}

/// Host do MX de menor preferência numa resposta do DNS-over-HTTPS do Google.
pub fn mx_host(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut best: Option<(u32, String)> = None;
    for a in v["Answer"].as_array()? {
        let mut it = a["data"].as_str()?.split_whitespace();
        let (pref, host) = (it.next()?.parse::<u32>().ok()?, it.next()?.to_string());
        if best.as_ref().map_or(true, |(p, _)| pref < *p) {
            best = Some((pref, host));
        }
    }
    best.map(|(_, h)| h)
}

/// Detecta os servidores do email. `None` só se o email for inválido; no pior caso devolve o palpite imap./smtp.
pub fn detect(email: &str) -> Option<Detected> {
    let (local, domain) = split(email)?;
    if let Some(p) = by_domain(&domain) {
        return Some(from_preset(p, "tabela", email, &domain));
    }
    for url in [
        format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
        format!("https://autoconfig.{domain}/mail/config-v1.1.xml"),
    ] {
        if let Some(d) = get_text(&url).and_then(|x| parse_ispdb(&x, email.trim(), local, &domain)) {
            return Some(d);
        }
    }
    let mx = get_text(&format!("https://dns.google/resolve?name={domain}&type=MX")).and_then(|j| mx_host(&j));
    if let Some(p) = mx.and_then(|h| by_mx(&h)) {
        return Some(from_preset(p, "mx", email, &domain));
    }
    Some(Detected {
        provider: String::new(),
        source: "palpite".into(),
        imap: SmtpServer { host: format!("imap.{domain}"), port: 993, security: Security::Tls },
        smtp: SmtpServer { host: format!("smtp.{domain}"), port: 465, security: Security::Tls },
        username: email.trim().into(),
        hint: None,
    })
}

fn auth_hint(email: &str) -> String {
    match split(email).and_then(|(_, d)| by_domain(&d)).and_then(|p| p.hint) {
        Some((t, _)) => t.to_string(),
        None => "Confira o email e a senha; se usa verificação em duas etapas, gere uma senha de app.".into(),
    }
}

/// Traduz o erro bruto de IMAP/SMTP numa mensagem que diz o que fazer.
pub fn explain(raw: &str, host: &str, email: &str) -> String {
    let r = raw.to_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| r.contains(k));
    if has(&["certificate", "invalid peer", "unknownissuer", "notvalidforname"]) {
        format!("O certificado de {host} não é confiável ou não corresponde ao servidor. Confira o servidor e a segurança em Configurações avançadas.")
    } else if has(&["timed out", "timeout", "deadline"]) {
        format!("{host} não respondeu a tempo. Confira o servidor, a porta e a sua internet (um firewall pode estar bloqueando).")
    } else if has(&["failed to lookup", "name or service", "no such host", "dns", "resolve", "nodename"]) {
        format!("Não encontrei o servidor {host}. Confira o endereço em Configurações avançadas ou o email digitado.")
    } else if has(&["refused", "unreachable", "reset by peer", "broken pipe"]) {
        format!("{host} recusou a conexão. Confira a porta e a segurança (TLS/STARTTLS) em Configurações avançadas.")
    } else if has(&["auth", "login", "credential", "password", "535", "534", "no "]) {
        format!("Senha recusada. {}", auth_hint(email))
    } else {
        format!("Não consegui conectar a {host}: {raw}")
    }
}

/// Faz login IMAP e autentica no SMTP, sem enviar nada. Erro já traduzido.
pub fn test_connection(acc: &ImapAccount) -> std::result::Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let a = acc.clone();
    std::thread::spawn(move || {
        let smtp_host = a.smtp().host;
        let r = crate::imap_sync::check_login(&a)
            .map_err(|e| (a.host.clone(), e.to_string()))
            .and_then(|_| {
                let t = crate::send::transport(&a).map_err(|e| (smtp_host.clone(), e.to_string()))?;
                match t.test_connection() {
                    Ok(true) => Ok(()),
                    Ok(false) => Err((smtp_host, "auth: servidor SMTP recusou".to_string())),
                    Err(e) => Err((smtp_host, e.to_string())),
                }
            });
        let _ = tx.send(r);
    });
    match rx.recv_timeout(Duration::from_secs(25)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err((host, raw))) => Err(explain(&raw, &host, &acc.username)),
        Err(_) => Err(explain("timed out", &acc.host, &acc.username)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISPDB: &str = r#"<?xml version="1.0"?>
<clientConfig version="1.1"><emailProvider id="exemplo.com.br">
<displayName>Exemplo Mail</displayName>
<incomingServer type="imap"><hostname>imap.%EMAILDOMAIN%</hostname><port>993</port><socketType>SSL</socketType><username>%EMAILLOCALPART%</username><authentication>password-cleartext</authentication></incomingServer>
<outgoingServer type="smtp"><hostname>smtp.exemplo.com.br</hostname><port>587</port><socketType>STARTTLS</socketType><username>%EMAILLOCALPART%</username></outgoingServer>
</emailProvider></clientConfig>"#;

    #[test]
    fn tabela_gmail_com_dica_de_senha_de_app() {
        let d = detect("ana@gmail.com").unwrap();
        assert_eq!((d.provider.as_str(), d.source.as_str()), ("Gmail", "tabela"));
        assert_eq!((d.imap.host.as_str(), d.imap.port, d.smtp.port), ("imap.gmail.com", 993, 465));
        assert_eq!(d.username, "ana@gmail.com");
        assert!(d.hint.unwrap().url.unwrap().contains("apppasswords"));
    }

    #[test]
    fn tabela_cobre_provedores_comuns_e_proton_avisa_do_bridge() {
        for dom in ["googlemail.com", "hotmail.com", "live.com", "msn.com", "yahoo.com.br", "me.com", "mac.com", "uol.com.br", "bol.com.br", "terra.com.br", "aol.com", "gmx.com", "yandex.com", "fastmail.com", "zoho.com"] {
            assert!(detect(&format!("a@{dom}")).unwrap().source == "tabela", "{dom}");
        }
        let p = detect("a@proton.me").unwrap();
        assert_eq!(p.imap.host, "127.0.0.1");
        assert!(p.hint.unwrap().text.contains("Bridge"));
    }

    #[test]
    fn email_invalido_nao_detecta() {
        assert!(detect("sem-arroba").is_none());
        assert!(detect("a@semponto").is_none());
    }

    #[test]
    fn ispdb_le_servidores_seguranca_e_login_local() {
        let d = parse_ispdb(ISPDB, "joao@exemplo.com.br", "joao", "exemplo.com.br").unwrap();
        assert_eq!(d.provider, "Exemplo Mail");
        assert_eq!(d.imap, SmtpServer { host: "imap.exemplo.com.br".into(), port: 993, security: Security::Tls });
        assert_eq!(d.smtp, SmtpServer { host: "smtp.exemplo.com.br".into(), port: 587, security: Security::StartTls });
        assert_eq!(d.username, "joao");
        assert!(parse_ispdb("<html>404</html>", "a@b.c", "a", "b.c").is_none());
    }

    #[test]
    fn mx_escolhe_menor_preferencia_e_mapeia() {
        let json = r#"{"Answer":[{"data":"20 alt.aspmx.l.google.com."},{"data":"10 aspmx.l.google.com."}]}"#;
        assert_eq!(mx_host(json).unwrap(), "aspmx.l.google.com.");
        assert_eq!(by_mx("aspmx.l.google.com.").unwrap().name, "Gmail");
        assert_eq!(by_mx("empresa-com-br.mail.protection.outlook.com").unwrap().name, "Microsoft 365");
        assert_eq!(by_mx("mx.locaweb.com.br").unwrap().name, "Locaweb");
        assert_eq!(by_mx("mx1.hostinger.com").unwrap().name, "Hostinger");
        assert_eq!(by_mx("mx.titan.email").unwrap().name, "Titan");
        assert!(by_mx("mx.desconhecido.net").is_none());
        let hg = from_preset(by_mx("mx.hostgator.com.br").unwrap(), "mx", "a@x.com.br", "x.com.br");
        assert_eq!(hg.imap.host, "mail.x.com.br");
    }

    /// Rede real: `cargo test -p mail-core -- --ignored --nocapture detecta_na_rede`.
    #[test]
    #[ignore]
    fn detecta_na_rede() {
        for e in ["a@gmail.com", "a@outlook.com", "a@uol.com.br", "a@web.de", "a@ig.com.br", "a@betinhos.com.br"] {
            let d = detect(e).unwrap();
            println!("{e}: {} [{}] imap {}:{} {:?} smtp {}:{} {:?} login {}", d.provider, d.source, d.imap.host, d.imap.port, d.imap.security, d.smtp.host, d.smtp.port, d.smtp.security, d.username);
        }
    }

    #[test]
    fn erros_traduzidos() {
        assert!(explain("No Response: [AUTHENTICATIONFAILED] Invalid credentials", "imap.gmail.com", "a@gmail.com").contains("senha de app"));
        assert!(explain("failed to lookup address information", "imap.x.com", "a@x.com").contains("Não encontrei"));
        assert!(explain("connection timed out", "imap.x.com", "a@x.com").contains("a tempo"));
        assert!(explain("invalid peer certificate: UnknownIssuer", "imap.x.com", "a@x.com").contains("certificado"));
        assert!(explain("Connection refused", "imap.x.com", "a@x.com").contains("recusou"));
    }
}
