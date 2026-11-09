---
name: verificar-oauth
description: Verifica o login Google/Microsoft do Posta (OAuth, XOAUTH2, renovação de token, mensagens "Entrar de novo") sem credenciais reais. Use ao mudar crates/mail-core/src/oauth.rs, a ligação com imap_sync/send, apps/client/src-tauri/src/oauth_app.rs, toast.tsx ou OauthSettings.tsx.
---

# Verificar OAuth (sem credenciais)

1. `cargo test -q -p mail-core --test oauth_flow` (servidores falsos locais): troca de código com PKCE, refresh (escopo Microsoft), `NeedsLogin` (4xx com código conhecido) vs `Failed` (5xx/rede), erro sem eco de segredo, IMAP XOAUTH2 (token bom; renova e repete uma vez; renovação recusada; sem laço; erro de rede não renova) e SMTP 535 que renova.
2. `cargo test -q -p mail-core` inteiro: inclui `oauth::tests` (PKCE, string XOAUTH2, URL, loopback, mensagens) e `account_serde` (contas antigas por senha continuam lendo).
3. Mudou `oauth_app.rs`/`lib.rs` em `src-tauri`: o crate `posta` não compila no host. Rode `cargo check -p posta` no Docker (`rust:1-bookworm`, volume `posta-target`, instalar libwebkit2gtk-4.1-dev etc.; em background) e depois `git checkout -- apps/client/src-tauri/gen/schemas`.
4. Front: `pnpm -C apps/client exec tsc --noEmit` (erros de `className` em ícones Phosphor são do ambiente) e `pnpm --filter client build`. UI em mock: siga `verificar-ui-oauth` (diálogo Adicionar conta) e confira em Configurações › Contas o provedor ("Conta Google · host"), "Entrar de novo" e o bloco Avançado (Client IDs).
5. Nunca logar tokens: `rg -n "println|eprintln|log::" crates/mail-core/src/oauth.rs apps/client/src-tauri/src/oauth_app.rs` não pode imprimir access/refresh token.
6. Não validável sem Client IDs reais: login no navegador, XOAUTH2 contra Gmail/Outlook, refresh real, expiração de 7 dias do Google em modo teste, consentimento do admin e SMTP AUTH do M365, Android.
