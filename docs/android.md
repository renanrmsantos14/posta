# Android: sincronização com o app fechado

## Como funciona

1. `MainActivity` agenda, uma vez, um `PeriodicWorkRequest` do WorkManager (`SyncWorker`, id único `posta-sync`, política `KEEP`) a cada 15 minutos e com rede (`NetworkType.CONNECTED`).
2. `SyncWorker.doWork` carrega `libposta_lib.so` e chama `nativeSync(context, dir)` (JNI), com `dir` = `dataDir/br.com.renan.posta`, a mesma pasta que o Tauri usa.
3. `src-tauri/src/android_sync.rs` abre o SQLite, monta um `AppState` próprio, reaplica a fila offline e sincroniza a Caixa de entrada de cada conta (mesmo caminho do app aberto).
4. Se chegou mensagem nova (e a opção de notificações não está desligada), devolve o total e até 3 remetentes/assuntos; o Worker mostra a notificação do canal "Novas mensagens". Tocar abre o app.
5. Com o app visível (`appVisible`), o Worker não faz nada: o IDLE já mantém tudo em dia.

A permissão `POST_NOTIFICATIONS` (Android 13+) é pedida pelo app ao ativar avisos; sem ela, ou com as notificações bloqueadas, o Worker sincroniza mas não mostra nada.

## Limites

- **Não validado em aparelho.** O CI só compila o APK. O ponto de risco é o `ndk-context`: o Tauri o registra ao abrir a Activity; num processo criado só para o Worker, `android_sync.rs` registra o seu e o solta no fim. O cofre de senhas (`android-native-keyring-store`) depende desse contexto.
- Intervalo mínimo do WorkManager: 15 minutos. O Android adia a execução em Doze, economia de bateria e restrição de segundo plano do fabricante; não há garantia de horário.
- Não há conexão IDLE com o app fechado: o aviso chega com atraso de até ~15 minutos.
- "Baixar corpo das últimas N" (`offlineN`, padrão 200) limita o que a Caixa de entrada baixa; mensagens locais mais antigas continuam. Anexos são baixados sob demanda e não abrem sem rede.
- Se a biblioteca ou o contexto falharem, o Worker registra em `PostaSync` (logcat) e tenta de novo (`Result.retry`).

## Para validar

Instale o APK assinado, abra o app, adicione uma conta, feche-o e envie um e-mail para a conta. Aguarde ~15 minutos ou force: `adb shell cmd jobscheduler run -f br.com.renan.posta <jobId>` (`adb shell dumpsys jobscheduler | grep posta`). Logs: `adb logcat -s PostaSync`.
