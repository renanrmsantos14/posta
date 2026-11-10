---
name: verificar-ui-conta
description: Verifica o assistente Adicionar conta do Posta (email, login por provedor, teste com progresso, personalizar) em modo mock e o núcleo em Rust. Use ao mudar AccountDialog.tsx, AccountSteps.tsx, accountSetup.ts, accountColors.ts, api.ts (detectProvider/checkAccount), mock.ts, setup.rs ou o CSS do assistente.
---

# Verificar o assistente de conta

1. Rust: `cargo test -p mail-core` (inclui `setup::tests`: parse do ISPDB, classificação de MX e tradução de erros, e `tests/account_check.rs`: senha recusada, porta fechada e host inexistente contra um IMAP falso). `src-tauri` não compila no host: `cargo check -p posta` no Docker (`rust:1-bookworm`, volume `posta-target`, libwebkit2gtk-4.1-dev etc.) e depois `git checkout -- apps/client/src-tauri/gen/schemas`. O GreenMail (`pnpm test:imap`) roda com autenticação desligada, então não prova senha errada.
2. `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`.
3. `npx vite --port 36486 --strictPort` em `apps/client` (setsid; encerre pelo PID da porta, nunca `pkill -f`). Playwright-core em 1280x820 (abaixo de ~900 px a barra lateral vira ícones). Abra "Adicionar conta", espere ~350 ms e confirme, no mock:
   - `x@gmail.com`: guia de senha de app (3 passos, link, "16 letras"); senha "errada" → progresso com ✗ em "Conferindo email e senha" e "O Google recusou a senha", erro cru só em "Detalhes"; "Corrigir" mantém email e senha; senha certa (aceita espaços) → 3 ✓ → Personalizar (cor pelo `ColorField`, o mesmo do Editar conta) → Concluir → conta na lista, cor na sidebar e Caixa de Entrada da conta nova aberta.
   - `x@outlook.com` e `fulano@empresa.com.br` (MX Microsoft no mock): "Entrar com Microsoft" como botão principal; "Usar senha em vez disso" mostra a senha. Senha "errada" no hotmail sugere "Entrar com Microsoft".
   - `a@exemplo.org`: Configuração manual preenchida pelo autoconfig (mail.exemplo.org:143, smtp.exemplo.org:587). `a@desconhecido.com`: `imap.`/`smtp.<domínio>`. `a@offline.com` (descoberta travada): cai no manual em ~6 s.
   - Falhas distintas: host com "invalido" (DNS), "lento" (tempo esgotado), "tls" (certificado), porta IMAP 1234 (recusada), senha "imap" (IMAP desativado).
   - Fechar: Esc/clique fora com dados → "Descartar alterações?" ("Continuar editando" mantém); durante o teste e a espera do navegador não fecha. Conta já cadastrada (`renan@exemplo.com`) → aviso, não avança.
   - Console sem erro.
4. Prints em `/home/renan/.t3/userdata/attachments/posta-assistente-<etapa>.png` (email, gmail-guia, microsoft, progresso, erro, personalizar). Encerre o vite.
5. Não validável sem rede/credenciais: login real Gmail/Microsoft, autoconfig e MX contra domínios reais no app instalado (o mock não chama a rede), OAuth real (`verificar-oauth`). Diga isso no PR.
