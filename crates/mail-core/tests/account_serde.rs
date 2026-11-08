//! Contas salvas em disco: compatibilidade com o formato anterior ao OAuth e sigilo do token.

use mail_core::oauth::Provider;
use mail_core::{AuthMode, ImapAccount};

#[test]
fn conta_salva_antes_do_oauth_continua_valendo_como_senha() {
    let old = r#"[{"id":"a@b.com@imap.b.com","host":"imap.b.com","port":993,"security":"tls","username":"a@b.com"}]"#;
    let accs: Vec<ImapAccount> = serde_json::from_str(old).unwrap();
    assert_eq!(accs[0].auth, AuthMode::Password);
    assert!(accs[0].password.is_empty());
}

#[test]
fn modo_oauth_vai_ao_disco_mas_o_token_nao() {
    let mut acc: ImapAccount = serde_json::from_str(
        r#"{"id":"x","host":"imap.gmail.com","port":993,"security":"tls","username":"x@gmail.com","auth":"google"}"#,
    )
    .unwrap();
    acc.password = "ya29.segredo".into();
    let json = serde_json::to_string(&acc).unwrap();
    assert!(json.contains(r#""auth":"google""#));
    assert!(!json.contains("segredo"));
    assert_eq!(acc.auth.provider(), Some(Provider::Google));
}
