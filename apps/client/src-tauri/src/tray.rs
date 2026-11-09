//! Bandeja do sistema (desktop): contagem de não lidas, menu e "fechar esconde a janela".

use mail_core::{Role, Store};
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const ID: &str = "posta-tray";

/// Traz a janela principal para a frente, mesmo escondida ou minimizada.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.unminimize().ok();
        w.set_focus().ok();
    }
}

/// Cria o ícone e o menu; sem bandeja no sistema (ex.: Linux sem AppIndicator) o app segue sem ela.
pub fn init(app: &AppHandle) {
    match build(app) {
        Ok(()) => {
            app.state::<AppState>().tray.store(true, Ordering::Relaxed);
            // Iniciado pelo sistema (`--minimized`): fica só na bandeja.
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    w.hide().ok();
                }
            }
        }
        Err(e) => eprintln!("bandeja indisponível: {e}"),
    }
}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Abrir", true, None::<&str>)?,
            &MenuItem::with_id(app, "compose", "Escrever", true, None::<&str>)?,
            &MenuItem::with_id(app, "sync", "Sincronizar", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?,
        ],
    )?;
    let mut b = TrayIconBuilder::with_id(ID)
        .tooltip("Posta")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show_main(app),
            "compose" => {
                show_main(app);
                app.emit("tray-compose", ()).ok();
            }
            "sync" => {
                app.emit("tray-sync", ()).ok();
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

/// Tooltip e selo com as não lidas da Caixa de entrada.
pub fn show_unread(app: &AppHandle, unread: u32) {
    if let Some(t) = app.tray_by_id(ID) {
        let tip = match unread {
            0 => "Posta".to_string(),
            1 => "Posta: 1 não lida".to_string(),
            n => format!("Posta: {n} não lidas"),
        };
        t.set_tooltip(Some(tip)).ok();
    }
    if let Some(w) = app.get_webview_window("main") {
        // Sem suporte em todos os sistemas (no Windows o selo seria um ícone sobreposto): o erro é ignorado.
        w.set_badge_count((unread > 0).then_some(i64::from(unread))).ok();
    }
}

/// Recalcula a contagem pelo banco (depois de sincronizar em segundo plano, sem a UI).
pub fn refresh(app: &AppHandle) {
    let st = app.state::<AppState>();
    let Ok(store) = Store::open(st.db_path()) else { return };
    if let Ok(counts) = store.unread_counts() {
        show_unread(app, counts.iter().find(|(r, _)| *r == Role::Inbox).map_or(0, |(_, n)| *n));
    }
}

/// Fechar esconde a janela? Só com bandeja criada e a opção `closeToTray` não desligada.
pub fn hide_on_close(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    st.tray.load(Ordering::Relaxed)
        && st.store.lock().unwrap().setting("closeToTray").ok().flatten().as_deref() != Some("0")
}
