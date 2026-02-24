mod audio;
mod commands;
mod db;
mod models;
mod state;
mod whisper;

use std::fs;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }

                    let app_handle = app.clone();
                    let state = app.state::<state::SharedState>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(err) =
                            commands::toggle_recording_impl(app_handle.clone(), state).await
                        {
                            commands::emit_error(&app_handle, err.to_string());
                        }
                    });
                })
                .build(),
        )
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data)?;

            let models_dir = app_data.join("models");
            fs::create_dir_all(&models_dir)?;

            let db_path = app_data.join("murmur.db");
            db::init(&db_path)?;

            let active_model = models::pick_default_model(&models_dir);
            app.manage(state::SharedState::new(db_path, models_dir, active_model));

            register_hotkey(app)?;
            setup_tray(app)?;

            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.hide();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::start_recording,
            commands::stop_recording,
            commands::toggle_recording,
            commands::get_history,
            commands::delete_transcription,
            commands::copy_text,
            commands::list_models,
            commands::set_active_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_hotkey(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);
    app.global_shortcut().register(shortcut)?;
    Ok(())
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "open", "Open Murmur", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Murmur", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(&tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        if is_visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
