//! Adiar, fixar no topo e agendar envio. Tudo local, chaveado como as categorias (sobrevive a recriar o esquema).

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::send::Outgoing;
use crate::store::{Store, MSG_KEY};
use crate::{Error, Result};

/// Quantas vezes o envio agendado tenta antes de ficar como falho.
const MAX_ATTEMPTS: i64 = 3;
/// Espera entre tentativas, em segundos.
const RETRY_AFTER: i64 = 120;

/// Item da lista "Agendados" (sem o corpo e os anexos).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScheduledItem {
    pub id: String,
    pub account: String,
    pub send_at: i64,
    /// `pending`, `sending` ou `failed`.
    pub status: String,
    pub error: String,
    pub to: String,
    pub subject: String,
}

/// Envio agendado completo, para a fila enviar ou para editar de volta no Composer.
#[derive(Debug, Clone)]
pub struct Scheduled {
    pub id: String,
    pub account: String,
    pub reply_to: Option<i64>,
    pub send_at: i64,
    pub out: Outgoing,
}

impl Store {
    /// Esconde as mensagens até `until` (Unix, segundos). Adiar de novo troca a hora.
    pub fn snooze(&mut self, ids: &[i64], until: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute(
                &format!(
                    "INSERT INTO snoozes (account, msg_key, until, woken)
                     SELECT m.account, {MSG_KEY}, ?2, 0 FROM messages m WHERE m.id = ?1
                     ON CONFLICT (account, msg_key) DO UPDATE SET until = excluded.until, woken = 0"
                ),
                params![id, until],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Cancela o adiamento: a mensagem volta à lista como estava.
    pub fn unsnooze(&mut self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute(
                &format!(
                    "DELETE FROM snoozes WHERE (account, msg_key) IN (SELECT m.account, {MSG_KEY} FROM messages m WHERE m.id = ?1)"
                ),
                params![id],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Mensagens cujo adiamento venceu: voltam ao topo como não lidas. Devolve os ids (o chamador desmarca a lida no servidor).
    pub fn wake_due(&mut self, now: i64) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let ids: Vec<i64> = {
            let mut st = tx.prepare(&format!(
                "SELECT m.id FROM messages m JOIN snoozes z ON z.account = m.account AND z.msg_key = {MSG_KEY}
                 WHERE z.woken = 0 AND z.until <= ?1"
            ))?;
            let rows = st.query_map(params![now], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        for id in &ids {
            tx.execute("UPDATE messages SET seen = 0 WHERE id = ?1", params![id])?;
        }
        tx.execute("UPDATE snoozes SET woken = 1 WHERE woken = 0 AND until <= ?1", params![now])?;
        tx.commit()?;
        Ok(ids)
    }

    pub fn set_pinned(&mut self, ids: &[i64], pinned: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            let sql = if pinned {
                format!("INSERT OR IGNORE INTO pins (account, msg_key) SELECT m.account, {MSG_KEY} FROM messages m WHERE m.id = ?1")
            } else {
                format!("DELETE FROM pins WHERE (account, msg_key) IN (SELECT m.account, {MSG_KEY} FROM messages m WHERE m.id = ?1)")
            };
            tx.execute(&sql, params![id])?;
        }
        Ok(tx.commit()?)
    }

    /// Põe a mensagem na fila para sair em `send_at`. O mesmo `id` substitui o agendamento anterior.
    pub fn schedule_send(&self, id: &str, account: &str, reply_to: Option<i64>, send_at: i64, out: &Outgoing) -> Result<()> {
        if id.trim().is_empty() {
            return Err(Error::Invalid("agendamento sem id".into()));
        }
        let payload = serde_json::to_string(out).map_err(|e| Error::Invalid(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO scheduled (id, account, reply_to, send_at, payload) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET account = excluded.account, reply_to = excluded.reply_to,
               send_at = excluded.send_at, payload = excluded.payload, status = 'pending', attempts = 0, error = ''",
            params![id, account, reply_to, send_at, payload],
        )?;
        Ok(())
    }

    pub fn list_scheduled(&self) -> Result<Vec<ScheduledItem>> {
        let mut st = self.conn.prepare("SELECT id, account, send_at, status, error, payload FROM scheduled ORDER BY send_at, id")?;
        let rows = st
            .query_map([], |r| {
                let out: Option<Outgoing> = serde_json::from_str(&r.get::<_, String>(5)?).ok();
                Ok(ScheduledItem {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    send_at: r.get(2)?,
                    status: r.get(3)?,
                    error: r.get(4)?,
                    to: out.as_ref().map(|o| o.to.clone()).unwrap_or_default(),
                    subject: out.map(|o| o.subject).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    fn scheduled_from_row(r: &rusqlite::Row) -> rusqlite::Result<Option<Scheduled>> {
        let out: Option<Outgoing> = serde_json::from_str(&r.get::<_, String>(4)?).ok();
        Ok(match out {
            Some(out) => Some(Scheduled { id: r.get(0)?, account: r.get(1)?, reply_to: r.get(2)?, send_at: r.get(3)?, out }),
            None => None,
        })
    }

    /// Tira da fila (cancelar ou editar) e devolve o conteúdo. `None` se já saiu ou não existe.
    pub fn take_scheduled(&self, id: &str) -> Result<Option<Scheduled>> {
        let found = self
            .conn
            .query_row("SELECT id, account, reply_to, send_at, payload FROM scheduled WHERE id = ?1", params![id], Self::scheduled_from_row)
            .optional()?
            .flatten();
        self.conn.execute("DELETE FROM scheduled WHERE id = ?1", params![id])?;
        Ok(found)
    }

    /// Reserva os envios cuja hora chegou (`pending` vira `sending`), para ninguém enviar duas vezes.
    pub fn claim_due(&self, now: i64) -> Result<Vec<Scheduled>> {
        let due: Vec<Scheduled> = {
            let mut st = self
                .conn
                .prepare("SELECT id, account, reply_to, send_at, payload FROM scheduled WHERE status = 'pending' AND send_at <= ?1 ORDER BY send_at")?;
            let rows = st.query_map(params![now], Self::scheduled_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().flatten().collect()
        };
        for s in &due {
            self.conn.execute("UPDATE scheduled SET status = 'sending' WHERE id = ?1", params![s.id])?;
        }
        Ok(due)
    }

    /// Ao abrir o app: o que ficou como `sending` (app fechou no meio) volta à fila.
    pub fn requeue_sending(&self) -> Result<()> {
        self.conn.execute("UPDATE scheduled SET status = 'pending' WHERE status = 'sending'", [])?;
        Ok(())
    }

    /// Saiu: sai da fila.
    pub fn scheduled_done(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM scheduled WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Falhou: tenta de novo em alguns minutos; na terceira vez fica `failed` com o erro à vista.
    pub fn scheduled_failed(&self, id: &str, error: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scheduled SET attempts = attempts + 1, error = ?2,
               status = CASE WHEN attempts + 1 >= ?4 THEN 'failed' ELSE 'pending' END,
               send_at = CASE WHEN attempts + 1 >= ?4 THEN send_at ELSE ?3 END
             WHERE id = ?1",
            params![id, error, now + RETRY_AFTER, MAX_ATTEMPTS],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewMessage;

    fn msg(uid: u32, date: i64) -> NewMessage {
        NewMessage {
            uid, from_name: "Ana".into(), from_addr: "ana@x.com".into(), subject: format!("m{uid}"), date,
            body_text: "oi".into(), body_html: String::new(), attachments: vec![], seen: true, message_id: format!("{uid}@x"),
            refs: String::new(), reply_addr: String::new(), flagged: false, to_addrs: String::new(), cc_addrs: String::new(),
        }
    }

    fn subjects(s: &Store) -> Vec<String> {
        s.list_folder(crate::Role::Inbox, 50, 0).unwrap().into_iter().map(|m| m.subject).collect()
    }

    fn out(to: &str) -> Outgoing {
        Outgoing { to: to.into(), subject: "Oi".into(), body: "corpo".into(), html: None, cc: String::new(), bcc: String::new(), attachments: vec![], from: None }
    }

    #[test]
    fn adiada_some_ate_a_hora_e_volta_ao_topo_como_nao_lida() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, 1000), msg(2, 2000), msg(3, 3000)]).unwrap();
        let id1 = s.list_folder(crate::Role::Inbox, 50, 0).unwrap().last().unwrap().id;
        let far = 4_000_000_000;
        s.snooze(&[id1], far).unwrap();
        assert_eq!(subjects(&s), ["m3", "m2"]);
        assert!(s.wake_due(far - 1).unwrap().is_empty());
        s.snooze(&[id1], 5_000).unwrap();
        assert_eq!(s.wake_due(5_001).unwrap(), [id1]);
        assert!(s.wake_due(5_002).unwrap().is_empty());
        assert_eq!(subjects(&s), ["m1", "m3", "m2"]);
        assert!(!s.list_folder(crate::Role::Inbox, 50, 0).unwrap()[0].seen);
        s.snooze(&[id1], far).unwrap();
        s.unsnooze(&[id1]).unwrap();
        assert_eq!(subjects(&s), ["m3", "m2", "m1"]);
    }

    #[test]
    fn fixada_fica_no_topo_e_desafixa() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, 1000), msg(2, 2000), msg(3, 3000)]).unwrap();
        let old = s.list_folder(crate::Role::Inbox, 50, 0).unwrap().last().unwrap().id;
        s.set_pinned(&[old], true).unwrap();
        let list = s.list_folder(crate::Role::Inbox, 50, 0).unwrap();
        assert_eq!((list[0].subject.as_str(), list[0].pinned, list[1].pinned), ("m1", true, false));
        s.set_pinned(&[old], false).unwrap();
        assert_eq!(subjects(&s), ["m3", "m2", "m1"]);
    }

    #[test]
    fn fila_de_envio_reserva_tenta_de_novo_e_falha_na_terceira() {
        let s = Store::open_in_memory().unwrap();
        s.schedule_send("a", "c", Some(7), 100, &out("x@y.com")).unwrap();
        s.schedule_send("b", "c", None, 500, &out("z@y.com")).unwrap();
        assert_eq!(
            s.list_scheduled().unwrap().iter().map(|i| (i.id.as_str(), i.to.as_str())).collect::<Vec<_>>(),
            [("a", "x@y.com"), ("b", "z@y.com")]
        );
        let due = s.claim_due(200).unwrap();
        assert_eq!((due.len(), due[0].id.as_str(), due[0].reply_to), (1, "a", Some(7)));
        assert!(s.claim_due(200).unwrap().is_empty());
        s.scheduled_failed("a", "SMTP caiu", 200).unwrap();
        assert!(s.claim_due(200).unwrap().is_empty());
        assert_eq!(s.claim_due(200 + RETRY_AFTER).unwrap().len(), 1);
        s.scheduled_failed("a", "SMTP caiu", 400).unwrap();
        s.claim_due(1000).unwrap();
        s.scheduled_failed("a", "SMTP caiu", 1000).unwrap();
        let all = s.list_scheduled().unwrap();
        let a = all.iter().find(|i| i.id == "a").unwrap();
        assert_eq!((a.status.as_str(), a.error.as_str()), ("failed", "SMTP caiu"));
        assert!(s.claim_due(9999).unwrap().is_empty());
        assert_eq!(s.take_scheduled("a").unwrap().unwrap().out.to, "x@y.com");
        assert!(s.take_scheduled("a").unwrap().is_none());
        s.claim_due(600).unwrap();
        s.requeue_sending().unwrap();
        assert_eq!(s.claim_due(600).unwrap().len(), 1, "reservado e interrompido volta à fila");
        s.scheduled_done("b").unwrap();
        assert!(s.list_scheduled().unwrap().is_empty());
    }
}
