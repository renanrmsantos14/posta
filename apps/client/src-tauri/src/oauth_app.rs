//! Entrar com Google e Microsoft: configuração dos Client IDs, login pelo navegador, token em cache e renovação.
//! O protocolo está em `mail_core::oauth`; aqui ficam o cofre do sistema, o estado do app e os comandos da UI.

use super::{blocking, err, secret, watch_inbox, AppState, CmdResult};
use mail_core::oauth::{self, Provider, RefreshError};
use mail_core::{imap_sync, AuthMode, ImapAccount};
use serde::{Deserialize, Serialize};
use std::{fs, time::{Duration, Instant}};
use tauri::{AppHandle, Manager};

/// Marca que o front-end troca pelo botão "Entrar de novo" (ver `toast.tsx`).
pub(crate) fn reauth(account: &str, msg: &str) -> String {
    format!("[reauth:{account}] {msg}")
}

fn refresh_key(id: &str) -> String {
    format!("{id}:oauth-refresh")
}

pub(crate) fn delete_refresh_token(id: &str) -> Result<(), String> {
    secret::delete(&refresh_key(id)).map_err(|e| format!("não consegui apagar o refresh token do cofre do sistema: {e}"))
}

/// Client IDs digitados em Configurações > Contas > Avançado. Valem mais que os do build.
#[derive(Serialize, Deserialize, Default, Clone)]
pub(crate) struct OauthConfig {
    #[serde(default)]
    google_id: String,
    #[serde(default)]
    google_secret: String,
    #[serde(default)]
    microsoft_id: String,
}

/// O que a UI mostra: os valores digitados e se o build já traz os IDs embutidos.
#[derive(Serialize)]
pub(crate) struct OauthConfigView {
    #[serde(flatten)]
    config: OauthConfig,
    built_in_google: bool,
    built_in_microsoft: bool,
}

fn built_in(p: Provider) -> (Option<&'static str>, Option<&'static str>) {
    match p {
        Provider::Google => (option_env!("POSTA_GOOGLE_CLIENT_ID"), option_env!("POSTA_GOOGLE_CLIENT_SECRET")),
        Provider::Microsoft => (option_env!("POSTA_MS_CLIENT_ID"), None),
    }
}

impl AppState {
    fn oauth_config_path(&self) -> std::path::PathBuf {
        self.dir.join("oauth-clients.json")
    }

    fn oauth_config(&self) -> OauthConfig {
        fs::read(self.oauth_config_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    /// Aplicativo OAuth: o digitado nas configurações, senão o embutido no build (`POSTA_*` na compilação).
    pub(crate) fn oauth_client(&self, p: Provider) -> Option<oauth::Client> {
        let cfg = self.oauth_config();
        let (typed_id, typed_secret) = match p {
            Provider::Google => (cfg.google_id, cfg.google_secret),
            Provider::Microsoft => (cfg.microsoft_id, String::new()),
        };
        let (build_id, build_secret) = built_in(p);
        let typed_id = typed_id.trim().to_string();
        let (id, secret) = if typed_id.is_empty() {
            (build_id.unwrap_or("").to_string(), build_secret.unwrap_or("").to_string())
        } else {
            (typed_id, typed_secret.trim().to_string())
        };
        (!id.is_empty()).then(|| oauth::Client { id, secret: (!secret.is_empty()).then_some(secret), token_url: None })
    }

    /// Access token válido: o da memória se ainda não vence em 60 s; senão renova com o refresh token do cofre.
    pub(crate) fn access_token(&self, id: &str, p: Provider) -> CmdResult<String> {
        // O cache fica travado durante a renovação: envio e sync simultâneos não gastam o mesmo refresh token (a Microsoft o rotaciona).
        let mut cache = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((t, exp)) = cache.get(id) {
            if *exp > Instant::now() + Duration::from_secs(60) {
                return Ok(t.clone());
            }
        }
        let client = self.oauth_client(p).ok_or_else(|| {
            "O login com este provedor não está configurado: informe o Client ID em Configurações > Contas > Avançado.".to_string()
        })?;
        let key = refresh_key(id);
        let Ok(refresh) = secret::get(&key) else {
            return Err(reauth(id, "O acesso salvo desta conta não está mais no cofre do sistema. Entre de novo."));
        };
        let t = oauth::refresh(p, &client, &refresh).map_err(|e| match e {
            RefreshError::NeedsLogin(m) => reauth(id, &m),
            RefreshError::Failed(e) => format!("Não consegui renovar o acesso: {e}"),
        })?;
        // A Microsoft rotaciona o refresh token a cada uso.
        if let Some(new) = &t.refresh_token {
            secret::set(&key, new).map_err(|e| format!("não consegui guardar o refresh token: {e}"))?;
        }
        cache.insert(id.to_string(), (t.access_token.clone(), Instant::now() + t.expires_in));
        Ok(t.access_token)
    }

    /// O servidor recusou o token em cache: descarta e renova de verdade.
    pub(crate) fn force_token(&self, id: &str) -> CmdResult<String> {
        let acc = self.accounts()?.into_iter().find(|a| a.id == id).ok_or_else(|| format!("conta {id} não encontrada"))?;
        let p = acc.auth.provider().ok_or_else(|| format!("{id} não usa OAuth"))?;
        self.tokens.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        self.access_token(id, p)
    }

    pub(crate) fn cache_token(&self, id: &str, t: &oauth::Tokens) {
        self.tokens.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), (t.access_token.clone(), Instant::now() + t.expires_in));
    }
}

