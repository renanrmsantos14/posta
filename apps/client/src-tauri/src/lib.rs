//! Ponte entre a UI e o mail-core. Leituras vêm do SQLite local; rede só em sync e flags.

use mail_core::rules::{self, Candidate};
use mail_core::{search, vacation};
use mail_core::{imap_sync, outbox, Delta, Hub, Kind, Page, send, Contact, DraftRecord, FlushReport, IdleEnd, ImapAccount, MessageDetail, MessageSummary, MsgFlag, OpKind, Outgoing, OutboxItem, ReplyRef, Role, SendReport, Store, UserFolder};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, fs, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::{Duration, Instant}};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use open_target::OpenTarget;

mod diag;
mod oauth_app;
mod open_target;
mod secret;
#[cfg(desktop)]
mod tray;
#[cfg(target_os = "android")]
mod android_sync;

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
    /// O ícone da bandeja foi criado: só então fechar a janela pode apenas escondê-la.
    tray: AtomicBool,
    /// Só um `flush` da fila offline por vez.
    flushing: Mutex<()>,
    /// Sessões IMAP persistentes por conta (comandos e IDLE), reaproveitadas entre operações.
    hub: Hub,
    /// Contas com pré-busca de corpos em andamento.
    prefetching: Mutex<HashSet<String>>,
    /// Destino do último clique numa notificação, até a UI buscar (`take_pending_open`).
    pending_open: Mutex<Option<OpenTarget>>,
}

/// Travas que sobrevivem ao pânico de outra thread. Com `lock().unwrap()`, um pânico segurando o `store` (num sync em
/// segundo plano, por exemplo) envenenava o Mutex e todo comando seguinte entrava em pânico também, inclusive na thread
/// do laço de eventos. No Android isso fechava o app e deixava o processo quebrado, e o app não abria mais até ser
/// forçado a parar. Os dados ficam no SQLite (transações), então seguir com o guard é seguro.
pub(crate) trait Lock<T> {
    fn lk(&self) -> std::sync::MutexGuard<'_, T>;
    /// `None` só quando outra thread segura a trava agora; trava envenenada também é devolvida.
    fn try_lk(&self) -> Option<std::sync::MutexGuard<'_, T>>;
}

impl<T> Lock<T> for Mutex<T> {
    fn lk(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn try_lk(&self) -> Option<std::sync::MutexGuard<'_, T>> {
        match self.try_lock() {
            Ok(g) => Some(g),
            Err(std::sync::TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }
}

/// No Android o `run()` roda numa thread do tao, não na principal do processo: se ela entra em pânico, o processo
/// continua vivo (o WorkManager também o segura) sem laço de eventos, e tocar no ícone reabre a Activity nesse processo
/// morto, com a tela travada ou fechando na hora. Encerrar o processo faz o próximo toque começar do zero.
#[cfg(target_os = "android")]
fn abort_if_event_loop_panics() {
    let event_loop = std::thread::current().id();
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        if std::thread::current().id() == event_loop {
            std::process::abort();
        }
    }));
}

impl AppState {
    fn new(dir: PathBuf, store: Store) -> Self {
        AppState {
            dir,
            store: Mutex::new(store),
            syncing: Mutex::new(()),
            tokens: Mutex::new(HashMap::new()),
            watching: Mutex::new(HashSet::new()),
            focused: AtomicBool::new(true),
            tray: AtomicBool::new(false),
            flushing: Mutex::new(()),
            hub: Hub::new(),
            prefetching: Mutex::new(HashSet::new()),
            pending_open: Mutex::new(None),
        }
    }
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

    /// Grava a conta na lista e tenta descobrir as pastas (falha não impede adicionar; o próximo sync tenta de novo).
    fn save_account(&self, account: &ImapAccount) -> CmdResult<()> {
        let mut all = self.accounts()?;
        all.retain(|a| a.id != account.id);
        all.push(account.clone());
        self.save_accounts(&all)?;
        // Senha ou servidor podem ter mudado: a sessão guardada não vale mais.
        self.hub.drop_account(&account.id);
        if let Ok(f) = imap_sync::discover_folders(account) {
            self.store.lk().set_folders(&account.id, &f).ok();
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
            .lk()
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

/// Quantas mensagens recentes da Entrada têm o corpo baixado para leitura offline (opção `offlineN`, padrão 200).
fn offline_keep(st: &AppState) -> usize {
    st.store.lk().setting("offlineN").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(200)
}

/// O servidor da conta responde a uma conexão TCP? Distingue "sem rede" de "o servidor recusou".
fn reachable(st: &AppState, account: &str) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let Some(acc) = st.accounts().ok().and_then(|l| l.into_iter().find(|a| a.id == account)) else { return false };
    (acc.host.as_str(), acc.port)
        .to_socket_addrs()
        .map(|mut it| it.any(|a| TcpStream::connect_timeout(&a, Duration::from_secs(3)).is_ok()))
        .unwrap_or(false)
}

/// Reaplica a fila offline (ações e Caixa de saída). Sem nada pendente é só uma leitura do banco.
fn flush_pending(st: &AppState) -> FlushReport {
    let Some(_guard) = st.flushing.try_lk() else { return FlushReport::default() };
    let Ok(store) = Store::open(st.db_path()) else { return FlushReport::default() };
    let accounts = st.accounts().unwrap_or_default();
    outbox::flush_via(&st.hub, &store, &mut |id| {
        if accounts.iter().any(|a| a.id == id) {
            st.account_with_password(id).map(Some).map_err(mail_core::Error::Invalid)
        } else {
            Ok(None)
        }
    })
    .unwrap_or_default()
}

/// Aplica a ação no servidor. Sem rede (servidor inalcançável) ou com fila pendente, guarda na fila para manter a
/// ordem e devolve `false`; recusa do servidor com rede é erro. A cópia local é sempre atualizada pelo chamador.
fn run_op(st: &AppState, account: &str, mailbox: &str, uids: &[u32], kind: OpKind, arg: &str) -> CmdResult<bool> {
    let pending = || st.store.lk().pending_count().map_err(err);
    if pending()? > 0 {
        flush_pending(st);
    }
    if pending()? == 0 {
        let done = st
            .account_with_password(account)
            .and_then(|a| outbox::apply_op_via(&st.hub, &a, mailbox, uids, kind, arg).map_err(err));
        match done {
            Ok(()) => return Ok(true),
            Err(e) if reachable(st, account) => return Err(e),
            Err(_) => {}
        }
    }
    let store = st.store.lk();
    for &uid in uids {
        store.enqueue_op(account, mailbox, uid, kind, arg).map_err(err)?;
    }
    Ok(false)
}

/// Sincroniza uma pasta pela sessão persistente da conta (reconecta com backoff se a rede cair).
fn sync_via_hub(st: &AppState, store: &mut Store, acc: &ImapAccount, mailbox: &str, keep: Option<usize>) -> CmdResult<mail_core::SyncReport> {
    let report = st.hub.with(acc, Kind::Cmd, |live| imap_sync::sync_on(live, acc, store, mailbox, keep)).map_err(err)?;
    // Sincronização grande: devolve o WAL ao arquivo principal para as leituras não ficarem lentas.
    if report.fetched >= 500 {
        store.checkpoint();
    }
    Ok(report)
}

/// Evento incremental para a UI: ids inseridos, alterados e removidos. `mail-changed` continua sendo emitido.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeltaEvent<'a> {
    account: &'a str,
    mailbox: &'a str,
    #[serde(flatten)]
    delta: &'a Delta,
}

fn emit_delta(app: &AppHandle, account: &str, mailbox: &str, delta: &Delta) {
    if !(delta.inserted.is_empty() && delta.changed.is_empty() && delta.removed.is_empty()) {
        app.emit("mail-delta", DeltaEvent { account, mailbox, delta }).ok();
    }
}

/// Quantos corpos recentes a pré-busca mantém no cache.
const PREFETCH_BODIES: u32 = 200;

/// Baixa em segundo plano os corpos das mensagens recentes da Entrada, para abrirem sem rede. Uma por conta por vez.
fn prefetch_inbox(app: &AppHandle, id: &str) {
    let st = app.state::<AppState>();
    if !st.prefetching.lk().insert(id.to_string()) {
        return;
    }
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let r = (|| -> CmdResult<Vec<i64>> {
            let acc = st.account_with_password(&id)?;
            let mut store = Store::open(st.db_path()).map_err(err)?;
            st.hub.with(&acc, Kind::Cmd, |live| imap_sync::prefetch_bodies_on(live, &acc, &mut store, "INBOX", PREFETCH_BODIES)).map_err(err)
        })();
        st.prefetching.lk().remove(&id);
        match r {
            Ok(ids) if !ids.is_empty() => {
                app.emit("mail-delta", DeltaEvent { account: &id, mailbox: "INBOX", delta: &Delta { changed: ids, ..Default::default() } }).ok();
            }
            Ok(_) => {}
            Err(e) => eprintln!("pré-busca de {id}: {e}"),
        }
    });
}

