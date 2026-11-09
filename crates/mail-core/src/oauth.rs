//! Entrada com Google e Microsoft: Authorization Code + PKCE, redirect em loopback e XOAUTH2.
//! Sem senha de app. O refresh token fica com quem chama (cofre do sistema); aqui só há protocolo.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};
use url::Url;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Google,
    Microsoft,
}

/// Aplicativo registrado no provedor. Em app desktop o secret (Google) não é confidencial.
#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub secret: Option<String>,
    /// Só para testes (servidor OAuth falso); em produção vale o endpoint oficial do provedor.
    pub token_url: Option<String>,
}

impl Provider {
    pub fn imap_host(self) -> &'static str {
        match self {
            Provider::Google => "imap.gmail.com",
            Provider::Microsoft => "outlook.office365.com",
        }
    }

    /// (host, porta, true se TLS direto). Microsoft usa STARTTLS na 587.
    pub fn smtp(self) -> (&'static str, u16, bool) {
        match self {
            Provider::Google => ("smtp.gmail.com", 465, true),
            Provider::Microsoft => ("smtp.office365.com", 587, false),
        }
    }

    fn auth_endpoint(self) -> &'static str {
        match self {
            Provider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Provider::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }

    fn token_endpoint(self) -> &'static str {
        match self {
            Provider::Google => "https://oauth2.googleapis.com/token",
            Provider::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }

    /// Escopos de email mais `openid email`, para ler o endereço no id_token sem outra chamada.
    fn scopes(self) -> &'static str {
        match self {
            Provider::Google => "https://mail.google.com/ openid email",
            Provider::Microsoft => {
                "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access openid email profile"
            }
        }
    }

    /// O Azure só aceita `localhost` no redirect de app desktop; o Google aceita o IP literal.
    fn redirect_host(self) -> &'static str {
        match self {
            Provider::Google => "127.0.0.1",
            Provider::Microsoft => "localhost",
        }
    }

    pub fn redirect_uri(self, port: u16) -> String {
        format!("http://{}:{port}", self.redirect_host())
    }
}

/// Resposta do endpoint de token.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub access_token: String,
    /// Ausente na renovação do Google (o anterior continua valendo).
    pub refresh_token: Option<String>,
    pub expires_in: Duration,
    /// Email da conta, quando o id_token veio junto.
    pub email: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    id_token: Option<String>,
}

/// Verificador e desafio S256 (RFC 7636).
pub fn pkce_pair() -> (String, String) {
    let verifier = random_token();
    let challenge = pkce_challenge(&verifier);
    (verifier, challenge)
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// 32 bytes aleatórios do sistema em base64url (43 caracteres).
pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("sem fonte de aleatoriedade do sistema");
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn authorization_url(p: Provider, client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    let mut url = Url::parse(p.auth_endpoint()).expect("endpoint fixo");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", p.scopes())
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);
        match p {
            // Garante o refresh token mesmo em novo consentimento.
            Provider::Google => q.append_pair("access_type", "offline").append_pair("prompt", "consent"),
            Provider::Microsoft => q.append_pair("prompt", "select_account"),
        };
    }
    url.into()
}

/// Valor de AUTHENTICATE XOAUTH2 (antes do base64, que o IMAP e o SMTP aplicam).
pub fn xoauth2(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

const PAGE: &str = "<!doctype html><meta charset=utf-8><title>Posta</title>\
<body style=\"font-family:system-ui;text-align:center;margin-top:20vh\"><h2>MSG</h2><p>Pode fechar esta aba e voltar ao Posta.</p>";

fn respond(stream: &mut TcpStream, status: &str, msg: &str) {
    let body = PAGE.replace("MSG", msg);
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// Interpreta a linha de requisição do redirect. `None` = não é o redirect (ex.: favicon ou estado alheio).
pub(crate) fn parse_redirect(request: &str, state: &str) -> Option<Result<String>> {
    let target = request.lines().next()?.strip_prefix("GET ")?.split(' ').next()?;
    let url = Url::parse(&format!("http://loopback{target}")).ok()?;
    let get = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
    if get("state").is_none() && get("error").is_none() {
        return None;
    }
    // Estado diferente: não é o nosso redirect (outra aba ou programa local batendo na porta); ignora e segue esperando.
    if get("state").as_deref() != Some(state) {
        return None;
    }
    if let Some(e) = get("error") {
        let aadsts = get("error_description").as_deref().and_then(aadsts_of);
        let hint = match (e.as_str(), aadsts.as_deref()) {
            (_, Some("65001" | "90094" | "65004")) => " O administrador do Microsoft 365 precisa aprovar o Posta para esta organização.",
            ("access_denied", _) => " Se for conta Google, confirme que ela está na lista de usuários de teste do app; se for Microsoft 365, o administrador pode precisar aprovar o Posta.",
            _ => "",
        };
        return Some(Err(Error::Invalid(format!("Login recusado ({e}).{hint}"))));
    }
    Some(get("code").ok_or_else(|| Error::Invalid("resposta de login sem código".into())))
}

/// Espera o redirect do navegador no listener local e devolve o `code`.
pub fn wait_for_code(listener: &TcpListener, state: &str, timeout: Duration) -> Result<String> {
    listener.set_nonblocking(true).map_err(|e| Error::Invalid(e.to_string()))?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).ok();
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                match parse_redirect(&String::from_utf8_lossy(&buf[..n]), state) {
                    Some(r) => {
                        match &r {
                            Ok(_) => respond(&mut stream, "200 OK", "Conta conectada"),
                            Err(_) => respond(&mut stream, "400 Bad Request", "Não foi possível entrar"),
                        }
                        return r;
                    }
                    None => respond(&mut stream, "404 Not Found", "Nada aqui"),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(Error::Invalid("tempo esgotado esperando o login no navegador".into()));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(Error::Invalid(e.to_string())),
        }
    }
}

