//! Ponte entre a UI e o mail-core. Leituras vêm do SQLite local; rede só em sync e flags.

use mail_core::{imap_sync, send, ImapAccount, MessageDetail, MessageSummary, Outgoing, ReplyRef, Role, SendReport, Store};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager, State};

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

    /// Pastas da conta; descobre no servidor na primeira vez.
    fn ensure_folders(&self, store: &mut Store, acc: &ImapAccount) -> CmdResult<()> {
        if store.folders(&acc.id).map_err(err)?.is_empty() {
            let found = imap_sync::discover_folders(acc).map_err(err)?;
            store.set_folders(&acc.id, &found).map_err(err)?;
        }
        Ok(())
    }

    /// Conta, pasta e UID de uma mensagem local.
    fn locate(&self, id: i64) -> CmdResult<ReplyRef> {
        self.store
            .lock()
            .unwrap()
            .reply_ref(id)
            .map_err(err)?
            .ok_or_else(|| "mensagem não encontrada".to_string())
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
        // Falha na descoberta não impede adicionar; o próximo sync tenta de novo.
        if let Ok(f) = imap_sync::discover_folders(&account) {
            st.store.lock().unwrap().set_folders(&account.id, &f).ok();
        }
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
    /// Mensagens que existem no servidor nas pastas sincronizadas.
    total: usize,
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
            let r = st.account_with_password(&acc.id).and_then(|a| {
                if let Err(e) = st.ensure_folders(&mut store, &a) {
                    result.errors.push(format!("{}: pastas: {e}", acc.id));
                }
                imap_sync::sync_mailbox(&a, &mut store, "INBOX", None).map_err(err)
            });
            match r {
                Ok(r) => {
                    result.fetched += r.fetched;
                    result.total += r.total;
                }
                Err(e) => result.errors.push(format!("{}: {e}", acc.id)),
            }
        }
        Ok(result)
    })
    .await
}

/// Papéis presentes em ao menos uma conta (Inbox sempre), na ordem de `Role::ALL`.
#[tauri::command]
fn list_folders(state: State<'_, AppState>) -> CmdResult<Vec<Role>> {
    let accounts = state.accounts()?;
    let store = state.store.lock().unwrap();
    let mut present = vec![Role::Inbox];
    for acc in accounts {
        for f in store.folders(&acc.id).map_err(err)? {
            if !present.contains(&f.role) {
                present.push(f.role);
            }
        }
    }
    present.sort_by_key(|r| Role::ALL.iter().position(|x| x == r));
    Ok(present)
}

#[tauri::command]
async fn sync_folder(app: AppHandle, role: Role, keep: usize) -> CmdResult<SyncResult> {
    blocking(app, move |st| {
        let Ok(_guard) = st.syncing.try_lock() else {
            return Ok(SyncResult { skipped: true, ..Default::default() });
        };
        let mut store = Store::open(st.db_path()).map_err(err)?;
        let mut result = SyncResult::default();
        for acc in st.accounts()? {
            let Some(name) = store.folder_name(&acc.id, role).map_err(err)? else { continue };
            match st
                .account_with_password(&acc.id)
                .and_then(|a| imap_sync::sync_mailbox(&a, &mut store, &name, Some(keep)).map_err(err))
            {
                Ok(r) => {
                    result.fetched += r.fetched;
                    result.total += r.total;
                }
                Err(e) => result.errors.push(format!("{}: {e}", acc.id)),
            }
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
fn list_messages(state: State<'_, AppState>, role: Role, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().list_folder(role, limit, offset).map_err(err)
}

#[tauri::command]
fn search_messages(state: State<'_, AppState>, query: String, role: Role) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().search_folder(&query, role, 200).map_err(err)
}

/// Move para a pasta do papel na mesma conta. A cópia local sai só depois do OK do servidor.
#[tauri::command]
async fn move_message(app: AppHandle, id: i64, role: Role) -> CmdResult<()> {
    blocking(app, move |st| {
        let loc = st.locate(id)?;
        let to = st
            .store
            .lock()
            .unwrap()
            .folder_name(&loc.account, role)
            .map_err(err)?
            .ok_or_else(|| "esta conta não tem essa pasta".to_string())?;
        let acc = st.account_with_password(&loc.account)?;
        imap_sync::move_message(&acc, &loc.mailbox, loc.uid, &to).map_err(err)?;
        st.store.lock().unwrap().remove(id).map_err(err)
    })
    .await
}

#[tauri::command]
async fn delete_forever(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(app, move |st| {
        let loc = st.locate(id)?;
        let acc = st.account_with_password(&loc.account)?;
        imap_sync::delete_forever(&acc, &loc.mailbox, loc.uid).map_err(err)?;
        st.store.lock().unwrap().remove(id).map_err(err)
    })
    .await
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
        let loc = store.reply_ref(id).map_err(err)?;
        (d, loc)
    };
    let (detail, loc) = detail;
    let Some(loc) = loc else { return Ok(Some(detail)) };
    let (account, mailbox, uid) = (loc.account, loc.mailbox, loc.uid);
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        if let Err(e) = st
            .account_with_password(&account)
            .and_then(|a| imap_sync::set_seen(&a, &mailbox, uid, true).map_err(err))
        {
            eprintln!("marcar lida no servidor falhou ({account}/{uid}): {e}");
        }
    });
    Ok(Some(detail))
}

#[derive(Deserialize)]
struct Draft {
    account: String,
    /// Id local da mensagem respondida, para encadear a conversa.
    reply_to: Option<i64>,
    #[serde(flatten)]
    message: Outgoing,
}

#[tauri::command]
async fn send_message(app: AppHandle, draft: Draft) -> CmdResult<SendReport> {
    blocking(app, move |st| {
        let reply = match draft.reply_to {
            Some(id) => st.store.lock().unwrap().reply_ref(id).map_err(err)?,
            None => None,
        };
        let acc = st.account_with_password(&draft.account)?;
        send::send(&acc, &draft.message, reply.as_ref()).map_err(err)
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Atualização pelo próprio app: só no desktop (Android atualiza pela loja).
            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
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
            list_folders,
            sync_folder,
            move_message,
            delete_forever,
            list_messages,
            search_messages,
            open_message,
            send_message
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Posta");
}
