//! Cancelar inscrição de listas e boletins: lê `List-Unsubscribe` (RFC 2369) e `List-Unsubscribe-Post` (RFC 8058).
//! Só interpreta e executa o que o usuário pediu; quem decide cancelar é sempre um clique dele.

use serde::Serialize;

use crate::session::Live;
use crate::{Error, Result};

/// O que o cabeçalho oferece para cancelar a inscrição.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unsubscribe {
    /// Endereço web (`https` ou `http`) para abrir no navegador.
    pub url: Option<String>,
    /// Destinatário do email de cancelamento (`mailto:`).
    pub mailto: Option<String>,
    /// Cancelamento com um POST direto: exige URL `https` e `List-Unsubscribe-Post: List-Unsubscribe=One-Click`.
    pub one_click: bool,
}

/// Email de cancelamento montado a partir de um `mailto:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubMail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Bloco de cabeçalhos desdobrado (linhas continuadas viram uma só), como pares `(nome em minúsculas, valor)`.
fn headers(raw: &[u8]) -> Vec<(String, String)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").or_else(|| raw.windows(2).position(|w| w == b"\n\n")).unwrap_or(raw.len());
    let mut out: Vec<(String, String)> = Vec::new();
    for line in String::from_utf8_lossy(&raw[..end]).lines() {
        if line.starts_with([' ', '\t']) {
            if let Some((_, v)) = out.last_mut() {
                v.push(' ');
                v.push_str(line.trim());
            }
        } else if let Some((n, v)) = line.split_once(':') {
            out.push((n.trim().to_lowercase(), v.trim().to_string()));
        }
    }
    out
}

/// Baixa só os cabeçalhos de cancelamento da mensagem (sem corpo nem anexos) numa sessão já aberta.
pub fn fetch_on(live: &mut Live, mailbox: &str, uid: u32) -> Result<Option<Unsubscribe>> {
    live.s.select(mailbox)?;
    let fetches = live.s.uid_fetch(uid.to_string(), "BODY.PEEK[HEADER.FIELDS (LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST)]")?;
    Ok(fetches.iter().find_map(|f| f.header()).and_then(parse))
}

/// Itens entre `<...>` de um valor de `List-Unsubscribe`.
fn items(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(a) = rest.find('<') {
        let Some(b) = rest[a..].find('>') else { break };
        out.push(rest[a + 1..a + b].trim());
        rest = &rest[a + b + 1..];
    }
    out
}

fn scheme_is(item: &str, scheme: &str) -> bool {
    item.get(..scheme.len()).is_some_and(|s| s.eq_ignore_ascii_case(scheme))
}

/// Lê o cabeçalho da mensagem bruta (ou só do bloco de cabeçalhos). `None`: a mensagem não oferece cancelamento utilizável.
pub fn parse(raw: &[u8]) -> Option<Unsubscribe> {
    let h = headers(raw);
    let value = h.iter().find(|(n, _)| n == "list-unsubscribe")?.1.clone();
    let list = items(&value);
    // Prefere https; aceita http só para abrir no navegador.
    let https = list.iter().find(|i| scheme_is(i, "https://")).map(|s| s.to_string());
    let url = https.clone().or_else(|| list.iter().find(|i| scheme_is(i, "http://")).map(|s| s.to_string()));
    let mailto = list.iter().find(|i| scheme_is(i, "mailto:")).map(|s| s.to_string());
    if url.is_none() && mailto.is_none() {
        return None;
    }
    let one_click = https.is_some()
        && h.iter().any(|(n, v)| n == "list-unsubscribe-post" && v.to_lowercase().replace(' ', "") == "list-unsubscribe=one-click");
    Some(Unsubscribe { url, mailto, one_click })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `mailto:destino?subject=...&body=...` em email pronto para enviar. Assunto e corpo padrão: "unsubscribe".
pub fn parse_mailto(m: &str) -> Option<UnsubMail> {
    let rest = m.get(7..).filter(|_| scheme_is(m, "mailto:"))?;
    let (to, query) = rest.split_once('?').unwrap_or((rest, ""));
    let to = percent_decode(to);
    // Sem quebra de linha nem vírgulas: um único destinatário, sem injetar cabeçalhos.
    if to.is_empty() || !to.contains('@') || to.contains(['\r', '\n', ',', ';', '<', '>']) {
        return None;
    }
    let (mut subject, mut body) = (String::new(), String::new());
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else { continue };
        match k.to_lowercase().as_str() {
            "subject" => subject = percent_decode(v).replace(['\r', '\n'], " "),
            "body" => body = percent_decode(v),
            _ => {}
        }
    }
    let fallback = |s: String| if s.trim().is_empty() { "unsubscribe".to_string() } else { s };
    Some(UnsubMail { to, subject: fallback(subject), body: fallback(body) })
}

