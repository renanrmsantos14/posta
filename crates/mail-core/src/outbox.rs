//! Fila offline: ações (lida, estrela, mover, excluir) e envios feitos sem rede ficam no SQLite
//! e são reaplicados no servidor, em ordem, quando a conexão volta (`flush`).

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::imap_sync::{self, ImapAccount, MsgFlag};
use crate::send::{self, Outgoing};
use crate::store::{ReplyRef, Store};
use crate::{Error, Result};

/// O que a ação faz no servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OpKind {
    Seen,
    Unseen,
    Star,
    Unstar,
    /// Mover para a pasta `arg`.
    Move,
    /// Excluir de vez (só vale dentro da Lixeira).
    Delete,
}

impl OpKind {
    fn as_str(self) -> &'static str {
        match self {
            OpKind::Seen => "seen",
            OpKind::Unseen => "unseen",
            OpKind::Star => "star",
            OpKind::Unstar => "unstar",
            OpKind::Move => "move",
            OpKind::Delete => "delete",
        }
    }

    fn parse(s: &str) -> Option<OpKind> {
        Some(match s {
            "seen" => OpKind::Seen,
            "unseen" => OpKind::Unseen,
            "star" => OpKind::Star,
            "unstar" => OpKind::Unstar,
            "move" => OpKind::Move,
            "delete" => OpKind::Delete,
            _ => return None,
        })
    }

    /// Flag de lida/estrela, ligada ou desligada; `None` para mover e excluir.
    pub fn flag(self) -> Option<(MsgFlag, bool)> {
        match self {
            OpKind::Seen => Some((MsgFlag::Seen, true)),
            OpKind::Unseen => Some((MsgFlag::Seen, false)),
            OpKind::Star => Some((MsgFlag::Flagged, true)),
            OpKind::Unstar => Some((MsgFlag::Flagged, false)),
            OpKind::Move | OpKind::Delete => None,
        }
    }
}

/// Ação à espera do servidor. Guarda conta/pasta/UID (e não o id local): a cópia local pode já ter saído.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingOp {
    pub id: i64,
    pub account: String,
    pub mailbox: String,
    pub uid: u32,
    pub kind: OpKind,
    pub arg: String,
}

/// Mensagem na Caixa de saída. `error` não vazio = o servidor recusou; fica parada até o usuário decidir.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutboxItem {
    pub id: i64,
    pub account: String,
    pub to: String,
    pub subject: String,
    /// Unix, em segundos.
    pub created: i64,
    pub error: String,
}

/// O que um `flush` fez.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct FlushReport {
    /// Ações reaplicadas no servidor.
    pub applied: usize,
    /// Ações descartadas: o servidor recusou (a mensagem sumiu, a pasta não existe mais) ou a conta foi removida.
    pub discarded: usize,
    /// Mensagens da Caixa de saída enviadas.
    pub sent: usize,
    /// Envios que o servidor recusou; continuam na Caixa de saída com o erro.
    pub failed: usize,
    /// Parou por falta de rede: o que sobrou será tentado depois.
    pub offline: bool,
}

#[derive(Serialize, Deserialize)]
struct Payload {
    out: Outgoing,
    reply: Option<ReplyRef>,
}

