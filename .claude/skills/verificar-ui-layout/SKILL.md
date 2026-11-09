---
name: verificar-ui-layout
description: Verifica em modo mock a página de Configurações, colunas redimensionáveis, tela cheia e preferências (tema, densidade, fonte, prévia, painel de leitura) do Posta. Use ao mudar App.tsx, SettingsPage.tsx, Resizer.tsx, mock.ts ou styles.css.
---

# Verificar configurações e layout (mock)

1. `pnpm --filter client build` (inclui tsc).
2. Porta 36480: `ss -ltnp | grep 36480`; se ocupada, `readlink /proc/PID/cwd` e encerre só se for deste worktree. Suba `npx vite --port 36480 --strictPort` em `apps/client` (o mock grava as preferências em `localStorage`, chave `posta-mock-settings`, então o reload as mantém).
3. Com playwright-core (ex.: `/tmp/claude-1000/pw/node_modules/playwright-core`; se faltar o browser da versão, use `executablePath` de `~/.cache/ms-playwright/chromium_headless_shell-*/`), em 1280x800 confirme:
   - Arrastar `.resizer[data-edge="list"]` (mouse down, move, up) muda a largura de `.list-pane` e, após `page.reload()`, a largura é a mesma. Limites: lista 280 a 720 px sem deixar a leitura abaixo de 360 px; pastas 180 a 360 px. `dblclick` na borda volta a 360 / 232 px.
   - `dblclick` numa `.row` ou o botão `.expand-btn` põe `data-full` no `.app`: só `.reader` visível, ocupando 1280 px. Esc e o botão `.back` voltam.
   - Configurações (`.settings-btn`): `.settings-page` substitui a lista; seções Contas, Leitura e layout, Escrita, Notificações, Atalhos, Atualizações; Esc ou "Voltar" retornam.
   - Em Leitura e layout: Tema Escuro troca o `background-color` do `body`; Fonte G põe `data-size="l"` no `<html>`; Compacta + Ocultar prévia diminuem a altura de `.row`; Embaixo põe a leitura abaixo da lista (borda `.resizer[data-axis="y"]`); Desligado abre a mensagem em tela cheia ao clicar. Reload mantém `data-pane`, `data-density`, `data-preview`.
   - A 390x844 não há `.resizer` nem `.expand-btn`. Console sem erro nem aviso.
   - Nota: atributos `data-*` booleanos do React valem `"true"` (ex.: `dataset.full === "true"`).
4. Prints de referência (sobrescreva): `/home/renan/.t3/userdata/attachments/posta-config.png`, `posta-colunas.png` (durante o arrasto), `posta-tela-cheia.png`, `posta-escuro.png`. Encerre o vite ao terminar.

Não coberto no mock: gravação real via `set_setting` do Tauri e notificações do sistema.
