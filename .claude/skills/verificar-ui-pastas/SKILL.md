---
name: verificar-ui-pastas
description: Verifica a UI de pastas, arquivar, desfazer e excluir do Posta em modo mock (navegador, sem Tauri). Use ao mudar apps/client/src/App.tsx, mock.ts, folders.ts ou styles.css.
---

# Verificar UI de pastas (mock)

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`.
2. Porta 36480: `ss -ltnp | grep 36480`; se ocupada, `readlink /proc/PID/cwd` e encerre só se for deste worktree. Suba `npx vite --port 36480 --strictPort` em `apps/client`.
3. Com playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), em 1280x800 e 390x844, confirme:
   - Mensagem aberta: `e` mostra toast "Arquivada" com Desfazer; Desfazer devolve a linha ao mesmo lugar; sem desfazer, some após 4 s.
   - `#` na entrada manda para Lixeira; `#` na Lixeira abre "Excluir de vez?" (foco em Cancelar, Esc fecha, Excluir remove).
   - Mover mensagem com assunto "fatura" falha no mock: ela volta e aparece toast de erro.
   - `.actions` em uma linha a 1280 px (altura ~40 px) e a 390 px; sem divisória curva solta entre linhas (a 390 px na Lixeira).
   - Console sem erro nem aviso.
4. Prints de referência: sobrescreva `/home/renan/.t3/userdata/attachments/posta-pastas-lixeira.png` (1280x800) e `posta-pastas-celular.png` (390x844). Encerre o vite ao terminar.