/// Falha de renovação do token.
#[derive(Debug)]
pub enum RefreshError {
    /// O provedor não aceita mais o refresh token (expirou, foi revogado, exige consentimento...). Só um novo login resolve.
    NeedsLogin(String),
    /// Rede fora, servidor instável ou configuração errada: tentar de novo mais tarde.
    Failed(Error),
}

impl std::fmt::Display for RefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshError::NeedsLogin(m) => f.write_str(m),
            RefreshError::Failed(e) => e.fmt(f),
        }
    }
}

/// Erro do endpoint de token: só o código e o número AADSTS (Microsoft); o corpo pode ecoar dados da requisição.
struct TokenFailure {
    status: u16,
    code: String,
    aadsts: Option<String>,
    cause: Option<Error>,
}

fn aadsts_of(text: &str) -> Option<String> {
    let at = text.find("AADSTS")?;
    let digits: String = text[at + 6..].chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

fn token_request(p: Provider, client: &Client, mut form: Vec<(&str, String)>) -> std::result::Result<Tokens, TokenFailure> {
    let net = |e: String| TokenFailure { status: 0, code: "rede".into(), aadsts: None, cause: Some(Error::Invalid(format!("OAuth: {e}"))) };
    form.push(("client_id", client.id.clone()));
    if let Some(s) = client.secret.as_ref().filter(|s| !s.is_empty()) {
        form.push(("client_secret", s.clone()));
    }
    let pairs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        // Sem prazo, um servidor mudo prende a thread (e o trava de sync) para sempre.
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let url = client.token_url.as_deref().unwrap_or_else(|| p.token_endpoint());
    let mut resp = agent.post(url).send_form(pairs).map_err(|e| net(e.to_string()))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().map_err(|e| net(e.to_string()))?;
    if status != 200 {
        let v = serde_json::from_str::<serde_json::Value>(&text).ok();
        let code = v.as_ref().and_then(|v| v["error"].as_str()).map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"));
        let aadsts = v.as_ref().and_then(|v| v["error_description"].as_str()).and_then(aadsts_of);
        return Err(TokenFailure { status, code, aadsts, cause: None });
    }
    let t: TokenResponse = serde_json::from_str(&text).map_err(|e| net(format!("resposta inválida: {e}")))?;
    Ok(Tokens {
        email: t.id_token.as_deref().and_then(email_from_id_token),
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_in: Duration::from_secs(t.expires_in.unwrap_or(3600)),
    })
}

/// Explica ao usuário por que o login ou a renovação foi recusado. `None` = erro que um novo login não resolve.
fn explain(p: Provider, f: &TokenFailure) -> Option<String> {
    let admin = matches!(f.aadsts.as_deref(), Some("65001" | "90094" | "65004" | "500011"));
    if p == Provider::Microsoft && admin {
        return Some(
            "O Microsoft 365 desta conta exige aprovação do administrador para o Posta. Peça a ele para conceder o consentimento ao aplicativo (permissões IMAP.AccessAsUser.All e SMTP.Send) e entre de novo."
                .into(),
        );
    }
    match (p, f.code.as_str()) {
        (Provider::Google, "invalid_grant") => Some(
            "O Google encerrou o acesso do Posta a esta conta (token expirado ou revogado). Se o app do Google Cloud ainda está em modo de teste, o acesso vale só 7 dias. Entre de novo."
                .into(),
        ),
        (Provider::Microsoft, "invalid_grant" | "interaction_required" | "consent_required" | "login_required") => Some(
            "A Microsoft pediu um novo login nesta conta (sessão expirada, senha alterada, verificação em duas etapas ou acesso revogado). Entre de novo."
                .into(),
        ),
        (_, "access_denied") => Some("O acesso foi recusado. Entre de novo e aceite as permissões pedidas.".into()),
        _ => None,
    }
}

fn plain(f: TokenFailure) -> Error {
    f.cause.unwrap_or_else(|| Error::Invalid(format!("OAuth: {}", f.code)))
}

pub fn exchange_code(p: Provider, client: &Client, code: &str, redirect_uri: &str, verifier: &str) -> Result<Tokens> {
    let form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", verifier.to_string()),
    ];
    token_request(p, client, form).map_err(|f| match explain(p, &f) {
        Some(m) => Error::Invalid(m),
        None => plain(f),
    })
}

