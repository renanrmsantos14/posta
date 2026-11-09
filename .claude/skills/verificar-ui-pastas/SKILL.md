---
name: verificar-ui-pastas
description: Verifica a UI de pastas (criar, renomear, excluir, arrastar), categorias, editar conta, arquivar, desfazer e excluir do Posta em modo mock (navegador, sem Tauri). Use ao mudar App.tsx, FolderTree, FolderDialog, Categories, AccountEditDialog, SettingsPage, mock.ts, folders.ts ou styles.css.
---

# Verificar UI de pastas (mock)

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`.
2. Porta 36480: `ss -ltnp | grep 36480`; se ocupada, `readlink /proc/PID/cwd` e encerre só se for deste worktree. Suba `npx vite --port 36480 --strictPort` em `apps/client`.
3. Com playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), em 1280x800 e 390x844, confirme:
   - Mensagem aberta: `e` mostra toast "Arquivada" com Desfazer; Desfazer devolve a linha ao mesmo lugar; sem desfazer, some após 4 s.
   - `#` na entrada manda para Lixeira; `#` na Lixeira abre "Excluir de vez?" (foco em Cancelar, Esc fecha, Excluir remove).
   - Mover mensagem com assunto "fatura" falha no mock: ela volta e aparece toast de erro.
   - Faixa `.cmdbar` em uma linha a 1280 px; a 390 px a faixa some e `.actions.mobile-actions` fica em uma linha, sem divisória curva solta (na Lixeira).
   - Rail: o contador `.count` não cruza o ícone (compare `getBoundingClientRect` de `.count` e do `svg` em 1280 e em 1024, modo estreito) e fica dentro da `.rail`; Favoritos e contas aparecem.
   - Console sem erro nem aviso.
4. Pastas, categorias e conta (1360x860), no mock:
   - Barra mostra Pastas (Clientes, subpasta Lumen) e Categorias (Trabalho, Pessoal, Importante).
   - "Nova pasta": cria; nome repetido mostra erro; clique fora com texto abre "Descartar alterações?" (Continuar editando / Descartar). Renomear e "Nova subpasta" (pasta mãe pré-selecionada) funcionam.
   - Árvore recolhível (mock tem Clientes/Lumen/Obras e Projetos): padrão = primeiro nível aberto, resto fechado; `.folder-caret` (aria-expanded), ← fecha e → abre com foco no item, duplo clique alterna; pasta fechada mostra a soma de não lidas das filhas; estado persiste após recarregar (configuração `folders:open`, chave conta+nome).
   - Arrasto por ponteiro (mouse, limite de 5 px; `p.mouse.down/move/up`, não HTML5): fantasma `.drag-ghost` com a contagem, alvo `[data-drop]` realçado (transição 140 ms), soltar na pasta move ("Movida para <pasta>"), Ctrl copia (fantasma com "+", a linha fica), Lixeira exclui e Arquivo arquiva com Desfazer/Ctrl+Z, Esc cancela, várias selecionadas arrastam juntas; 600 ms sobre pasta fechada abre.
   - Cabeçalho compacto a 1280 px: `.topbar` 34 + `.cmdbar-host` 26 = 60 px (antes 88); a 390 px / ponteiro grosso os alvos continuam com 44 px (topbar 52).
   - Fora do mock (só no app instalado): arrastar para fora gera .eml/anexo pelo sistema (`prepare_drag_*` + `startDrag`) e soltar arquivos externos no Composer anexa.
   - Arrastar a primeira linha para uma pasta mostra "Movida para <pasta>" e a mensagem aparece na pasta. Excluir pasta com mensagens pede confirmação (clique fora cancela); pasta vazia some direto. Pasta "Falha" falha ao criar.
   - Categoria: menu "Categoria" na mensagem e na barra em lote aplica o chip; clicar na categoria filtra; "Nenhuma" na visão da categoria remove a linha.
   - Configurações > Categorias: "Adicionar categoria". Contas > Editar: senha com menos de 3 caracteres e servidor com "erro" mostram erro inline sem salvar; salvar válido mostra "nome · usuário".
   - Prints: `posta-pastas.png` e `posta-categorias.png`.
5. Prints de referência: sobrescreva `/home/renan/.t3/userdata/attachments/posta-pastas-lixeira.png` (1280x800) e `posta-pastas-celular.png` (390x844). Encerre o vite ao terminar.
