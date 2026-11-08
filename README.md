# Posta

Cliente de email pessoal, rápido e personalizado. Windows, Linux e Android (iOS depois), com Tauri 2 + React e núcleo em Rust.

## Como funciona

- `crates/mail-core`: sincroniza IMAP para um SQLite local (WAL + busca FTS5 sem acento). A UI só lê o banco local, por isso abre e busca na hora.
- `apps/client`: interface React. Fora do Tauri (`pnpm dev` no navegador) usa dados de exemplo de `src/mock.ts`.
- `apps/client/src-tauri`: comandos Tauri. Contas ficam em `accounts.json` na pasta de dados do app; senhas no cofre do sistema (crate `keyring`).

## Comandos

| Comando | O que faz |
| --- | --- |
| `pnpm dev` | UI no navegador em http://localhost:36480 com dados de exemplo |
| `pnpm --filter client tauri dev` | App desktop real (precisa das dependências do Tauri no sistema) |
| `pnpm --filter client tauri build` | Instalador; incrementa a versão (`v0.0.x · data/hora`) |
| `pnpm test` | Testes do núcleo (banco local) |
| `pnpm test:imap` | Sobe Greenmail no Docker e testa sincronização IMAP real, incluindo 1.000 mensagens |

Linux precisa de `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev` e de um serviço de segredos (GNOME Keyring/KWallet).

## Atalhos

`j`/`k` ou setas navegam, `/` busca, `r` sincroniza, `Esc` limpa a busca ou volta à lista.

## Próximos passos

1. Responder, escrever e enviar (SMTP).
2. Arquivar e excluir; pastas além da caixa de entrada.
3. IMAP IDLE para chegada instantânea, no lugar do intervalo de 60 s.
4. OAuth: Microsoft Graph e Gmail API.
5. Android: guardar a senha no Keystore (o `keyring` padrão não cobre Android) e sincronizar em segundo plano.
6. Emails em HTML, exibidos em iframe isolado.
