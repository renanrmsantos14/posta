---
name: verificar-anexos
description: Verifica anexos e Cc/Bcc do Posta (parsing, multipart, IMAP real e UI em modo mock). Use ao mudar crates/mail-core/src/{attachments,send,imap_sync,store}.rs, o comando save_attachment, Composer.tsx, format.ts ou o bloco de anexos do App.tsx/mock.ts.
---

# Verificar anexos e Cc/Bcc

1. `cargo test -q -p mail-core`: `attachments::tests` (lista sem imagem `cid:`, extração por índice, `cid:` vira `data:`, nome seguro) e `send::tests` (Cc/Bcc no envelope, Bcc fora do cabeçalho, multipart/mixed, limite de 25 MB, base64 inválido).
2. `pnpm test:imap` (Docker): `envia_com_anexo_e_cc_e_le_de_volta` envia com anexo + Cc por SMTP e lê de volta (destinatário e copiado), conferindo byte a byte via `fetch_raw` + `extract`. Use um arquivo de log próprio (`> /tmp/.../imap-anexos.log`): outros agentes escrevem em `/tmp/claude-1000/imap.log`.
3. Mexeu em `apps/client/src-tauri` (comando `save_attachment`): cargo check no Docker, como em `verificar-mail-core`.
4. UI: `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`; suba `npx vite --port 36480 --strictPort` em `apps/client` (encerre pelo PID, conferindo `readlink /proc/PID/cwd`; `pkill -f` mata a própria shell). Com playwright-core (caminho em `verificar-ui-pastas`), 1280x800 e 390x844:
   - Mensagem "Planta e orçamento da sala de reunião": 3 chips de anexo; imagem inline aparece no corpo; clicar num chip mostra toast "Salvo em ...".
   - Responder: botão "Cc/Bcc" abre Cc e Bcc; `input[type=file]` aceita arquivos e mostra chips removíveis com tamanho e "x MB de 25 MB"; arquivo de 30 MB é recusado com aviso; o X remove o chip.
   - Console sem erro nem aviso.
5. Print: `/home/renan/.t3/userdata/attachments/posta-anexos.png` (leitor em cima, composer embaixo).
6. Limites conhecidos: salvar/abrir anexo exige rede (o conteúdo é buscado no IMAP sob demanda); no Android o arquivo vai para a pasta Downloads do app e não é aberto automaticamente.
