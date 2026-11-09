---
name: verificar-ui-oauth
description: Verifica o diálogo Adicionar conta com Entrar com Google/Microsoft do Posta em modo mock (navegador, sem Tauri). Use ao mudar AccountDialog.tsx, api.ts (oauth*), mock.ts ou styles.css do diálogo.
---

# Verificar diálogo de conta (OAuth, mock)

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`. Se faltarem `@tauri-apps/plugin-updater`/`plugin-process` (node_modules antigo), rode `pnpm install` ou use um config temporário de vite com alias para um stub, sem versioná-lo.
2. Suba `npx vite --port <livre, diferente de 36480> --strictPort` em `apps/client`.
3. Com playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), em 1280x820 (abaixo de ~900 px a barra lateral vira ícones e "Adicionar conta" some): abra "Adicionar conta", espere ~800 ms (animação de entrada) e confirme:
   - "Entrar com Google" e "Entrar com Microsoft" acima do formulário, habilitados no mock, com divisória "ou use email e senha".
   - Só aparecem os botões de providers com Client ID (`oauthProviders`). Troque o retorno do mock para `["microsoft"]` (como no release atual, sem ID do Google) e confirme que o Google some; com `[]` some o bloco inteiro, inclusive a divisória. Reverta o mock.
   - Email `@gmail.com`: dica de senha de app com o link "Gerar senha de app" (abre `https://myaccount.google.com/apppasswords`).
   - Clicar em Google mostra "Aguardando o navegador…", bloqueia X/Esc e fecha o diálogo ao concluir.
   - Console sem erro (403 de fonte fora do `fs.allow` do vite não conta).
4. Print: sobrescreva `/home/renan/.t3/userdata/attachments/posta-oauth.png`. Encerre o vite.
5. Ajustes e Client IDs: `verificar-oauth`. Não validável sem credenciais: login real, XOAUTH2 contra Google/Microsoft e renovação de token. Diga isso no PR.
