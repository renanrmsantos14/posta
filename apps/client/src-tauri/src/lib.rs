//! Ponte entre a UI e o mail-core. Leituras vêm do SQLite local; rede só em sync e flags.

use mail_core::oauth::{self, Provider};
use mail_core::{imap_sync, send, AuthMode, Contact, DraftRecord, IdleEnd, ImapAccount, MessageDetail, MessageSummary, MsgFlag, Outgoing, ReplyRef, Role, SendReport, Store};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, fs, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::{Duration, Instant}};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

mod secret;

struct AppState {
    dir: PathBuf,
    /// Conexão de leitura da UI. A sincronização abre a sua (WAL permite as duas).
    store: Mutex<Store>,
    syncing: Mutex<()>,
    /// Access tokens OAuth por conta, só em memória, com o instante em que vencem.
    tokens: Mutex<HashMap<String, (String, Instant)>>,
    /// Contas que já têm thread de IDLE.
    watching: Mutex<HashSet<String>>,
    /// A UI avisa (`set_focused`) quando a janela ganha ou perde o foco; sem foco, email novo notifica.
    focused: AtomicBool,
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
        acc.password = match acc.auth.provider() {
            Some(p) => self.access_token(id, p)?,
            None => secret::get(id).map_err(|e| format!("senha de {id} no cofre do sistema: {e}"))?,
        };
        Ok(acc)
    }

    /// Access token válido: o da memória se ainda não vence em 60 s; senão renova com o refresh token do cofre.
    fn access_token(&self, id: &str, p: Provider) -> CmdResult<String> {
        // O cache fica travado durante a renovação: envio e sync simultâneos não gastam o mesmo refresh token (a Microsoft o rotaciona).
        let mut cache = self.tokens.lock().unwrap();
        if let Some((t, exp)) = cache.get(id) {
            if *exp > Instant::now() + Duration::from_secs(60) {
                return Ok(t.clone());
            }
        }
        let client = oauth_client(p).ok_or_else(|| "login OAuth não configurado nesta versão do Posta".to_string())?;
        let key = refresh_key(id);
        let refresh = secret::get(&key)
            .map_err(|e| format!("refresh token de {id} no cofre do sistema: {e}"))?;
        let t = oauth::refresh(p, &client, &refresh).map_err(|e| format!("{e}. Entre de novo nesta conta."))?;
        // A Microsoft rotaciona o refresh token a cada uso.
        if let Some(new) = &t.refresh_token {
            secret::set(&key, new).map_err(|e| format!("não consegui guardar o refresh token: {e}"))?;
        }
        cache.insert(id.to_string(), (t.access_token.clone(), Instant::now() + t.expires_in));
        Ok(t.access_token)
    }

    fn cache_token(&self, id: &str, t: &oauth::Tokens) {
        self.tokens.lock().unwrap().insert(id.to_string(), (t.access_token.clone(), Instant::now() + t.expires_in));
    }

    /// Grava a conta na lista e tenta descobrir as pastas (falha não impede adicionar; o próximo sync tenta de novo).
    fn save_account(&self, account: &ImapAccount) -> CmdResult<()> {
        let mut all = self.accounts()?;
        all.retain(|a| a.id != account.id);
        all.push(account.clone());
        self.save_accounts(&all)?;
        if let Ok(f) = imap_sync::discover_folders(account) {
            self.store.lock().unwrap().set_folders(&account.id, &f).ok();
        }
        Ok(())
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

    /// Mensagens locais agrupadas por (conta, pasta), com os UIDs, para uma conexão IMAP por grupo.
    fn locate_groups(&self, ids: &[i64]) -> CmdResult<Vec<((String, String), Vec<(i64, u32)>)>> {
        let mut groups: Vec<((String, String), Vec<(i64, u32)>)> = Vec::new();
        for &id in ids {
            let loc = self.locate(id)?;
            let key = (loc.account, loc.mailbox);
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push((id, loc.uid)),
                None => groups.push((key, vec![(id, loc.uid)])),
            }
        }
        Ok(groups)
    }
}