/// POST `List-Unsubscribe=One-Click` (RFC 8058) na URL https. Sucesso só com resposta 2xx; 3xx conta como falha (sem seguir redirecionamento).
pub fn post_one_click(url: &str) -> Result<()> {
    if !scheme_is(url, "https://") {
        return Err(Error::Invalid("o cancelamento direto exige um endereço https".into()));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        // RFC 8058: não seguir redirecionamentos (evita SSRF para endereços internos).
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .into();
    let resp = agent
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send("List-Unsubscribe=One-Click")
        .map_err(|e| Error::Invalid(format!("não consegui falar com o servidor da lista: {e}")))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(Error::Invalid(format!("o servidor da lista recusou o cancelamento (HTTP {status})")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(lines: &str) -> Vec<u8> {
        format!("From: a@b.c\r\n{lines}\r\nSubject: x\r\n\r\ncorpo <https://nao-e-cabecalho>\r\n").into_bytes()
    }

    #[test]
    fn ausente() {
        assert_eq!(parse(&h("")), None);
        // O corpo não conta, nem itens sem esquema conhecido.
        assert_eq!(parse(&h("List-Unsubscribe: <ftp://x.com/u>")), None);
    }

    #[test]
    fn so_mailto() {
        let u = parse(&h("List-Unsubscribe: <mailto:sair@lista.com?subject=sair>")).unwrap();
        assert_eq!(u, Unsubscribe { url: None, mailto: Some("mailto:sair@lista.com?subject=sair".into()), one_click: false });
    }

    #[test]
    fn so_https_sem_post() {
        let u = parse(&h("list-unsubscribe: <https://lista.com/u?id=1>")).unwrap();
        assert_eq!(u.url.as_deref(), Some("https://lista.com/u?id=1"));
        assert!(!u.one_click && u.mailto.is_none());
    }

    #[test]
    fn one_click_exige_https_e_post() {
        let raw = h("List-Unsubscribe: <https://lista.com/u>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click");
        assert!(parse(&raw).unwrap().one_click);
        // POST declarado, mas só http: não é one-click.
        let raw = h("List-Unsubscribe: <http://lista.com/u>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click");
        let u = parse(&raw).unwrap();
        assert!(!u.one_click);
        assert_eq!(u.url.as_deref(), Some("http://lista.com/u"));
    }

    #[test]
    fn varios_itens_e_cabecalho_dobrado() {
        let raw = h("List-Unsubscribe: <mailto:sair@lista.com>,\r\n <https://lista.com/u>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click");
        let u = parse(&raw).unwrap();
        assert_eq!(u.mailto.as_deref(), Some("mailto:sair@lista.com"));
        assert_eq!(u.url.as_deref(), Some("https://lista.com/u"));
        assert!(u.one_click);
    }

    #[test]
    fn mailto_com_assunto_e_corpo() {
        let m = parse_mailto("mailto:sair@lista.com?subject=Cancelar%20inscri%C3%A7%C3%A3o&body=por%20favor").unwrap();
        assert_eq!(m, UnsubMail { to: "sair@lista.com".into(), subject: "Cancelar inscrição".into(), body: "por favor".into() });
        let m = parse_mailto("mailto:sair@lista.com").unwrap();
        assert_eq!((m.subject.as_str(), m.body.as_str()), ("unsubscribe", "unsubscribe"));
    }

    #[test]
    fn mailto_invalido() {
        assert_eq!(parse_mailto("mailto:"), None);
        assert_eq!(parse_mailto("https://x.com"), None);
        assert_eq!(parse_mailto("mailto:a@b.com%0D%0ABcc:c@d.com"), None);
        assert_eq!(parse_mailto("mailto:a@b.com,c@d.com"), None);
    }

    #[test]
    fn post_recusa_http() {
        assert!(post_one_click("http://lista.com/u").is_err());
    }
}
