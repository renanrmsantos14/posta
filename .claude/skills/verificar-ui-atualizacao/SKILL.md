---
name: verificar-ui-atualizacao
description: Verifica em modo mock o modal "Nova versão do Posta" (novidades agrupadas por versão, tamanho, fechar, progresso) e o agrupamento real dos commits por versão no GitHub. Use ao mudar UpdateDialog.tsx, updater.ts, os estilos .dialog.update/.update-* em styles.css ou o commit "Lança vX" do release.yml.
---

# Verificar o modal de atualização

1. `pnpm --filter client exec tsc --noEmit`.
2. Porta 36480 (mesma regra de verificar-ui-layout). `npx vite --port 36480 --strictPort` em `apps/client` e abra `/?update` (mock em `mockCheck`: duas versões com novidades) e `/?update=android` (sem novidades: a lista não aparece).
3. Com playwright-core (`/tmp/claude-1000/pw/node_modules/playwright-core`):
   - 1440x900: `.dialog.update` com 640 px de largura; um `h3` por versão (`v0.1.1`, `v0.1.0`), mais recente primeiro, cada uma com sua `ul`.
   - 390x844: `.dialog.update` cabe na tela e `document.documentElement.scrollWidth <= innerWidth`; `.update-notes` rola por dentro quando passa de `min(60dvh, 520px)`.
   - "Atualizar agora" mostra a barra e "Reiniciando…"; durante o download X/Esc/clique fora não fecham. "Depois", X, Esc e clique fora fecham.
   - Console sem erro nem aviso.
4. Dados reais (não passa pelo mock): `curl -s https://api.github.com/repos/renanrmsantos14/posta/compare/vA...vB` entre duas tags. `releasesUpTo` ignora commits com 2+ pais (merge de PR) e usa cada "Lança vX" como fim do grupo da versão X; confira que os assuntos listados batem com os grupos esperados. Sem rede ou 404 (versão de dev sem tag): a lista some e o botão de atualizar continua.
5. Print de referência (sobrescreva): `/home/renan/.t3/userdata/attachments/posta-update-modal.png`. Encerre o vite.

Não coberto: download/instalação real do Tauri e o APK no Android.
