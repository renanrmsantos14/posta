---
name: verificar-mail-core
description: Verifica o mail-core do Posta (pastas, mover, excluir, sync IMAP) sem ajuda humana. Use ao mudar crates/mail-core ou apps/client/src-tauri/src/lib.rs.
---

# Verificar mail-core (pastas / IMAP)

1. `cargo test -q -p mail-core` (unitários: descoberta de pastas, store por papel).
2. `pnpm test:imap` (GreenMail em Docker: sync por pasta, mover, excluir de vez). Precisa de Docker rodando.
3. Se mexeu em `apps/client/src-tauri`: o crate `posta` não compila no host (falta libdbus). Rode o cargo check no Docker (comando na Tarefa 4 de `docs/superpowers/plans/2026-11-08-arquivar-excluir-pastas.md`, `timeout 600000`, em background) e depois `git checkout -- apps/client/src-tauri/gen/schemas` (gerados pelo Docker).
4. Passou quando: todos os testes verdes e o cargo check termina com `Finished` sem `error`.