/// Sincroniza a INBOX de uma conta, respeitando `syncing`, e devolve quantas mensagens vieram.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn sync_account_inbox(st: &AppState, id: &str) -> CmdResult<usize> {
    let acc = st.account_with_password(id)?;
    let _guard = st.syncing.lk();
    let mut store = Store::open(st.db_path()).map_err(err)?;
    st.ensure_folders(&mut store, &acc)?;
    Ok(sync_inbox_hooked(st, &mut store, &acc)?.fetched)
}

/// Mostra a notificação do sistema para `fetched` mensagens novas da Caixa de entrada, se a janela está sem foco
/// e a opção `notify` não foi desligada. Até 3 mensagens aparecem uma a uma (remetente e assunto); mais que isso vira um resumo.
fn notify_new(app: &AppHandle, st: &AppState, account: &str, fetched: usize) {
    if fetched == 0 || st.focused.load(Ordering::Relaxed) {
        return;
    }
    let enabled = st.store.lk().setting("notify").ok().flatten().as_deref() != Some("0");
    if !enabled {
        return;
    }
    let Ok(store) = Store::open(st.db_path()) else { return };
    let Ok(new) = store.newest_unseen(account, "INBOX", fetched.min(50) as u32) else { return };
    let n = app.notification();
    // No Android, sem ícone o plugin usa o "i" genérico do sistema na barra de status (res/drawable/ic_stat_posta.xml).
    let note = || {
        let b = n.builder();
        #[cfg(target_os = "android")]
        let b = b.icon("ic_stat_posta");
        b
    };
    if new.len() > 3 {
        let target = OpenTarget::folder(account, "inbox");
        note().title("Posta").body(format!("{} mensagens novas em {account}", new.len())).extra(open_target::EXTRA, target).show().ok();
        return;
    }
    for m in new {
        let who = if m.from_name.is_empty() { m.from_addr } else { m.from_name };
        let subject = if m.subject.is_empty() { "(sem assunto)".to_string() } else { m.subject };
        let target = OpenTarget::message(account, "inbox", m.id);
        note().title(who).body(subject).extra(open_target::EXTRA, target).show().ok();
    }
}

/// Um ciclo: espera o IDLE e sincroniza ao fim. Também sincroniza no prazo (`TimedOut`): o que chegou
/// entre o fim do IDLE anterior e este SELECT não gera aviso, então o ciclo renovado serve de varredura.
fn idle_cycle(app: &AppHandle, id: &str) -> CmdResult<()> {
    let st = app.state::<AppState>();
    let acc = st.account_with_password(id)?;
    // IDLE numa sessão própria, encerrável por `drop_account`; ao acordar, sincroniza na sessão de comandos (já aberta) sem novo login.
    let end = st.hub.idle(&acc, "INBOX", IDLE_RENEW).map_err(err)?;
    let changed = match end {
        IdleEnd::Changed => true,
        IdleEnd::TimedOut => false,
        // Conta salva ou removida: a sessão já foi fechada; o laço recomeça (e sai se a conta não existe mais).
        IdleEnd::Cancelled => return Ok(()),
        IdleEnd::Unsupported => {
            std::thread::sleep(POLL_EVERY);
            true
        }
    };
    let (fetched, delta) = {
        let _guard = st.syncing.lk();
        let mut store = Store::open(st.db_path()).map_err(err)?;
        st.ensure_folders(&mut store, &acc)?;
        let r = sync_inbox_hooked(&st, &mut store, &acc)?;
        (r.fetched, r.delta)
    };
    emit_delta(app, id, "INBOX", &delta);
    notify_new(app, &st, id, fetched);
    #[cfg(desktop)]
    tray::refresh(app);
    if changed || fetched > 0 {
        app.emit("mail-changed", fetched).ok();
    }
    prefetch_inbox(app, id);
    Ok(())
}