/// Renovação do IDLE (o RFC 2177 pede menos de 29 min).
const IDLE_RENEW: Duration = Duration::from_secs(25 * 60);
/// Servidor sem IDLE: volta ao polling neste intervalo.
const POLL_EVERY: Duration = Duration::from_secs(60);
const BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);

/// Sincroniza a INBOX de uma conta, respeitando `syncing`, e devolve quantas mensagens vieram.
fn sync_account_inbox(st: &AppState, id: &str) -> CmdResult<usize> {
    let acc = st.account_with_password(id)?;
    let _guard = st.syncing.lock().unwrap_or_else(|p| p.into_inner());
    let mut store = Store::open(st.db_path()).map_err(err)?;
    st.ensure_folders(&mut store, &acc)?;
    Ok(imap_sync::sync_mailbox(&acc, &mut store, "INBOX", None).map_err(err)?.fetched)
}

/// Mostra a notificação do sistema para `fetched` mensagens novas da Caixa de entrada, se a janela está sem foco
/// e a opção `notify` não foi desligada. Até 3 mensagens aparecem uma a uma (remetente e assunto); mais que isso vira um resumo.
fn notify_new(app: &AppHandle, st: &AppState, account: &str, fetched: usize) {
    if fetched == 0 || st.focused.load(Ordering::Relaxed) {
        return;
    }
    let enabled = st.store.lock().unwrap().setting("notify").ok().flatten().as_deref() != Some("0");
    if !enabled {
        return;
    }
    let Ok(store) = Store::open(st.db_path()) else { return };
    let Ok(new) = store.newest_unseen(account, "INBOX", fetched.min(50) as u32) else { return };
    let n = app.notification();
    if new.len() > 3 {
        n.builder().title("Posta").body(format!("{} mensagens novas em {account}", new.len())).show().ok();
        return;
    }
    for m in new {
        let who = if m.from_name.is_empty() { m.from_addr } else { m.from_name };
        let subject = if m.subject.is_empty() { "(sem assunto)".to_string() } else { m.subject };
        n.builder().title(who).body(subject).show().ok();
    }
}

/// Um ciclo: espera o IDLE e sincroniza ao fim. Também sincroniza no prazo (`TimedOut`): o que chegou
/// entre o fim do IDLE anterior e este SELECT não gera aviso, então o ciclo renovado serve de varredura.
fn idle_cycle(app: &AppHandle, id: &str) -> CmdResult<()> {
    let st = app.state::<AppState>();
    let acc = st.account_with_password(id)?;
    let changed = match imap_sync::idle_wait(&acc, "INBOX", IDLE_RENEW).map_err(err)? {
        IdleEnd::Changed => true,
        IdleEnd::TimedOut => false,
        IdleEnd::Unsupported => {
            std::thread::sleep(POLL_EVERY);
            true
        }
    };
    let fetched = sync_account_inbox(&st, id)?;
    notify_new(app, &st, id, fetched);
    if changed || fetched > 0 {
        app.emit("mail-changed", fetched).ok();
    }
    Ok(())
}

/// Uma thread por conta: mantém IDLE na INBOX e avisa a UI (`mail-changed`) quando algo muda.
fn watch_inbox(app: &AppHandle, id: String) {
    if !app.state::<AppState>().watching.lock().unwrap().insert(id.clone()) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut backoff = Duration::from_secs(5);
        loop {
            match idle_cycle(&app, &id) {
                Ok(()) => backoff = Duration::from_secs(5),
                Err(e) => {
                    // Conta removida: encerra a thread (e libera o id, caso a conta volte).
                    if !app.state::<AppState>().accounts().map_or(true, |l| l.iter().any(|a| a.id == id)) {
                        app.state::<AppState>().watching.lock().unwrap().remove(&id);
                        return;
                    }
                    eprintln!("IDLE de {id}: {e}");
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                }
            }
        }
    });
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
    let handle = app.clone();
    let account = blocking(app, move |st| {
        imap_sync::check_login(&account).map_err(err)?;
        account.id = format!("{}@{}", account.username, account.host);
        secret::set(&account.id, &account.password)
            .map_err(|e| format!("não consegui guardar a senha no cofre do sistema: {e}"))?;
        st.save_account(&account)?;
        Ok(account)
    })
    .await?;
    watch_inbox(&handle, account.id.clone());
    Ok(account)
}

