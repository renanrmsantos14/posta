---
name: verificar-offline
description: Verifica o modo offline do Posta (fila de ações, Caixa de saída, selo Offline, baixar corpo das últimas N) e o sync em segundo plano do Android. Use ao mudar crates/mail-core/src/outbox.rs, src-tauri/src/lib.rs (run_op, flush_queue), android_sync.rs, SyncWorker.kt, src/Offline.tsx ou a configuração offlineN.
---

# Verificar offline

1. Núcleo: `cargo test -q -p mail-core` (testes de `outbox.rs`: ordem, agrupamento, rede cai no meio, mensagem sumida descartada, recusa do servidor).
2. Servidor real: `pnpm test:imap` (GreenMail em Docker). O teste `fila_offline_reaplica_quando_a_rede_volta` enfileira ações e envio sem rede e confere que saem em ordem quando a rede volta. Variáveis manuais: `POSTA_IMAP_TEST=127.0.0.1:3143 POSTA_SMTP_TEST=3025`.
3. Cliente: `pnpm -C apps/client exec tsc --noEmit` e `pnpm -C apps/client build`. Crate `posta`: cargo check no Docker como em `verificar-android`.
4. Interface no mock (`pnpm -C apps/client dev --port <porta> --strictPort`, sem Tauri): `window.__mockOffline(true)` mostra o selo "Offline"; marque como lida/estrela, escreva e envie (aguarde os ~6 s do desfazer): a mensagem vai para a "Caixa de saída" ("Aguardando conexão"); `window.__mockOffline(false)` esvazia a fila e o selo some. Console sem erros. Capture com playwright-core e confira a barra (o selo não pode empurrar o botão Sincronizar para fora).
5. Android: o CI só compila (`verificar-android`). Worker, JNI e notificação exigem aparelho: instale o APK, feche o app, mande um e-mail e espere ~15 min (ou `adb shell cmd jobscheduler run -f br.com.renan.posta <id>`). Registre como não validado em dispositivo se não houve.

Limites: o mock não prova SMTP/IMAP reais; GreenMail não prova OAuth nem rede móvel instável.
