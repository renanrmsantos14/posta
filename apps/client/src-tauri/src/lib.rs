//! Ponte entre a UI e o mail-core. Leituras vêm do SQLite local; rede só em sync e flags.

use mail_core::{imap_sync, ImapAccount, MessageDetail, MessageSummary, Store};
use serde::Serialize;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager, State};

const INBOX: &str = "INBOX";
const KEYRING_SERVICE: &str = "posta";

struct AppState {
    dir: PathBuf,
    /// Conexão de leitura da UI. A sincronização abre a sua (WAL permite as duas).
    store: Mutex<Store>,
    syncing: Mutex<()>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl AppState {
    fn db_path(&self) -> PathBuf {
        self.dir.join("mail.sqlite")
    }

    fn accounts_path(&self) -> PathBuf {
        self.dir.join("accounts.json")
    }

    fn accounts(&self) -> CmdResult<Vec<ImapAccount>> {
        match fs::read(self.accounts_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(err),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(err(e)),
        }
    }

    fn save_accounts(&self, accounts: &[ImapAccount]) -> CmdResult<()> {
        fs::write(self.accounts_path(), serde_json::to_vec_pretty(accounts).map_err(err)?).map_err(err)
    }

    fn account_with_password(&self, id: &str) -> CmdResult<ImapAccount> {
        let mut acc = self
            .accounts()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("conta {id} não encontrada"))?;
        acc.password = keyring::Entry::new(KEYRING_SERVICE, id)
            .and_then(|e| e.get_password())
            .map_err(|e| format!("senha de {id} no cofre do sistema: {e}"))?;
        Ok(acc)
    }
}

/// Roda trabalho de rede/disco fora da thread da janela.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn add_imap_account(app: AppHandle, mut account: ImapAccount) -> CmdResult<ImapAccount> {
    blocking(app, move |st| {
        imap_sync::check_login(&account).map_err(err)?;
        account.id = format!("{}@{}", account.username, account.host);
        keyring::Entry::new(KEYRING_SERVICE, &account.id)
            .and_then(|e| e.set_password(&account.password))
            .map_err(|e| format!("não consegui guardar a senha no cofre do sistema: {e}"))?;
        let mut all = st.accounts()?;
        all.retain(|a| a.id != account.id);
        all.push(account.clone());
        st.save_accounts(&all)?;
        Ok(account)
    })
    .await
}

#[tauri::command]
fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<ImapAccount>> {
    state.accounts()
}

#[derive(Serialize, Default)]
struct SyncResult {
    fetched: usize,
    errors: Vec<String>,
    /// true quando outra sincronização já estava em andamento.
    skipped: bool,
}

#[tauri::command]
async fn sync_inbox(app: AppHandle) -> CmdResult<SyncResult> {
    blocking(app, |st| {
        let Ok(_guard) = st.syncing.try_lock() else {
            return Ok(SyncResult { skipped: true, ..Default::default() });
        };
        let mut store = Store::open(st.db_path()).map_err(err)?;
        let mut result = SyncResult::default();
        for acc in st.accounts()? {
            match st
                .account_with_password(&acc.id)
                .and_then(|a| imap_sync::sync_mailbox(&a, &mut store, INBOX).map_err(err))
            {
                Ok(r) => result.fetched += r.fetched,
                Err(e) => result.errors.push(format!("{}: {e}", acc.id)),
            }
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
fn list_messages(state: State<'_, AppState>, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().list(INBOX, limit, offset).map_err(err)
}

#[tauri::command]
fn search_messages(state: State<'_, AppState>, query: String) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().search(&query, 200).map_err(err)
}

/// Abre a mensagem e, se estava não lida, marca como lida aqui e no servidor.
#[tauri::command]
async fn open_message(app: AppHandle, id: i64) -> CmdResult<Option<MessageDetail>> {
    let st = app.state::<AppState>();
    let detail = {
        let store = st.store.lock().unwrap();
        let Some(mut d) = store.get(id).map_err(err)? else {
            return Ok(None);
        };
        if d.summary.seen {
            return Ok(Some(d));
        }
        store.set_seen(id, true).map_err(err)?;
        d.summary.seen = true;
        d
    };
    let (account, uid) = (detail.summary.account.clone(), detail.summary.uid);
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        if let Err(e) = st
            .account_with_password(&account)
            .and_then(|a| imap_sync::set_seen(&a, INBOX, uid, true).map_err(err))
        {
            eprintln!("marcar lida no servidor falhou ({account}/{uid}): {e}");
        }
    });
    Ok(Some(detail))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let store = Store::open(dir.join("mail.sqlite"))?;
            app.manage(AppState { dir, store: Mutex::new(store), syncing: Mutex::new(()) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_imap_account,
            list_accounts,
            sync_inbox,
            list_messages,
            search_messages,
            open_message
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Posta");
}