pub fn refresh(p: Provider, client: &Client, refresh_token: &str) -> std::result::Result<Tokens, RefreshError> {
    let mut form = vec![("grant_type", "refresh_token".to_string()), ("refresh_token", refresh_token.to_string())];
    if p == Provider::Microsoft {
        form.push(("scope", p.scopes().to_string()));
    }
    token_request(p, client, form).map_err(|f| match f.status {
        // 5xx e rede não significam "refresh token morto": só 4xx com código conhecido pedem novo login.
        400..=499 => match explain(p, &f) {
            Some(m) => RefreshError::NeedsLogin(m),
            None => RefreshError::Failed(plain(f)),
        },
        _ => RefreshError::Failed(plain(f)),
    })
}

/// Entrega um access token recém-renovado da conta; registrada por quem guarda o refresh token.
type Refresher = Box<dyn Fn(&str) -> std::result::Result<String, String> + Send + Sync>;
static REFRESHER: std::sync::OnceLock<Refresher> = std::sync::OnceLock::new();

/// Registra (uma vez) como obter um access token novo da conta `id`.
/// Usada quando o servidor recusa o token em cache (revogado ou vencido antes do previsto).
pub fn set_refresher(f: impl Fn(&str) -> std::result::Result<String, String> + Send + Sync + 'static) {
    let _ = REFRESHER.set(Box::new(f));
}

/// Roda `op`; se o servidor recusar a autenticação OAuth, renova o token e tenta de novo uma vez.
pub(crate) fn with_fresh_token_retry<T>(
    acc: &crate::ImapAccount,
    is_auth_error: impl Fn(&Error) -> bool,
    op: impl Fn(&crate::ImapAccount) -> Result<T>,
) -> Result<T> {
    let first = op(acc);
    let Err(e) = &first else { return first };
    if acc.auth == crate::AuthMode::Password || !is_auth_error(e) {
        return first;
    }
    let Some(refresher) = REFRESHER.get() else { return first };
    match refresher(&acc.id) {
        Ok(token) => {
            let mut retry = acc.clone();
            retry.password = token;
            match op(&retry) {
                Err(e2) if is_auth_error(&e2) => Err(explain_server_refusal(acc, e2)),
                other => other,
            }
        }
        // Renovação recusada: a mensagem já diz o que fazer (entrar de novo).
        Err(msg) => Err(Error::Invalid(msg)),
    }
}

/// O servidor recusou mesmo com token novo: tenta dizer o motivo provável.
fn explain_server_refusal(acc: &crate::ImapAccount, e: Error) -> Error {
    let text = e.to_string();
    if acc.auth == crate::AuthMode::Microsoft {
        if text.contains("5.7.139") || text.contains("SmtpClientAuthentication") {
            return Error::Invalid(
                "O SMTP AUTH está desativado nesta caixa do Microsoft 365. O administrador precisa ativá-lo (Exchange admin > usuário > Gerenciar aplicativos de email) para o Posta enviar.".into(),
            );
        }
        if text.contains("not connected") {
            return Error::Invalid("A Microsoft recusou o acesso IMAP desta caixa. O administrador precisa ativar o IMAP para ela.".into());
        }
    }
    e
}

/// Email do payload do id_token. Sem verificar a assinatura: veio direto do endpoint de token por TLS.
fn email_from_id_token(jwt: &str) -> Option<String> {
    let payload = URL_SAFE_NO_PAD.decode(jwt.split('.').nth(1)?).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    ["email", "preferred_username"]
        .iter()
        .find_map(|k| v[k].as_str().filter(|s| s.contains('@')))
        .map(str::to_string)
}

