---
name: verificar-ui-imagens
description: Verifica o bloqueio de imagens remotas no leitor do Posta e a liberação por remetente ("Sempre mostrar de …", setting `images:trusted`, lista em Configurações › Leitura e layout) em modo mock. Use ao mudar a faixa `.img-banner` em ReaderPane.tsx, HtmlBody.tsx (`hasRemoteImages`/CSP), `imagesOk`/`imagesFrom`/`trustSender` em App.tsx ou o campo em SettingsPage.tsx.
---

# Verificar imagens remotas (mock)

1. `npx tsc --noEmit -p apps/client`.
2. Porta 36472: confira com `ss -ltnp | grep 36472` (encerre só se o cwd do PID for deste worktree) e suba `npx vite --port 36472 --strictPort` em `apps/client`.
3. Rode (sai com código 1 se algo falhar; `CHROME=<binário>` se o playwright-core não tiver navegador próprio):
   `node .claude/skills/verificar-ui-imagens/check.cjs http://localhost:36472 <pasta-dos-prints> <caminho do playwright-core>`
   No email "Últimos dias: casacos com 40%" (Loja Norte, imagem remota do placehold.co), ele confere:
   - a faixa mostra "Mostrar imagens" e "Sempre mostrar de ofertas@lojanorte.com.br";
   - liberar o remetente tira a faixa; o toast "Desfazer" volta a bloquear;
   - Configurações › Leitura e layout lista o endereço (um por linha, editável);
   - console sem erros, exceto a recusa da CSP (é o bloqueio funcionando) e 403 do placehold.co.
4. Olhe `imagens-faixa.png` (botões lado a lado, quebram linha em tela estreita) e `imagens-config.png`.
5. Regras: "Mostrar imagens" vale só para aquela mensagem e só nesta sessão; a liberação por remetente compara o endereço em minúsculas (`addrOf`) e persiste no setting. Remetente desconhecido continua bloqueado.
6. Não validável no mock: persistência após reiniciar o app instalado (settings no SQLite).
