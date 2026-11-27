//! Property tests (proptest) dos parsers e da busca do `mail-core`.
//!
//! Dois tipos de propriedade:
//! - **Robustez**: bytes arbitrários, mensagens truncadas ou corrompidas nunca derrubam o parser nem a gravação.
//! - **Fidelidade**: mensagens montadas por nós (encoded-words RFC 2047 em utf-8/latin1/windows-1252, corpos em
//!   8bit/quoted-printable/base64, multipart aninhado com anexos) voltam do parser com o mesmo conteúdo; a busca FTS5
//!   ignora acento, caixa e aceita prefixo.
//!
//! Rodam no `cargo test` comum com `PROPTEST_CASES` (padrão 256). Para uma rodada longa de fuzz:
//! `PROPTEST_CASES=200000 cargo test -p mail-core --release proptests -- --test-threads=1`.
//! Falhas ficam em `crates/mail-core/proptest-regressions/` e são reexecutadas primeiro nas próximas rodadas
//! (esse diretório é versionado: é o corpus de regressão). Propriedades marcadas `#[ignore = "BUG ..."]` expõem bugs
//! reais já confirmados, corrigidos em PR próprio (número na mensagem); depois do merge de cada um, remova o `ignore` dele.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use proptest::prelude::*;

use crate::folders::Role;
use crate::search::{self, Scope};
use crate::store::{NewMessage, Store};
use crate::{attachments, imap_sync, rules, unsubscribe};

// ---------------------------------------------------------------------------------------------------------------
// Geradores
// ---------------------------------------------------------------------------------------------------------------

/// Letras acentuadas do português (todas existem em Latin-1/Windows-1252).
const ACENTOS: &str = "áàâãäéèêëíìîïóòôõöúùûüçñÁÀÂÃÉÊÍÓÔÕÚÇ";

fn letter() -> impl Strategy<Value = char> {
    prop_oneof![
        8 => proptest::char::range('a', 'z'),
        1 => proptest::char::range('A', 'Z'),
        3 => proptest::sample::select(ACENTOS.chars().collect::<Vec<_>>()),
    ]
}

fn word() -> impl Strategy<Value = String> {
    prop::collection::vec(letter(), 1..9).prop_map(|c| c.into_iter().collect())
}

/// Frase sem espaços nas pontas nem duplos.
fn phrase() -> impl Strategy<Value = String> {
    prop::collection::vec(word(), 1..6).prop_map(|w| w.join(" "))
}

/// Frase que pode ter emoji e CJK (só utf-8 aguenta).
fn wide_phrase() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![5 => word(), 1 => Just("😀".to_string()), 1 => Just("日本語".to_string()), 1 => Just("€".to_string())],
        1..6,
    )
    .prop_map(|w| w.join(" "))
}

#[derive(Debug, Clone, Copy)]
enum Cs {
    Utf8,
    Latin1,
    Win1252,
}

impl Cs {
    fn name(self) -> &'static str {
        match self {
            Cs::Utf8 => "utf-8",
            Cs::Latin1 => "iso-8859-1",
            Cs::Win1252 => "windows-1252",
        }
    }
    /// `s` só tem caracteres < U+0100 quando o charset não é utf-8.
    fn bytes(self, s: &str) -> Vec<u8> {
        match self {
            Cs::Utf8 => s.as_bytes().to_vec(),
            _ => s.chars().map(|c| u8::try_from(u32::from(c)).expect("latin1")).collect(),
        }
    }
}

fn cs() -> impl Strategy<Value = Cs> {
    prop_oneof![Just(Cs::Utf8), Just(Cs::Latin1), Just(Cs::Win1252)]
}

fn q_encode(b: &[u8]) -> String {
    b.iter().map(|&c| if c.is_ascii_alphanumeric() { (c as char).to_string() } else { format!("={c:02X}") }).collect()
}

