---
name: verificar-ui-oauth
description: Verifica os botões Entrar com Google/Microsoft do assistente Adicionar conta do Posta em modo mock (navegador, sem Tauri). Use ao mudar api.ts (oauth*) ou a disponibilidade dos providers; o fluxo completo do assistente está em verificar-ui-conta.
---

# Verificar diálogo de conta (OAuth, mock)

O diálogo agora é um assistente em etapas: rode primeiro `verificar-ui-conta`. Aqui ficam só as checagens de OAuth.

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`. Se faltarem `@tauri-apps/plugin-updater`/`plugin-process` (node_modules antigo), rode `pnpm install` ou use um config temporário de vite com alias para um stub, sem versioná-lo.
2. Suba `npx vite --port <livre, diferente de 36480> --strictPort` em `apps/client`.
3. Com playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), em 1280x820 (abaixo de ~900 px a barra lateral vira ícones e "Adicionar conta" some): abra "Adicionar conta", espere ~800 ms (animação de entrada) e confirme:
   - Email Microsoft (`x@outlook.com`): "Entrar com Microsoft" é o botão principal. Email `@gmail.com`: guia de senha de app, e "Entrar com Google" só como alternativa quando há Client ID.
   - Só aparecem os botões de providers com Client ID (`oauthProviders`). Troque o retorno do mock para `[]` e confirme que `x@outlook.com` cai direto na senha e o Gmail não mostra "Entrar com Google". Reverta o mock.
   - Clicar em "Entrar com Microsoft" mostra "Aguardando o navegador…", bloqueia X/Esc e segue para Personalizar ao concluir.
   - Console sem erro (403 de fonte fora do `fs.allow` do vite não conta).
4. Print: sobrescreva `/home/renan/.t3/userdata/attachments/posta-oauth.png`. Encerre o vite.
5. Ajustes e Client IDs: `verificar-oauth`. Não validável sem credenciais: login real, XOAUTH2 contra Google/Microsoft e renovação de token. Diga isso no PR.
