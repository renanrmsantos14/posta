//! Sessões IMAP persistentes: uma conexão autenticada por conta e finalidade, reaproveitada entre operações.
//!
//! Abrir conexão (TCP, TLS, login, CAPABILITY, ENABLE) custa centenas de milissegundos. Em vez de pagar isso
//! a cada sync, marcação ou busca de corpo, o `Hub` guarda a sessão e só reconecta quando ela cai.

use crate::imap_sync::{connect, ImapAccount, ImapSession};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Conexão autenticada e o que o servidor anunciou nela.
pub struct Live {
    pub(crate) s: ImapSession,
    /// O servidor anuncia IDLE.
    pub idle: bool,
    /// CONDSTORE ativo nesta conexão (HIGHESTMODSEQ no SELECT e `CHANGEDSINCE` no FETCH).
    pub condstore: bool,
    /// QRESYNC ativo: o servidor informa também as exclusões (`VANISHED`).
    pub qresync: bool,
    /// MOVE (RFC 6851) anunciado.
    pub(crate) has_move: bool,
    /// UIDPLUS anunciado (EXPUNGE só dos UIDs pedidos).
    pub(crate) uidplus: bool,
}

impl Live {
    /// Conecta, autentica e liga CONDSTORE/QRESYNC quando o servidor anuncia.
    pub fn open(acc: &ImapAccount) -> Result<Live> {
        let mut s = connect(acc)?;
        let caps = s.capabilities()?;
        let (idle, can_enable) = (caps.has_str("IDLE"), caps.has_str("ENABLE"));
        let (qresync, condstore) = (caps.has_str("QRESYNC"), caps.has_str("CONDSTORE"));
        let (mut cond_on, mut q_on) = (false, false);
        if can_enable && (qresync || condstore) {
            let cmd = if qresync { "ENABLE QRESYNC" } else { "ENABLE CONDSTORE" };
            // Se o servidor recusar, a sync segue sem a extensão.
            if s.run_command_and_check_ok(cmd).is_ok() {
                cond_on = true;
                q_on = qresync;
            }
        }
        let (has_move, uidplus) = (caps.has_str("MOVE"), caps.has_str("UIDPLUS"));
        drop(caps);
        Ok(Live { s, idle, condstore: cond_on, qresync: q_on, has_move, uidplus })
    }

    pub fn logout(mut self) {
        self.s.logout().ok();
    }

    /// Conexão avulsa: abre, roda `f` e encerra. Para chamadores sem `Hub` (testes, ferramentas).
    pub fn once<T>(acc: &ImapAccount, f: impl FnOnce(&mut Live) -> Result<T>) -> Result<T> {
        let mut live = Live::open(acc)?;
        let r = f(&mut live);
        live.logout();
        r
    }
}

/// Para que serve a conexão. IDLE ocupa a sua por inteiro, então não divide com as operações.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Sync, marcações, corpos sob demanda.
    Cmd,
    /// Espera de novidades (IDLE) e sync disparada por ela.
    Idle,
}

type Slot = Arc<Mutex<Option<Live>>>;

/// Sessões por (conta, finalidade). Barato de clonar por referência (`Arc` no estado do app).
#[derive(Default)]
pub struct Hub {
    slots: Mutex<HashMap<(String, Kind), Slot>>,
}

/// Espera antes de recomeçar a tentativa `n` (0 = a conexão guardada pode só ter caído: tenta já).
fn backoff(n: usize) -> Duration {
    Duration::from_millis([0, 300, 1000][n.min(2)])
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    fn slot(&self, id: &str, kind: Kind) -> Slot {
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).entry((id.to_string(), kind)).or_default().clone()
    }

    /// Roda `f` na sessão da conta, abrindo-a se preciso. Se a conexão cair no meio (ou já estiver morta),
    /// descarta, reconecta com espera crescente e repete `f` (até 3 tentativas). Erro que não é de rede
    /// (NO/BAD, banco) volta direto e mantém a sessão.
    pub fn with<T>(&self, acc: &ImapAccount, kind: Kind, mut f: impl FnMut(&mut Live) -> Result<T>) -> Result<T> {
        let slot = self.slot(&acc.id, kind);
        let mut guard = slot.lock().unwrap_or_else(|p| p.into_inner());
        let mut last: Option<Error> = None;
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(backoff(attempt - 1));
            }
            if guard.is_none() {
                match Live::open(acc) {
                    Ok(l) => *guard = Some(l),
                    Err(e) if e.is_network() => {
                        last = Some(e);
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            match f(guard.as_mut().expect("sessão aberta acima")) {
                Ok(v) => return Ok(v),
                Err(e) if e.is_network() => {
                    *guard = None;
                    last = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| Error::Invalid("sem conexão com o servidor".into())))
    }

    /// Esquece as sessões da conta (senha trocada, conta removida): a próxima operação reconecta.
    pub fn drop_account(&self, id: &str) {
        let slots: Vec<Slot> = {
            let map = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            [Kind::Cmd, Kind::Idle].iter().filter_map(|k| map.get(&(id.to_string(), *k)).cloned()).collect()
        };
        for s in slots {
            // Sessão em uso (IDLE esperando): não trava aqui; ela cai sozinha no próximo erro de rede.
            if let Ok(mut g) = s.try_lock() {
                if let Some(live) = g.take() {
                    live.logout();
                }
            }
        }
    }

    /// Há sessão aberta para a conta e finalidade? (Para testes e diagnóstico.)
    pub fn is_open(&self, id: &str, kind: Kind) -> bool {
        self.slot(id, kind).lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }
}
