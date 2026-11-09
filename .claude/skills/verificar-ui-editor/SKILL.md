---
name: verificar-ui-editor
description: Verifica o editor formatado do Composer, os chips de destinatários com autocompletar e o envio multipart/alternative do Posta (mock no navegador + testes do mail-core). Use ao mudar Composer.tsx, Recipients.tsx, richtext.ts, App.tsx (resposta/encaminhar/assinatura), api.ts, mock.ts, styles.css, send.rs, store.rs (contatos, rascunhos) ou o comando suggest_contacts em src-tauri/src/lib.rs.
---

# Verificar editor formatado e destinatários

1. `cargo test -q -p mail-core` (testes `html_vira_multipart_alternative_com_texto_e_html`, `contatos_sugeridos_por_frequencia_e_recencia`, `rascunho_*`) e `pnpm -C apps/client build`.
2. Com Docker: `pnpm test:imap` (o teste de anexos envia HTML + texto pelo GreenMail e confere `body_html`/`body_text`). Pare o container `posta-greenmail` depois.
3. `npx vite --port <livre, diferente de 36480> --strictPort` em `apps/client` e playwright-core (`/home/renan/.codex/worktrees/3b95/Auto-Prime-Interno/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core`), 1280x800, "c" abre o Composer:
   - Para: digitar "mari" abre `.suggest-list` (role listbox); setas movem, Enter e Tab confirmam e criam `.rcpt`; `zeca@x.com,` cria chip; Backspace com campo vazio remove o último; X do chip remove; Esc com lista aberta fecha só a lista (o Composer continua).
   - Editor (`role=textbox` "Mensagem", contentEditable): Ctrl+B/I viram `<b>`/`<i>`; botão de lista gera `<ul><li>`; Ctrl+K abre `.link-bar`, Enter aplica `<a href="https://...">`; Citação e Limpar formatação alternam `<blockquote>`.
   - Colar HTML hostil (`<script>`, `onclick`, `<img onerror>`, `style`, `javascript:`) via `ClipboardEvent` com `DataTransfer`: o `innerHTML` sai sem nada disso.
   - Esperar ~1,9 s mostra "Rascunho salvo"; Esc, Rascunhos, abrir o rascunho: texto continua formatado e os chips voltam.
   - Responder (`r`) abre com assinatura e a original dentro de `<blockquote>`.
   - Console sem erro nem aviso (403 de fonte fora do `fs.allow` do vite não conta).
4. Print: `/home/renan/.t3/userdata/attachments/posta-editor.png` (chip + lista de sugestões + texto formatado). Encerre o vite pelo PID de `ss -ltnp`.
5. Não validável no mock: o comando Tauri `suggest_contacts` rodando no app real e o envio por SMTP de verdade (só GreenMail). `src-tauri` não compila no host sem libdbus; use `cargo check` em Docker (rust:1-bookworm + libwebkit2gtk-4.1-dev, libdbus-1-dev, libgtk-3-dev, libssl-dev).
