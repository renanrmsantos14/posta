---
name: verificar-conectar-conta
description: Verifica o fluxo "conectar só com email e senha" do Posta (autodetecção de servidores, teste de conexão, AccountDialog). Use ao mudar crates/mail-core/src/autoconfig.rs, AccountDialog.tsx, mock.ts ou os comandos detect_servers/test_connection/add_imap_account.
---

# Verificar conectar conta

1. `cargo test -q -p mail-core autoconfig`: tabela de provedores, parse do XML do ISPDB (fixture), MX (menor preferência e mapeamento Google/Microsoft 365/hospedagens BR) e erros traduzidos.
2. Rede real (opcional, precisa de internet): `cargo test -p mail-core -- --ignored --nocapture detecta_na_rede`. Esperado: gmail.com/outlook.com/uol.com.br = `[tabela]`, web.de = `[ispdb]` (login só a parte local), betinhos.com.br = `[mx]` Locaweb, ig.com.br = `[palpite]`.
3. `pnpm --filter client build` (tipos de `api.ts`, `mock.ts` e `AccountDialog.tsx`).
4. UI no mock: `cd apps/client && npx vite --port <porta livre> --strictPort` (confira `ss -ltnp`: outras worktrees usam 36480/36481). Com playwright-core e `/usr/bin/brave-browser` (`--no-sandbox`): abrir "Adicionar conta", digitar `renan@gmail.com` + senha, tirar o foco do email; deve aparecer "Detectado: Gmail" com a dica e o link "Abrir página oficial"; "Configurações avançadas" (recolhidas) mostram imap.gmail.com / smtp.gmail.com; Conectar chega em "Conectada". Print em `/home/renan/.t3/userdata/attachments/`.
5. Se mexeu em `apps/client/src-tauri/src/lib.rs`: cargo check do crate `posta` em Docker, conforme `verificar-mail-core` (volume `posta-target`, em background).
6. Login real (senha de app do Gmail etc.) não é automatizável aqui; `test_connection` só é provado contra servidor real ou GreenMail (`pnpm test:imap`).
