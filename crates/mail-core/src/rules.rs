//! Regras automáticas: condição simples sobre remetente, assunto ou destinatário e uma ação.
//! A regra é avaliada aqui; quem aplica as ações (servidor e banco) é o app.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::folders::Role;
use crate::search::plain;
use crate::store::Store;
use crate::Result;

/// Condição de uma regra. Todas as condições da regra precisam casar (E).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    /// `from` (nome ou endereço), `domain` (domínio do remetente, com subdomínios), `subject`,
    /// `to`, `cc`, `recipients` (Para ou Cc) ou `attachment` (`yes`/vazio = tem anexo, `no` = não tem).
    pub field: String,
    #[serde(default)]
    pub value: String,
}

/// Ação de uma regra.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
    /// `move`, `read`, `star`, `category`, `archive` ou `trash` (bloquear remetente).
    pub kind: String,
    /// Pasta (`move`) ou id da categoria (`category`).
    #[serde(default)]
    pub arg: String,
}

/// Regra guardada na configuração `rules` (JSON), editada em Configurações. A ordem da lista é a ordem de avaliação.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
    /// Depois de casar, as regras seguintes não rodam para a mensagem.
    pub stop: bool,
    pub enabled: bool,
}

/// Formato do JSON: aceita o antigo (`field`/`contains`/`action`/`arg`, uma condição e uma ação) e o novo.
#[derive(Deserialize)]
struct RawRule {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    conditions: Vec<Condition>,
    #[serde(default)]
    actions: Vec<Action>,
    #[serde(default)]
    field: String,
    #[serde(default)]
    contains: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    arg: String,
    #[serde(default)]
    stop: bool,
    #[serde(default = "yes")]
    enabled: bool,
}

impl From<RawRule> for Rule {
    fn from(r: RawRule) -> Rule {
        let mut conditions = r.conditions;
        let mut actions = r.actions;
        if conditions.is_empty() && !r.field.is_empty() {
            conditions.push(Condition { field: r.field, value: r.contains });
        }
        if actions.is_empty() && !r.action.is_empty() {
            actions.push(Action { kind: r.action, arg: r.arg });
        }
        Rule { id: r.id, name: r.name, conditions, actions, stop: r.stop, enabled: r.enabled }
    }
}

impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        RawRule::deserialize(d).map(Rule::from)
    }
}

fn yes() -> bool {
    true
}

/// Mensagem da Caixa de entrada vista pelas regras.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: i64,
    pub account: String,
    pub uid: u32,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub to_addrs: String,
    pub cc_addrs: String,
    pub reply_addr: String,
    pub date: i64,
    pub has_attachment: bool,
}

/// O que fazer com uma mensagem depois de passar pelas regras.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plan {
    pub read: bool,
    pub star: bool,
    pub category: Option<String>,
    pub archive: bool,
    /// Vai para a Lixeira; vence mover e arquivar.
    pub trash: bool,
    /// Pasta do usuário de destino. Só a primeira regra de mover vale.
    pub move_to: Option<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        *self == Plan::default()
    }
}

/// Lê as regras do JSON da configuração; JSON inválido vira lista vazia e item inválido é ignorado.
pub fn parse_rules(json: Option<&str>) -> Vec<Rule> {
    let Some(Ok(items)) = json.map(serde_json::from_str::<Vec<serde_json::Value>>) else { return Vec::new() };
    items.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()
}

/// Domínio do endereço, em minúsculas (`ana@Mail.Itau.com.br` → `mail.itau.com.br`).
fn domain_of(addr: &str) -> String {
    addr.rsplit_once('@').map(|(_, d)| d).unwrap_or("").trim().trim_end_matches('>').to_lowercase()
}

/// Campos da mensagem já normalizados (sem acento, minúsculos): feito uma vez por mensagem, não por regra.
struct Prepared {
    from: String,
    domain: String,
    subject: String,
    to: String,
    cc: String,
    attachment: bool,
}

impl Prepared {
    fn new(c: &Candidate) -> Prepared {
        Prepared {
            from: plain(&format!("{} {}", c.from_name, c.from_addr)),
            domain: domain_of(&c.from_addr),
            subject: plain(&c.subject),
            to: plain(&c.to_addrs),
            cc: plain(&c.cc_addrs),
            attachment: c.has_attachment,
        }
    }
}

/// Condição com o texto já normalizado.
struct Needle {
    field: String,
    text: String,
    /// Só para `attachment`: exige que NÃO tenha anexo.
    negate: bool,
}

