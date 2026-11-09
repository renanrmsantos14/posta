//! Sincronização da Entrada com o app fechado. O `SyncWorker` (WorkManager, Kotlin) carrega esta biblioteca e chama
//! `nativeSync`; aqui reaplica a fila offline, sincroniza a Entrada de cada conta e devolve o que notificar.
//! Não passa pelo Tauri: monta um `AppState` próprio sobre a mesma pasta de dados (SQLite em WAL aguenta os dois).

use super::{flush_pending, sync_account_inbox, AppState};
use jni::{
    objects::{JClass, JObject, JString},
    sys::jstring,
    JNIEnv,
};
use mail_core::Store;
use std::{panic::catch_unwind, path::PathBuf};

/// Resposta: primeira linha = total de mensagens novas; depois até 3 linhas `remetente<TAB>assunto`.
/// Vazia quando não há nada a notificar (nada novo, notificações desligadas nas opções ou erro).
fn run(dir: PathBuf) -> Result<String, String> {
    let store = Store::open(dir.join("mail.sqlite")).map_err(|e| e.to_string())?;
    let st = AppState::new(dir, store);
    flush_pending(&st);
    if st.store.lock().unwrap().setting("notify").ok().flatten().as_deref() == Some("0") {
        // Ainda sincroniza (o cache fica em dia), só não avisa.
        for acc in st.accounts()? {
            sync_account_inbox(&st, &acc.id).ok();
        }
        return Ok(String::new());
    }
    let mut total = 0;
    let mut lines = Vec::new();
    for acc in st.accounts()? {
        let Ok(n) = sync_account_inbox(&st, &acc.id) else { continue };
        total += n;
        if n > 0 && lines.len() < 3 {
            let store = Store::open(st.db_path()).map_err(|e| e.to_string())?;
            for m in store.newest_unseen(&acc.id, "INBOX", n.min(3) as u32).map_err(|e| e.to_string())? {
                let who = if m.from_name.is_empty() { m.from_addr } else { m.from_name };
                let subject = if m.subject.is_empty() { "(sem assunto)".to_string() } else { m.subject };
                lines.push(format!("{who}\t{subject}"));
            }
        }
    }
    if total == 0 {
        return Ok(String::new());
    }
    lines.truncate(3);
    Ok(format!("{total}\n{}", lines.join("\n")))
}

/// O Tauri registra o `ndk-context` ao abrir a Activity; num processo criado só para o Worker isso não aconteceu.
/// Registra aqui (o cofre de senhas precisa) e devolve a referência a soltar no fim, junto com `release`.
fn ensure_context(env: &mut JNIEnv, context: &JObject) -> Result<Option<jni::objects::GlobalRef>, String> {
    if catch_unwind(ndk_context::android_context).is_ok() {
        return Ok(None);
    }
    let vm = env.get_java_vm().map_err(|e| e.to_string())?;
    let global = env.new_global_ref(context).map_err(|e| e.to_string())?;
    // SAFETY: a GlobalRef vive até `release_context`, depois de a sincronização terminar.
    unsafe { ndk_context::initialize_android_context(vm.get_java_vm_pointer().cast(), global.as_obj().as_raw().cast()) };
    Ok(Some(global))
}

#[no_mangle]
pub extern "system" fn Java_br_com_renan_posta_SyncWorker_nativeSync<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    context: JObject<'local>,
    dir: JString<'local>,
) -> jstring {
    let out = (|| {
        let dir: String = env.get_string(&dir).map_err(|e| e.to_string())?.into();
        let held = ensure_context(&mut env, &context)?;
        // Um panic não pode atravessar a fronteira JNI.
        let result = catch_unwind(|| run(PathBuf::from(dir))).unwrap_or_else(|_| Err("pânico na sincronização".into()));
        if held.is_some() {
            // Se a Activity abrir depois neste processo, o Tauri registra o seu contexto: o nosso sai de cena.
            // SAFETY: só chega aqui quem registrou acima.
            unsafe { ndk_context::release_android_context() };
        }
        result
    })();
    // Erro vira texto "ERRO: ..." para o Worker registrar e tentar de novo mais tarde.
    let text = out.unwrap_or_else(|e| format!("ERRO: {e}"));
    env.new_string(text).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
