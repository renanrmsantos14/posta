---
name: verificar-mail-core
description: Verifica o mail-core do Posta (pastas, mover, excluir, sync IMAP) sem ajuda humana. Use ao mudar crates/mail-core ou apps/client/src-tauri/src/lib.rs.
---

# Verificar mail-core (pastas / IMAP)

1. `cargo test -q -p mail-core` (unitários: descoberta de pastas, store por papel, `parse` com reply_addr de mensagem enviada, `set_reply_addr`, rascunhos locais que sobrevivem à recriação do esquema, `remove_account`, configurações e `newest_unseen` das notificações).
2. `pnpm test:imap` (GreenMail em Docker: sync por pasta, mover, excluir de vez, `fetch_to` do destinatário, `idle_acorda_quando_chega_mensagem` = IDLE). Precisa de Docker rodando.
3. Disco do /home cheio ou Docker Desktop sem acesso a /tmp: use volume nomeado (`-v posta-target:/dt -e CARGO_TARGET_DIR=/dt`) em vez de `target/docker`. Se mexeu em `apps/client/src-tauri` (threads de IDLE, evento `mail-changed`): o crate `posta` não compila no host (falta libdbus). Rode o cargo check no Docker (comando na Tarefa 4 de `docs/superpowers/plans/2026-11-08-arquivar-excluir-pastas.md`, `timeout 600000`, em background) e depois `git checkout -- apps/client/src-tauri/gen/schemas` (gerados pelo Docker).
3b. Mudou `oauth.rs`/`AuthMode`: os unitários `oauth::tests` (XOAUTH2, PKCE, URL, loopback) e `tests/account_serde.rs` (contas antigas) rodam no passo 1. Login real com Google/Microsoft exige credenciais e não é automatizável aqui.
3c. Mudou rascunhos, `remove_account`, `notify_new` ou `set_focused` em `src-tauri/src/lib.rs`: o cargo check do passo 3 cobre (plugin `tauri-plugin-notification`; o Cargo.lock muda ao adicioná-lo). Sem Android SDK, o lado Android (`POST_NOTIFICATIONS` no manifesto) só é provado pelo CI do workflow *Android*.
4. Passou quando: todos os testes verdes e o cargo check termina com `Finished` sem `error`.