/// `=?charset?B|Q?...?=` de um pedaço de texto.
fn encoded_word(cs: Cs, base64: bool, s: &str) -> String {
    let b = cs.bytes(s);
    let body = if base64 { B64.encode(&b) } else { q_encode(&b) };
    format!("=?{}?{}?{body}?=", cs.name(), if base64 { 'B' } else { 'Q' })
}

/// Parte `s` em pedaços nas fronteiras de caractere (RFC 2047 proíbe cortar um caractere ao meio).
fn chunks(s: &str, cuts: &[usize]) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut points: Vec<usize> = cuts.iter().map(|c| c % (chars.len() + 1)).collect();
    points.push(0);
    points.push(chars.len());
    points.sort_unstable();
    points.dedup();
    points.windows(2).map(|w| chars[w[0]..w[1]].iter().collect()).collect()
}

/// Valor de cabeçalho com `text` em encoded-words (um por pedaço), separados por espaço ou dobra de linha.
fn header_value(cs: Cs, base64: bool, text: &str, cuts: &[usize], fold: bool) -> String {
    let sep = if fold { "\r\n " } else { " " };
    chunks(text, cuts).iter().map(|c| encoded_word(cs, base64, c)).collect::<Vec<_>>().join(sep)
}

fn message(headers: &str, body: &[u8]) -> Vec<u8> {
    let mut m = format!(
        "From: Fulano <fulano@exemplo.com>\r\nTo: b@exemplo.com\r\nMessage-ID: <abc@exemplo.com>\r\nDate: Mon, 02 Nov 2026 10:00:00 -0300\r\nMIME-Version: 1.0\r\n{headers}\r\n"
    )
    .into_bytes();
    m.extend_from_slice(body);
    m
}

fn parsed(raw: &[u8]) -> NewMessage {
    imap_sync::parse(1, raw, false, false, 0, "eu@exemplo.com")
}

fn norm(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}

fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'Á' | 'À' | 'Â' | 'Ã' => 'A',
            'É' | 'Ê' => 'E',
            'Í' => 'I',
            'Ó' | 'Ô' | 'Õ' => 'O',
            'Ú' => 'U',
            'Ç' => 'C',
            c => c,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Cabeçalhos: encoded-words RFC 2047
// ---------------------------------------------------------------------------------------------------------------

proptest! {
    /// Assunto em encoded-words (B ou Q; utf-8, latin1, windows-1252), em vários pedaços, volta idêntico.
    #[test]
    fn assunto_encoded_word_volta_identico(
        text in phrase(), cs in cs(), b64 in any::<bool>(), fold in any::<bool>(),
        cuts in prop::collection::vec(any::<usize>(), 0..4),
    ) {
        let h = format!("Subject: {}\r\n", header_value(cs, b64, &text, &cuts, fold));
        let m = parsed(&message(&h, b"\r\ncorpo\r\n"));
        prop_assert_eq!(m.subject, text);
    }

    /// Emoji, CJK e euro só existem em utf-8; cortes sempre entre caracteres.
    #[test]
    fn assunto_utf8_com_emoji_volta_identico(
        text in wide_phrase(), b64 in any::<bool>(), fold in any::<bool>(),
        cuts in prop::collection::vec(any::<usize>(), 0..4),
    ) {
        let h = format!("Subject: {}\r\n", header_value(Cs::Utf8, b64, &text, &cuts, fold));
        let m = parsed(&message(&h, b"\r\ncorpo\r\n"));
        prop_assert_eq!(m.subject, text);
    }

    /// Texto ASCII antes do encoded-word (`Re: =?...?=`) é preservado.
    #[test]
    fn assunto_misto_ascii_e_encoded_word(text in phrase(), cs in cs(), b64 in any::<bool>()) {
        let h = format!("Subject: Re: {}\r\n", encoded_word(cs, b64, &text));
        let m = parsed(&message(&h, b"\r\ncorpo\r\n"));
        prop_assert_eq!(m.subject, format!("Re: {text}"));
    }

    /// Nome do remetente em encoded-word e endereço intactos.
    #[test]
    fn remetente_encoded_word(text in phrase(), cs in cs(), b64 in any::<bool>()) {
        let raw = format!(
            "From: {} <quem@exemplo.com>\r\nTo: b@exemplo.com\r\nSubject: x\r\nMessage-ID: <a@b>\r\n\r\ncorpo\r\n",
            encoded_word(cs, b64, &text)
        );
        let m = parsed(raw.as_bytes());
        prop_assert_eq!(m.from_name, text);
        prop_assert_eq!(m.from_addr, "quem@exemplo.com");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Corpos: charset x transfer-encoding
// ---------------------------------------------------------------------------------------------------------------

fn qp_encode(b: &[u8]) -> String {
    let mut out = String::new();
    let mut line = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
            out.push_str("\r\n");
            line = 0;
            i += 2;
            continue;
        }
        let tok = if (b[i].is_ascii_graphic() || b[i] == b' ') && b[i] != b'=' { (b[i] as char).to_string() } else { format!("={:02X}", b[i]) };
        if line + tok.len() > 72 {
            out.push_str("=\r\n");
            line = 0;
        }
        line += tok.len();
        out.push_str(&tok);
        i += 1;
    }
    out
}

