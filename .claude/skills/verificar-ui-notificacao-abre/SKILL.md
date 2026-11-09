---
name: verificar-ui-notificacao-abre
description: Verifica o clique na notificação de email novo do Posta (abrir conta, pasta e email; fallback quando o email sumiu). Use ao mudar src-tauri/src/open_target.rs, notify_new em lib.rs, android_sync.rs, SyncWorker.kt, MainActivity.kt, src/notificationOpen.ts ou o efeito `watchOpenTarget` do App.tsx.
---

# Verificar o clique na notificação (Rust + mock + manual)

Fluxo: `notify_new` (desktop/Android vivo) ou `SyncWorker` (Android fechado) grava `{account, folder, id?}` na notificação -> clique -> `open_target::request` (desktop: `show_main`) guarda o pedido em `AppState.pending_open` e emite `open-target` -> `watchOpenTarget` (notificationOpen.ts) o busca (`take_pending_open`, ou `window.PostaNative.takePending()` no Android) -> App.tsx abre a pasta e o email.

1. `cargo test -p posta --lib open_target` (parse do payload: mensagem, pasta, ausente, malformado, sem conta/pasta, serialização) no Docker (crate `posta` não compila no host; ver `verificar-mail-core` passo 3, com `-v posta-target:/dt -e CARGO_TARGET_DIR=/dt`). Também `cargo check -p posta` e `pnpm -C apps/client build`; depois `git checkout -- apps/client/src-tauri/gen/schemas`.
2. Mock (Vite em porta livre, `--strictPort`, playwright-core, 1280x800). O clique é simulado com `window.dispatchEvent(new CustomEvent("posta-open", { detail }))`, conta `renan@imap.exemplo.com`, email `1100` (Entrada). Confirme, partindo de "Enviados":
   - `{account, folder: "inbox", id: 1100}`: a Caixa de entrada fica ativa e o leitor mostra "Planta e orçamento da sala de reunião".
   - `{account, folder: "inbox"}` (grupo): abre a Caixa de entrada, leitor vazio, sem toast.
   - `id: 999999` (excluído): abre a pasta e o toast "Esse email não está mais aqui. Abri a pasta.".
   - Email arquivado (tecla `e` no 1100) e, após ~6 s (janela do Desfazer), o mesmo clique: pasta + toast. Dentro da janela do Desfazer o email ainda existe e abre.
   - Payload sem `account`: nada acontece.
   - Console sem erro (403 de fonte fora do `fs.allow` não conta).
3. Android (só o CI e o aparelho provam): `MainActivity.kt` e `SyncWorker.kt` não compilam sem SDK local; ver `verificar-android`.
4. Não validável sem app real: o clique do sistema. Windows: instalar o MSI/NSIS (em dev o toast sai como "PowerShell" e o clique pode não voltar), minimizar ou esconder na bandeja, receber email, clicar no toast: a janela vem à frente e abre o email. O clique só chega com o toast ainda na tela. Android: ver o roteiro no PR (app fechado, em segundo plano e aberto).
