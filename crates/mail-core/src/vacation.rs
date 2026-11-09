//! Resposta automática de férias. Local: só responde com o app aberto e o período ativo.
//! Responde uma vez por remetente e nunca a listas, endereços "noreply" ou mensagens automáticas.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::search::Ymd;
use crate::store::Store;
use crate::Result;

/// Configuração `vacation` (JSON). Datas como `2026-11-08` (ou `08/11/2026`); o último dia vale inteiro.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Vacation {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body: String,
}

impl Vacation {
    pub fn parse(json: Option<&str>) -> Vacation {
        json.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default()
    }

    /// Primeiro instante do período (Unix), se a data inicial é válida.
    pub fn start_ts(&self) -> Option<i64> {
        Ymd::parse(self.start.trim()).map(Ymd::unix)
    }

    /// Instante em que o período acaba (meia-noite depois do último dia).
    pub fn end_ts(&self) -> Option<i64> {
        Ymd::parse(self.end.trim()).map(|d| d.unix() + 86_400)
    }

    /// Ligada, com texto e com `now` dentro do período. Data vazia deixa aquele lado aberto.
    pub fn active(&self, now: i64) -> bool {
        self.enabled
            && !self.body.trim().is_empty()
            && self.start_ts().is_none_or(|s| now >= s)
            && self.end_ts().is_none_or(|e| now < e)
    }
}

/// Endereços que não leem resposta.
pub fn is_noreply(addr: &str) -> bool {
    let a = addr.trim().to_lowercase();
    let local = a.split('@').next().unwrap_or("");
    let key: String = local.chars().filter(|c| c.is_alphanumeric()).collect();
    key.is_empty()
        || ["noreply", "donotreply", "naoresponda", "naoresponder", "mailerdaemon", "postmaster", "bounce", "bounces"]
            .iter()
            .any(|p| key.starts_with(p))
}

/// Cabeçalhos que indicam lista, boletim ou mensagem de robô. Olha só o bloco de cabeçalhos do email bruto.
pub fn is_automatic(raw: &[u8]) -> bool {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").or_else(|| raw.windows(2).position(|w| w == b"\n\n")).unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..end]).to_lowercase();
    for line in head.lines() {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match name.trim() {
            "auto-submitted" if value != "no" => return true,
            "precedence" if ["bulk", "list", "junk", "auto_reply"].contains(&value) => return true,
            "list-id" | "list-unsubscribe" | "x-auto-response-suppress" | "x-autoreply" | "x-autorespond" => return true,
            "return-path" if value == "<>" => return true,
            _ => {}
        }
    }
    false
}

impl Store {
    /// Já respondemos a este remetente neste período?
    pub fn vacation_replied(&self, account: &str, addr: &str, since: i64) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM vacation_replied WHERE account = ?1 AND addr = ?2 AND at >= ?3",
            params![account, addr.trim().to_lowercase(), since],
            |r| r.get::<_, i64>(0),
        )? > 0)
    }

    pub fn mark_vacation_replied(&self, account: &str, addr: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO vacation_replied (account, addr, at) VALUES (?1, ?2, ?3)
             ON CONFLICT (account, addr) DO UPDATE SET at = excluded.at",
            params![account, addr.trim().to_lowercase(), now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(enabled: bool, start: &str, end: &str) -> Vacation {
        Vacation { enabled, start: start.into(), end: end.into(), subject: "Fora".into(), body: "Volto dia 20.".into() }
    }

    #[test]
    fn so_responde_dentro_do_periodo() {
        let start = Ymd::parse("2026-11-10").unwrap().unix();
        let a = v(true, "2026-11-10", "2026-11-12");
        assert!(!a.active(start - 1));
        assert!(a.active(start));
        assert!(a.active(Ymd::parse("2026-11-12").unwrap().unix() + 86_399));
        assert!(!a.active(Ymd::parse("2026-11-13").unwrap().unix()));
        assert!(!v(false, "2026-11-10", "2026-11-12").active(start));
        assert!(v(true, "", "").active(start));
        assert!(!Vacation { body: "  ".into(), ..v(true, "", "") }.active(start));
        assert_eq!(Vacation::parse(Some("{")), Vacation::default());
    }

    #[test]
    fn reconhece_noreply() {
        for a in ["noreply@x.com", "no-reply@x.com", "do_not_reply@x.com", "nao-responda@x.com", "MAILER-DAEMON@x.com", "no.reply@x.com"] {
            assert!(is_noreply(a), "{a}");
        }
        assert!(!is_noreply("ana@x.com"));
        assert!(!is_noreply("reply@x.com"));
    }

    #[test]
    fn reconhece_mensagens_automaticas() {
        let base = "From: a@x.com\r\nSubject: oi\r\n";
        let raw = |extra: &str| format!("{base}{extra}\r\nCorpo List-Id: x\r\n").into_bytes();
        assert!(!is_automatic(&raw("")));
        assert!(!is_automatic(&raw("Auto-Submitted: no\r\n")));
        assert!(is_automatic(&raw("Auto-Submitted: auto-replied\r\n")));
        assert!(is_automatic(&raw("Precedence: bulk\r\n")));
        assert!(is_automatic(&raw("List-Unsubscribe: <mailto:x@y.com>\r\n")));
        assert!(is_automatic(&raw("List-Id: <l.x.com>\r\n")));
        assert!(is_automatic(&raw("Return-Path: <>\r\n")));
        assert!(is_automatic(&raw("X-Auto-Response-Suppress: All\r\n")));
    }

    #[test]
    fn responde_uma_vez_por_remetente_no_periodo() {
        let s = Store::open_in_memory().unwrap();
        assert!(!s.vacation_replied("a", "Ana@X.com", 100).unwrap());
        s.mark_vacation_replied("a", "Ana@X.com", 150).unwrap();
        assert!(s.vacation_replied("a", "ana@x.com", 100).unwrap());
        assert!(!s.vacation_replied("a", "ana@x.com", 200).unwrap());
        assert!(!s.vacation_replied("b", "ana@x.com", 100).unwrap());
    }
}