impl Store {
    pub fn enqueue_op(&self, account: &str, mailbox: &str, uid: u32, kind: OpKind, arg: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO pending_ops (account, mailbox, uid, kind, arg) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![account, mailbox, uid, kind.as_str(), arg],
        )?;
        Ok(())
    }

    /// Em ordem de criação.
    pub fn pending_ops(&self) -> Result<Vec<PendingOp>> {
        let mut st = self.conn.prepare("SELECT id, account, mailbox, uid, kind, arg FROM pending_ops ORDER BY id")?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, String>(4)?, r.get(5)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, account, mailbox, uid, kind, arg)| {
                Some(PendingOp { id, account, mailbox, uid, kind: OpKind::parse(&kind)?, arg })
            })
            .collect())
    }

    pub fn pending_count(&self) -> Result<u32> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM pending_ops", [], |r| r.get(0))?)
    }

    fn drop_ops(&self, ids: &[i64]) -> Result<()> {
        for id in ids {
            self.conn.execute("DELETE FROM pending_ops WHERE id = ?1", params![id])?;
        }
        Ok(())
    }

    /// Guarda uma mensagem para enviar quando houver rede. Devolve o id na Caixa de saída.
    pub fn enqueue_send(&self, account: &str, out: &Outgoing, reply: Option<&ReplyRef>) -> Result<i64> {
        let payload = serde_json::to_string(&Payload { out: out.clone(), reply: reply.cloned() })
            .map_err(|e| Error::Invalid(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO outbox (account, to_addr, subject, payload) VALUES (?1, ?2, ?3, ?4)",
            params![account, out.to, out.subject, payload],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn outbox(&self) -> Result<Vec<OutboxItem>> {
        let mut st = self.conn.prepare("SELECT id, account, to_addr, subject, created, error FROM outbox ORDER BY id")?;
        let rows = st
            .query_map([], |r| {
                Ok(OutboxItem { id: r.get(0)?, account: r.get(1)?, to: r.get(2)?, subject: r.get(3)?, created: r.get(4)?, error: r.get(5)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn outbox_count(&self) -> Result<u32> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))?)
    }

    /// Tira da Caixa de saída (descartar ou já enviada).
    pub fn drop_outbox(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM outbox WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Limpa o erro para o próximo `flush` tentar de novo.
    pub fn retry_outbox(&self, id: i64) -> Result<()> {
        self.conn.execute("UPDATE outbox SET error = '' WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn set_outbox_error(&self, id: i64, error: &str) -> Result<()> {
        self.conn.execute("UPDATE outbox SET error = ?2 WHERE id = ?1", params![id, error])?;
        Ok(())
    }

    fn outbox_payload(&self, id: i64) -> Result<Payload> {
        let raw: String = self.conn.query_row("SELECT payload FROM outbox WHERE id = ?1", params![id], |r| r.get(0))?;
        serde_json::from_str(&raw).map_err(|e| Error::Invalid(format!("item da Caixa de saída corrompido: {e}")))
    }
}

/// Aplica no servidor uma ação sobre UIDs da mesma pasta.
pub fn apply_op(acc: &ImapAccount, mailbox: &str, uids: &[u32], kind: OpKind, arg: &str) -> Result<()> {
    match kind.flag() {
        Some((flag, on)) => imap_sync::set_flag(acc, mailbox, uids, flag, on),
        None if kind == OpKind::Move => imap_sync::move_messages(acc, mailbox, uids, arg),
        None => imap_sync::delete_forever_many(acc, mailbox, uids),
    }
}

/// Reaplica a fila no servidor: primeiro as ações, depois os envios, cada um na ordem em que foi feito.
/// `resolve` entrega a conta com senha/token: `Ok(None)` = a conta foi removida (a fila dela é descartada);
/// `Err` = não deu para obter as credenciais agora (a fila dela espera).
/// Falha de rede interrompe tudo e preserva o resto; recusa do servidor descarta só o item afetado.
pub fn flush(store: &Store, resolve: &mut dyn FnMut(&str) -> Result<Option<ImapAccount>>) -> Result<FlushReport> {
    let mut report = FlushReport::default();
    let mut blocked: Vec<String> = Vec::new();

    let ops = store.pending_ops()?;
    let mut i = 0;
    while i < ops.len() {
        // Ações seguidas iguais na mesma pasta vão numa conexão só.
        let first = &ops[i];
        let n = ops[i..]
            .iter()
            .take_while(|o| o.account == first.account && o.mailbox == first.mailbox && o.kind == first.kind && o.arg == first.arg)
            .count();
        let group = &ops[i..i + n];
        i += n;
        let ids: Vec<i64> = group.iter().map(|o| o.id).collect();
        if blocked.contains(&first.account) {
            continue;
        }
        let acc = match resolve(&first.account) {
            Ok(Some(a)) => a,
            Ok(None) => {
                store.drop_ops(&ids)?;
                report.discarded += n;
                continue;
            }
            Err(_) => {
                blocked.push(first.account.clone());
                continue;
            }
        };
        let uids: Vec<u32> = group.iter().map(|o| o.uid).collect();
        match apply_op(&acc, &first.mailbox, &uids, first.kind, &first.arg) {
            Ok(()) => {
                store.drop_ops(&ids)?;
                report.applied += n;
            }
            Err(e) if e.is_network() => {
                report.offline = true;
                return Ok(report);
            }
            Err(_) => {
                store.drop_ops(&ids)?;
                report.discarded += n;
            }
        }
    }

    for item in store.outbox()?.into_iter().filter(|m| m.error.is_empty()) {
        if blocked.contains(&item.account) {
            continue;
        }
        let acc = match resolve(&item.account) {
            Ok(Some(a)) => a,
            Ok(None) => {
                store.drop_outbox(item.id)?;
                continue;
            }
            Err(_) => {
                blocked.push(item.account.clone());
                continue;
            }
        };
        let sent = store.outbox_payload(item.id).and_then(|p| send::send(&acc, &p.out, p.reply.as_ref()));
        match sent {
            Ok(_) => {
                store.drop_outbox(item.id)?;
                report.sent += 1;
            }
            Err(e) if e.is_network() => {
                report.offline = true;
                return Ok(report);
            }
            Err(e) => {
                store.set_outbox_error(item.id, &e.to_string())?;
                report.failed += 1;
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imap_sync::{AuthMode, Security};

    /// Conta apontando para uma porta fechada: toda conexão falha como falta de rede.
    fn dead(id: &str) -> ImapAccount {
        ImapAccount {
            id: id.into(),
            host: "127.0.0.1".into(),
            port: 1,
            security: Security::Plaintext,
            username: format!("{id}@exemplo.com"),
            name: None,
            smtp: Some(crate::SmtpServer { host: "127.0.0.1".into(), port: 1, security: Security::Plaintext }),
            auth: AuthMode::Password,
            password: "x".into(),
        }
    }

    fn out(to: &str) -> Outgoing {
        Outgoing { to: to.into(), subject: "Oi".into(), body: "Texto".into(), html: None, cc: String::new(), bcc: String::new(), attachments: vec![] }
    }

    #[test]
    fn fila_guarda_na_ordem_e_sobrevive_a_reabrir() {
        let dir = std::env::temp_dir().join(format!("posta-fila-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.sqlite");
        {
            let s = Store::open(&path).unwrap();
            s.enqueue_op("a", "INBOX", 7, OpKind::Seen, "").unwrap();
            s.enqueue_op("a", "INBOX", 7, OpKind::Move, "Archive").unwrap();
            s.enqueue_send("a", &out("ana@x.com"), None).unwrap();
        }
        let s = Store::open(&path).unwrap();
        let ops = s.pending_ops().unwrap();
        assert_eq!(ops.iter().map(|o| (o.uid, o.kind, o.arg.as_str())).collect::<Vec<_>>(), [(7, OpKind::Seen, ""), (7, OpKind::Move, "Archive")]);
        assert_eq!((s.pending_count().unwrap(), s.outbox_count().unwrap()), (2, 1));
        let item = &s.outbox().unwrap()[0];
        assert_eq!((item.to.as_str(), item.subject.as_str(), item.error.as_str()), ("ana@x.com", "Oi", ""));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sem_rede_mantem_a_fila_intacta() {
        let s = Store::open_in_memory().unwrap();
        s.enqueue_op("a", "INBOX", 1, OpKind::Star, "").unwrap();
        s.enqueue_op("a", "INBOX", 2, OpKind::Star, "").unwrap();
        s.enqueue_send("a", &out("ana@x.com"), None).unwrap();
        let r = flush(&s, &mut |id| Ok(Some(dead(id)))).unwrap();
        assert_eq!(r, FlushReport { offline: true, ..Default::default() });
        assert_eq!((s.pending_count().unwrap(), s.outbox_count().unwrap()), (2, 1));
        assert!(s.outbox().unwrap()[0].error.is_empty());
    }

    #[test]
    fn conta_removida_descarta_e_credencial_indisponivel_espera() {
        let s = Store::open_in_memory().unwrap();
        s.enqueue_op("sumiu", "INBOX", 1, OpKind::Seen, "").unwrap();
        s.enqueue_op("espera", "INBOX", 1, OpKind::Seen, "").unwrap();
        s.enqueue_send("sumiu", &out("ana@x.com"), None).unwrap();
        let r = flush(&s, &mut |id| if id == "sumiu" { Ok(None) } else { Err(Error::Invalid("cofre bloqueado".into())) }).unwrap();
        assert_eq!((r.discarded, r.offline), (1, false));
        let left = s.pending_ops().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].account, "espera");
        assert_eq!(s.outbox_count().unwrap(), 0);
    }

    #[test]
    fn envio_recusado_fica_na_caixa_de_saida_com_o_erro() {
        let s = Store::open_in_memory().unwrap();
        let id = s.enqueue_send("a", &out("sem-arroba"), None).unwrap();
        let r = flush(&s, &mut |id| Ok(Some(dead(id)))).unwrap();
        assert_eq!((r.failed, r.sent, r.offline), (1, 0, false));
        let item = &s.outbox().unwrap()[0];
        assert!(item.error.contains("endereço inválido"), "{}", item.error);
        // Parado: o próximo flush não tenta de novo até o usuário pedir.
        assert_eq!(flush(&s, &mut |id| Ok(Some(dead(id)))).unwrap(), FlushReport::default());
        s.retry_outbox(id).unwrap();
        assert!(s.outbox().unwrap()[0].error.is_empty());
        s.drop_outbox(id).unwrap();
        assert_eq!(s.outbox_count().unwrap(), 0);
    }

    #[test]
    fn so_falha_de_rede_conta_como_rede() {
        assert!(Error::Imap(imap::Error::ConnectionLost).is_network());
        assert!(Error::Imap(imap::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused))).is_network());
        assert!(!Error::Invalid("x".into()).is_network());
    }

    #[test]
    fn remover_conta_limpa_a_fila_dela() {
        let mut s = Store::open_in_memory().unwrap();
        s.enqueue_op("a", "INBOX", 1, OpKind::Seen, "").unwrap();
        s.enqueue_op("b", "INBOX", 1, OpKind::Seen, "").unwrap();
        s.enqueue_send("a", &out("ana@x.com"), None).unwrap();
        s.remove_account("a").unwrap();
        assert_eq!((s.pending_count().unwrap(), s.outbox_count().unwrap()), (1, 0));
    }
}