/// Uma thread por conta: mantém IDLE na INBOX e avisa a UI (`mail-changed`) quando algo muda.
fn watch_inbox(app: &AppHandle, id: String) {
    if !app.state::<AppState>().watching.lk().insert(id.clone()) {
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
                        app.state::<AppState>().watching.lk().remove(&id);
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

/// Descobre servidores (autoconfig do Thunderbird) e hospedagem (MX) do domínio. Falha de rede = resposta vazia.
#[tauri::command]
async fn detect_provider(domain: String) -> CmdResult<mail_core::setup::Detection> {
    tauri::async_runtime::spawn_blocking(move || mail_core::setup::detect(&domain)).await.map_err(err)
}

/// Uma etapa do teste de conexão do assistente de conta (não salva nada nem envia email).
#[tauri::command]
async fn check_account(account: ImapAccount, step: mail_core::setup::Step) -> Result<(), mail_core::setup::CheckFailure> {
    tauri::async_runtime::spawn_blocking(move || mail_core::setup::check_step(&account, step))
        .await
        .map_err(|e| mail_core::setup::CheckFailure { step, kind: mail_core::setup::FailKind::Unknown, detail: e.to_string() })?
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
    /// Sem rede: nada foi sincronizado e o app segue com o que está salvo.
    offline: bool,
}

#[tauri::command]
async fn sync_inbox(app: AppHandle) -> CmdResult<SyncResult> {
    let handle = app.clone();
    blocking(app, move |st| {
        let Some(_guard) = st.syncing.try_lk() else {
            return Ok(SyncResult { skipped: true, ..Default::default() });
        };
        flush_pending(st);
        let mut store = Store::open(st.db_path()).map_err(err)?;
        let mut result = SyncResult::default();
        for acc in st.accounts()? {
            let r = st.account_with_password(&acc.id).and_then(|a| {
                if let Err(e) = st.ensure_folders(&mut store, &a) {
                    result.errors.push(format!("{}: pastas: {e}", acc.id));
                }
                // Pastas criadas em outro cliente aparecem aqui; falha não atrapalha o sync da Entrada.
                refresh_user_folders(st, &a).ok();
                sync_inbox_hooked(st, &mut store, &a)
            });
            match r {
                Ok(r) => {
                    result.fetched += r.fetched;
                    result.total += r.total;
                    emit_delta(&handle, &acc.id, "INBOX", &r.delta);
                    notify_new(&handle, st, &acc.id, r.fetched);
                }
                Err(_) if !reachable(st, &acc.id) => result.offline = true,
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
    let store = state.store.lk();
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
    let handle = app.clone();
    blocking(app, move |st| {
        let Some(_guard) = st.syncing.try_lk() else {
            return Ok(SyncResult { skipped: true, ..Default::default() });
        };
        let mut store = Store::open(st.db_path()).map_err(err)?;
        let mut result = SyncResult::default();
        for acc in st.accounts()? {
            let Some(name) = store.folder_name(&acc.id, role).map_err(err)? else { continue };
            match st
                .account_with_password(&acc.id)
                .and_then(|a| sync_via_hub(st, &mut store, &a, &name, Some(keep)))
            {
                Ok(r) => {
                    result.fetched += r.fetched;
                    result.total += r.total;
                    emit_delta(&handle, &acc.id, &name, &r.delta);
                }
                Err(_) if !reachable(st, &acc.id) => result.offline = true,
                Err(e) => result.errors.push(format!("{}: {e}", acc.id)),
            }
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
fn list_messages(state: State<'_, AppState>, role: Role, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().list_folder(role, limit, offset).map_err(err)
}

/// Página por cursor (custo O(limit), estável com chegada de mensagens). `cursor` vem do `next` da página anterior.
#[tauri::command]
fn list_messages_page(state: State<'_, AppState>, folder: Role, cursor: Option<String>, limit: u32) -> CmdResult<Page> {
    state.store.lk().list_folder_page(folder, cursor.as_deref(), limit).map_err(err)
}

/// Resumos por id (aplica o `mail-delta` sem recarregar a lista). Ids ausentes ou adiados não voltam.
#[tauri::command]
fn get_summaries(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().summaries(&ids).map_err(err)
}

/// Pasta virtual "Com estrela": todas as contas, fora Lixeira e Spam.
#[tauri::command]
fn list_starred(state: State<'_, AppState>, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().list_starred(limit, offset).map_err(err)
}

/// Mensagens do mesmo fio (conversa) da mensagem `id`, da mais antiga à mais nova.
#[tauri::command]
fn thread_messages(state: State<'_, AppState>, id: i64) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().thread(id).map_err(err)
}

#[tauri::command]
fn search_starred(state: State<'_, AppState>, query: String) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().search_starred(&query, 200).map_err(err)
}

/// Não lidas por id de conta e papel de pasta (chaves `inbox`, `sent`...); a bandeja recebe a soma da Entrada.
#[tauri::command]
fn unread_counts(app: AppHandle, state: State<'_, AppState>) -> CmdResult<HashMap<String, HashMap<String, u32>>> {
    let by = state.store.lk().unread_by_account().map_err(err)?;
    #[cfg(desktop)]
    tray::show_unread(&app, by.iter().filter(|(_, r, _)| *r == Role::Inbox).map(|(_, _, n)| n).sum());
    #[cfg(not(desktop))]
    let _ = &app;
    let mut by_account: HashMap<String, HashMap<String, u32>> = HashMap::new();
    for (account, role, n) in by {
        by_account.entry(account).or_default().insert(role.as_str().to_string(), n);
    }
    Ok(by_account)
}

/// Liga/desliga `\Seen` ou `\Flagged` em várias mensagens: o servidor primeiro, o banco local só se ele aceitar.
fn set_flag_many(st: &AppState, ids: &[i64], flag: MsgFlag, on: bool) -> CmdResult<()> {
    for ((account, mailbox), items) in st.locate_groups(ids)? {
        let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
        let kind = match (flag, on) {
            (MsgFlag::Seen, true) => OpKind::Seen,
            (MsgFlag::Seen, false) => OpKind::Unseen,
            (MsgFlag::Flagged, true) => OpKind::Star,
            (MsgFlag::Flagged, false) => OpKind::Unstar,
        };
        run_op(st, &account, &mailbox, &uids, kind, "")?;
        let store = st.store.lk();
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
    blocking(app, move |st| move_to_role(st, &ids, role)).await
}

fn move_to_role(st: &AppState, ids: &[i64], role: Role) -> CmdResult<()> {
    for ((account, mailbox), items) in st.locate_groups(ids)? {
        let to = st
            .store
            .lk()
            .folder_name(&account, role)
            .map_err(err)?
            .ok_or_else(|| "esta conta não tem essa pasta".to_string())?;
        let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
        run_op(st, &account, &mailbox, &uids, OpKind::Move, &to)?;
        let store = st.store.lk();
        for (id, _) in items {
            store.remove(id).map_err(err)?;
        }
    }
    Ok(())
}

/// Exclui de vez várias que estão na Lixeira.
#[tauri::command]
async fn delete_forever_many(app: AppHandle, ids: Vec<i64>) -> CmdResult<()> {
    blocking(app, move |st| {
        for ((account, mailbox), items) in st.locate_groups(&ids)? {
            let trash = st.store.lk().folder_name(&account, Role::Trash).map_err(err)?;
            if trash.as_deref() != Some(mailbox.as_str()) {
                return Err("só dá para excluir de vez dentro da Lixeira".to_string());
            }
            let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
            run_op(st, &account, &mailbox, &uids, OpKind::Delete, "")?;
            let store = st.store.lk();
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
    state.store.lk().search_folder(&query, role, 200).map_err(err)
}

/// Move para a pasta do papel na mesma conta. A cópia local sai só depois do OK do servidor.
#[tauri::command]
async fn move_message(app: AppHandle, id: i64, role: Role) -> CmdResult<()> {
    blocking(app, move |st| {
        let loc = st.locate(id)?;
        let to = st
            .store
            .lk()
            .folder_name(&loc.account, role)
            .map_err(err)?
            .ok_or_else(|| "esta conta não tem essa pasta".to_string())?;
        run_op(st, &loc.account, &loc.mailbox, &[loc.uid], OpKind::Move, &to)?;
        st.store.lk().remove(id).map_err(err)
    })
    .await
}

#[tauri::command]
async fn delete_forever(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(app, move |st| {
        let loc = st.locate(id)?;
        // Só a Lixeira apaga de vez; em qualquer outra pasta o caminho é mover para a Lixeira.
        let trash = st.store.lk().folder_name(&loc.account, Role::Trash).map_err(err)?;
        if trash.as_deref() != Some(loc.mailbox.as_str()) {
            return Err("só dá para excluir de vez dentro da Lixeira".to_string());
        }
        run_op(st, &loc.account, &loc.mailbox, &[loc.uid], OpKind::Delete, "")?;
        st.store.lk().remove(id).map_err(err)
    })
    .await
}

/// Baixa o corpo de uma mensagem que está só com cabeçalho no cache e devolve o detalhe completo.
async fn load_body_now(app: &AppHandle, id: i64) -> CmdResult<MessageDetail> {
    let handle = app.clone();
    let done = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Option<MessageDetail>> {
        let st = handle.state::<AppState>();
        let loc = st.locate(id)?;
        let acc = st.account_with_password(&loc.account)?;
        let mut store = Store::open(st.db_path()).map_err(err)?;
        match imap_sync::load_body(&st.hub, &acc, &mut store, id) {
            Ok(_) => st.store.lk().get(id).map_err(err),
            Err(_) if !reachable(&st, &loc.account) => Err("Sem conexão: o corpo desta mensagem ainda não foi baixado.".to_string()),
            Err(e) => Err(format!("Não consegui baixar o corpo da mensagem: {e}")),
        }
    })
    .await
    .map_err(err)??;
    match done {
        Some(d) if d.body_loaded => Ok(d),
        _ => Err("A mensagem não está mais no servidor e o corpo não foi baixado.".to_string()),
    }
}

/// Abre a mensagem e, se estava não lida, marca como lida aqui e no servidor.
#[tauri::command]
async fn open_message(app: AppHandle, id: i64) -> CmdResult<Option<MessageDetail>> {
    let st = app.state::<AppState>();
    let cached = st.store.lk().get(id).map_err(err)?;
    let Some(cached) = cached else { return Ok(None) };
    // Só cabeçalho no cache: baixa o corpo agora, pela sessão persistente. Sem corpo e sem como baixar,
    // o erro é explícito (a mensagem continua não lida) em vez de abrir uma tela vazia.
    let cached = if cached.body_loaded { cached } else { load_body_now(&app, id).await? };
    let (mut detail, loc) = {
        let store = st.store.lk();
        let mut d = cached;
        let was_seen = d.summary.seen;
        if !was_seen {
            store.set_seen(id, true).map_err(err)?;
            d.summary.seen = true;
        }
        (d, store.reply_ref(id).map_err(err)?.map(|l| (l, was_seen)))
    };
    let Some((loc, was_seen)) = loc else { return Ok(Some(detail)) };
    let (account, mailbox, uid) = (loc.account, loc.mailbox, loc.uid);
    let handle = app.clone();
    let acc = st.account_with_password(&account);
    // Mensagem enviada por mim, baixada antes da correção: busca o destinatário real.
    let own = acc.as_ref().ok().filter(|a| detail.reply_addr.eq_ignore_ascii_case(&a.username)).cloned();
    if let Some(own) = own {
        let mailbox = mailbox.clone();
        let loader = app.clone();
        let to = tauri::async_runtime::spawn_blocking(move || {
            let hub = &loader.state::<AppState>().hub;
            hub.with(&own, Kind::Read, |l| imap_sync::fetch_to_on(l, &mailbox, uid))
        })
        .await
            .map_err(err)?;
        match to {
            Ok(Some(to)) => {
                st.store.lk().set_reply_addr(id, &to).map_err(err)?;
                detail.reply_addr = to;
            }
            Ok(None) => {}
            Err(e) => eprintln!("buscar destinatário falhou ({account}/{uid}): {e}"),
        }
    }
    if !was_seen {
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = run_op(&handle.state::<AppState>(), &account, &mailbox, &[uid], OpKind::Seen, "") {
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

/// Downloads do sistema, ou uma pasta do app quando o sistema não tem Downloads.
fn download_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    app.path().download_dir().or_else(|_| app.path().app_data_dir().map(|d| d.join("downloads"))).map_err(err)
}

/// Baixa do servidor a mensagem inteira (anexos não ficam no cache). Sem rede, explica em vez de mostrar o erro IMAP.
fn fetch_raw_online(st: &AppState, id: i64) -> CmdResult<Vec<u8>> {
    let loc = st.locate(id)?;
    let fetched = st
        .account_with_password(&loc.account)
        .and_then(|a| st.hub.with(&a, Kind::Read, |l| imap_sync::fetch_raw_on(l, &loc.mailbox, loc.uid)).map_err(err));
    match fetched {
        Ok(raw) => raw.ok_or_else(|| "mensagem não encontrada no servidor".to_string()),
        Err(_) if !reachable(st, &loc.account) => Err("Sem conexão: o anexo ainda não foi baixado.".to_string()),
        Err(e) => Err(e),
    }
}

/// Baixa o anexo do servidor, grava em Downloads (ou numa pasta do app, se o sistema não tiver
/// Downloads) e, no desktop, abre com o programa padrão. Devolve o caminho gravado.
#[tauri::command]
async fn save_attachment(app: AppHandle, id: i64, index: u32, open: bool) -> CmdResult<String> {
    let dir = download_dir(&app)?;
    let path = blocking(app.clone(), move |st| {
        let raw = fetch_raw_online(st, id)?;
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

/// Grava a mensagem original (RFC 822, como o servidor a guarda) em Downloads como `.eml`. Devolve o caminho.
#[tauri::command]
async fn save_eml(app: AppHandle, id: i64) -> CmdResult<String> {
    let dir = download_dir(&app)?;
    let path = blocking(app, move |st| {
        let subject = st.store.lk().get(id).map_err(err)?.map(|d| d.summary.subject).unwrap_or_default();
        let loc = st.locate(id)?;
        let acc = st.account_with_password(&loc.account)?;
        let raw = st
            .hub
            .with(&acc, Kind::Read, |l| imap_sync::fetch_raw_on(l, &loc.mailbox, loc.uid))
            .map_err(err)?
            .ok_or_else(|| "mensagem não encontrada no servidor".to_string())?;
        let title: String = subject.trim().chars().take(80).collect();
        let name = format!("{}.eml", if title.is_empty() { "mensagem" } else { title.as_str() });
        fs::create_dir_all(&dir).map_err(err)?;
        let path = free_path(&dir, &mail_core::attachments::safe_name(&name));
        fs::write(&path, raw).map_err(err)?;
        Ok(path)
    })
    .await?;
    Ok(path.display().to_string())
}

/// Arquivos prontos para o arrasto nativo para fora do app (um `.eml` por mensagem, ou um anexo) e a miniatura do cursor.
#[derive(Serialize)]
struct DragFiles {
    files: Vec<String>,
    icon: String,
}

/// Pasta temporária própria do arrasto (uma por processo, só do usuário); recriada a cada preparo
/// (os arquivos antigos já foram soltos ou descartados).
fn drag_dir() -> CmdResult<PathBuf> {
    let dir = std::env::temp_dir().join(format!("posta-drag-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    Ok(dir)
}

fn drag_icon(dir: &std::path::Path) -> CmdResult<String> {
    let icon = dir.join(".icone.png");
    fs::write(&icon, include_bytes!("../icons/32x32.png")).map_err(err)?;
    Ok(icon.display().to_string())
}

/// Grava as mensagens como `.eml` (nome = assunto limpo) numa pasta temporária para arrastar para fora do app.
#[tauri::command]
async fn prepare_drag_eml(app: AppHandle, ids: Vec<i64>) -> CmdResult<DragFiles> {
    blocking(app, move |st| {
        let dir = drag_dir()?;
        let mut files = Vec::new();
        for id in ids {
            let subject = st.store.lk().get(id).map_err(err)?.map(|d| d.summary.subject).unwrap_or_default();
            let raw = fetch_raw_online(st, id)?;
            // Nome de arquivo tem limite em bytes (255): 80 caracteres de 4 bytes estouram.
            let mut title = String::new();
            for c in subject.trim().chars().take(80) {
                if title.len() + c.len_utf8() > 150 {
                    break;
                }
                title.push(c);
            }
            let name = format!("{}.eml", if title.is_empty() { "mensagem" } else { title.as_str() });
            let path = free_path(&dir, &mail_core::attachments::safe_name(&name));
            fs::write(&path, raw).map_err(err)?;
            files.push(path.display().to_string());
        }
        Ok(DragFiles { files, icon: drag_icon(&dir)? })
    })
    .await
}

/// Grava um anexo da mensagem numa pasta temporária para arrastar para fora do app.
#[tauri::command]
async fn prepare_drag_attachment(app: AppHandle, id: i64, index: u32) -> CmdResult<DragFiles> {
    blocking(app, move |st| {
        let dir = drag_dir()?;
        let raw = fetch_raw_online(st, id)?;
        let (att, data) = mail_core::attachments::extract(&raw, index).ok_or_else(|| "anexo não encontrado na mensagem".to_string())?;
        let path = free_path(&dir, &mail_core::attachments::safe_name(&att.name));
        fs::write(&path, data).map_err(err)?;
        Ok(DragFiles { files: vec![path.display().to_string()], icon: drag_icon(&dir)? })
    })
    .await
}

/// Anexos da mensagem `id` prontos para o rascunho de um encaminhamento (baixa a mensagem do servidor).
#[tauri::command]
async fn forward_attachments(app: AppHandle, id: i64) -> CmdResult<Vec<mail_core::OutAttachment>> {
    blocking(app, move |st| {
        let listed = match st.store.lk().get(id).map_err(err)? {
            Some(d) if !d.attachments.is_empty() => d.attachments,
            _ => return Ok(vec![]),
        };
        let raw = fetch_raw_online(st, id)?;
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
            Some(id) => st.store.lk().reply_ref(id).map_err(err)?,
            None => None,
        };
        let acc = st.account_with_password(&draft.account)?;
        check_from(st, &acc, &draft)?;
        match send::send(&acc, &draft.message, reply.as_ref()).map_err(err) {
            Err(_) if !reachable(st, &draft.account) => {
                st.store.lk().enqueue_send(&draft.account, &draft.message, reply.as_ref()).map_err(err)?;
                Ok(SendReport { queued: true, ..Default::default() })
            }
            other => other,
        }
    })
    .await
}

/// Remove a conta: senha e refresh token do cofre, lista de contas, mensagens, pastas, rascunhos e assinatura locais.
/// O servidor não é tocado. A thread de IDLE da conta encerra sozinha no próximo ciclo.
#[tauri::command]
async fn remove_account(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |st| {
        // Espera um sync em andamento terminar: senão ele regravaria mensagens da conta já apagada.
        let _guard = st.syncing.lk();
        let mut accounts = st.accounts()?;
        let Some(acc) = accounts.iter().find(|a| a.id == id).cloned() else {
            return Err(format!("conta {id} não encontrada"));
        };
        secret::delete(&id).map_err(|e| format!("não consegui apagar a senha do cofre do sistema: {e}"))?;
        if acc.auth.provider().is_some() {
            oauth_app::delete_refresh_token(&id)?;
        }
        accounts.retain(|a| a.id != id);
        st.save_accounts(&accounts)?;
        st.tokens.lk().remove(&id);
        st.hub.drop_account(&id);
        send::forget(&id);
        st.store.lk().remove_account(&id).map_err(err)
    })
    .await
}

/// Relê as pastas do usuário no servidor e grava a lista local da conta; devolve o separador da hierarquia.
fn refresh_user_folders(st: &AppState, acc: &ImapAccount) -> CmdResult<String> {
    let (names, delimiter) = imap_sync::list_user_folders(acc).map_err(err)?;
    let delimiter = delimiter.unwrap_or_default();
    st.store.lk().set_user_folders(&acc.id, &names, &delimiter).map_err(err)?;
    Ok(delimiter)
}

fn all_user_folders(st: &AppState) -> CmdResult<Vec<UserFolder>> {
    st.store.lk().user_folders().map_err(err)
}

/// Pastas criadas pelo usuário, de todas as contas (cópia local; o sync as atualiza).
#[tauri::command]
fn list_user_folders(state: State<'_, AppState>) -> CmdResult<Vec<UserFolder>> {
    all_user_folders(&state)
}

/// Cria a pasta (ou subpasta, com `parent`) no servidor e devolve a lista atualizada.
#[tauri::command]
async fn create_folder(app: AppHandle, account: String, parent: Option<String>, name: String) -> CmdResult<Vec<UserFolder>> {
    blocking(app, move |st| {
        let acc = st.account_with_password(&account)?;
        imap_sync::create_folder(&acc, parent.as_deref(), &name).map_err(err)?;
        refresh_user_folders(st, &acc)?;
        all_user_folders(st)
    })
    .await
}

/// Renomeia no servidor e, depois do OK, no banco local (subpastas e mensagens vão junto).
#[tauri::command]
async fn rename_folder(app: AppHandle, account: String, folder: String, name: String) -> CmdResult<Vec<UserFolder>> {
    blocking(app, move |st| {
        // Um sync no meio regravaria mensagens com o nome antigo.
        let _guard = st.syncing.lk();
        let acc = st.account_with_password(&account)?;
        let delimiter = refresh_user_folders(st, &acc)?;
        let new = imap_sync::rename_folder(&acc, &folder, &name).map_err(err)?;
        st.store.lk().rename_box(&account, &folder, &new, &delimiter).map_err(err)?;
        refresh_user_folders(st, &acc)?;
        all_user_folders(st)
    })
    .await
}

/// Quantas mensagens a pasta e as subpastas têm no servidor (para confirmar a exclusão).
#[tauri::command]
async fn count_folder(app: AppHandle, account: String, folder: String) -> CmdResult<u32> {
    blocking(app, move |st| {
        let acc = st.account_with_password(&account)?;
        imap_sync::count_messages(&acc, &folder).map_err(err)
    })
    .await
}

/// Exclui a pasta e as subpastas no servidor; o banco local só apaga o que o servidor confirmou.
#[tauri::command]
async fn delete_folder(app: AppHandle, account: String, folder: String) -> CmdResult<Vec<UserFolder>> {
    blocking(app, move |st| {
        let _guard = st.syncing.lk();
        let acc = st.account_with_password(&account)?;
        let gone = imap_sync::delete_folder(&acc, &folder).map_err(err)?;
        st.store.lk().drop_boxes(&account, &gone).map_err(err)?;
        refresh_user_folders(st, &acc)?;
        all_user_folders(st)
    })
    .await
}

/// Sincroniza uma pasta do usuário (as mais recentes, como `sync_folder`).
#[tauri::command]
async fn sync_user_folder(app: AppHandle, account: String, folder: String, keep: usize) -> CmdResult<SyncResult> {
    let handle = app.clone();
    blocking(app, move |st| {
        let Some(_guard) = st.syncing.try_lk() else {
            return Ok(SyncResult { skipped: true, ..Default::default() });
        };
        let mut store = Store::open(st.db_path()).map_err(err)?;
        match st.account_with_password(&account).and_then(|a| sync_via_hub(st, &mut store, &a, &folder, Some(keep))) {
            Ok(r) => {
                emit_delta(&handle, &account, &folder, &r.delta);
                Ok(SyncResult { fetched: r.fetched, total: r.total, ..Default::default() })
            }
            Err(_) if !reachable(st, &account) => Ok(SyncResult { offline: true, ..Default::default() }),
            Err(e) => Err(e),
        }
    })
    .await
}

/// Não lidas por pasta do usuário (para o contador da árvore de pastas).
#[tauri::command]
fn user_folder_unread(state: State<'_, AppState>) -> CmdResult<Vec<mail_core::FolderUnread>> {
    state.store.lk().user_folder_unread().map_err(err)
}

#[tauri::command]
fn list_user_folder(state: State<'_, AppState>, account: String, folder: String, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().list_box(&account, &folder, limit, offset).map_err(err)
}

#[tauri::command]
fn search_user_folder(state: State<'_, AppState>, account: String, folder: String, query: String) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().search_box(&account, &folder, &query, 200).map_err(err)
}

/// Move para uma pasta do usuário. Só mensagens da mesma conta da pasta; devolve quantas foram movidas.
#[tauri::command]
async fn move_to_folder(app: AppHandle, ids: Vec<i64>, account: String, folder: String) -> CmdResult<usize> {
    blocking(app, move |st| move_to_user_folder(st, &ids, &account, &folder)).await
}

fn move_to_user_folder(st: &AppState, ids: &[i64], account: &str, folder: &str) -> CmdResult<usize> {
    let mut moved = 0;
    for ((acc_id, mailbox), items) in st.locate_groups(ids)? {
        if acc_id != account || mailbox == folder {
            continue;
        }
        let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
        run_op(st, &acc_id, &mailbox, &uids, OpKind::Move, folder)?;
        let store = st.store.lk();
        for (id, _) in &items {
            store.remove(*id).map_err(err)?;
        }
        moved += items.len();
    }
    Ok(moved)
}

/// Copia para a pasta do papel (cada uma na sua conta); as originais ficam onde estão.
#[tauri::command]
async fn copy_messages(app: AppHandle, ids: Vec<i64>, role: Role) -> CmdResult<usize> {
    blocking(app, move |st| {
        let mut copied = 0;
        for ((account, mailbox), items) in st.locate_groups(&ids)? {
            let to = st.store.lk().folder_name(&account, role).map_err(err)?.ok_or_else(|| "esta conta não tem essa pasta".to_string())?;
            if to == mailbox {
                continue;
            }
            let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
            run_op(st, &account, &mailbox, &uids, OpKind::Copy, &to)?;
            copied += items.len();
        }
        Ok(copied)
    })
    .await
}

/// Copia para uma pasta do usuário. Só mensagens da mesma conta da pasta; devolve quantas foram copiadas.
#[tauri::command]
async fn copy_to_folder(app: AppHandle, ids: Vec<i64>, account: String, folder: String) -> CmdResult<usize> {
    blocking(app, move |st| {
        let mut copied = 0;
        for ((acc_id, mailbox), items) in st.locate_groups(&ids)? {
            if acc_id != account || mailbox == folder {
                continue;
            }
            let uids: Vec<u32> = items.iter().map(|(_, u)| *u).collect();
            run_op(st, &acc_id, &mailbox, &uids, OpKind::Copy, &folder)?;
            copied += items.len();
        }
        Ok(copied)
    })
    .await
}

/// Aplica (ou tira, com `None`) a categoria colorida. Só local.
#[tauri::command]
fn set_category(state: State<'_, AppState>, ids: Vec<i64>, category: Option<String>) -> CmdResult<()> {
    state.store.lk().set_category(&ids, category.as_deref()).map_err(err)
}

#[tauri::command]
fn list_category(state: State<'_, AppState>, category: String, limit: u32, offset: u32) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().list_category(&category, limit, offset).map_err(err)
}

#[tauri::command]
fn search_category(state: State<'_, AppState>, category: String, query: String) -> CmdResult<Vec<MessageSummary>> {
    state.store.lk().search_category(&category, &query, 200).map_err(err)
}

/// Edita a conta: nome, senha (só se vier uma nova), servidores e portas. IMAP e SMTP são testados antes de gravar;
/// com falha nada muda. O usuário e o id ficam. Contas OAuth só trocam o nome (o login é do provedor).
#[tauri::command]
async fn update_account(app: AppHandle, account: ImapAccount, password: Option<String>) -> CmdResult<ImapAccount> {
    blocking(app, move |st| {
        let mut all = st.accounts()?;
        let slot = all.iter_mut().find(|a| a.id == account.id).ok_or_else(|| format!("conta {} não encontrada", account.id))?;
        let name = account.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
        if slot.auth.provider().is_some() {
            slot.name = name;
        } else {
            let new_password = password.filter(|p| !p.is_empty());
            let mut next = ImapAccount {
                id: slot.id.clone(),
                username: slot.username.clone(),
                auth: slot.auth,
                name,
                host: account.host.trim().to_string(),
                port: account.port,
                security: account.security,
                smtp: account.smtp,
                password: String::new(),
            };
            next.password = match &new_password {
                Some(p) => p.clone(),
                None => secret::get(&next.id).map_err(|e| format!("senha de {} no cofre do sistema: {e}", next.id))?,
            };
            imap_sync::check_login(&next).map_err(err)?;
            send::check_smtp(&next).map_err(err)?;
            if let Some(p) = &new_password {
                secret::set(&next.id, p).map_err(|e| format!("não consegui guardar a senha no cofre do sistema: {e}"))?;
            }
            *slot = next;
        }
        let saved = slot.clone();
        st.save_accounts(&all)?;
        Ok(saved)
    })
    .await
}

#[tauri::command]
fn save_draft(state: State<'_, AppState>, draft: DraftRecord) -> CmdResult<()> {
    state.store.lk().save_draft(&draft).map_err(err)
}

#[tauri::command]
fn list_drafts(state: State<'_, AppState>) -> CmdResult<Vec<DraftRecord>> {
    state.store.lk().list_drafts().map_err(err)
}

#[tauri::command]
fn suggest_contacts(state: State<'_, AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<Contact>> {
    state.store.lk().suggest_contacts(&query, limit.unwrap_or(8).min(50)).map_err(err)
}

#[tauri::command]
fn delete_draft(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.store.lk().delete_draft(&id).map_err(err)
}

/// Assinaturas por conta (`signature:<id>`) e `notify` ("0" desliga as notificações).
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> CmdResult<HashMap<String, String>> {
    state.store.lk().settings().map_err(err)
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    state.store.lk().set_setting(&key, &value).map_err(err)
}

#[derive(Serialize)]
struct QueueStatus {
    /// O servidor da primeira conta responde (sem contas, conta como online).
    online: bool,
    /// Ações esperando o servidor.
    pending: u32,
    /// Mensagens na Caixa de saída (com ou sem erro).
    outbox: u32,
}

#[tauri::command]
async fn queue_status(app: AppHandle) -> CmdResult<QueueStatus> {
    blocking(app, |st| {
        let online = match st.accounts()?.first() {
            Some(a) => reachable(st, &a.id),
            None => true,
        };
        let store = st.store.lk();
        Ok(QueueStatus { online, pending: store.pending_count().map_err(err)?, outbox: store.outbox_count().map_err(err)? })
    })
    .await
}

/// Tenta reaplicar a fila agora (a UI chama quando a rede volta).
#[tauri::command]
async fn flush_queue(app: AppHandle) -> CmdResult<FlushReport> {
    blocking(app, |st| Ok(flush_pending(st))).await
}

#[tauri::command]
fn list_outbox(state: State<'_, AppState>) -> CmdResult<Vec<OutboxItem>> {
    state.store.lk().outbox().map_err(err)
}

#[tauri::command]
fn discard_outbox(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    state.store.lk().drop_outbox(id).map_err(err)
}

/// Limpa o erro de um envio recusado e tenta de novo.
#[tauri::command]
async fn retry_outbox(app: AppHandle, id: i64) -> CmdResult<FlushReport> {
    blocking(app, move |st| {
        st.store.lk().retry_outbox(id).map_err(err)?;
        Ok(flush_pending(st))
    })
    .await
}

#[tauri::command]
fn set_focused(state: State<'_, AppState>, focused: bool) {
    state.focused.store(focused, Ordering::Relaxed);
}

/// Iniciar com o sistema (só desktop; no Android devolve false).
#[tauri::command]
fn get_autostart(app: AppHandle) -> CmdResult<bool> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().map_err(err)
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(false)
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let l = app.autolaunch();
        if enabled { l.enable() } else { l.disable() }.map_err(err)
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Err("iniciar com o sistema só existe no desktop".to_string())
    }
}

/// Pede a permissão de notificações (Android 13+ mostra o diálogo do sistema); true se concedida.
#[tauri::command]
fn request_notify_permission(app: AppHandle) -> CmdResult<bool> {
    let state = app.notification().request_permission().map_err(err)?;
    Ok(state == tauri_plugin_notification::PermissionState::Granted)
}

// ---- Produtividade: regras, férias, busca avançada, adiar/fixar e envio agendado ----

fn now_ts() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn setting_of(st: &AppState, key: &str) -> Option<String> {
    st.store.lk().setting(key).ok().flatten()
}

/// Sincroniza a Entrada e, se chegou algo, roda o que depende de mensagem nova: férias e regras.
fn sync_inbox_hooked(st: &AppState, store: &mut Store, acc: &ImapAccount) -> CmdResult<mail_core::SyncReport> {
    let mark = store.max_id().map_err(err)?;
    let report = sync_via_hub(st, store, acc, "INBOX", Some(offline_keep(st)))?;
    if report.fetched > 0 {
        let new = st.store.lk().inbox_after(&acc.id, mark).map_err(err)?;
        auto_reply(st, acc, &new);
        apply_rules(st, &new);
    }
    Ok(report)
}

/// Resposta de férias: uma por remetente no período, só a mensagens do período e nunca a listas, noreply ou robôs.
fn auto_reply(st: &AppState, acc: &ImapAccount, new: &[Candidate]) {
    let vac = vacation::Vacation::parse(setting_of(st, "vacation").as_deref());
    if !vac.active(now_ts()) {
        return;
    }
    let since = vac.start_ts().unwrap_or(0);
    for c in new {
        let addr = if c.reply_addr.is_empty() { c.from_addr.as_str() } else { c.reply_addr.as_str() };
        if c.date < since || vacation::is_noreply(addr) || addr.eq_ignore_ascii_case(&acc.username) {
            continue;
        }
        if st.store.lk().vacation_replied(&acc.id, addr, since).unwrap_or(true) {
            continue;
        }
        let Ok(Some(raw)) = st.hub.with(acc, Kind::Cmd, |l| imap_sync::fetch_raw_on(l, "INBOX", c.uid)) else { continue };
        if vacation::is_automatic(&raw) {
            continue;
        }
        let reply = st.store.lk().reply_ref(c.id).ok().flatten();
        let subject = if vac.subject.trim().is_empty() { format!("Resposta automática: {}", c.subject) } else { vac.subject.clone() };
        let out = Outgoing { to: addr.to_string(), subject, body: vac.body.clone(), html: None, cc: String::new(), bcc: String::new(), attachments: Vec::new(), from: None, importance: None };
        match send::send_auto(acc, &out, reply.as_ref()) {
            Ok(_) => {
                st.store.lk().mark_vacation_replied(&acc.id, addr, now_ts()).ok();
            }
            Err(e) => eprintln!("resposta de férias falhou ({}): {e}", acc.id),
        }
    }
}

/// Aplica as regras ativas às mensagens e devolve quantas mudaram. Falha em uma ação não impede as outras.
fn apply_rules(st: &AppState, msgs: &[Candidate]) -> usize {
    let rules = rules::parse_rules(setting_of(st, "rules").as_deref());
    let (mut read, mut star, mut archive) = (Vec::new(), Vec::new(), Vec::new());
    let mut cats: HashMap<String, Vec<i64>> = HashMap::new();
    let mut moves: HashMap<(String, String), Vec<i64>> = HashMap::new();
    let mut touched: HashSet<i64> = HashSet::new();
    let user_folders = st.store.lk().user_folders().unwrap_or_default();
    for c in msgs {
        let plan = rules::plan(&rules, c);
        if plan.is_empty() {
            continue;
        }
        touched.insert(c.id);
        if plan.read {
            read.push(c.id);
        }
        if plan.star {
            star.push(c.id);
        }
        if let Some(cat) = plan.category {
            cats.entry(cat).or_default().push(c.id);
        }
        match plan.move_to {
            Some(f) if user_folders.iter().any(|u| u.account == c.account && u.name == f) => moves.entry((c.account.clone(), f)).or_default().push(c.id),
            _ if plan.archive => archive.push(c.id),
            _ => {}
        }
    }
    let log = |what: &str, r: CmdResult<()>| {
        if let Err(e) = r {
            eprintln!("regra ({what}) falhou: {e}");
        }
    };
    if !read.is_empty() {
        log("lida", set_flag_many(st, &read, MsgFlag::Seen, true));
    }
    if !star.is_empty() {
        log("estrela", set_flag_many(st, &star, MsgFlag::Flagged, true));
    }
    for (cat, ids) in cats {
        log("categoria", st.store.lk().set_category(&ids, Some(&cat)).map_err(err));
    }
    // Mover por último: depois disso a mensagem sai da Entrada.
    for ((account, folder), ids) in moves {
        log("mover", move_to_user_folder(st, &ids, &account, &folder).map(|_| ()));
    }
    if !archive.is_empty() {
        log("arquivar", move_to_role(st, &archive, Role::Archive));
    }
    touched.len()
}

/// "Aplicar agora à Entrada": roda as regras sobre tudo o que já está na Entrada. Devolve quantas mensagens mudaram.
#[tauri::command]
async fn apply_rules_now(app: AppHandle) -> CmdResult<usize> {
    blocking(app, move |st| {
        let mut changed = 0;
        for acc in st.accounts()? {
            let msgs = st.store.lk().inbox_after(&acc.id, 0).map_err(err)?;
            changed += apply_rules(st, &msgs);
        }
        Ok(changed)
    })
    .await
}

/// Busca com filtros (`de:`, `para:`, `assunto:`, `tem:anexo`, `é:não-lida`, `antes:`, `depois:`) no que está no banco local.
#[tauri::command]
fn search_filtered(state: State<'_, AppState>, scope: search::Scope, query: String) -> CmdResult<Vec<MessageSummary>> {
    let f = search::parse(&query);
    state.store.lk().search_filtered(&scope, &f, 200).map_err(err)
}

/// "Buscar no servidor": IMAP SEARCH nas pastas do escopo; o que ainda não estava no banco é baixado.
#[tauri::command]
async fn search_server(app: AppHandle, scope: search::Scope, query: String) -> CmdResult<Vec<MessageSummary>> {
    blocking(app, move |st| {
        let f = search::parse(&query);
        if f.is_empty() {
            return Ok(Vec::new());
        }
        let targets: Vec<(String, String)> = match &scope {
            search::Scope::Role { role } => {
                let mut v = Vec::new();
                for a in st.accounts()? {
                    let name = st.store.lk().folder_name(&a.id, *role).map_err(err)?;
                    match (name, role) {
                        (Some(n), _) => v.push((a.id, n)),
                        (None, Role::Inbox) => v.push((a.id, "INBOX".to_string())),
                        _ => {}
                    }
                }
                v
            }
            search::Scope::Box { account, mailbox } => vec![(account.clone(), mailbox.clone())],
            _ => return Err("A busca no servidor vale para pastas, não para Com estrela ou categorias.".to_string()),
        };
        let _guard = st.syncing.lk();
        let mut store = Store::open(st.db_path()).map_err(err)?;
        let mut found: Vec<MessageSummary> = Vec::new();
        for (account, mailbox) in targets {
            let acc = st.account_with_password(&account)?;
            let uids = imap_sync::search_server(&acc, &mut store, &mailbox, &f).map_err(err)?;
            found.extend(st.store.lk().by_uids(&account, &mailbox, &uids).map_err(err)?);
        }
        found.sort_by(|a, b| b.date.cmp(&a.date));
        found.truncate(200);
        Ok(found)
    })
    .await
}

/// Esconde as mensagens até `until` (Unix, segundos); depois voltam ao topo como não lidas.
#[tauri::command]
fn snooze_messages(state: State<'_, AppState>, ids: Vec<i64>, until: i64) -> CmdResult<()> {
    state.store.lk().snooze(&ids, until).map_err(err)
}

#[tauri::command]
fn unsnooze_messages(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<()> {
    state.store.lk().unsnooze(&ids).map_err(err)
}

#[tauri::command]
fn set_pinned(state: State<'_, AppState>, ids: Vec<i64>, pinned: bool) -> CmdResult<()> {
    state.store.lk().set_pinned(&ids, pinned).map_err(err)
}

/// O remetente alternativo só vale se for o endereço da conta ou um alias cadastrado.
fn check_from(st: &AppState, acc: &ImapAccount, draft: &Draft) -> CmdResult<()> {
    if let Some(from) = draft.message.from.as_deref().filter(|f| !f.trim().is_empty()) {
        let aliases = st.store.lk().setting(&format!("aliases:{}", draft.account)).map_err(err)?.unwrap_or_default();
        if !send::from_allowed(&acc.username, &aliases, from) {
            return Err(format!("{from} não é um remetente cadastrado nesta conta"));
        }
    }
    Ok(())
}

/// Agenda o envio. O mesmo `id` troca o agendamento. A mensagem é montada já aqui para recusar endereço inválido na hora.
#[tauri::command]
async fn schedule_send(app: AppHandle, id: String, send_at: i64, draft: Draft) -> CmdResult<()> {
    blocking(app, move |st| {
        let reply = match draft.reply_to {
            Some(r) => st.store.lk().reply_ref(r).map_err(err)?,
            None => None,
        };
        let acc = st.account_with_password(&draft.account)?;
        check_from(st, &acc, &draft)?;
        send::build(&acc, &draft.message, reply.as_ref()).map_err(err)?;
        st.store.lk().schedule_send(&id, &draft.account, draft.reply_to, send_at, &draft.message).map_err(err)
    })
    .await
}

#[tauri::command]
fn list_scheduled(state: State<'_, AppState>) -> CmdResult<Vec<mail_core::later::ScheduledItem>> {
    state.store.lk().list_scheduled().map_err(err)
}

#[derive(Serialize)]
struct ScheduledEdit {
    id: String,
    account: String,
    reply_to: Option<i64>,
    send_at: i64,
    #[serde(flatten)]
    message: Outgoing,
}

/// Tira da fila e devolve o conteúdo: serve para cancelar e para editar (a UI reabre no Composer).
#[tauri::command]
fn take_scheduled(state: State<'_, AppState>, id: String) -> CmdResult<Option<ScheduledEdit>> {
    let taken = state.store.lk().take_scheduled(&id).map_err(err)?;
    Ok(taken.map(|s| ScheduledEdit { id: s.id, account: s.account, reply_to: s.reply_to, send_at: s.send_at, message: s.out }))
}

/// Adiadas que venceram voltam não lidas (a marca de lida também sai no servidor); devolve se algo mudou.
fn wake_snoozed(st: &AppState) -> bool {
    let ids = st.store.lk().wake_due(now_ts()).unwrap_or_default();
    if ids.is_empty() {
        return false;
    }
    if let Err(e) = set_flag_many(st, &ids, MsgFlag::Seen, false) {
        eprintln!("desmarcar lida no servidor falhou: {e}");
    }
    true
}

/// Envia o que está agendado e venceu. Falha tenta de novo em 2 min; na terceira fica como falha na lista.
fn send_due(st: &AppState) -> bool {
    let now = now_ts();
    let due = st.store.lk().claim_due(now).unwrap_or_default();
    for s in &due {
        let sent = (|| -> CmdResult<()> {
            let reply = match s.reply_to {
                Some(r) => st.store.lk().reply_ref(r).map_err(err)?,
                None => None,
            };
            let acc = st.account_with_password(&s.account)?;
            match send::send(&acc, &s.out, reply.as_ref()) {
                Ok(_) => Ok(()),
                // Sem rede: passa para a Caixa de saída offline, que envia quando a conexão volta (um único caminho, sem duplicar).
                Err(e) if e.is_network() && !reachable(st, &s.account) => {
                    st.store.lk().enqueue_send(&s.account, &s.out, reply.as_ref()).map(|_| ()).map_err(err)
                }
                Err(e) => Err(err(e)),
            }
        })();
        let store = st.store.lk();
        match sent {
            Ok(()) => store.scheduled_done(&s.id).ok(),
            Err(e) => store.scheduled_failed(&s.id, &e, now).ok(),
        };
    }
    !due.is_empty()
}

/// Relógio do app: a cada 20 s acorda adiadas e envia agendados (só roda com o app aberto).
fn start_scheduler(app: AppHandle) {
    app.state::<AppState>().store.lk().requeue_sending().ok();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(20));
        let st = app.state::<AppState>();
        let woke = wake_snoozed(&st);
        let sent = send_due(&st);
        if woke || sent {
            app.emit("mail-changed", 0).ok();
        }
    });
}

/// O rustls-native-certs não conhece os certificados do Android; aponta para a pasta do sistema.
/// Uma pasta só: o imap usa rustls-native-certs 0.7, que não aceita lista separada por ':'
/// (vira um caminho inexistente e o IMAP fica sem nenhuma raiz confiável).
/// Vale para o app e para o processo aberto só pelo `SyncWorker`.
#[cfg(target_os = "android")]
pub(crate) fn android_cert_dir() {
    let apex = "/apex/com.android.conscrypt/cacerts";
    let dir = if std::path::Path::new(apex).is_dir() { apex } else { "/system/etc/security/cacerts" };
    std::env::set_var("SSL_CERT_DIR", dir);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::log("run: início");
    #[cfg(target_os = "android")]
    {
        abort_if_event_loop_panics();
        android_cert_dir();
    }
    let builder = tauri::Builder::default();
    // Precisa ser o primeiro plugin: uma segunda abertura só mostra a janela da instância que já roda.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)));
    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let t0 = Instant::now();
            diag::log("setup: início");
            // Atualização pelo próprio app: só no desktop (no Android, reinstala-se o APK).
            #[cfg(desktop)]
            {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
                app.handle().plugin(tauri_plugin_drag::init())?;
                app.handle().plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))?;
            }
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let store = Store::open(dir.join("mail.sqlite"))?;
            diag::log(&format!("setup: banco aberto em {} ms", t0.elapsed().as_millis()));
            app.manage(AppState::new(dir, store));
            #[cfg(desktop)]
            tray::init(app.handle());
            open_target::init(app.handle());
            oauth_app::install_refresher(app.handle());
            let st = app.state::<AppState>();
            for acc in st.accounts().unwrap_or_default() {
                watch_inbox(app.handle(), acc.id);
            }
            start_scheduler(app.handle().clone());
            // Fila offline: tenta reaplicar de tempos em tempos; a UI recarrega quando algo muda.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let r = flush_pending(&handle.state::<AppState>());
                if r.applied + r.discarded + r.sent + r.failed > 0 {
                    handle.emit("queue-changed", ()).ok();
                }
            });
            diag::watch(app.handle().clone());
            diag::log(&format!("setup: fim em {} ms", t0.elapsed().as_millis()));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Fechar a janela só a esconde na bandeja (opção `closeToTray`, ligada por padrão).
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if tray::hide_on_close(window.app_handle()) {
                    api.prevent_close();
                    window.hide().ok();
                }
            }
            #[cfg(not(desktop))]
            let _ = (window, event);
        })
        .invoke_handler(diag::timed(tauri::generate_handler![
            add_imap_account,
            detect_provider,
            check_account,
            oauth_app::oauth_providers,
            oauth_app::oauth_login,
            oauth_app::oauth_relogin,
            oauth_app::oauth_config_get,
            oauth_app::oauth_config_set,
            list_accounts,
            sync_inbox,
            list_folders,
            sync_folder,
            move_message,
            delete_forever,
            list_messages,
            list_messages_page,
            get_summaries,
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
            save_eml,
            send_message,
            remove_account,
            list_user_folders,
            user_folder_unread,
            create_folder,
            rename_folder,
            count_folder,
            delete_folder,
            sync_user_folder,
            list_user_folder,
            search_user_folder,
            move_to_folder,
            copy_messages,
            copy_to_folder,
            prepare_drag_eml,
            prepare_drag_attachment,
            set_category,
            list_category,
            search_category,
            update_account,
            save_draft,
            list_drafts,
            delete_draft,
            suggest_contacts,
            get_settings,
            set_setting,
            set_focused,
            open_target::take_pending_open,
            queue_status,
            flush_queue,
            list_outbox,
            discard_outbox,
            retry_outbox,
            get_autostart,
            set_autostart,
            request_notify_permission,
            apply_rules_now,
            search_filtered,
            search_server,
            snooze_messages,
            unsnooze_messages,
            set_pinned,
            schedule_send,
            list_scheduled,
            take_scheduled
        ]))
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Posta");
}
