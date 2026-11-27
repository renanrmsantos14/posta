---
name: verificar-ui-produtividade
description: Verifica a UI de produtividade do RenanMail (regras, busca com filtros, adiar, enviar mais tarde/Agendados, fixar, respostas prontas, férias) em modo mock (navegador, sem Tauri). Use ao mudar App.tsx, Composer.tsx, WhenMenu, Rules.tsx, ProductivitySettings, productivity.ts, SettingsPage, mock.ts ou styles.css.
---

# Verificar UI de produtividade (mock)

1. `pnpm -C apps/client exec tsc --noEmit` e `pnpm --filter client build`.
2. Porta 36480: `ss -ltnp | grep 36480`; se ocupada, `readlink /proc/PID/cwd` e encerre só se for deste worktree. Suba `pnpm --filter client exec vite --port 36480 --strictPort` em background.
3. Com playwright-core (qualquer `node_modules/playwright-core` do disco; `chromium.launch({ executablePath })` com o `chrome-headless-shell` de `~/.cache/ms-playwright/` se o navegador da versão não estiver instalado), em 1280x800, confirme:
   - Configurações > Regras: "Nova regra" abre o painel. Escolha "Remetente contém" + texto, uma segunda condição "Anexo" e a ação; a linha "N mensagens da Entrada casam…" aparece ~0,3 s depois. "Salvar" cria o cartão com a descrição ("Remetente contém … → …"). Regra sem texto: "Salvar" desabilitado. Esc/clique fora/X: sem alteração fecha direto; com alteração mostra "Descartar alterações?" ("Continuar editando"/"Descartar") e o Esc não sai das Configurações. Duas regras conflitantes ou pasta inexistente mostram aviso no cartão. "Aplicar todas à Entrada" e "Salvar e aplicar às existentes" mostram o toast "N mensagens atualizadas pelas regras".
   - Configurações > Respostas prontas: "Adicionar modelo" (nome e texto). Configurações > Férias: liga/desliga e avisa texto vazio ou fim antes do início.
   - Busca: botão de filtros (funil) insere `é:não-lida` etc. na consulta; com filtro, a lista usa a busca filtrada; "Buscar no servidor" espera ~0,7 s e troca o aviso para "(com o servidor)".
   - Mensagem aberta: "Adiar" > "Amanhã" tira a linha e mostra toast "Adiada até ..." com Desfazer. O menu abre alinhado à esquerda, sem ficar sob a lista.
   - Tecla `p` fixa a linha selecionada no topo com a marca de alfinete; `p` de novo desafixa.
   - Novo email: botão "Modelos de resposta" insere o texto no editor; "Enviar mais tarde" > "Próxima semana" fecha o painel; "Agendados" na barra lista a mensagem com a hora e "Cancelar envio" (volta para Rascunhos). Destinatário com "erro" deixa o painel aberto com a mensagem de erro.
   - Clique direito numa mensagem: "Criar regra com este remetente" abre Configurações > Regras com uma regra desligada (Remetente = endereço da linha); "Bloquear remetente" mostra o toast com Desfazer, e a ação "Mover para a Lixeira" existe no seletor de ação.
   - Console sem erro nem aviso.
4. Prints: `posta-pr1-regras-lista-depois.png`, `posta-busca.png` e `posta-adiar.png` em `/home/renan/.t3/userdata/attachments/`. Encerre o vite ao terminar e remova o `node_modules` do worktree.
