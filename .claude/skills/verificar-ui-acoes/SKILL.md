---
name: verificar-ui-acoes
description: Verifica as ações em mensagens do Posta (lida/não lida, estrela, responder a todos, encaminhar, seleção em lote, atalhos) em modo mock. Use ao mudar App.tsx, mock.ts, folders.ts, api.ts ou styles.css nessas ações.
---

# Verificar UI de ações em mensagens (mock)

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`.
2. Suba o vite como em `verificar-ui-pastas` (porta 36480, só encerre processo deste worktree).
3. Com playwright-core em 1280x800, confirme:
   - Contador de não lidas na Caixa de entrada (`.nav-item .count`) e pasta "Com estrela" na barra lateral com as mensagens `.star[data-on]`.
   - Mensagem aberta: `s` alterna a estrela e `u` marca não lida, sem animação (nenhum `[data-pop]`); clique na estrela anima (`data-pop`).
   - Caixa de seleção (`.row .check`): clique marca, Shift+clique marca intervalo, aparece `.bulk` com "N selecionadas"; Lida/Não lida/Arquivar/Excluir/Estrela valem para todas; `Esc` limpa a seleção.
   - `f` abre a escrita com "Fwd:" e o corpo com "---------- Mensagem encaminhada ----------"; `a` preenche todos os destinatários sem a própria conta.
   - `.actions` em uma linha a 1280 px (altura ~40 px).
   - Console sem erro nem aviso.
4. Print: sobrescreva `/home/renan/.t3/userdata/attachments/posta-acoes.png` (lista com seleção, estrelas e contadores). Encerre o vite.
5. Backend: rode `verificar-mail-core` (inclui o cargo check do `posta` no Docker, pois os comandos novos de lib.rs não compilam no host).
