---
name: verificar-android
description: Verifica o build Android do Posta (APK pelo GitHub Actions, senha no Keystore, sync ao retomar). Use ao mudar .github/workflows/android.yml, apps/client/src-tauri/gen/android, src-tauri/src/secret.rs, Cargo.toml do src-tauri ou updater.ts.
---

# Verificar Android

Sem Android SDK local: a prova do build é o CI.

1. Desktop não quebrou: `cargo test -q -p mail-core` e `pnpm -C apps/client build`. Para o crate `posta`, cargo check no Docker (só `/home` é compartilhado com o Docker; sem espaço em `/home`, use `-v postatarget:/src/target/docker` no lugar do target local), depois `git checkout -- apps/client/src-tauri/gen/schemas` e `docker volume rm postatarget`.
2. Workflow: `gh run list --workflow android.yml -b <branch>`; se não disparou, `gh workflow run android.yml --ref <branch>`. Acompanhe com `gh run watch <id>`; em falha, `gh run view <id> --log-failed | tail -60`.
3. Passou quando: job `android` verde, artefato `posta-android-apk` com `Posta_<versão>_android.apk` e o passo "Alinha e assina" imprime o certificado (`apksigner verify`).
4. `gen/android` regenerado por `tauri android init --ci` exige JDK, `ANDROID_HOME`, `NDK_HOME` e `rustup target add`; sem eles localmente, edite os arquivos à mão e deixe o CI validar.
5. Segredo: `src-tauri/src/secret.rs` usa `keyring` fora do Android e `android-native-keyring-store` no Android. Nunca imprima senhas.
6. R8 (release): construtores usados por reflexão precisam de keep em `gen/android/app/proguard-rules.pro`. Prova no APK do CI: `unzip -o <apk> 'classes*.dex'`, `~/Android/Sdk/build-tools/34.0.0/dexdump classes*.dex` e confira que `Landroidx/work/impl/WorkDatabase_Impl;` tem método `<init>` (sem ele o app fecha ao abrir com `NoSuchMethodException`).
7. TLS: `SSL_CERT_DIR` precisa ser uma pasta só (`android_cert_dir()` em `lib.rs`, chamada no `run()` e no `SyncWorker`). O `imap` usa `rustls-native-certs` 0.7, que não aceita lista com `:`; com lista, o IMAP fica sem raiz e toda conta falha em "Conectando ao servidor" com certificado não confiável.
8. Área segura: a WebView do Android devolve 0 em `env(safe-area-inset-*)` com edge-to-edge. A `MainActivity` mede as barras do sistema e o teclado (`ViewCompat.setOnApplyWindowInsetsListener`) e expõe `window.PostaInsets.get()` + evento `posta-insets`; `src/safeArea.ts` grava `--sa-top/right/bottom/left`. No aparelho: a busca não fica sob a barra de status e o Enviar não fica sob o teclado.
9. Ícone: `gen/android/.../res/mipmap-anydpi-v26/ic_launcher{,_round}.xml` é adaptive-icon (fundo `@color/ic_launcher_background` #111111, frente `drawable/ic_launcher_foreground.xml` vetorial 108dp com o glifo em escala 0.072, monochrome para ícone temático do Android 13+); PNGs `mipmap-*/ic_launcher{,_round}.png` são o legado (<26). O manifesto declara `android:roundIcon`. Pré-visualizar: renderize os vetores num HTML com playwright (ImageMagick desenha errado). `src-tauri/icons/android` ainda é o ícone antigo: não rode `tauri icon` sem refazer estes arquivos.
   Notificação: ícone da barra de status é `drawable/ic_stat_posta.xml` (branco, 24dp). `SyncWorker.kt` usa `R.drawable.ic_stat_posta`; `notify_new` em `lib.rs` passa `.icon("ic_stat_posta")` só no Android (sem isso o plugin usa o "i" do sistema; a config do plugin é `()`, não aceita ícone no `tauri.conf.json`). No aparelho: ícone na gaveta e na barra de status.
10. `ndk-context` é global e só aceita uma inicialização por processo (`assert` fecha o app). Só a Activity (tao) e `nativeInit` do `SyncWorker` registram, sempre sob `SyncWorker.CONTEXT_LOCK`; não volte a segurar o contexto durante a sincronização.

Limites: instalar o APK e testar o Keystore/TLS exige aparelho ou emulador (não disponível no CI); registre como não validado em dispositivo.

## Aviso de versão nova (Android)

`pnpm -C apps/client dev` e abra `http://localhost:36480/?update=android`: aparece o toast "Nova versão v0.1.2 disponível" com o botão Atualizar. No aparelho, o app consulta `releases/latest` do GitHub (CSP `connect-src` em `tauri.conf.json`), compara com `src/version.json` e o Atualizar abre o `.apk` do release pelo plugin opener. Só vale a pena validar no aparelho depois de existir um release com o APK assinado.