fn b64_wrapped(b: &[u8]) -> String {
    B64.encode(b).as_bytes().chunks(76).map(|c| std::str::from_utf8(c).unwrap().to_string()).collect::<Vec<_>>().join("\r\n")
}

fn text_body() -> impl Strategy<Value = String> {
    prop::collection::vec(phrase(), 1..5).prop_map(|l| l.join("\r\n"))
}

proptest! {
    /// Corpo text/plain em cada charset e cada Content-Transfer-Encoding volta idêntico.
    #[test]
    fn corpo_charset_e_transfer_encoding(body in text_body(), cs in cs(), cte in 0..3u8) {
        let bytes = cs.bytes(&body);
        let (name, enc) = match cte {
            0 => ("8bit", bytes),
            1 => ("quoted-printable", qp_encode(&bytes).into_bytes()),
            _ => ("base64", b64_wrapped(&bytes).into_bytes()),
        };
        let h = format!("Subject: x\r\nContent-Type: text/plain; charset={}\r\nContent-Transfer-Encoding: {name}\r\n", cs.name());
        let m = parsed(&message(&h, &[enc.as_slice(), b"\r\n"].concat()));
        prop_assert_eq!(norm(&m.body_text), norm(&body));
    }

    /// Emoji e CJK em corpo utf-8 (8bit e base64).
    #[test]
    fn corpo_utf8_com_emoji(body in wide_phrase(), b64 in any::<bool>()) {
        let (name, enc) = if b64 { ("base64", b64_wrapped(body.as_bytes()).into_bytes()) } else { ("8bit", body.as_bytes().to_vec()) };
        let h = format!("Subject: x\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: {name}\r\n");
        let m = parsed(&message(&h, &[enc.as_slice(), b"\r\n"].concat()));
        prop_assert_eq!(norm(&m.body_text), norm(&body));
    }

    /// Sem charset declarado, utf-8 válido é lido como utf-8.
    #[test]
    fn corpo_utf8_sem_charset(body in text_body()) {
        let m = parsed(&message("Subject: x\r\nContent-Type: text/plain\r\nContent-Transfer-Encoding: 8bit\r\n", &[body.as_bytes(), b"\r\n"].concat()));
        prop_assert_eq!(norm(&m.body_text), norm(&body));
    }

    /// Sem charset e sem Content-Type, corpo 8-bit em Latin-1 (cliente antigo) não vira mojibake.
    #[test]
    #[ignore = "BUG: corpo 8-bit Latin-1 sem charset vira mojibake (corrigido no PR #70)"]
    fn corpo_latin1_sem_charset(body in text_body()) {
        let m = parsed(&message("Subject: x\r\n", &[Cs::Latin1.bytes(&body).as_slice(), b"\r\n"].concat()));
        prop_assert_eq!(norm(&m.body_text), norm(&body));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Multipart aninhado e anexos
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Att { name: String, style: u8, data: Vec<u8> },
    Multi { kind: &'static str, kids: Vec<Node> },
}

fn att_name() -> impl Strategy<Value = String> {
    (prop::collection::vec(prop_oneof![8 => proptest::char::range('a', 'z'), 2 => proptest::sample::select(vec!['ç', 'ã', 'é', 'ú'])], 1..8), proptest::sample::select(vec!["pdf", "txt", "bin"]))
        .prop_map(|(c, ext)| format!("{}.{ext}", c.into_iter().collect::<String>()))
}

fn node() -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
        phrase().prop_map(Node::Text),
        (att_name(), 0..3u8, prop::collection::vec(any::<u8>(), 1..400)).prop_map(|(name, style, data)| Node::Att { name, style, data }),
    ];
    leaf.prop_recursive(3, 14, 4, |inner| {
        (proptest::sample::select(vec!["mixed", "alternative"]), prop::collection::vec(inner, 1..4))
            .prop_map(|(kind, kids)| Node::Multi { kind, kids })
    })
}

