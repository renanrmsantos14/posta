//! Agrupamento de mensagens em conversas (fios).
//!
//! Primeiro por Message-ID: uma mensagem entra no fio das que cita em References/In-Reply-To.
//! Fallback por assunto normalizado (sem Re:/Fwd:/RES:/ENC:) para respostas cujo pai não está no banco;
//! só vale quando alguém do grupo tem prefixo de resposta, para não juntar newsletters de mesmo título.

use std::collections::HashMap;

/// O que o agrupamento precisa de cada mensagem de UMA conta.
pub struct Entry<'a> {
    pub id: i64,
    pub message_id: &'a str,
    /// IDs de References (e In-Reply-To), separados por espaço.
    pub refs: &'a str,
    pub subject: &'a str,
}

const PREFIXES: [&str; 8] = ["re:", "res:", "fwd:", "fw:", "enc:", "aw:", "tr:", "rv:"];

/// Assunto sem prefixos de resposta/encaminhamento, em minúsculas; `true` se havia algum prefixo.
pub fn normalize_subject(s: &str) -> (String, bool) {
    let mut t = s.trim().to_lowercase();
    let mut prefixed = false;
    while let Some(rest) = PREFIXES.iter().find_map(|p| t.strip_prefix(p)) {
        t = rest.trim_start().to_string();
        prefixed = true;
    }
    (t.split_whitespace().collect::<Vec<_>>().join(" "), prefixed)
}

fn find(p: &mut [usize], mut i: usize) -> usize {
    while p[i] != i {
        p[i] = p[p[i]];
        i = p[i];
    }
    i
}

fn union(p: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(p, a), find(p, b));
    if ra != rb {
        p[ra.max(rb)] = ra.min(rb);
    }
}

/// Chave do fio de cada entrada (o menor `id` do fio), na mesma ordem da entrada.
pub fn group(entries: &[Entry]) -> Vec<i64> {
    let mut parent: Vec<usize> = (0..entries.len()).collect();
    let by_msgid: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.message_id.is_empty())
        .map(|(i, e)| (e.message_id, i))
        .collect();
    for (i, e) in entries.iter().enumerate() {
        for r in e.refs.split_whitespace() {
            if let Some(&j) = by_msgid.get(r) {
                union(&mut parent, i, j);
            }
        }
    }
    let mut buckets: HashMap<String, (bool, Vec<usize>)> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        let (key, prefixed) = normalize_subject(e.subject);
        if key.is_empty() {
            continue;
        }
        let b = buckets.entry(key).or_default();
        b.0 |= prefixed;
        b.1.push(i);
    }
    for (prefixed, idx) in buckets.into_values() {
        if prefixed {
            for &i in &idx[1..] {
                union(&mut parent, idx[0], i);
            }
        }
    }
    let mut min_id: HashMap<usize, i64> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        let r = find(&mut parent, i);
        let m = min_id.entry(r).or_insert(e.id);
        *m = (*m).min(e.id);
    }
    (0..entries.len()).map(|i| min_id[&find(&mut parent, i)]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e<'a>(id: i64, message_id: &'a str, refs: &'a str, subject: &'a str) -> Entry<'a> {
        Entry { id, message_id, refs, subject }
    }

    #[test]
    fn assunto_perde_prefixos() {
        assert_eq!(normalize_subject("RES: Re:  Fwd: Contrato  novo"), ("contrato novo".into(), true));
        assert_eq!(normalize_subject("ENC: Nota"), ("nota".into(), true));
        assert_eq!(normalize_subject("Resumo"), ("resumo".into(), false));
    }

    #[test]
    fn references_encadeiam_o_fio_mesmo_com_assunto_diferente() {
        let k = group(&[
            e(5, "a", "", "Orçamento"),
            e(6, "b", "a", "Assunto mudou"),
            e(7, "c", "a b", "Outro ainda"),
            e(8, "z", "", "Conversa à parte"),
        ]);
        assert_eq!(k, vec![5, 5, 5, 8]);
    }

    #[test]
    fn fallback_por_assunto_so_com_prefixo_de_resposta() {
        let k = group(&[
            e(1, "a", "", "Churrasco"),
            e(2, "b", "x-ausente", "RES: Churrasco"),
            e(3, "c", "", "Newsletter"),
            e(4, "d", "", "Newsletter"),
        ]);
        assert_eq!(k, vec![1, 1, 3, 4]);
    }
}