#[cfg(not(target_os = "android"))]
fn open_browser(url: &str) -> CmdResult<()> {
    open::that(url).map_err(|e| format!("não consegui abrir o navegador: {e}"))
}

/// No Android o login OAuth (redirect em 127.0.0.1) ainda não foi validado; use senha de app por enquanto.
#[cfg(target_os = "android")]
fn open_browser(_url: &str) -> CmdResult<()> {
    Err("o login com Google e Microsoft ainda não está disponível no Android; use a senha de app".to_string())
}

fn refresh_key(id: &str) -> String {
    format!("{id}:oauth-refresh")
}

/// Aplicativo OAuth embutido no build (`POSTA_*` na compilação); sem ele o login do provedor fica desligado.
fn oauth_client(p: Provider) -> Option<oauth::Client> {
    let (id, secret) = match p {
        Provider::Google => (option_env!("POSTA_GOOGLE_CLIENT_ID"), option_env!("POSTA_GOOGLE_CLIENT_SECRET")),
        Provider::Microsoft => (option_env!("POSTA_MS_CLIENT_ID"), None),
    };
    let id = id.filter(|s| !s.is_empty())?;
    Some(oauth::Client { id: id.to_string(), secret: secret.map(str::to_string) })
}

/// Provedores com login OAuth disponível neste build.
#[tauri::command]
fn oauth_providers() -> Vec<Provider> {
    [Provider::Google, Provider::Microsoft].into_iter().filter(|p| oauth_client(*p).is_some()).collect()
}

