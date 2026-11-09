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
7. `ndk-context` é global e só aceita uma inicialização por processo (`assert` fecha o app). Só a Activity (tao) e `nativeInit` do `SyncWorker` registram, sempre sob `SyncWorker.CONTEXT_LOCK`; não volte a segurar o contexto durante a sincronização.

Limites: instalar o APK e testar o Keystore/TLS exige aparelho ou emulador (não disponível no CI); registre como não validado em dispositivo.

## Aviso de versão nova (Android)

`pnpm -C apps/client dev` e abra `http://localhost:36480/?update=android`: aparece o toast "Nova versão v0.1.2 disponível" com o botão Atualizar. No aparelho, o app consulta `releases/latest` do GitHub (CSP `connect-src` em `tauri.conf.json`), compara com `src/version.json` e o Atualizar abre o `.apk` do release pelo plugin opener. Só vale a pena validar no aparelho depois de existir um release com o APK assinado.
