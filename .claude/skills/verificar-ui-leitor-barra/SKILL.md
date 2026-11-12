---
name: verificar-ui-leitor-barra
description: Verifica em modo mock o ajuste à largura e o zoom do corpo HTML no leitor do Posta (botões − / % / +, Ctrl+roda, Ctrl+0, persistência em `posta.readerZoom`) e as preferências da barra de ferramentas em Configurações › Aparência (tamanho, rótulos, botões visíveis). Use ao mudar HtmlBody.tsx (fit/zoom), ReaderPane.tsx (ZoomControls), CommandBar.tsx, TOOLBAR_ITEMS/hiddenTools em commands.ts, AppearanceSettings.tsx ou o CSS de `.cmdbar`/`.zoom` em styles.css.
---

# Verificar leitor (ajuste e zoom) e barra de ferramentas (mock)

Onde está: `fit()` em `HtmlBody.tsx` mede a largura natural do e-mail e aplica `zoom` no `<html>` do iframe (ajuste × zoom do usuário); o zoom do usuário fica em `localStorage["posta.readerZoom"]`. A barra lê `settings.toolbar` (s/m/l), `settings.toolbarLabels` (auto/always/icons) e `settings.toolbarHidden` (ids em CSV, lista em `TOOLBAR_ITEMS`). O e-mail de teste é "Boletim Rota" (`htmlSamples` em `mock.ts`, tabela fixa de 680 px, aba Outros).

1. `pnpm -C apps/client exec tsc -b` e `pnpm --filter client build`.
2. Porta 36480: `ss -ltnp | grep 36480`; se ocupada, `readlink /proc/PID/cwd` e encerre só se for deste worktree. Suba `node_modules/.bin/vite --port 36480 --strictPort` em `apps/client`, em background.
3. `PLAYWRIGHT=<pasta playwright-core> EXE=<chrome de ~/.cache/ms-playwright> OUT=<pasta de evidências> node .claude/skills/verificar-ui-leitor-barra/verificar.mjs`. Tem de terminar com "tudo ok". Confere: painel largo sem zoom; a 1100 px a tabela encolhe (zoom < 1) sem rolagem lateral; + + dá 125% = ajuste × 1,25 com rolagem lateral; zoom persiste no reload; Ctrl+0 e Ctrl+roda dentro do iframe; barra Média 32 px, Grande 40 px; Só ícones esconde rótulos (menos o primário); botão desmarcado some; preferências persistem; "Sempre" a 900 px rola a faixa sem rolar a página; console limpo.
4. Olhe os PNG: `posta-leitor-ajuste.png` (e-mail inteiro visível no painel estreito), `posta-barra-config.png` (seletores e checkboxes) e `posta-barra-grande.png`.
5. Limiares dos rótulos/botões da barra (container queries em `styles.css`): se mexer, meça a borda direita do último `.cmd` visível contra `.cmdbar.clientWidth` em 1920/1800/1380/1240/1180/900 nos tamanhos m e l; nenhum pode passar da largura.
5b. Cancelar inscrição (`UnsubscribeBar` em `ReaderPane.tsx`, mock `unsubscribeInfo`): no navegador, aba Outros > "Últimos dias: casacos com 40%" (Loja Norte) mostra a faixa "Cancelar inscrição"; o clique dá o toast "Inscrição cancelada". Mensagens sem cabeçalho não mostram a faixa.
6. Aviso de remetente suspeito e "Relatar phishing": `pnpm --filter client test` (detecção em `phishing.ts`: Reply-To de outro domínio, endereço no nome exibido; ignora provedor gratuito, remetente automático e "via"; subdomínio do mesmo domínio não alerta). No mock, abra "Atualize seu cadastro hoje" (Banco Aurora, Reply-To `aurora-seguro.top`): a faixa amarela aparece acima do corpo; "Relatar phishing" (faixa, botão da barra ou menu de contexto) pede confirmação e move para o Spam com "Desfazer". Obs.: pela barra o alvo é a linha inteira (conversa), como Arquivar.
6b. Abertura lenta (`opening` em `ReaderPane.tsx`, prazo `OPEN_TIMEOUT_MS` em `App.tsx`): com mensagem selecionada e sem detalhe, o leitor mostra "Abrindo…" só depois de 300 ms (nunca "Selecione uma mensagem"); passando de 25 s vira "Não foi possível abrir a mensagem / Demorou demais para abrir este email." com "Tentar de novo" (chama `select(id)` de novo). Para ver no mock, atrase temporariamente `api.open` em `mock.ts` (e reduza o prazo) e reverta depois; resultado de outra seleção continua ignorado (`selectedRef`).
7. Encerre o vite ao terminar.

Armadilhas: o `wheel` do iframe não é `instanceof WheelEvent` do documento pai (use `"deltaY" in e`); `e.timeStamp` do iframe tem outra origem de tempo (use `performance.now()`); a roda com `deltaY` 0 não muda o zoom.
