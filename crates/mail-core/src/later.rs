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
/// Saiu com mais atraso que isso (o relógio do app roda a cada 20 s): o app estava fechado ou sem rede.
pub const LATE_AFTER: i64 = 120;

/// Item da lista "Agendados" (sem o corpo e os anexos).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScheduledItem {
    pub id: String,
    pub account: String,
    pub send_at: i64,
    /// `pending`, `sending` ou `failed`.
    pub status: String,
    pub error: String,
    /// Tentativas que já falharam (a lista mostra "tentando de novo").
    pub attempts: i64,
    pub to: String,
    pub subject: String,
}

/// Como terminou uma tentativa de envio da fila.
#[derive(Debug, Clone, PartialEq)]
pub enum DueOutcome {
    /// Saiu pelo SMTP.
    Sent,
    /// Sem rede: foi para a Caixa de saída offline, que envia quando a conexão volta.
    Offline,
    /// Falhou, com o motivo.
    Failed(String),
}

/// O que uma rodada da fila fez; a UI transforma em aviso.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct DueReport {
    /// Saíram agora.
    pub sent: u32,
    /// Dos que saíram, quantos já estavam atrasados (app fechado na hora).
    pub late: u32,
    /// Foram para a Caixa de saída offline por falta de rede.
    pub offline: u32,
    /// Falharam e serão tentados de novo.
    pub retrying: u32,
    /// Esgotaram as tentativas e ficaram como falha.
    pub failed: u32,
    /// Assunto e motivo do último que esgotou as tentativas.
    pub last_error: Option<String>,
}