fn filename_params(name: &str, style: u8) -> String {
    match style {
        0 if name.is_ascii() => format!("filename=\"{name}\""),
        2 => {
            let pct: String = name.bytes().map(|b| if b.is_ascii_alphanumeric() || b == b'.' { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
            format!("filename*=UTF-8''{pct}")
        }
        _ => format!("filename=\"{}\"", encoded_word(Cs::Utf8, true, name)),
    }
}

fn render(n: &Node, next: &mut u32, out: &mut Vec<u8>) {
    match n {
        Node::Text(t) => {
            out.extend_from_slice(format!("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{t}\r\n").as_bytes());
        }
        Node::Att { name, style, data } => {
            out.extend_from_slice(
                format!(
                    "Content-Type: application/octet-stream\r\nContent-Disposition: attachment; {}\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n",
                    filename_params(name, *style),
                    b64_wrapped(data)
                )
                .as_bytes(),
            );
        }
        Node::Multi { kind, kids } => {
            *next += 1;
            let b = format!("=_limite{next}");
            out.extend_from_slice(format!("Content-Type: multipart/{kind}; boundary=\"{b}\"\r\n\r\n").as_bytes());
            for k in kids {
                out.extend_from_slice(format!("--{b}\r\n").as_bytes());
                render(k, next, out);
            }
            out.extend_from_slice(format!("--{b}--\r\n").as_bytes());
        }
    }
}

fn atts(n: &Node, out: &mut Vec<(String, Vec<u8>)>) {
    match n {
        Node::Att { name, data, .. } => out.push((name.clone(), data.clone())),
        Node::Multi { kids, .. } => kids.iter().for_each(|k| atts(k, out)),
        Node::Text(_) => {}
    }
}

fn tree_message(root: &Node) -> Vec<u8> {
    let mut body = Vec::new();
    render(root, &mut 0, &mut body);
    // `render` começa pelos cabeçalhos da parte raiz: viram os cabeçalhos da mensagem.
    let split = body.windows(4).position(|w| w == b"\r\n\r\n").expect("raiz tem cabeçalhos") + 2;
    let headers = format!("Subject: com anexos\r\n{}", String::from_utf8_lossy(&body[..split]));
    message(&headers, &body[split + 2..])
}

fn root() -> impl Strategy<Value = Node> {
    prop::collection::vec(node(), 1..4).prop_map(|kids| Node::Multi { kind: "mixed", kids })
}

proptest! {
    /// Anexos de um multipart aninhado: mesma quantidade, mesma ordem, mesmo nome e mesmos bytes.
    #[test]
    fn multipart_aninhado_anexos_batem(root in root()) {
        let mut expected = Vec::new();
        atts(&root, &mut expected);
        let raw = tree_message(&root);
        let m = parsed(&raw);
        prop_assert_eq!(m.attachments.len(), expected.len(), "lista: {:?}", m.attachments);
        for (i, (name, data)) in expected.iter().enumerate() {
            let (a, bytes) = attachments::extract(&raw, i as u32).expect("anexo existe");
            prop_assert_eq!(&a.name, name);
            prop_assert_eq!(&bytes, data);
            prop_assert_eq!(a.size as usize, data.len());
        }
        prop_assert!(attachments::extract(&raw, expected.len() as u32).is_none());
    }

    /// Mensagem íntegra, truncada em qualquer ponto ou com bytes trocados: nunca panic, e grava no banco.
    #[test]
    fn mensagem_corrompida_nao_derruba_parser_nem_gravacao(
        root in root(), cut in any::<prop::sample::Index>(),
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
    ) {
        let mut raw = tree_message(&root);
        for (i, v) in &flips {
            let at = i.index(raw.len());
            raw[at] = *v;
        }
        raw.truncate(cut.index(raw.len() + 1));
        survives(&raw);
    }

    /// Bytes quaisquer.
    #[test]
    fn bytes_arbitrarios_nao_derrubam(raw in prop::collection::vec(any::<u8>(), 0..2048)) {
        survives(&raw);
    }

    /// Texto qualquer depois de um cabeçalho plausível (exercita corpo, charset e base64 inválidos).
    #[test]
    fn corpo_arbitrario_com_cabecalho_plausivel(
        ct in proptest::sample::select(vec!["text/plain; charset=utf-8", "text/html; charset=iso-8859-1", "text/plain; charset=nao-existe", "multipart/mixed; boundary=x", "text/plain"]),
        cte in proptest::sample::select(vec!["base64", "quoted-printable", "7bit", "8bit", "binary", "x-uuencode"]),
        body in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        let h = format!("Subject: =?utf-8?Q?=ZZ?=\r\nContent-Type: {ct}\r\nContent-Transfer-Encoding: {cte}\r\n");
        survives(&message(&h, &body));
    }
}

/// Parseia, lista anexos, lê o cabeçalho de cancelamento e grava no banco. Só falha por panic ou erro de gravação.
fn survives(raw: &[u8]) {
    let m = parsed(raw);
    for i in 0..3 {
        let _ = attachments::extract(raw, i);
    }
    let _ = unsubscribe::parse(raw);
    let _ = unsubscribe::header_lines(raw);
    let mut store = Store::open_in_memory().unwrap();
    store.insert_messages("c", "INBOX", &[m]).expect("gravar mensagem arbitrária");
}

// ---------------------------------------------------------------------------------------------------------------
// Busca FTS5 (sem acento, sem caixa, prefixo)
// ---------------------------------------------------------------------------------------------------------------

fn store_with(subject: &str, body: &str) -> Store {
    let mut store = Store::open_in_memory().unwrap();
    let m = NewMessage { uid: 1, subject: subject.into(), body_text: body.into(), from_addr: "123@456.789".into(), date: 1_700_000_000, message_id: "x@y".into(), ..Default::default() };
    store.insert_messages("c", "INBOX", &[m]).unwrap();
    store
}

proptest! {
    /// Cada palavra do corpo e do assunto é achada em maiúsculas, sem acento e por qualquer prefixo.
    #[test]
    fn fts_ignora_acento_caixa_e_aceita_prefixo(subject in phrase(), body in phrase(), k in 1..9usize) {
        let store = store_with(&subject, &body);
        for w in subject.split(' ').chain(body.split(' ')) {
            let prefix: String = w.chars().take(k).collect();
            for q in [w.to_string(), w.to_uppercase(), w.to_lowercase(), strip_accents(w), strip_accents(w).to_uppercase(), prefix.clone(), strip_accents(&prefix).to_uppercase()] {
                let hits = store.search(&q, 10).unwrap();
                prop_assert_eq!(hits.len(), 1, "consulta {:?} (de {:?}) não achou {:?}/{:?}", q, w, subject, body);
            }
        }
    }

    /// Duas palavras na consulta: as duas precisam existir (E), em qualquer ordem.
    #[test]
    fn fts_varias_palavras_sao_e(a in word(), b in word(), c in word()) {
        let (la, lb, lc) = (strip_accents(&a).to_lowercase(), strip_accents(&b).to_lowercase(), strip_accents(&c).to_lowercase());
        prop_assume!(!la.starts_with(&lc) && !lb.starts_with(&lc));
        let store = store_with(&format!("{a} {b}"), "0");
        let (a2, b2) = (strip_accents(&a), strip_accents(&b));
        prop_assert_eq!(store.search(&format!("{b2} {a2}"), 10).unwrap().len(), 1);
        prop_assert_eq!(store.search(&format!("{a2} {c}"), 10).unwrap().len(), 0, "{:?} não está na mensagem", c);
    }

    /// Qualquer texto digitado na busca (aspas, `*`, `NEAR`, parênteses, `-`, `:`) vira consulta válida, sem erro de SQL/FTS.
    #[test]
    fn busca_com_texto_arbitrario_nao_erra(q in prop_oneof![any::<String>(), "[\"*()\\-:^+ a-zA-Z0-9{}\\\\%_]{0,30}"]) {
        let store = store_with("Reunião", "diretoria");
        prop_assert!(store.search(&q, 10).is_ok(), "search({:?})", q);
        let f = search::parse(&q);
        let r = store.search_filtered(&Scope::Role { role: Role::Inbox }, &f, 10);
        prop_assert!(r.is_ok(), "search_filtered({:?}) = {:?}", q, r);
    }

    /// `assunto:` com `%`, `_` e `\` casa literalmente (não vira curinga).
    #[test]
    fn filtro_assunto_trata_curingas_como_literais(pre in "[a-z]{0,5}", mid in "[%_\\\\]{1,3}", post in "[a-z]{0,5}") {
        let subject = format!("{pre}{mid}{post}");
        let store = store_with(&subject, "corpo");
        let ok = store.search_filtered(&Scope::Role { role: Role::Inbox }, &search::parse(&format!("assunto:\"{mid}\"")), 10).unwrap();
        prop_assert_eq!(ok.len(), 1);
        let store = store_with("sem curinga", "corpo");
        let none = store.search_filtered(&Scope::Role { role: Role::Inbox }, &search::parse(&format!("assunto:\"{mid}\"")), 10).unwrap();
        prop_assert_eq!(none.len(), 0, "{:?} casou como curinga", mid);
    }

    /// `assunto:`/`de:` ignoram acento e caixa como a busca de texto.
    #[test]
    #[ignore = "BUG: filtros assunto:/de: usam LIKE do SQLite (só ASCII sem acento); corrigido no PR #79"]
    fn filtro_assunto_ignora_acento_e_caixa(w in word()) {
        let store = store_with(&format!("Aviso {w}"), "corpo");
        for q in [strip_accents(&w), w.to_uppercase(), strip_accents(&w).to_uppercase()] {
            let hits = store.search_filtered(&Scope::Role { role: Role::Inbox }, &search::parse(&format!("assunto:{q}")), 10).unwrap();
            prop_assert_eq!(hits.len(), 1, "assunto:{} não achou {:?}", q, w);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Filtros: parse e consulta IMAP SEARCH
// ---------------------------------------------------------------------------------------------------------------

proptest! {
    /// `parse` e `imap_query` aceitam qualquer entrada.
    #[test]
    fn parse_de_filtros_nao_entra_em_panico(q in prop_oneof![any::<String>(), "[a-zç:\"\\\\ \\-0-9\\r\\n]{0,40}"]) {
        let f = search::parse(&q);
        let _ = f.imap_query();
        let _ = f.is_plain();
    }

    /// A consulta IMAP nunca tem quebra de linha (senão vira outro comando) e as aspas fecham.
    #[test]
    #[ignore = "BUG: CR/LF dentro de valor entre aspas passa para o comando IMAP SEARCH (corrigido no PR #78)"]
    fn imap_query_sem_quebra_de_linha_e_com_aspas_fechadas(
        key in proptest::sample::select(vec!["de", "para", "assunto", "from", "to", "subject"]),
        val in "[a-z \\r\\n\"\\\\\\x00]{1,12}",
    ) {
        let f = search::parse(&format!("{key}:\"{}\"", val.replace('"', "")));
        if let Some(q) = f.imap_query() {
            prop_assert!(!q.contains(['\r', '\n', '\0']), "consulta com controle: {:?}", q);
            let mut unescaped = q.replace("\\\\", "").replace("\\\"", "");
            unescaped.retain(|c| c == '"');
            prop_assert_eq!(unescaped.len() % 2, 0, "aspas abertas: {:?}", q);
        }
    }

    /// Valores comuns viram `KEY "valor"` com aspas e barras escapadas.
    #[test]
    fn imap_query_escapa_aspas_e_barras(val in "[a-z\\\\]{1,8}") {
        let f = search::Filters { from: vec![format!("{val}\"")], ..Default::default() };
        let q = f.imap_query().unwrap();
        prop_assert!(q.starts_with("FROM \"") && q.ends_with("\\\"\""), "{q}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Cancelar inscrição (List-Unsubscribe) e mailto:
// ---------------------------------------------------------------------------------------------------------------

proptest! {
    /// `parse_mailto` aceita qualquer texto; se devolver destinatário, é um só e sem controle de cabeçalho.
    #[test]
    fn mailto_arbitrario_nunca_injeta_cabecalho(m in prop_oneof![any::<String>(), "mailto:[a-z@%0-9A-F,;<>?=&+\\r\\n ]{0,40}"]) {
        if let Some(u) = unsubscribe::parse_mailto(&m) {
            prop_assert!(u.to.contains('@'));
            prop_assert!(!u.to.contains(['\r', '\n', ',', ';', '<', '>']), "to = {:?}", u.to);
            prop_assert!(!u.subject.contains(['\r', '\n']), "subject = {:?}", u.subject);
        }
    }

    /// Endereço com `+` (plus-addressing) fica intacto: em `mailto:` o `+` é literal (RFC 6068).
    #[test]
    #[ignore = "BUG: parse_mailto troca '+' por espaço no destinatário (corrigido no PR #77)"]
    fn mailto_preserva_plus_addressing(local in "[a-z0-9]{1,6}", tag in "[a-z0-9]{1,6}") {
        let u = unsubscribe::parse_mailto(&format!("mailto:{local}+{tag}@lista.exemplo.com?subject=sair")).unwrap();
        prop_assert_eq!(u.to, format!("{local}+{tag}@lista.exemplo.com"));
    }

    /// Cabeçalho List-Unsubscribe dobrado em linhas, com caixa qualquer, é entendido.
    #[test]
    fn list_unsubscribe_dobrado_e_com_caixa_mista(host in "[a-z]{1,8}", path in "[a-z0-9/]{0,10}", fold in any::<bool>(), upper in any::<bool>()) {
        let name = if upper { "LIST-UNSUBSCRIBE" } else { "List-Unsubscribe" };
        let sep = if fold { ",\r\n  " } else { ", " };
        let raw = format!("From: a@b.com\r\n{name}: <mailto:sair@{host}.com>{sep}<https://{host}.com/{path}>\r\nSubject: x\r\n\r\ncorpo");
        let u = unsubscribe::parse(raw.as_bytes()).expect("cabeçalho válido");
        prop_assert_eq!(u.url, Some(format!("https://{host}.com/{path}")));
        prop_assert_eq!(u.mailto, Some(format!("mailto:sair@{host}.com")));
        let h = unsubscribe::header_lines(raw.as_bytes());
        prop_assert!(h.is_some());
        prop_assert_eq!(unsubscribe::parse(h.unwrap().as_bytes()), unsubscribe::parse(raw.as_bytes()));
    }

    /// Cabeçalhos arbitrários não derrubam o parser, e o resultado só tem esquemas esperados.
    #[test]
    fn list_unsubscribe_arbitrario(v in "[ -~\\r\\n]{0,80}") {
        let raw = format!("List-Unsubscribe: {v}\r\n\r\n");
        if let Some(u) = unsubscribe::parse(raw.as_bytes()) {
            prop_assert!(u.url.as_deref().is_none_or(|x| x.to_lowercase().starts_with("http")));
            prop_assert!(u.mailto.as_deref().is_none_or(|x| x.to_lowercase().starts_with("mailto:")));
            prop_assert!(!u.one_click || u.url.as_deref().is_some_and(|x| x.to_lowercase().starts_with("https://")));
        }
    }

    /// Nome de arquivo salvo em disco: sem pasta, sem caractere proibido no Windows, não vazio.
    #[test]
    #[ignore = "BUG: safe_name deixa espaço no fim (\"a .\" vira \"a \"); o Windows apaga o espaço e o nome gravado difere do mostrado (corrigido no PR #76)"]
    fn nome_de_anexo_seguro_para_disco(name in prop_oneof![any::<String>(), "[a-z./\\\\:*?\"<>| ]{0,16}"]) {
        let s = attachments::safe_name(&name);
        prop_assert!(!s.is_empty());
        prop_assert!(!s.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) && !s.chars().any(char::is_control), "{s:?}");
        prop_assert!(!s.starts_with('.') && !s.ends_with('.') && !s.ends_with(' '), "{s:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Regras (comparação sem acento)
// ---------------------------------------------------------------------------------------------------------------

fn candidate(subject: &str) -> rules::Candidate {
    rules::Candidate {
        id: 1,
        account: "c".into(),
        uid: 1,
        from_name: String::new(),
        from_addr: "a@b.com".into(),
        subject: subject.into(),
        to_addrs: String::new(),
        cc_addrs: String::new(),
        reply_addr: String::new(),
        date: 0,
    }
}

fn rule(contains: &str) -> rules::Rule {
    rules::Rule { id: "r".into(), field: "subject".into(), contains: contains.into(), action: "read".into(), arg: String::new(), enabled: true }
}

proptest! {
    /// Regra "contém" ignora caixa e os acentos mais comuns (sem ç e trema).
    #[test]
    fn regra_contem_ignora_caixa_e_acento_comum(pre in "[a-z ]{0,6}", w in "[a-zéêãáâíóôú]{1,8}", post in "[a-z ]{0,6}") {
        let c = candidate(&format!("{}{}{}", pre, w.to_uppercase(), post));
        prop_assert!(rule(&strip_accents(&w)).matches(&c));
        prop_assert!(rule(&w).matches(&candidate(&format!("{}{}{}", pre, strip_accents(&w), post))), "regra com acento");
    }

    /// Qualquer letra acentuada do português: a regra sem acento casa com o assunto acentuado e vice-versa.
    #[test]
    #[ignore = "BUG: search::plain só dobra 9 acentos (falta ç, à, õ, ü, ñ...); corrigido no PR #79"]
    fn regra_contem_ignora_todos_os_acentos(w in word()) {
        let c = candidate(&format!("Aviso {}", w));
        prop_assert!(rule(&strip_accents(&w)).matches(&c), "regra {:?} não casou com {:?}", strip_accents(&w), w);
        prop_assert!(rule(&w).matches(&candidate(&format!("Aviso {}", strip_accents(&w)))), "regra com acento");
    }
}
