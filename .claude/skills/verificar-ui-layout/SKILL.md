---
name: verificar-ui-layout
description: Verifica em modo mock a página de Configurações, colunas redimensionáveis, tela cheia e preferências (tema, densidade, fonte, prévia, painel de leitura) do Posta. Use ao mudar App.tsx, SettingsPage.tsx, Resizer.tsx, mock.ts ou styles.css.
---

# Verificar configurações e layout (mock)

1. `pnpm --filter client build` (inclui tsc).
2. Porta 36480: `ss -ltnp | grep 36480`; se ocupada, `readlink /proc/PID/cwd` e encerre só se for deste worktree. Suba `npx vite --port 36480 --strictPort` em `apps/client` (o mock grava as preferências em `localStorage`, chave `posta-mock-settings`, então o reload as mantém).
3. Com playwright-core (ex.: `/tmp/claude-1000/pw/node_modules/playwright-core`; se faltar o browser da versão, use `executablePath` de `~/.cache/ms-playwright/chromium_headless_shell-*/`), em 1280x800 confirme:
   - Arrastar `.resizer[data-edge="list"]` (mouse down, move, up) muda a largura de `.list-pane` e, após `page.reload()`, a largura é a mesma. Limites: lista 280 a 720 px sem deixar a leitura abaixo de 360 px; pastas 180 a 360 px. `dblclick` na borda volta a 360 / 232 px.
   - `dblclick` numa linha (`[role=option]`) ou o botão `.expand-btn` põe `data-full` no `.app`: só `.reader` visível, ocupando 1280 px. Esc e o botão `.back` voltam.
   - Configurações (botão `aria-label="Configurações"` da `.topbar`; no celular ele continua na barra superior): `.settings-page` substitui a lista; seções Contas, Aparência, Leitura e layout, Escrita, Notificações, Atalhos, Atualizações; Esc ou "Voltar" retornam.
   - Em Aparência: clicar no cartão "Padrão escuro" troca o `background-color` do `body` (detalhes em verificar-ui-temas); Fonte G põe `data-size="l"` no `<html>`; Compacta + Ocultar prévia diminuem a altura das linhas (`ROW_H` em MessageList: 32 / 56 / 78 px) e o seletor Densidade (em Aparência) vale para a lista virtualizada; Embaixo põe a leitura abaixo da lista (borda `.resizer[data-axis="y"]`); Desligado abre a mensagem em tela cheia ao clicar. Reload mantém `data-pane`, `data-density`, `data-preview`.
   - A 390x844 não há `.resizer` nem `.expand-btn`, `document.documentElement.scrollWidth <= innerWidth` e a Configurações abre pela `.topbar`.
   - Estrutura: `.app` é grade com áreas top/cmd/route/status; `.route` tem rail/list/reader. Nenhum axe Critical/Serious (inclui `color-contrast` e `nested-interactive`) a 1024/1280/1440/390, claro e escuro.
   - Lista de 50 mil (`window.__mockMany(50000)` e voltar à Entrada): poucas dezenas de `[role=option]` no DOM, rolagem sem long tasks relevantes (PerformanceObserver `longtask`), cabeçalhos Hoje/Ontem/Esta semana/Semana passada/mês, abas Foco/Outros persistem no reload.
   - Console sem erro nem aviso (a mensagem `Blocked script execution in 'about:srcdoc'` do iframe do HtmlBody em headless já existe no master e é ignorada).
   - Nota: atributos `data-*` booleanos do React valem `"true"` (ex.: `dataset.full === "true"`).
4. Prints de referência (sobrescreva): `/home/renan/.t3/userdata/attachments/posta-config.png`, `posta-colunas.png` (durante o arrasto), `posta-tela-cheia.png`, `posta-escuro.png`. Encerre o vite ao terminar.

Não coberto no mock: gravação real via `set_setting` do Tauri e notificações do sistema.