/// Registra a renovação forçada no núcleo (IMAP/SMTP chamam quando o servidor recusa o token).
pub(crate) fn install_refresher(app: &AppHandle) {
    let app = app.clone();
    oauth::set_refresher(move |id| app.state::<AppState>().force_token(id));
}

#[cfg(not(target_os = "android"))]
fn open_browser(url: &str) -> CmdResult<()> {
    open::that(url).map_err(|e| format!("não consegui abrir o navegador: {e}"))
}

/// Android: o redirect em loopback não foi validado (ver docs/oauth-setup.md); use a senha de app.
#[cfg(target_os = "android")]
fn open_browser(_url: &str) -> CmdResult<()> {
    Err("O login com Google e Microsoft ainda não está disponível no Android; use a senha de app.".to_string())
}

/// Provedores com login OAuth disponível (Client ID digitado ou embutido no build).
#[tauri::command]
pub(crate) fn oauth_providers(app: AppHandle) -> Vec<Provider> {
    let st = app.state::<AppState>();
    [Provider::Google, Provider::Microsoft].into_iter().filter(|p| st.oauth_client(*p).is_some()).collect()
}

#[tauri::command]
pub(crate) fn oauth_config_get(app: AppHandle) -> OauthConfigView {
    OauthConfigView {
        config: app.state::<AppState>().oauth_config(),
        built_in_google: built_in(Provider::Google).0.is_some_and(|s| !s.is_empty()),
        built_in_microsoft: built_in(Provider::Microsoft).0.is_some_and(|s| !s.is_empty()),
    }
}

#[tauri::command]
pub(crate) fn oauth_config_set(app: AppHandle, config: OauthConfig) -> CmdResult<()> {
    let st = app.state::<AppState>();
    let trim = |s: String| s.trim().to_string();
    let cfg = OauthConfig { google_id: trim(config.google_id), google_secret: trim(config.google_secret), microsoft_id: trim(config.microsoft_id) };
    fs::write(st.oauth_config_path(), serde_json::to_vec_pretty(&cfg).map_err(err)?).map_err(err)
}

/// Abre o navegador, espera o login (até 3 min) e troca o código por tokens.
fn authorize(st: &AppState, provider: Provider) -> CmdResult<(oauth::Tokens, String, String)> {
    let client = st.oauth_client(provider).ok_or_else(|| {
        "O login com este provedor não está configurado: informe o Client ID em Configurações > Contas > Avançado.".to_string()
    })?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(err)?;
    let redirect = provider.redirect_uri(listener.local_addr().map_err(err)?.port());
    let (verifier, challenge) = oauth::pkce_pair();
    let state = oauth::random_token();
    open_browser(&oauth::authorization_url(provider, &client.id, &redirect, &challenge, &state))?;
    let code = oauth::wait_for_code(&listener, &state, Duration::from_secs(180)).map_err(err)?;
    let tokens = oauth::exchange_code(provider, &client, &code, &redirect, &verifier).map_err(err)?;
    let email = tokens.email.clone().ok_or_else(|| "o provedor não informou o email da conta".to_string())?;
    let refresh = tokens.refresh_token.clone().ok_or_else(|| "o provedor não devolveu refresh token".to_string())?;
    Ok((tokens, email, refresh))
}

/// Entra com Google/Microsoft no navegador do sistema e adiciona a conta.
#[tauri::command]
pub(crate) async fn oauth_login(app: AppHandle, provider: Provider) -> CmdResult<ImapAccount> {
    let handle = app.clone();
    let account = blocking(app, move |st| {
        let (tokens, email, refresh) = authorize(st, provider)?;
        let (smtp_host, smtp_port, tls) = provider.smtp();
        let host = provider.imap_host().to_string();
        let account = ImapAccount {
            id: format!("{email}@{host}"),
            host,
            port: 993,
            security: mail_core::Security::Tls,
            username: email,
            name: None,
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

/// "Entrar de novo": refaz o login de uma conta OAuth existente e troca o refresh token. Só aceita a mesma conta.
#[tauri::command]
pub(crate) async fn oauth_relogin(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |st| {
        let acc = st.accounts()?.into_iter().find(|a| a.id == id).ok_or_else(|| format!("conta {id} não encontrada"))?;
        let provider = acc.auth.provider().ok_or_else(|| "esta conta usa senha, não login do provedor".to_string())?;
        let (tokens, email, refresh) = authorize(st, provider)?;
        if !email.eq_ignore_ascii_case(&acc.username) {
            return Err(format!("Você entrou como {email}, mas esta conta é {}. Entre com a conta certa.", acc.username));
        }
        secret::set(&refresh_key(&id), &refresh).map_err(|e| format!("não consegui guardar o refresh token no cofre do sistema: {e}"))?;
        st.cache_token(&id, &tokens);
        // Sessões antigas (IDLE, comandos) usam o token recusado.
        st.hub.drop_account(&id);
        Ok(())
    })
    .await
}
