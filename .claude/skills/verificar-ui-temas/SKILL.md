---
name: verificar-ui-temas
description: Verifica os temas do Posta (18 presets, Seguir o sistema, cor de destaque, densidade e fonte em Configurações > Aparência) em modo mock: contraste WCAG por preset, troca instantânea, persistência, ausência de flash ao abrir e axe em claro, escuro e alto contraste. Use ao mudar themes.css, themes.ts, public/theme-init.js, AppearanceSettings.tsx, SettingsPage.tsx, o efeito de tema do App.tsx ou qualquer cor em styles.css.
---

# Verificar UI de temas (mock)

Onde está: tokens de cor em `apps/client/src/themes.css` (`[data-theme="id"]`, valem em qualquer elemento, inclusive nas miniaturas); lógica em `themes.ts`; `public/theme-init.js` aplica o tema antes do primeiro paint a partir do espelho `localStorage["posta.appearance"]`; a escolha vai em `settings.theme` (`system` ou o id do preset) e o destaque próprio em `settings.accent` (`#rrggbb`). CSS não pode ter cor literal fora de `themes.css` (use `var(--...)`; texto sobre destaque usa `--on-blue`).

1. Contraste, sem navegador: `node .claude/skills/verificar-ui-temas/contraste.mjs`. AA (4,5:1) em texto, destaque, semânticas e botão de cada preset; `hc-*` exigem AAA (7:1). Tem de terminar com "0 falhas". Preset novo: some em `themes.css` (família escura precisa estar na lista de seletores da família) e em `THEMES` de `themes.ts`.
2. `pnpm -C apps/client exec tsc --noEmit` e `pnpm --filter client build`.
3. Porta 36481 (a 36480 pode ser de outro agente): `ss -ltnp | grep 36481`; suba `node_modules/.bin/vite --port 36481 --strictPort` em `apps/client`, em background.
4. `PLAYWRIGHT=<pasta playwright-core> AXE=<axe.min.js> EXE=<chrome-headless-shell de ~/.cache/ms-playwright> node .claude/skills/verificar-ui-temas/verificar.mjs`. Confere: grade com >= 17 cartões; clique troca `data-theme` e o fundo; recarregar mantém o tema e o `data-theme` já existe quando o `<body>` aparece; destaque (bolinha) aplica `--blue` e "Do tema" remove; "Seguir o sistema" acompanha `prefers-color-scheme`; axe sem Critical/Serious no app e em Aparência com `light`, `dark`, `hc-light`, `hc-dark`, `nord` e `paper`; console sem erro nem aviso.
5. Prints gerados em `/home/renan/.t3/userdata/attachments/`: `posta-temas-aparencia.png` e `posta-temas-app-{padrao,nord,papel,alto-contraste}.png`. Olhe os PNG: miniaturas com lista, item selecionado e leitor coerentes com o tema, cartão ativo com marca.
6. Encerre o vite ao terminar e remova o `node_modules` do worktree se foi instalado só para isto.

Armadilhas: toast saindo faz o axe medir cor em transição (o script espera o `.toast` sumir); `[data-theme-swap]` anima 200 ms ao trocar, então espere ~450 ms antes de medir.

Pendência conhecida (axe 4.14): `label-content-name-mismatch` nos `.contact-chip` do leitor (`title` = endereço, texto = nome) aparece em "axe no app" e já existe no master; não é regressão de tema.