impl Needle {
    /// `None` = condição incompleta ou desconhecida (a regra inteira não vale).
    fn compile(c: &Condition) -> Option<Needle> {
        match c.field.as_str() {
            "attachment" => Some(Needle { field: c.field.clone(), text: String::new(), negate: c.value.trim().eq_ignore_ascii_case("no") }),
            "from" | "subject" | "to" | "cc" | "recipients" => {
                let text = plain(c.value.trim());
                (!text.is_empty()).then(|| Needle { field: c.field.clone(), text, negate: false })
            }
            "domain" => {
                let text = c.value.trim().trim_start_matches('@').to_lowercase();
                (!text.is_empty()).then(|| Needle { field: c.field.clone(), text, negate: false })
            }
            _ => None,
        }
    }

    fn matches(&self, p: &Prepared) -> bool {
        match self.field.as_str() {
            "from" => p.from.contains(&self.text),
            "domain" => p.domain == self.text || p.domain.ends_with(&format!(".{}", self.text)),
            "subject" => p.subject.contains(&self.text),
            "to" => p.to.contains(&self.text),
            "cc" => p.cc.contains(&self.text),
            "recipients" => p.to.contains(&self.text) || p.cc.contains(&self.text),
            "attachment" => p.attachment != self.negate,
            _ => false,
        }
    }
}

struct Compiled<'a> {
    rule: &'a Rule,
    needles: Vec<Needle>,
}

/// Regras prontas para avaliar muitas mensagens: textos normalizados uma vez, regras inativas ou incompletas já fora.
pub struct Ruleset<'a> {
    rules: Vec<Compiled<'a>>,
}

