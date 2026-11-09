---
name: verificar-ui-rascunhos
description: Verifica rascunhos, assinatura, remoção de conta, filtro por conta e notificações do Posta em modo mock (navegador, sem Tauri). Use ao mudar Composer.tsx, SettingsDialog.tsx, App.tsx (rascunhos/contas/título), mock.ts, api.ts, styles.css ou os comandos de rascunho/configuração/notificação em src-tauri/src/lib.rs.
---

# Verificar rascunhos, contas e notificações (mock)

1. `pnpm -C apps/client build` (inclui `tsc -b`) e `cargo test -q -p mail-core` (testes `rascunho_*`, `remover_conta_*`, `configuracoes_e_novas_nao_lidas`).
2. Suba `npx vite --port <livre, diferente de 36480> --strictPort` em `apps/client` (o mock já traz 2 rascunhos e uma assinatura para a conta de exemplo).
3. Com playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), em 1280x800, confirme:
   - Barra lateral: "Rascunhos" com contagem; clicar lista os rascunhos (destinatário, assunto, trecho); clicar num abre o Composer preenchido.
   - Editar o corpo e esperar ~1,9 s mostra "Rascunho salvo" no rodapé; Esc fecha, mostra o toast "Rascunho salvo" e o texto continua na lista (nunca perde texto).
   - "Escrever" abre o corpo com `\n\n-- \n<assinatura>\n`; responder coloca a assinatura acima da citação.
   - Lixeira do Composer: rascunho aberto sem edição descarta direto; com edição pede "Descartar rascunho?" e remove da lista.
   - Configurações: switch "Avisar de email novo", textarea de Assinatura (grava ao digitar), lista Contas com "Remover" que abre "Remover conta?" (Cancelar/Esc/clique fora fecham; Remover apaga a conta e os rascunhos dela).
   - Com 2+ contas (adicione uma em "Adicionar conta") a barra lateral mostra "Todas as contas" e cada conta como filtro.
   - Anexos no rascunho: anexe um arquivo, espere o autosave, feche com Esc e reabra em Rascunhos: o chip do anexo volta.
   - Trocar o "De" (2+ contas) troca o bloco `-- ` da assinatura (conta sem assinatura remove o bloco; voltar a recoloca). Anexos acima de 25 MB (ex.: encaminhar) mostram o aviso e o Enviar recusa.
   - Configurações > Envio: "Desfazer envio" (Desligado/5/10 s, padrão 5 s). Enviar mostra o toast "Enviando… Desfazer" com a barra de tempo; Desfazer reabre o painel; desligado envia na hora.
   - Remover a conta do "De" com o Composer aberto: o painel passa para outra conta (toast) ou fecha se não sobrou nenhuma.
   - Com filtro de conta, os contadores da barra lateral contam só aquela conta (a Caixa de entrada de uma conta sem mensagens some do contador); o título da janela soma todas.
   - Console sem erro nem aviso (403 de fonte fora do `fs.allow` do vite não conta).
4. Print: sobrescreva `/home/renan/.t3/userdata/attachments/posta-rascunhos.png` (Rascunhos atrás + Configurações). Encerre o vite pelo PID de `ss -ltnp`.
5. Não validável sem app real: a notificação do sistema (permissão no Android 13+, texto, o clique), `set_title` da janela e a remoção do segredo no cofre. No desktop o plugin de notificação não expõe o clique: o aviso não foca o app. Registre no PR.
