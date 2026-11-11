---
name: verificar-ui-pesquisas
description: Verifica pesquisas salvas do Posta (salvar, abrir, renomear, excluir) no mock e a lógica de `savedSearches.ts`. Use ao mudar savedSearches.ts, SearchDialog.tsx, TopBar.tsx (botão Salvar pesquisa), Rail.tsx (seção Pesquisas) ou o trecho de pesquisas em App.tsx.
---

# Verificar pesquisas salvas

1. `node --experimental-strip-types --test apps/client/tests/savedSearches.test.mjs` e `pnpm --filter client build`.
2. Suba `npx vite --port <livre> --strictPort` em `apps/client` (a 36480 costuma estar ocupada por outro serviço: confira com `ss -ltnp`). Com playwright-core, em 1440x800 e 390x800 (touch):
   - Digite `de:ana tem:anexo`: aparece o botão `[aria-label="Salvar pesquisa"]` (some com a busca vazia). Ele abre o diálogo com o nome sugerido selecionado; Enter salva e mostra o toast.
   - Seção `nav[aria-label="Pesquisas"]` na barra lateral (no celular, dentro da gaveta; há duas cópias no DOM, filtre por `:visible`).
   - Clicar na pesquisa restaura a consulta (e o escopo/pasta); `aria-current="page"` enquanto a busca é a mesma.
   - Renomear (ícone, hover no desktop; sempre visível no toque) e Excluir (com Desfazer no toast); botão direito abre o menu com as mesmas ações.
   - Persistência: `localStorage["posta-mock-settings"].searches`; recarregar mantém a lista.
3. Prints: `pesquisas-barra-desktop.png`, `pesquisas-dialogo-desktop.png`, `pesquisas-barra-celular.png` em `/home/renan/.t3/userdata/attachments/`. Encerre o vite ao terminar.
