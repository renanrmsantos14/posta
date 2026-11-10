---
name: verificar-ui-conversas
description: Verifica conversas agrupadas (fio) e a caixa unificada "Todas as caixas" do Posta em modo mock (navegador, sem Tauri). Use ao mudar threads.rs, store.rs (thread_id, thread), imap_sync.rs (refs), mock.ts, App.tsx (rows, thread, view "all") ou a opção "Agrupar por conversa" em SettingsPage.tsx.
---

# Verificar conversas e caixa unificada (mock)

1. `cargo test -q -p mail-core` (testes de `threads::`, `conversa_*` e `banco_da_v5_*` em store) e `pnpm --filter client build`.
2. Suba `npx vite --port <livre, diferente de 36480> --strictPort` em `apps/client`. O mock tem 2 contas e fios: Marina (4 mensagens, conta 1) e Helena (3, conta 2).
3. Com playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), 1280x800, confirme:
   - Caixa de entrada: uma linha "Planta e orçamento" com chip `.thread-n` = 4; abrir mostra `.thread-msg` com mensagens antigas recolhidas (`.thread-fold`); clicar numa expande.
   - Estrela/arquivar/excluir no fio valem para todas as mensagens do fio (a linha fica com estrela).
   - "Todas as caixas" (só com 2+ contas): linhas das duas contas por data, cada uma com etiqueta `.acct` (no celular, ponto colorido depois do nome); Responder numa mensagem da conta 2 abre o Composer com essa conta.
   - Configurações > "Leitura e layout" > "Agrupar por conversa" = Não: `group` = "0" nas configurações e a caixa lista as mensagens soltas (3 linhas "Planta").
   - Console sem erro (403 de fonte fora do `fs.allow` do vite não conta).
4. Prints: `/home/renan/.t3/userdata/attachments/posta-conversas.png` e `posta-unificada.png`. Encerre o vite pelo PID de `ss -ltnp`.
5. Não validável sem app real: References/In-Reply-To de servidores IMAP reais, custo de `rebuild_threads` em contas muito grandes e o comando `thread_messages` no Tauri (src-tauri não compila neste host).