/// Resposta XOAUTH2 do IMAP: o token na primeira vez; vazio no desafio de erro (o servidor então recusa).
pub(crate) struct XOauth2 {
    payload: String,
    sent: std::cell::Cell<bool>,
}

impl XOauth2 {
    pub(crate) fn new(user: &str, token: &str) -> Self {
        Self { payload: xoauth2(user, token), sent: std::cell::Cell::new(false) }
    }
}

impl imap::Authenticator for XOauth2 {
    type Response = String;
    fn process(&self, _challenge: &[u8]) -> String {
        if self.sent.replace(true) {
            String::new()
        } else {
            self.payload.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use std::collections::HashMap;

    #[test]
    fn xoauth2_segue_o_formato_documentado() {
        let s = xoauth2("someuser@example.com", "ya29.vF9dft4qmTc2Nvb3RlckBhdHRhdmlzdGEuY29tCg");
        assert_eq!(s, "user=someuser@example.com\u{1}auth=Bearer ya29.vF9dft4qmTc2Nvb3RlckBhdHRhdmlzdGEuY29tCg\u{1}\u{1}");
        // Exemplo oficial da Microsoft, em base64.
        assert_eq!(
            STANDARD.encode(s),
            "dXNlcj1zb21ldXNlckBleGFtcGxlLmNvbQFhdXRoPUJlYXJlciB5YTI5LnZGOWRmdDRxbVRjMk52YjNSbGNrQmhkSFJoZG1semRHRXVZMjl0Q2cBAQ=="
        );
    }

    #[test]
    fn xoauth2_do_imap_responde_vazio_ao_desafio_de_erro() {
        use imap::Authenticator;
        let a = XOauth2::new("a@b.c", "tok");
        assert_eq!(a.process(b""), xoauth2("a@b.c", "tok"));
        assert_eq!(a.process(b"{\"status\":\"400\"}"), "");
    }

    #[test]
    fn pkce_confere_com_o_vetor_da_rfc() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let (v, c) = pkce_pair();
        assert_eq!(v.len(), 43);
        assert_eq!(pkce_challenge(&v), c);
    }

    #[test]
    fn url_de_autorizacao_leva_pkce_e_escopos() {
        let g = Url::parse(&authorization_url(Provider::Google, "cid", "http://127.0.0.1:5000", "CH", "ST")).unwrap();
        let q: HashMap<_, _> = g.query_pairs().into_owned().collect();
        assert_eq!(g.host_str(), Some("accounts.google.com"));
        assert_eq!(q["code_challenge"], "CH");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["state"], "ST");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:5000");
        assert!(q["scope"].starts_with("https://mail.google.com/"));
        assert_eq!(q["access_type"], "offline");

        let redirect = Provider::Microsoft.redirect_uri(5000);
        let m = Url::parse(&authorization_url(Provider::Microsoft, "cid", &redirect, "CH", "ST")).unwrap();
        let q: HashMap<_, _> = m.query_pairs().into_owned().collect();
        assert_eq!(q["redirect_uri"], "http://localhost:5000");
        assert!(q["scope"].contains("https://outlook.office.com/IMAP.AccessAsUser.All"));
        assert!(q["scope"].contains("https://outlook.office.com/SMTP.Send"));
        assert!(q["scope"].contains("offline_access"));
    }

    #[test]
    fn redirect_valida_estado_e_extrai_codigo() {
        let ok = parse_redirect("GET /?code=abc%2F1&state=S HTTP/1.1\r\nHost: x\r\n\r\n", "S");
        assert_eq!(ok.unwrap().unwrap(), "abc/1");
        assert!(parse_redirect("GET /?code=abc&state=outro HTTP/1.1\r\n", "S").is_none());
        assert!(parse_redirect("GET /?error=access_denied&state=S HTTP/1.1\r\n", "S").unwrap().is_err());
        assert!(parse_redirect("GET /favicon.ico HTTP/1.1\r\n", "S").is_none());
    }

    #[test]
    fn loopback_recebe_o_redirect_do_navegador() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET /?code=XYZ&state=S HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        });
        assert_eq!(wait_for_code(&listener, "S", Duration::from_secs(5)).unwrap(), "XYZ");
        assert!(t.join().unwrap().starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn email_vem_do_id_token() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"preferred_username":"a@outlook.com"}"#);
        assert_eq!(email_from_id_token(&format!("h.{payload}.s")).as_deref(), Some("a@outlook.com"));
        assert_eq!(email_from_id_token("lixo"), None);
    }
}