/// Entra com Google/Microsoft no navegador do sistema e adiciona a conta (até 3 min de espera).
#[tauri::command]
async fn oauth_login(app: AppHandle, provider: Provider) -> CmdResult<ImapAccount> {
    let handle = app.clone();
    let account = blocking(app, move |st| {
        let client = oauth_client(provider).ok_or_else(|| "login OAuth não configurado nesta versão do Posta".to_string())?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(err)?;
        let redirect = provider.redirect_uri(listener.local_addr().map_err(err)?.port());
        let (verifier, challenge) = oauth::pkce_pair();
        let state = oauth::random_token();
        let url = oauth::authorization_url(provider, &client.id, &redirect, &challenge, &state);
        open_browser(&url)?;
        let code = oauth::wait_for_code(&listener, &state, Duration::from_secs(180)).map_err(err)?;
        let tokens = oauth::exchange_code(provider, &client, &code, &redirect, &verifier).map_err(err)?;
        let email = tokens.email.clone().ok_or_else(|| "o provedor não informou o email da conta".to_string())?;
        let refresh = tokens.refresh_token.clone().ok_or_else(|| "o provedor não devolveu refresh token".to_string())?;

        let (smtp_host, smtp_port, tls) = provider.smtp();
        let host = provider.imap_host().to_string();
        let account = ImapAccount {
            id: format!("{email}@{host}"),
            host,
            port: 993,
            security: mail_core::Security::Tls,
            username: email,
            smtp: Some(mail_core::SmtpServer {
                host: smtp_host.into(),
                port: smtp_port,
                security: if tls { mail_core::Security::Tls } else { mail_core::Security::StartTls },
            }),
            auth: AuthMode::from(provider),
            password: tokens.access_token.clone(),
        };
        imap_sync::check_login(&account).map_err(err)?;
        secret::set(&refresh_key(&account.id), &refresh)
            .map_err(|e| format!("não consegui guardar o refresh token no cofre do sistema: {e}"))?;
        st.cache_token(&account.id, &tokens);
        st.save_account(&account)?;
        Ok(account)
    })
    .await?;
    watch_inbox(&handle, account.id.clone());
    Ok(account)
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
    let handle = app.clone();
    blocking(app, move |st| {
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
                    notify_new(&handle, st, &acc.id, r.fetched);
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

/// Pasta virtual "Com estrela": todas as contas, fora Lixeira e Spam.
#[tauri::command]
fn list_starred(state: State<'_, AppState>, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().list_starred(limit, offset).map_err(err)
}

/// Mensagens do mesmo fio (conversa) da mensagem `id`, da mais antiga à mais nova.
#[tauri::command]
fn thread_messages(state: State<'_, AppState>, id: i64) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().thread(id).map_err(err)
}

#[tauri::command]
fn search_starred(state: State<'_, AppState>, query: String) -> CmdResult<Vec<MessageSummary>> {
    state.store.lock().unwrap().search_starred(&query, 200).map_err(err)
}

/// Não lidas por papel de pasta (chaves `inbox`, `sent`...), somando as contas.
#[tauri::command]
fn unread_counts(state: State<'_, AppState>) -> CmdResult<HashMap<String, u32>> {
    let counts = state.store.lock().unwrap().unread_counts().map_err(err)?;
    Ok(counts.into_iter().map(|(r, n)| (r.as_str().to_string(), n)).collect())
}

/// Liga/desliga `\Seen` ou `\Flagged` em várias mensagens: o servidor primeiro, o banco local só se ele aceitar.
fn set_flag_many(st: &AppState, ids: &[i64], flag: MsgFlag, on: bool) -> CmdResult<()> {
    for ((account, mailbox), items) in st.locate_groups(ids)? {
        let acc = st.account_with_password(&account)?;
        let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
        imap_sync::set_flag(&acc, &mailbox, &uids, flag, on).map_err(err)?;
        let store = st.store.lock().unwrap();
        for (id, _) in items {
            match flag {
                MsgFlag::Seen => store.set_seen(id, on),
                MsgFlag::Flagged => store.set_flagged(id, on),
            }
            .map_err(err)?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn set_seen_many(app: AppHandle, ids: Vec<i64>, seen: bool) -> CmdResult<()> {
    blocking(app, move |st| set_flag_many(st, &ids, MsgFlag::Seen, seen)).await
}

#[tauri::command]
async fn set_flagged_many(app: AppHandle, ids: Vec<i64>, flagged: bool) -> CmdResult<()> {
    blocking(app, move |st| set_flag_many(st, &ids, MsgFlag::Flagged, flagged)).await
}

/// Move várias para a pasta do papel (cada uma na sua conta). As cópias locais saem depois do OK do servidor.
#[tauri::command]
async fn move_messages(app: AppHandle, ids: Vec<i64>, role: Role) -> CmdResult<()> {
    blocking(app, move |st| {
        for ((account, mailbox), items) in st.locate_groups(&ids)? {
            let to = st
                .store
                .lock()
                .unwrap()
                .folder_name(&account, role)
                .map_err(err)?
                .ok_or_else(|| "esta conta não tem essa pasta".to_string())?;
            let acc = st.account_with_password(&account)?;
            let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
            imap_sync::move_messages(&acc, &mailbox, &uids, &to).map_err(err)?;
            let store = st.store.lock().unwrap();
            for (id, _) in items {
                store.remove(id).map_err(err)?;
            }
        }
        Ok(())
    })
    .await
}

/// Exclui de vez várias que estão na Lixeira.
#[tauri::command]
async fn delete_forever_many(app: AppHandle, ids: Vec<i64>) -> CmdResult<()> {
    blocking(app, move |st| {
        for ((account, mailbox), items) in st.locate_groups(&ids)? {
            let trash = st.store.lock().unwrap().folder_name(&account, Role::Trash).map_err(err)?;
            if trash.as_deref() != Some(mailbox.as_str()) {
                return Err("só dá para excluir de vez dentro da Lixeira".to_string());
            }
            let acc = st.account_with_password(&account)?;
            let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
            imap_sync::delete_forever_many(&acc, &mailbox, &uids).map_err(err)?;
            let store = st.store.lock().unwrap();
            for (id, _) in items {
                store.remove(id).map_err(err)?;
            }
        }
        Ok(())
    })
    .await
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
        // Só a Lixeira apaga de vez; em qualquer outra pasta o caminho é mover para a Lixeira.
        let trash = st.store.lock().unwrap().folder_name(&loc.account, Role::Trash).map_err(err)?;
        if trash.as_deref() != Some(loc.mailbox.as_str()) {
            return Err("só dá para excluir de vez dentro da Lixeira".to_string());
        }
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
    let (mut detail, loc) = {
        let store = st.store.lock().unwrap();
        let Some(mut d) = store.get(id).map_err(err)? else {
            return Ok(None);
        };
        let was_seen = d.summary.seen;
        if !was_seen {
            store.set_seen(id, true).map_err(err)?;
            d.summary.seen = true;
        }
        (d, store.reply_ref(id).map_err(err)?.map(|l| (l, was_seen)))
    };
    let Some((loc, was_seen)) = loc else { return Ok(Some(detail)) };
    let (account, mailbox, uid) = (loc.account, loc.mailbox, loc.uid);
    let acc = st.account_with_password(&account);
    // Mensagem enviada por mim, baixada antes da correção: busca o destinatário real.
    let own = acc.as_ref().ok().filter(|a| detail.reply_addr.eq_ignore_ascii_case(&a.username)).cloned();
    if let Some(own) = own {
        let mailbox = mailbox.clone();
        let to = tauri::async_runtime::spawn_blocking(move || imap_sync::fetch_to(&own, &mailbox, uid))
            .await
            .map_err(err)?;
        match to {
            Ok(Some(to)) => {
                st.store.lock().unwrap().set_reply_addr(id, &to).map_err(err)?;
                detail.reply_addr = to;
            }
            Ok(None) => {}
            Err(e) => eprintln!("buscar destinatário falhou ({account}/{uid}): {e}"),
        }
    }
    if !was_seen {
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = acc.and_then(|a| imap_sync::set_seen(&a, &mailbox, uid, true).map_err(err)) {
                eprintln!("marcar lida no servidor falhou ({account}/{uid}): {e}");
            }
        });
    }
    Ok(Some(detail))
}

/// Primeiro nome livre na pasta: "a.pdf", "a (2).pdf", ...
fn free_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = std::path::Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .expect("sempre há um nome livre")
}

/// Baixa o anexo do servidor, grava em Downloads (ou numa pasta do app, se o sistema não tiver
/// Downloads) e, no desktop, abre com o programa padrão. Devolve o caminho gravado.
#[tauri::command]
async fn save_attachment(app: AppHandle, id: i64, index: u32, open: bool) -> CmdResult<String> {
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir().map(|d| d.join("downloads")))
        .map_err(err)?;
    let path = blocking(app.clone(), move |st| {
        let loc = st.locate(id)?;
        let acc = st.account_with_password(&loc.account)?;
        let raw = imap_sync::fetch_raw(&acc, &loc.mailbox, loc.uid)
            .map_err(err)?
            .ok_or_else(|| "mensagem não encontrada no servidor".to_string())?;
        let (att, data) = mail_core::attachments::extract(&raw, index)
            .ok_or_else(|| "anexo não encontrado na mensagem".to_string())?;
        fs::create_dir_all(&dir).map_err(err)?;
        let path = free_path(&dir, &mail_core::attachments::safe_name(&att.name));
        fs::write(&path, data).map_err(err)?;
        Ok(path)
    })
    .await?;
    #[cfg(desktop)]
    if open {
        use tauri_plugin_opener::OpenerExt;
        if let Err(e) = app.opener().open_path(path.to_string_lossy(), None::<&str>) {
            eprintln!("abrir anexo falhou: {e}");
        }
    }
    #[cfg(not(desktop))]
    let _ = open;
    Ok(path.display().to_string())
}

/// Anexos da mensagem `id` prontos para o rascunho de um encaminhamento (baixa a mensagem do servidor).
#[tauri::command]
async fn forward_attachments(app: AppHandle, id: i64) -> CmdResult<Vec<mail_core::OutAttachment>> {
    blocking(app, move |st| {
        let listed = match st.store.lock().unwrap().get(id).map_err(err)? {
            Some(d) if !d.attachments.is_empty() => d.attachments,
            _ => return Ok(vec![]),
        };
        let loc = st.locate(id)?;
        let acc = st.account_with_password(&loc.account)?;
        let raw = imap_sync::fetch_raw(&acc, &loc.mailbox, loc.uid)
            .map_err(err)?
            .ok_or_else(|| "mensagem não encontrada no servidor".to_string())?;
        Ok(mail_core::attachments::to_outgoing(&raw, &listed))
    })
    .await
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

/// Remove a conta: senha e refresh token do cofre, lista de contas, mensagens, pastas, rascunhos e assinatura locais.
/// O servidor não é tocado. A thread de IDLE da conta encerra sozinha no próximo ciclo.
#[tauri::command]
async fn remove_account(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |st| {
        // Espera um sync em andamento terminar: senão ele regravaria mensagens da conta já apagada.
        let _guard = st.syncing.lock().unwrap_or_else(|p| p.into_inner());
        let mut accounts = st.accounts()?;
        let Some(acc) = accounts.iter().find(|a| a.id == id).cloned() else {
            return Err(format!("conta {id} não encontrada"));
        };
        secret::delete(&id).map_err(|e| format!("não consegui apagar a senha do cofre do sistema: {e}"))?;
        if acc.auth.provider().is_some() {
            secret::delete(&refresh_key(&id)).map_err(|e| format!("não consegui apagar o refresh token do cofre do sistema: {e}"))?;
        }
        accounts.retain(|a| a.id != id);
        st.save_accounts(&accounts)?;
        st.tokens.lock().unwrap().remove(&id);
        st.store.lock().unwrap().remove_account(&id).map_err(err)
    })
    .await
}

#[tauri::command]
fn save_draft(state: State<'_, AppState>, draft: DraftRecord) -> CmdResult<()> {
    state.store.lock().unwrap().save_draft(&draft).map_err(err)
}

#[tauri::command]
fn list_drafts(state: State<'_, AppState>) -> CmdResult<Vec<DraftRecord>> {
    state.store.lock().unwrap().list_drafts().map_err(err)
}

#[tauri::command]
fn suggest_contacts(state: State<'_, AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<Contact>> {
    state.store.lock().unwrap().suggest_contacts(&query, limit.unwrap_or(8).min(50)).map_err(err)
}

#[tauri::command]
fn delete_draft(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.store.lock().unwrap().delete_draft(&id).map_err(err)
}

/// Assinaturas por conta (`signature:<id>`) e `notify` ("0" desliga as notificações).
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> CmdResult<HashMap<String, String>> {
    state.store.lock().unwrap().settings().map_err(err)
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    state.store.lock().unwrap().set_setting(&key, &value).map_err(err)
}

#[tauri::command]
fn set_focused(state: State<'_, AppState>, focused: bool) {
    state.focused.store(focused, Ordering::Relaxed);
}

/// Pede a permissão de notificações (Android 13+ mostra o diálogo do sistema); true se concedida.
#[tauri::command]
fn request_notify_permission(app: AppHandle) -> CmdResult<bool> {
    let state = app.notification().request_permission().map_err(err)?;
    Ok(state == tauri_plugin_notification::PermissionState::Granted)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // O rustls-native-certs não conhece os certificados do Android; aponta para a pasta do sistema.
    #[cfg(target_os = "android")]
    std::env::set_var("SSL_CERT_DIR", "/apex/com.android.conscrypt/cacerts:/system/etc/security/cacerts");
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Atualização pelo próprio app: só no desktop (no Android, reinstala-se o APK).
            #[cfg(desktop)]
            {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let store = Store::open(dir.join("mail.sqlite"))?;
            app.manage(AppState { dir, store: Mutex::new(store), syncing: Mutex::new(()), tokens: Mutex::new(HashMap::new()), watching: Mutex::new(HashSet::new()), focused: AtomicBool::new(true) });
            let st = app.state::<AppState>();
            for acc in st.accounts().unwrap_or_default() {
                watch_inbox(app.handle(), acc.id);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_imap_account,
            oauth_providers,
            oauth_login,
            list_accounts,
            sync_inbox,
            list_folders,
            sync_folder,
            move_message,
            delete_forever,
            list_messages,
            search_messages,
            list_starred,
            search_starred,
            thread_messages,
            unread_counts,
            set_seen_many,
            set_flagged_many,
            move_messages,
            delete_forever_many,
            open_message,
            save_attachment,
            forward_attachments,
            send_message,
            remove_account,
            save_draft,
            list_drafts,
            delete_draft,
            suggest_contacts,
            get_settings,
            set_setting,
            set_focused,
            request_notify_permission
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Posta");
}
