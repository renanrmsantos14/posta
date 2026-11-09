//! Anexos: lista para a UI, extração sob demanda e imagens `cid:` embutidas no HTML.

use base64::Engine;
use mail_parser::{Message, MessageParser};
use serde::{Deserialize, Serialize};

/// Imagem inline maior que isso não é embutida no HTML (o banco local engordaria).
const MAX_INLINE: usize = 2 * 1024 * 1024;

/// Anexo de uma mensagem recebida (sem o conteúdo, que se busca no servidor ao salvar).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// Posição entre as partes de anexo da mensagem; vale para `extract`.
    pub index: u32,
}

fn mime_of(part: &mail_parser::MessagePart) -> String {
    part.content_type()
        .map(|c| match c.subtype() {
            Some(s) => format!("{}/{s}", c.ctype()),
            None => c.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".into())
}

fn cid_ref(html: &str, cid: &str) -> bool {
    html.to_ascii_lowercase().contains(&format!("cid:{}", cid.to_ascii_lowercase()))
}

fn name_of(part: &mail_parser::MessagePart, index: usize) -> String {
    part.attachment_name().map(str::to_string).unwrap_or_else(|| format!("anexo-{}", index + 1))
}

/// Anexos de verdade: ficam de fora as imagens que o HTML já mostra pelo `cid:`.
pub fn list(msg: &Message, html: &str) -> Vec<Attachment> {
    msg.attachments()
        .enumerate()
        .filter(|(_, p)| !p.content_id().is_some_and(|c| !html.is_empty() && cid_ref(html, c)))
        .map(|(i, p)| Attachment { name: name_of(&p, i), mime: mime_of(&p), size: p.decoded_len() as u64, index: i as u32 })
        .collect()
}

/// Troca `cid:ID` por `data:` URL, para a imagem aparecer no iframe isolado e sem rede.
pub fn inline_cids(msg: &Message, html: String) -> String {
    let mut html = html;
    for p in msg.attachments() {
        let Some(cid) = p.content_id().filter(|c| !c.is_empty()) else { continue };
        let mime = mime_of(&p);
        if !mime.starts_with("image/") || p.decoded_len() > MAX_INLINE || !cid_ref(&html, cid) {
            continue;
        }
        let url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(p.decoded()));
        let needle = format!("cid:{cid}").to_ascii_lowercase();
        let lower = html.to_ascii_lowercase();
        let mut out = String::with_capacity(html.len());
        let mut at = 0;
        while let Some(pos) = lower[at..].find(&needle) {
            out.push_str(&html[at..at + pos]);
            out.push_str(&url);
            at += pos + needle.len();
        }
        out.push_str(&html[at..]);
        html = out;
    }
    html
}

/// Conteúdo do anexo `index` de uma mensagem completa.
pub fn extract(raw: &[u8], index: u32) -> Option<(Attachment, Vec<u8>)> {
    let msg = MessageParser::default().parse(raw)?;
    let (i, p) = msg.attachments().enumerate().find(|(i, _)| *i as u32 == index)?;
    let data = p.decoded().into_owned();
    let att = Attachment { name: name_of(&p, i), mime: mime_of(&p), size: data.len() as u64, index };
    Some((att, data))
}

/// Anexos listados de uma mensagem, prontos para reenviar (encaminhar): conteúdo em base64.
pub fn to_outgoing(raw: &[u8], listed: &[Attachment]) -> Vec<crate::send::OutAttachment> {
    listed
        .iter()
        .filter_map(|a| extract(raw, a.index))
        .map(|(a, data)| crate::send::OutAttachment {
            name: a.name,
            mime: a.mime,
            data: base64::engine::general_purpose::STANDARD.encode(data),
        })
        .collect()
}

/// Nome seguro para gravar em disco: sem pastas, sem caracteres proibidos no Windows.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let clean = clean.trim().trim_matches('.').to_string();
    if clean.is_empty() {
        "anexo".into()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &[u8] = b"From: a@b.c\r\nSubject: x\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"m\"\r\n\r\n\
--m\r\nContent-Type: multipart/related; boundary=\"r\"\r\n\r\n\
--r\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>oi</p><img src=\"cid:logo@x\">\r\n\
--r\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n\
--r--\r\n\
--m\r\nContent-Type: application/pdf; name=\"proposta.pdf\"\r\nContent-Disposition: attachment; filename=\"proposta.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nSGVsbG8gUERG\r\n\
--m\r\nContent-Type: text/plain; name=\"notas.txt\"\r\nContent-Disposition: attachment; filename=\"notas.txt\"\r\n\r\nlinha um\r\n\
--m--\r\n";

    #[test]
    fn lista_anexos_sem_a_imagem_inline() {
        let msg = MessageParser::default().parse(RAW).unwrap();
        let l = list(&msg, "<p>oi</p><img src=\"cid:logo@x\">");
        let nomes: Vec<_> = l.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(nomes, ["proposta.pdf", "notas.txt"], "{l:?}");
        assert_eq!(l[0].mime, "application/pdf");
        assert_eq!(l[0].size, 9);
        assert_eq!(l[1].mime, "text/plain");
    }

    #[test]
    fn extrai_pelo_indice_e_decodifica() {
        let l = list(&MessageParser::default().parse(RAW).unwrap(), "<img src=\"cid:logo@x\">");
        let (att, data) = extract(RAW, l[0].index).unwrap();
        assert_eq!(att.name, "proposta.pdf");
        assert_eq!(data, b"Hello PDF");
        let (_, txt) = extract(RAW, l[1].index).unwrap();
        assert_eq!(txt, b"linha um");
        assert!(extract(RAW, 99).is_none());
    }

    #[test]
    fn prepara_anexos_para_encaminhar() {
        let l = list(&MessageParser::default().parse(RAW).unwrap(), "<img src=\"cid:logo@x\">");
        let out = to_outgoing(RAW, &l);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].name.as_str(), out[0].data.as_str()), ("proposta.pdf", "SGVsbG8gUERG"));
        assert_eq!(out[1].mime, "text/plain");
    }

    #[test]
    fn cid_vira_data_url() {
        let msg = MessageParser::default().parse(RAW).unwrap();
        let out = inline_cids(&msg, "<img src=\"cid:logo@x\"><img src=\"CID:LOGO@x\">".into());
        assert_eq!(out.matches("data:image/png;base64,iVBORw0KGgo=").count(), 2, "{out}");
        assert!(!out.to_lowercase().contains("cid:"));
    }

    #[test]
    fn nome_seguro() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("a:b?.txt"), "a_b_.txt");
        assert_eq!(safe_name(".."), "anexo");
    }
}
