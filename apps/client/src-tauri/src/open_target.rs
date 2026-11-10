//! Clique numa notificação de email novo: traz o app para a frente e leva a UI à conta, pasta e mensagem.
//!
//! A notificação carrega `extra.open = {account, folder, id?}`. O clique chega por `on_action` do plugin
//! (Windows, Linux, macOS e Android com o app vivo); o pedido fica guardado até a UI buscá-lo
//! (`take_pending_open`), então vale também com a janela escondida na bandeja ou o webview ainda carregando.
//! O Android com o app fechado entrega o mesmo pedido pelo Intent (ver `MainActivity.kt`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use crate::{AppState, Lock};

/// Chave do payload em `extra` da notificação.
pub const EXTRA: &str = "open";

/// Destino do clique: `id` ausente = abrir só a pasta (notificação que agrupa vários emails).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTarget {
    pub account: String,
    pub folder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
}

impl OpenTarget {
    pub fn message(account: &str, folder: &str, id: i64) -> Self {
        OpenTarget { account: account.into(), folder: folder.into(), id: Some(id) }
    }

    pub fn folder(account: &str, folder: &str) -> Self {
        OpenTarget { account: account.into(), folder: folder.into(), id: None }
    }

    /// Lê o destino do `extra` da notificação; payload ausente, malformado ou sem conta/pasta vira `None`.
    pub fn from_extra(extra: &HashMap<String, Value>) -> Option<Self> {
        let t: OpenTarget = serde_json::from_value(extra.get(EXTRA)?.clone()).ok()?;
        (!t.account.is_empty() && !t.folder.is_empty()).then_some(t)
    }
}

/// Registra o tratamento do clique (`tap`) nas notificações do app.
pub fn init(app: &AppHandle) {
    let handle = app.clone();
    let hooked = app.notification().on_action(move |a| {
        if a.action_id() == "tap" {
            request(&handle, a.notification().and_then(|n| OpenTarget::from_extra(n.extra())));
        }
    });
    if let Err(e) = hooked {
        eprintln!("clique em notificação indisponível: {e}");
    }
}

/// Traz a janela para a frente (desktop) e avisa a UI; sem destino reconhecido só foca o app.
fn request(app: &AppHandle, target: Option<OpenTarget>) {
    #[cfg(desktop)]
    crate::tray::show_main(app);
    let Some(target) = target else { return };
    *app.state::<AppState>().pending_open.lk() = Some(target);
    app.emit("open-target", ()).ok();
}

/// O pedido de abertura guardado pelo clique, uma única vez.
#[tauri::command]
pub fn take_pending_open(state: State<'_, AppState>) -> Option<OpenTarget> {
    state.pending_open.lk().take()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extra(v: Value) -> HashMap<String, Value> {
        HashMap::from([(EXTRA.to_string(), v)])
    }

    #[test]
    fn le_mensagem_e_pasta() {
        let t = OpenTarget::from_extra(&extra(json!({"account": "a@x.com", "folder": "inbox", "id": 42})));
        assert_eq!(t, Some(OpenTarget::message("a@x.com", "inbox", 42)));
        let t = OpenTarget::from_extra(&extra(json!({"account": "a@x.com", "folder": "inbox"})));
        assert_eq!(t, Some(OpenTarget::folder("a@x.com", "inbox")));
    }

    #[test]
    fn rejeita_payload_ruim() {
        assert_eq!(OpenTarget::from_extra(&HashMap::new()), None);
        assert_eq!(OpenTarget::from_extra(&extra(json!("inbox"))), None);
        assert_eq!(OpenTarget::from_extra(&extra(json!({"account": "a@x.com"}))), None);
        assert_eq!(OpenTarget::from_extra(&extra(json!({"account": "", "folder": "inbox"}))), None);
        assert_eq!(OpenTarget::from_extra(&extra(json!({"account": "a", "folder": "inbox", "id": "x"}))), None);
    }

    #[test]
    fn serializa_sem_id_quando_e_pasta() {
        let v = serde_json::to_value(OpenTarget::folder("a", "inbox")).unwrap();
        assert_eq!(v, json!({"account": "a", "folder": "inbox"}));
    }
}
