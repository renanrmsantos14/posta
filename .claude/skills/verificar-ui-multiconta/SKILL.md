---
name: verificar-ui-multiconta
description: Verifica a navegação entre várias contas do Posta (Rail em árvore por conta no estilo Outlook, "Todas as contas", contadores por conta, gaveta do celular, apelido e cor) em modo mock. Use ao mudar Rail.tsx, NavDrawer.tsx, accountMeta.ts, FolderTree.tsx, ListParts.tsx (ListHead/scope), MessageList.tsx (listra da conta), TopBar.tsx, a contagem por conta em App.tsx, `unread_counts`/`unread_by_account` ou o CSS `.acct-*`/`.drawer-*`.
---

# Verificar a navegação multiconta (mock)

Spec: `docs/superpowers/specs/2026-11-10-navegacao-multiconta-design.md`.

1. `pnpm -C apps/client exec tsc --noEmit -p apps/client` e `cargo test -p mail-core -q` (teste `nao_lidas_por_conta_somam_o_total_por_papel`). O comando `unread_counts` em `src-tauri/src/lib.rs` só compila no Docker (`verificar-mail-core`, passo 3).
2. Porta 36472: confira com `ss -ltnp | grep 36472` (encerre só se o cwd do PID for deste worktree) e suba `npx vite --port 36472 --strictPort` em `apps/client`.
3. Rode o script (sai com código 1 se algo falhar):
   `node .claude/skills/verificar-ui-multiconta/check.cjs http://localhost:36472 <pasta-dos-prints> <caminho do playwright-core>`
   Ele confere, no mock:
   - `?contas=5`: uma seção por conta, sem faixa de avatares, contador de cada seção = não lidas da Entrada da conta, "Todas as contas › Entrada" = soma, várias seções abertas ao mesmo tempo (padrão), cabeçalho da seção com o endereço e da lista com o apelido da conta.
   - `?contas=1`: sem faixa, sem seções e sem "Todas as contas" (Rail igual à anterior).
   - 390×844: ≡ "Pastas e contas" abre a gaveta; escolher pasta troca a conta e fecha; toque fora, X e Voltar fecham.
   - Console sem erro nem aviso.
4. Olhe os prints `multiconta-desktop.png`, `multiconta-gaveta.png` e `multiconta-uma-conta.png`: cada `.acct-head` mostra o endereço da conta, os apelidos são curtos (parte local, ou o provedor quando a parte local se repete) e a listra de 3 px da conta aparece inteira em cada linha de "Todas as contas". No celular (412x915) a conta vira um ponto de 7 px na cor dela logo depois do nome (`.from::after`), sem listra e sem etiqueta `.acct`.
5. Apelido e cor: Configurações › Contas › Editar muda apelido e cor sem reconectar (só nome/apelido/cor fecha direto). A cor também vale no assistente (Personalizar, `ColorField`).
6. Movimento: gaveta com translateX 220 ms `cubic-bezier(0.32,0.72,0,1)` e saída 180 ms; acordeão com `grid-template-rows` 200 ms; tudo desligado com `prefers-reduced-motion` (emule com `p.emulateMedia({ reducedMotion: 'reduce' })`).
6b. Barra lateral (estilo Outlook): sem faixa de avatares; cada `.acct-head` mostra o e-mail da conta; Configurações › Aparência › Barra lateral: "Contas abertas" `railAccounts` = "Várias abertas" (padrão; `rail:accounts` guarda as abertas) ou "Uma por vez" (sanfona: abrir uma fecha as outras e escolher pasta de outra conta a substitui). Confira os dois modos no `.pv` da tela e na Rail real.
7. Não validável no mock: contagem real por conta no app instalado (IMAP) e o Voltar físico do Android (`verificar-android`).