impl DueReport {
    pub fn is_empty(&self) -> bool {
        self.sent == 0 && self.offline == 0 && self.retrying == 0 && self.failed == 0
    }
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

/// Mantém as colunas desnormalizadas (`pinned`, `snooze_until`) de todas as cópias da mensagem
/// (mesma chave: Message-ID, ou pasta/UID quando não há) iguais às tabelas `pins` e `snoozes`.
fn mark(tx: &rusqlite::Connection, id: i64, col: &str, value: i64) -> Result<()> {
    let key: Option<(String, String)> = tx
        .query_row("SELECT account, message_id FROM messages WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((account, message_id)) = key else { return Ok(()) };
    if message_id.is_empty() {
        tx.execute(&format!("UPDATE messages SET {col} = ?2 WHERE id = ?1"), params![id, value])?;
    } else {
        tx.execute(
            &format!("UPDATE messages SET {col} = ?3 WHERE account = ?1 AND message_id = ?2"),
            params![account, message_id, value],
        )?;
    }
    Ok(())
}

impl Store {
    /// Esconde as mensagens até `until` (Unix, segundos). Adiar de novo troca a hora. `now` é o relógio de quem chama:
    /// hora no passado (ou agora) é recusada, porque a mensagem sumiria da lista sem motivo.
    pub fn snooze(&mut self, ids: &[i64], until: i64, now: i64) -> Result<()> {
        if until <= now {
            return Err(Error::Invalid("o horário para adiar já passou".into()));
        }
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
            mark(&tx, *id, "snooze_until", until)?;
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
            mark(&tx, *id, "snooze_until", 0)?;
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
            mark(&tx, *id, "pinned", pinned as i64)?;
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
        let mut st = self.conn.prepare("SELECT id, account, send_at, status, error, payload, attempts FROM scheduled ORDER BY send_at, id")?;
        let rows = st
            .query_map([], |r| {
                let out: Option<Outgoing> = serde_json::from_str(&r.get::<_, String>(5)?).ok();
                Ok(ScheduledItem {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    send_at: r.get(2)?,
                    status: r.get(3)?,
                    error: r.get(4)?,
                    attempts: r.get(6)?,
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
    /// Devolve `true` quando esgotou as tentativas.
    pub fn scheduled_failed(&self, id: &str, error: &str, now: i64) -> Result<bool> {
        self.conn.execute(
            "UPDATE scheduled SET attempts = attempts + 1, error = ?2,
               status = CASE WHEN attempts + 1 >= ?4 THEN 'failed' ELSE 'pending' END,
               send_at = CASE WHEN attempts + 1 >= ?4 THEN send_at ELSE ?3 END
             WHERE id = ?1",
            params![id, error, now + RETRY_AFTER, MAX_ATTEMPTS],
        )?;
        let status: Option<String> = self.conn.query_row("SELECT status FROM scheduled WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
        Ok(status.as_deref() == Some("failed"))
    }

    /// "Tentar de novo" numa que ficou como falha: volta à fila para sair já, com as tentativas zeradas.
    pub fn scheduled_retry(&self, id: &str, now: i64) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE scheduled SET status = 'pending', attempts = 0, error = '', send_at = ?2 WHERE id = ?1 AND status = 'failed'",
            params![id, now],
        )?;
        Ok(n > 0)
    }

    /// Registra o resultado de uma tentativa e soma ao relatório. O envio em si acontece fora, sem segurar o banco.
    pub fn settle_due(&self, s: &Scheduled, outcome: DueOutcome, now: i64, report: &mut DueReport) -> Result<()> {
        match outcome {
            DueOutcome::Sent => {
                self.scheduled_done(&s.id)?;
                report.sent += 1;
                if now - s.send_at > LATE_AFTER {
                    report.late += 1;
                }
            }
            DueOutcome::Offline => {
                self.scheduled_done(&s.id)?;
                report.offline += 1;
            }
            DueOutcome::Failed(error) => {
                if self.scheduled_failed(&s.id, &error, now)? {
                    report.failed += 1;
                    report.last_error = Some(format!("{}: {error}", if s.out.subject.trim().is_empty() { "(sem assunto)" } else { s.out.subject.trim() }));
                } else {
                    report.retrying += 1;
                }
            }
        }
        Ok(())
    }

    /// Uma rodada: reserva o que venceu, envia com `send` (o caminho real é o SMTP) e registra cada resultado.
    /// Sem segurar o banco durante o envio só quando o chamador separa `claim_due` e `settle_due`; aqui é a versão simples, usada nos testes.
    pub fn run_due(&self, now: i64, mut send: impl FnMut(&Scheduled) -> DueOutcome) -> Result<DueReport> {
        let mut report = DueReport::default();
        for s in self.claim_due(now)? {
            let outcome = send(&s);
            self.settle_due(&s, outcome, now, &mut report)?;
        }
        Ok(report)
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
            refs: String::new(), reply_addr: String::new(), flagged: false, to_addrs: String::new(), cc_addrs: String::new(), ..Default::default()
        }
    }

    fn subjects(s: &Store) -> Vec<String> {
        s.list_folder(crate::Role::Inbox, 50, 0).unwrap().into_iter().map(|m| m.subject).collect()
    }

    fn out(to: &str) -> Outgoing {
        Outgoing { to: to.into(), subject: "Oi".into(), body: "corpo".into(), html: None, cc: String::new(), bcc: String::new(), attachments: vec![], from: None, importance: None }
    }

    #[test]
    fn adiada_some_ate_a_hora_e_volta_ao_topo_como_nao_lida() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, 1000), msg(2, 2000), msg(3, 3000)]).unwrap();
        let id1 = s.list_folder(crate::Role::Inbox, 50, 0).unwrap().last().unwrap().id;
        let far = 4_000_000_000;
        s.snooze(&[id1], far, 0).unwrap();
        assert_eq!(subjects(&s), ["m3", "m2"]);
        assert!(s.wake_due(far - 1).unwrap().is_empty());
        s.snooze(&[id1], 5_000, 0).unwrap();
        assert_eq!(s.wake_due(5_001).unwrap(), [id1]);
        assert!(s.wake_due(5_002).unwrap().is_empty());
        assert_eq!(subjects(&s), ["m1", "m3", "m2"]);
        assert!(!s.list_folder(crate::Role::Inbox, 50, 0).unwrap()[0].seen);
        s.snooze(&[id1], far, 0).unwrap();
        s.unsnooze(&[id1]).unwrap();
        assert_eq!(subjects(&s), ["m3", "m2", "m1"]);
    }

    #[test]
    fn adiar_para_o_passado_e_recusado_e_nada_muda() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, 1000), msg(2, 2000)]).unwrap();
        let id = s.list_folder(crate::Role::Inbox, 50, 0).unwrap()[0].id;
        let (now, far) = (3_999_999_999, 4_000_000_000);
        for until in [now - 1, now] {
            let e = s.snooze(&[id], until, now).unwrap_err();
            assert!(matches!(e, Error::Invalid(_)), "{e:?}");
        }
        assert_eq!(subjects(&s), ["m2", "m1"], "a mensagem continua na lista");
        s.snooze(&[id], far, now).unwrap();
        assert_eq!(subjects(&s), ["m1"]);
    }

    #[test]
    fn adiada_na_virada_do_dia_volta_no_primeiro_acordar_depois_da_hora() {
        // 31/12/2026 23:59 em Brasília = 01/01/2027 02:59 UTC; "amanhã 8h" = 01/01/2027 11:00 UTC.
        let mut s = Store::open_in_memory().unwrap();
        s.insert_messages("a", "INBOX", &[msg(1, 1000)]).unwrap();
        let id = s.list_folder(crate::Role::Inbox, 50, 0).unwrap()[0].id;
        let (now, at) = (1_798_772_340, 1_798_801_200);
        assert_eq!(at - now, 8 * 3600 + 60, "8h01 depois");
        s.snooze(&[id], at, now).unwrap();
        assert!(s.wake_due(at - 1).unwrap().is_empty());
        assert_eq!(s.wake_due(at).unwrap(), [id]);
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

    #[test]
    fn fila_sobrevive_a_reiniciar_e_envia_o_atrasado_ao_abrir() {
        let path = std::env::temp_dir().join(format!("posta-fila-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let s = Store::open(&path).unwrap();
            s.schedule_send("a", "c1", None, 1000, &out("a@x.com")).unwrap();
            s.schedule_send("b", "c1", None, 9000, &out("b@x.com")).unwrap();
            // O app fechou no meio de um envio: o item ficou reservado.
            assert_eq!(s.claim_due(1000).unwrap().len(), 1);
        }
        // Reabre horas depois: nada se perdeu e o reservado volta à fila.
        let s = Store::open(&path).unwrap();
        s.requeue_sending().unwrap();
        let ids: Vec<_> = s.list_scheduled().unwrap().into_iter().map(|i| (i.id, i.status)).collect();
        assert_eq!(ids, vec![("a".into(), "pending".into()), ("b".into(), "pending".into())]);
        let mut sent = vec![];
        let rep = s.run_due(5000, |x| { sent.push(x.id.clone()); DueOutcome::Sent }).unwrap();
        assert_eq!(sent, vec!["a"], "só o que venceu sai; o de 9000 espera");
        assert_eq!((rep.sent, rep.late), (1, 1), "saiu 4000 s depois da hora: conta como atrasado");
        assert_eq!(s.list_scheduled().unwrap().len(), 1);
        drop(s);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_horario_nao_e_atraso_e_sem_rede_vai_para_a_caixa_de_saida() {
        let s = Store::open_in_memory().unwrap();
        s.schedule_send("a", "c1", None, 1000, &out("a@x.com")).unwrap();
        s.schedule_send("b", "c1", None, 1000, &out("b@x.com")).unwrap();
        let rep = s.run_due(1000 + 20, |x| if x.id == "a" { DueOutcome::Sent } else { DueOutcome::Offline }).unwrap();
        assert_eq!((rep.sent, rep.late, rep.offline), (1, 0, 1));
        assert!(s.list_scheduled().unwrap().is_empty(), "os dois saíram da fila de agendados");
    }

    #[test]
    fn falha_tenta_de_novo_avisa_e_na_terceira_fica_como_falha() {
        let s = Store::open_in_memory().unwrap();
        s.schedule_send("a", "c1", None, 1000, &out("a@x.com")).unwrap();
        let fail = |_: &Scheduled| DueOutcome::Failed("servidor recusou".into());
        let r1 = s.run_due(1000, fail).unwrap();
        assert_eq!((r1.retrying, r1.failed), (1, 0));
        let item = &s.list_scheduled().unwrap()[0];
        assert_eq!((item.status.as_str(), item.attempts, item.send_at), ("pending", 1, 1000 + RETRY_AFTER));
        assert!(s.run_due(1000 + 10, fail).unwrap().is_empty(), "ainda não chegou a hora de tentar de novo");
        s.run_due(1000 + RETRY_AFTER, fail).unwrap();
        let r3 = s.run_due(1000 + 2 * RETRY_AFTER, fail).unwrap();
        assert_eq!((r3.retrying, r3.failed), (0, 1));
        assert_eq!(r3.last_error.as_deref(), Some("Oi: servidor recusou"));
        assert_eq!(s.list_scheduled().unwrap()[0].status, "failed");
        // "Tentar de novo": volta à fila e sai.
        assert!(s.scheduled_retry("a", 5000).unwrap());
        assert!(!s.scheduled_retry("a", 5000).unwrap(), "só vale para quem está como falha");
        let ok = s.run_due(5000, |_| DueOutcome::Sent).unwrap();
        assert_eq!(ok.sent, 1);
    }

    #[test]
    fn varios_agendados_saem_em_ordem_de_hora() {
        let s = Store::open_in_memory().unwrap();
        for (id, at) in [("c", 300), ("a", 100), ("b", 200)] {
            s.schedule_send(id, "c1", None, at, &out("x@y.com")).unwrap();
        }
        let mut order = vec![];
        s.run_due(1000, |x| { order.push(x.id.clone()); DueOutcome::Sent }).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }
}
