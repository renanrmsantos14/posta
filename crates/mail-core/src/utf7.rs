//! Nomes de pasta no fio IMAP: UTF-7 modificado (RFC 3501 §5.1.3). "Preços" viaja como `Pre&AOc-os`.
//! Dentro do app os nomes ficam sempre em Unicode; só a conversa com o servidor usa a forma do fio.

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";

fn printable(c: char) -> bool {
    ('\u{20}'..='\u{7e}').contains(&c)
}

fn b64_val(b: u8) -> Option<u32> {
    B64.iter().position(|&x| x == b).map(|p| p as u32)
}

/// Decodifica o trecho entre `&` e `-`; `None` se não for UTF-16BE válido.
fn decode_run(run: &str) -> Option<String> {
    let (mut bits, mut n) = (0u32, 0u32);
    let mut bytes = Vec::with_capacity(run.len() * 3 / 4);
    for b in run.bytes() {
        bits = (bits << 6) | b64_val(b)?;
        n += 6;
        if n >= 8 {
            n -= 8;
            bytes.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    if bits != 0 || bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    String::from_utf16(&units).ok()
}

/// Nome do fio → Unicode. O que não for UTF-7 modificado válido (inclusive UTF-8 cru de servidor com `UTF8=ACCEPT`)
/// passa como veio.
pub fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let run = &after[..end];
        match (run.is_empty(), decode_run(run)) {
            (true, _) => out.push('&'),
            (false, Some(text)) => out.push_str(&text),
            (false, None) => out.push_str(&rest[i..i + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Unicode → nome do fio. Nome só ASCII que já é UTF-7 modificado (guardado antes de o app decodificar) segue como está:
/// regras e filas antigas continuam achando a pasta. Custo: uma pasta chamada literalmente `&AOc-` não é representável.
pub fn encode(s: &str) -> String {
    if s.chars().all(printable) && decode(s) != s {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut run: Vec<u16> = Vec::new();
    let flush = |run: &mut Vec<u16>, out: &mut String| {
        if run.is_empty() {
            return;
        }
        let bytes: Vec<u8> = run.iter().flat_map(|u| u.to_be_bytes()).collect();
        out.push('&');
        for chunk in bytes.chunks(3) {
            let v = chunk.iter().enumerate().fold(0u32, |a, (i, &b)| a | u32::from(b) << (16 - 8 * i));
            for k in 0..=(chunk.len()) {
                out.push(B64[((v >> (18 - 6 * k)) & 63) as usize] as char);
            }
        }
        out.push('-');
        run.clear();
    };
    for c in s.chars() {
        if printable(c) {
            flush(&mut run, &mut out);
            if c == '&' {
                out.push_str("&-");
            } else {
                out.push(c);
            }
        } else {
            let mut buf = [0u16; 2];
            run.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemplos_da_rfc_3501() {
        assert_eq!(decode("~peter/mail/&U,BTFw-/&ZeVnLIqe-"), "~peter/mail/台北/日本語");
        assert_eq!(encode("~peter/mail/台北/日本語"), "~peter/mail/&U,BTFw-/&ZeVnLIqe-");
        assert_eq!((decode("&-"), encode("&")), ("&".to_string(), "&-".to_string()));
    }

    #[test]
    fn acentos_do_portugues_e_emoji() {
        for (plain, wire) in [
            ("Preços e Cotações", "Pre&AOc-os e Cota&AOcA9Q-es"),
            ("INBOX.Notícias", "INBOX.Not&AO0-cias"),
            ("Café ☕ 😀", "Caf&AOk- &JhU- &2D3eAA-"),
            ("R&D", "R&-D"),
        ] {
            assert_eq!(encode(plain), wire, "encode {plain}");
            assert_eq!(decode(wire), plain, "decode {wire}");
        }
    }

    #[test]
    fn ascii_nao_muda_e_o_que_ja_esta_no_fio_nao_dobra() {
        assert_eq!(encode("INBOX.Sent Items"), "INBOX.Sent Items");
        assert_eq!(encode("Pre&AOc-os"), "Pre&AOc-os");
        assert_eq!(decode("INBOX"), "INBOX");
    }

    #[test]
    fn entrada_estranha_nao_quebra() {
        // UTF-8 cru (servidor com UTF8=ACCEPT), `&` sem fechar, base64 inválido, UTF-16 truncado.
        for s in ["Preços", "AT&T", "a&", "&!!-x", "&AO-", "&-&-", "&"] {
            let _ = decode(s);
            let _ = encode(s);
        }
        assert_eq!(decode("Preços"), "Preços");
        assert_eq!(decode("AT&T"), "AT&T");
        assert_eq!(decode("&!!-x"), "&!!-x");
        assert_eq!(decode("&AO-"), "&AO-");
    }

    #[test]
    fn ida_e_volta_com_texto_qualquer() {
        for s in ["", "a", "ção", "日本語 mail", "𝄞 clave", "a&b&-c", "  espaço  ", "Ç&Ç"] {
            assert_eq!(decode(&encode(s)), s, "{s:?}");
        }
    }
}