impl<'a> Ruleset<'a> {
    pub fn new(rules: &'a [Rule]) -> Ruleset<'a> {
        let rules = rules
            .iter()
            .filter(|r| r.enabled && !r.conditions.is_empty())
            .filter_map(|r| {
                let needles = r.conditions.iter().map(Needle::compile).collect::<Option<Vec<_>>>()?;
                Some(Compiled { rule: r, needles })
            })
            .collect();
        Ruleset { rules }
    }

    /// O que fazer com a mensagem, somando as ações das regras que casam.
    pub fn plan(&self, c: &Candidate) -> Plan {
        let p = Prepared::new(c);
        let mut plan = Plan::default();
        for r in &self.rules {
            if !r.needles.iter().all(|n| n.matches(&p)) {
                continue;
            }
            for a in &r.rule.actions {
                match a.kind.as_str() {
                    "read" => plan.read = true,
                    "star" => plan.star = true,
                    "archive" => plan.archive = true,
                    "trash" => plan.trash = true,
                    "category" if plan.category.is_none() && !a.arg.is_empty() => plan.category = Some(a.arg.clone()),
                    "move" if plan.move_to.is_none() && !a.arg.is_empty() => plan.move_to = Some(a.arg.clone()),
                    _ => {}
                }
            }
            if r.rule.stop {
                break;
            }
        }
        plan
    }

    /// Quantas das mensagens têm alguma regra casando (mesmo sem ação útil).
    pub fn count_matching(&self, msgs: &[Candidate]) -> usize {
        msgs.iter()
            .filter(|c| {
                let p = Prepared::new(c);
                self.rules.iter().any(|r| r.needles.iter().all(|n| n.matches(&p)))
            })
            .count()
    }
}

impl Rule {
    /// A regra (ativa e completa) casa com a mensagem?
    pub fn matches(&self, c: &Candidate) -> bool {
        Ruleset::new(std::slice::from_ref(self)).count_matching(std::slice::from_ref(c)) == 1
    }
}

/// Junta as ações de todas as regras que casam, na ordem da lista.
pub fn plan(rules: &[Rule], c: &Candidate) -> Plan {
    Ruleset::new(rules).plan(c)
}

impl Store {
    /// Maior id local; o app o guarda antes de sincronizar para saber o que é novo depois.
    pub fn max_id(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| r.get(0))?)
    }

    /// Mensagens da Caixa de entrada de `account` com id maior que `after` (`0` = todas).
    pub fn inbox_after(&self, account: &str, after: i64) -> Result<Vec<Candidate>> {
        let mut st = self.conn.prepare(
            "SELECT m.id, m.account, m.uid, m.from_name, m.from_addr, m.subject, m.to_addrs, m.cc_addrs, m.reply_addr, m.date, m.attachments <> '[]'
             FROM messages m
             WHERE m.account = ?1 AND m.id > ?2 AND m.mailbox = COALESCE(
               (SELECT f.name FROM folders f WHERE f.account = m.account AND f.role = ?3), 'INBOX')
             ORDER BY m.id",
        )?;
        let rows = st
            .query_map(params![account, after, Role::Inbox.as_str()], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    uid: r.get(2)?,
                    from_name: r.get(3)?,
                    from_addr: r.get(4)?,
                    subject: r.get(5)?,
                    to_addrs: r.get(6)?,
                    cc_addrs: r.get(7)?,
                    reply_addr: r.get(8)?,
                    date: r.get(9)?,
                    has_attachment: r.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewMessage;

    fn cond(field: &str, value: &str) -> Condition {
        Condition { field: field.into(), value: value.into() }
    }

    fn act(kind: &str, arg: &str) -> Action {
        Action { kind: kind.into(), arg: arg.into() }
    }

    /// Regra de uma condição e uma ação (o caso comum).
    fn rule(field: &str, contains: &str, action: &str, arg: &str) -> Rule {
        Rule { id: format!("{field}{contains}{action}"), name: String::new(), conditions: vec![cond(field, contains)], actions: vec![act(action, arg)], stop: false, enabled: true }
    }

    fn cand() -> Candidate {
        Candidate {
            id: 1, account: "a".into(), uid: 1, from_name: "Banco Itaú".into(), from_addr: "avisos@mail.itau.com.br".into(),
            subject: "Fatura de Novembro".into(), to_addrs: "eu@x.com".into(), cc_addrs: "fin@x.com".into(), reply_addr: String::new(), date: 0,
            has_attachment: true,
        }
    }

    #[test]
    fn casa_com_qualquer_acento_nos_dois_lados() {
        let mut c = cand();
        c.subject = "Cotação de preços — São Paulo".into();
        for needle in ["cotacao", "COTAÇÃO", "precos", "sao paulo", "são paulo"] {
            assert!(rule("subject", needle, "read", "").matches(&c), "{needle}");
        }
        c.from_name = "Çelik Müller".into();
        assert!(rule("from", "celik muller", "read", "").matches(&c));
        assert!(rule("from", "ÇELIK MÜLLER", "read", "").matches(&c));
    }

    #[test]
    fn casa_sem_ligar_para_caixa_e_acento() {
        let c = cand();
        assert!(rule("from", "itau", "read", "").matches(&c));
        assert!(rule("from", "ITAÚ", "read", "").matches(&c));
        assert!(rule("subject", "fatura", "read", "").matches(&c));
        assert!(rule("subject", "NOVÉMBRO", "read", "").matches(&c));
        assert!(rule("to", "eu@x", "read", "").matches(&c));
        assert!(rule("cc", "fin@x", "read", "").matches(&c));
        assert!(!rule("cc", "eu@x", "read", "").matches(&c), "cc não olha o Para");
        assert!(!rule("to", "fin@x", "read", "").matches(&c), "to não olha o Cc");
        assert!(rule("recipients", "fin@x", "read", "").matches(&c));
        assert!(rule("recipients", "eu@x", "read", "").matches(&c));
        assert!(!rule("subject", "boleto", "read", "").matches(&c));
        assert!(!rule("from", "  ", "read", "").matches(&c), "condição vazia nunca casa");
        let mut off = rule("from", "itau", "read", "");
        off.enabled = false;
        assert!(!off.matches(&c));
        assert!(!rule("campo", "itau", "read", "").matches(&c));
    }

    #[test]
    fn dominio_casa_o_dominio_e_subdominios_mas_nao_pedacos() {
        let c = cand();
        assert!(rule("domain", "itau.com.br", "read", "").matches(&c));
        assert!(rule("domain", "@Itau.com.br", "read", "").matches(&c));
        assert!(rule("domain", "mail.itau.com.br", "read", "").matches(&c));
        assert!(!rule("domain", "tau.com.br", "read", "").matches(&c), "pedaço de domínio não vale");
        assert!(!rule("domain", "com", "read", "").matches(&c));
        let mut other = cand();
        other.from_addr = "x@itau.com.br.golpe.net".into();
        assert!(!rule("domain", "itau.com.br", "read", "").matches(&other));
        other.from_addr = "Banco <avisos@itau.com.br>".into();
        assert!(rule("domain", "itau.com.br", "read", "").matches(&other), "endereço com <>");
    }

    #[test]
    fn varias_condicoes_exigem_todas() {
        let c = cand();
        let mut r = rule("from", "itau", "move", "Bancos");
        r.conditions.push(cond("subject", "fatura"));
        r.conditions.push(cond("attachment", ""));
        assert!(r.matches(&c));
        r.conditions.push(cond("subject", "boleto"));
        assert!(!r.matches(&c), "uma condição falsa derruba a regra");
        let mut no_att = cand();
        no_att.has_attachment = false;
        let mut only = rule("attachment", "yes", "star", "");
        assert!(only.matches(&c) && !only.matches(&no_att));
        only.conditions[0].value = "no".into();
        assert!(!only.matches(&c) && only.matches(&no_att));
        // Uma condição incompleta invalida a regra (não vira "casa com tudo").
        let mut inc = rule("from", "itau", "read", "");
        inc.conditions.push(cond("subject", " "));
        assert!(!inc.matches(&c));
        let mut none = rule("from", "itau", "read", "");
        none.conditions.clear();
        assert!(!none.matches(&c));
    }

    #[test]
    fn bloquear_manda_para_a_lixeira() {
        let p = plan(&[rule("from", "itau", "trash", ""), rule("from", "itau", "archive", ""), rule("from", "itau", "move", "Bancos")], &cand());
        assert!(p.trash && p.archive && p.move_to.as_deref() == Some("Bancos"));
    }

    #[test]
    fn plano_junta_acoes_e_a_primeira_de_mover_vence() {
        let rules = vec![
            rule("from", "itau", "move", "Bancos"),
            rule("subject", "fatura", "move", "Outra"),
            rule("subject", "fatura", "star", ""),
            rule("from", "itau", "category", "c1"),
            rule("from", "itau", "category", "c2"),
            rule("from", "itau", "archive", ""),
            rule("subject", "x", "read", ""),
        ];
        let p = plan(&rules, &cand());
        assert_eq!(
            p,
            Plan { read: false, star: true, category: Some("c1".into()), archive: true, trash: false, move_to: Some("Bancos".into()) }
        );
        assert!(plan(&rules[6..], &cand()).is_empty());
    }

    #[test]
    fn a_ordem_da_lista_decide_regras_em_conflito() {
        let a = rule("from", "itau", "move", "Bancos");
        let b = rule("subject", "fatura", "move", "Contas");
        assert_eq!(plan(&[a.clone(), b.clone()], &cand()).move_to.as_deref(), Some("Bancos"));
        assert_eq!(plan(&[b, a], &cand()).move_to.as_deref(), Some("Contas"));
    }

    #[test]
    fn parar_impede_as_regras_seguintes() {
        let mut first = rule("from", "itau", "move", "Bancos");
        first.stop = true;
        let rules = vec![rule("subject", "boleto", "star", ""), first, rule("from", "itau", "read", ""), rule("subject", "fatura", "move", "Contas")];
        let p = plan(&rules, &cand());
        assert_eq!((p.read, p.star, p.move_to.as_deref()), (false, false, Some("Bancos")));
        // Regra que não casa não para nada.
        let mut miss = rule("from", "naoexiste", "read", "");
        miss.stop = true;
        assert!(plan(&[miss, rule("from", "itau", "read", "")], &cand()).read);
    }

    #[test]
    fn uma_regra_pode_ter_varias_acoes() {
        let mut r = rule("from", "itau", "read", "");
        r.actions = vec![act("read", ""), act("star", ""), act("move", "Bancos"), act("category", "")];
        let p = plan(&[r], &cand());
        assert_eq!(p, Plan { read: true, star: true, category: None, archive: false, trash: false, move_to: Some("Bancos".into()) });
    }

    #[test]
    fn le_o_json_antigo_e_o_novo() {
        let old = r#"[{"id":"1","field":"from","contains":"a","action":"read"},{"id":"2","field":"to","contains":"b","action":"move","arg":"X","enabled":false}]"#;
        let r = parse_rules(Some(old));
        assert_eq!((r.len(), r[0].enabled, r[1].enabled), (2, true, false));
        assert_eq!(r[0].conditions, vec![cond("from", "a")]);
        assert_eq!(r[1].actions, vec![act("move", "X")]);
        let new = r#"[{"id":"1","name":"Bancos","conditions":[{"field":"domain","value":"itau.com.br"},{"field":"attachment"}],"actions":[{"kind":"read"},{"kind":"move","arg":"Bancos"}],"stop":true}]"#;
        let r = parse_rules(Some(new));
        assert_eq!((r[0].name.as_str(), r[0].conditions.len(), r[0].actions.len(), r[0].stop, r[0].enabled), ("Bancos", 2, 2, true, true));
        // Item inválido não derruba as outras regras.
        let mixed = r#"[{"id":"1","field":"from","contains":"a","action":"read"}, {"sem":"id"}, 7]"#;
        assert_eq!(parse_rules(Some(mixed)).len(), 1);
        assert!(parse_rules(Some("lixo")).is_empty());
        assert!(parse_rules(None).is_empty());
    }

    #[test]
    fn contagem_ignora_regras_inativas_e_incompletas() {
        let mut off = rule("from", "itau", "read", "");
        off.enabled = false;
        let rules = vec![off, rule("from", "", "read", ""), rule("subject", "fatura", "read", "")];
        let set = Ruleset::new(&rules);
        assert_eq!(set.count_matching(&[cand(), cand()]), 2);
        assert_eq!(Ruleset::new(&rules[..2]).count_matching(&[cand()]), 0);
    }

    #[test]
    fn dez_mil_mensagens_com_cinquenta_regras() {
        let rules: Vec<Rule> = (0..50)
            .map(|i| {
                let mut r = rule("from", &format!("remetente{i}@"), if i % 3 == 0 { "read" } else { "star" }, "");
                r.conditions.push(cond("subject", &format!("assunto {i} da")));
                r
            })
            .collect();
        let msgs: Vec<Candidate> = (0..10_000)
            .map(|i| Candidate {
                id: i, account: "a".into(), uid: i as u32, from_name: format!("Remetente {}", i % 80), from_addr: format!("remetente{}@dominio{}.com", i % 80, i % 7),
                subject: format!("Assunto {} da semana {i}", i % 80), to_addrs: "eu@x.com".into(), cc_addrs: String::new(), reply_addr: String::new(), date: 0,
                has_attachment: i % 5 == 0,
            })
            .collect();
        let set = Ruleset::new(&rules);
        let t = std::time::Instant::now();
        let touched = msgs.iter().filter(|m| !set.plan(m).is_empty()).count();
        let took = t.elapsed();
        eprintln!("10 mil mensagens x 50 regras: {took:?}, {touched} com ação");
        assert!(touched > 0 && touched < msgs.len());
        assert!(took.as_millis() < 3_000, "casar 10 mil mensagens demorou {took:?}");
    }

    fn new_msg(uid: u32, attachments: Vec<crate::Attachment>) -> NewMessage {
        NewMessage {
            uid, from_name: String::new(), from_addr: "a@b.com".into(), subject: format!("s{uid}"), date: uid as i64,
            body_text: String::new(), body_html: String::new(), attachments, seen: false, message_id: format!("{uid}@x"),
            refs: String::new(), reply_addr: String::new(), flagged: false, to_addrs: String::new(), cc_addrs: String::new(), ..Default::default()
        }
    }

    #[test]
    fn so_traz_novas_da_entrada() {
        let mut s = Store::open_in_memory().unwrap();
        let m = |uid: u32| new_msg(uid, vec![]);
        s.insert_messages("a", "INBOX", &[m(1), m(2)]).unwrap();
        let mark = s.max_id().unwrap();
        s.insert_messages("a", "INBOX", &[m(3)]).unwrap();
        s.insert_messages("a", "Outra", &[m(4)]).unwrap();
        s.insert_messages("b", "INBOX", &[m(5)]).unwrap();
        let new = s.inbox_after("a", mark).unwrap();
        assert_eq!(new.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["s3"]);
        assert_eq!(s.inbox_after("a", 0).unwrap().len(), 3);
    }

    #[test]
    fn candidata_sabe_se_tem_anexo() {
        let mut s = Store::open_in_memory().unwrap();
        let att = crate::Attachment { name: "a.pdf".into(), mime: "application/pdf".into(), size: 10, index: 0 };
        s.insert_messages("a", "INBOX", &[new_msg(1, vec![att]), new_msg(2, vec![])]).unwrap();
        let got = s.inbox_after("a", 0).unwrap();
        assert_eq!(got.iter().map(|c| c.has_attachment).collect::<Vec<_>>(), [true, false]);
    }
}
