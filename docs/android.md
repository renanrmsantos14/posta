# Android: sincronização com o app fechado

## Como funciona

1. `MainActivity` agenda, uma vez, um `PeriodicWorkRequest` do WorkManager (`SyncWorker`, id único `posta-sync`, política `KEEP`) a cada 15 minutos e com rede (`NetworkType.CONNECTED`).
2. `SyncWorker.doWork` carrega `libposta_lib.so` e chama `nativeInit(context)` e `nativeSync(dir)` (JNI), com `dir` = `dataDir/br.com.renan.posta`, a mesma pasta que o Tauri usa.
3. `src-tauri/src/android_sync.rs` abre o SQLite, monta um `AppState` próprio, reaplica a fila offline e sincroniza a Caixa de entrada de cada conta (mesmo caminho do app aberto).
4. Se chegou mensagem nova (e a opção de notificações não está desligada), devolve o total e até 3 remetentes/assuntos; o Worker mostra a notificação do canal "Novas mensagens". Tocar abre o app já no email (um só) ou na Caixa de entrada da conta (vários): o Intent leva `conta`, `pasta` e `id` local; `MainActivity` guarda o destino e a UI o busca por `window.RenanMailNative.takePending()` ao carregar (app fechado) ou ao receber o evento `renanmail-open` (`onNewIntent`, app em segundo plano).
5. Com o app visível (`appVisible`), o Worker não faz nada: o IDLE já mantém tudo em dia.

A permissão `POST_NOTIFICATIONS` (Android 13+) é pedida pelo app ao ativar avisos; sem ela, ou com as notificações bloqueadas, o Worker sincroniza mas não mostra nada.

## Limites

- **Não validado em aparelho.** O CI só compila o APK. O ponto de risco é o `ndk-context` (global, só uma inicialização por processo; registrar duas vezes dá `assert` e fecha o app): o Tauri o registra no `onCreate` da Activity; num processo criado só para o Worker, `nativeInit` registra o seu, cria o cofre (que guarda JavaVM e contexto) e o solta na hora. Worker e `onCreate` usam o mesmo lock (`SyncWorker.CONTEXT_LOCK`) para nunca registrar ao mesmo tempo. O cofre de senhas (`android-native-keyring-store`) depende desse contexto.
- Intervalo mínimo do WorkManager: 15 minutos. O Android adia a execução em Doze, economia de bateria e restrição de segundo plano do fabricante; não há garantia de horário.
- Não há conexão IDLE com o app fechado: o aviso chega com atraso de até ~15 minutos.
- "Baixar corpo das últimas N" (`offlineN`, padrão 200) limita o que a Caixa de entrada baixa; mensagens locais mais antigas continuam. Anexos são baixados sob demanda e não abrem sem rede.
- Se a biblioteca ou o contexto falharem, o Worker registra em `RenanMailSync` (logcat) e tenta de novo (`Result.retry`).

## Para validar

Instale o APK assinado, abra o app, adicione uma conta, feche-o e envie um e-mail para a conta. Aguarde ~15 minutos ou force: `adb shell cmd jobscheduler run -f br.com.renan.posta <jobId>` (`adb shell dumpsys jobscheduler | grep posta`). Logs: `adb logcat -s RenanMailSync`.

## Tela branca ao abrir

A UI fica invisível até as configurações chegarem do núcleo (`.app[data-ready]`). Se isso não acontece em 8 s, `public/boot-guard.js` mostra "O RenanMail está demorando para abrir" com o botão Recarregar. Para saber onde a abertura parou, com o cabo ligado logo depois de reproduzir: `adb logcat -d -s RenanMail RenanMailUI RenanMailSync`. `RenanMail` vem do Rust (`src/diag.rs`: etapas do `setup`, comandos lentos ou presos, laço de eventos sem resposta); `RenanMailUI` traz o ciclo da Activity e as etapas da página (`html carregado`, `bundle iniciou`, `pronta`).

## Travamentos: comandos e ponte JS

No Android o Tauri leva cada chamada da UI ao núcleo por `window.ipc.postMessage`, uma `@JavascriptInterface` do wry. Essa chamada é síncrona para a página: até o Rust devolver, o JavaScript não roda, não desenha e não responde ao toque. Um comando síncrono (`fn`) roda inteiro ali dentro; um `async fn` só é despachado e responde depois. Por isso todo comando que lê o banco, a rede ou o cofre é `async` e usa `blocking(...)` (`spawn_blocking`). Comando síncrono, só para coisa instantânea (`set_focused`, `take_pending_open`). Antes, a abertura disparava uma dúzia de leituras síncronas na mesma trava do banco, e uma sync em andamento bastava para a tela ficar parada em "carregando".

Voltar: sem histórico na WebView, o Voltar manda o app para o fundo (`moveTaskToBack`) em vez de encerrar a Activity. Reabrir depois de encerrada recriava Activity e WebView num processo em que o núcleo já rodava, o caminho mais comum da tela branca.

SQLite: no Android cada conexão usa 16 MB de cache e 64 MB de mmap (no desktop, 64 MB e 256 MB). O app abre várias conexões ao mesmo tempo (UI, sync, IDLE por conta, pré-busca, fila).
