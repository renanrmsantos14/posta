---
name: verificar-conforto
description: Verifica bandeja do sistema, gestos de deslizar, imprimir/salvar .eml e aliases de remetente do Posta. Use ao mudar src-tauri/src/tray.rs, autostart em lib.rs, apps/client/src/{swipe,aliases}.ts, o bloco @media print do styles.css, save_eml, Outgoing.from em mail-core/send.rs ou a seção Sistema/aliases da SettingsPage.
---

# Verificar conforto (bandeja, gestos, imprimir, aliases)

1. `cargo test -q -p mail-core`: `send::tests::alias_vira_o_remetente_e_so_vale_se_cadastrado` (alias vira From; só vale se cadastrado em `aliases:<conta>`). Se mexeu no IMAP/envio: `pnpm test:imap`.
2. `pnpm -C apps/client build` (tsc + vite).
3. Crate `posta` (bandeja, autostart, `save_eml`): `cargo check -p posta` no Docker `rust:1-bookworm` com `libwebkit2gtk-4.1-dev libdbus-1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev libssl-dev pkg-config`; depois `git checkout -- apps/client/src-tauri/gen/schemas`. Tudo da bandeja/autostart fica em `#[cfg(desktop)]` (Android não compila isso).
4. UI em mock: `npx vite --port 36491 --strictPort` em `apps/client` e playwright-core (caminho em `verificar-ui-pastas`):
   - Gesto: contexto `hasTouch: true, isMobile: true` (400x800); confira `matchMedia("(pointer: coarse)")`; use CDP `Input.dispatchTouchEvent` (touchStart, vários touchMove, touchEnd). Direita >= 88 px arquiva (toast "Arquivada" + Desfazer), esquerda exclui ("Movida para a Lixeira"); Desfazer devolve a linha. Com mouse não há gesto. Sequência completa: arquiva a linha 1 e em seguida exclui a linha 2 (64 → 63 → 62); o deslizar não abre a próxima mensagem (se abrisse, o leitor cobriria a lista no celular e o gesto seguinte cairia nele, com `pointercancel`). `pnpm test:imap` inclui `envia_com_alias_como_remetente`.
   - Imprimir: botão "Imprimir ou salvar como PDF" chama `window.print` (substitua por contador); com `page.emulateMedia({ media: "print" })` rail, lista e barra de ações ficam `display: none` e só o leitor aparece.
   - Alias: abrir "Marina Duarte" e responder (`r`): o campo "De" já vem com `Betinhos Executive <contato@exemplo.com>` (a mensagem foi enviada para esse alias); o select lista conta + aliases.
5. Prints: `/home/renan/.t3/userdata/attachments/posta-gestos.png` e `posta-aliases.png`.

Limites (exigem desktop/aparelho reais, não validados no CI): ícone/menu/contador da bandeja, minimizar ao fechar, "Iniciar com o sistema", foco da janela ao clicar na notificação (só em notificações exibidas depois do registro do handler; no Windows só enquanto o aviso está na tela), badge (depende do SO), .eml real via IMAP (não há cache do bruto), gesto num aparelho Android. Rascunho salvo não guarda o alias escolhido.
