---
name: verificar-ui-lista-paginada
description: Verifica a lista paginada por cursor e a atualização por delta do Posta em modo mock (navegador, sem Tauri). Use ao mudar useMessageList.ts, MessageList.tsx, mock.ts (listPage, summaries, arrive), api.ts (listPage, summaries, MailDelta), o listener de mail-delta em App.tsx ou os comandos list_messages_page/get_summaries.
---

# Verificar lista paginada e delta (mock)

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`.
2. Meça no build de produção (o dev do React distorce): `pnpm -C apps/client exec vite preview --port 36480 --strictPort`.
3. Com playwright-core (chromium) abra a página e rode `window.__mockMany(50000)`:
   - a primeira página aparece em menos de 300 ms; `window.__mockCalls` mostra `listPage` crescendo só ao rolar;
   - role `.list` até `#50000` (~20 s no mock): 0 long tasks, heap estável (dezenas de MB);
   - de volta ao topo, `window.__mockArrive(1)`: a mensagem entra no topo sem recarregar (`listPage` e `list` não aumentam; `summaries` +1).
4. Delete na primeira linha e Ctrl+Z restauram a mesma linha; busca, abas Foco/Outros e cabeçalhos de data continuam funcionando.
5. Offline: `window.__mockOffline(true)`, busque `#18`, abra uma mensagem sem corpo: aparece `.reader-error` com "Sem conexão: o corpo desta mensagem ainda não foi baixado." e o botão "Tentar de novo"; volte online e clique: a mensagem abre.
6. axe-core: 0 Critical/Serious (a lista precisa ser focável mesmo com a linha de foco fora da janela). Console sem erros novos (o 404 de favicon já existia).
7. Encerre o vite pelo PID.

Limite: o mock não prova o evento `mail-delta` real do Tauri; isso depende do app nativo.
