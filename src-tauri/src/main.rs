mod audio;
mod commands;
mod db;
mod models;
mod settings;
mod state;
mod whisper;

use std::fs;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{ActivationPolicy, AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

fn main() {
    let app = tauri::Builder::default()
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
            #[cfg(target_os = "macos")]
            app.set_activation_policy(ActivationPolicy::Accessory);

            let app_data = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data)?;

            let models_dir = app_data.join("models");
            fs::create_dir_all(&models_dir)?;

            let db_path = app_data.join("murmur.db");
            db::init(&db_path)?;

            let settings_path = app_data.join("settings.json");
            let settings = settings::load(&settings_path);
            let active_model = models::pick_default_model(&models_dir);
            let hotkey = settings.hotkey.clone();
            app.manage(state::SharedState::new(
                db_path,
                models_dir,
                settings_path,
                active_model,
                hotkey.clone(),
            ));

            register_hotkey(app, &hotkey)?;
            setup_tray(app)?;

            if let Some(main_window) = app.get_webview_window("main") {
                let window_for_close = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_for_close.hide();
                    }
                });
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
            commands::get_hotkey,
            commands::set_hotkey,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            show_window(app_handle);
        }
    });
}

fn register_hotkey(app: &tauri::App, hotkey: &str) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut: Shortcut = hotkey.parse()?;
    app.global_shortcut().register(shortcut)?;
    Ok(())
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "open", "Open Murmur", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Murmur", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state,
                ..
            } = event
            {
                if button_state == MouseButtonState::Up {
                    toggle_window(&tray.app_handle());
                }
            }
        });

    builder = builder.icon(tray_template_icon());

    builder.build(app)?;

    Ok(())
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
            }
            _ => show_window(app),
        }
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn tray_template_icon() -> tauri::image::Image<'static> {
    const WIDTH: usize = 18;
    const HEIGHT: usize = 18;
    let mut rgba = vec![0_u8; WIDTH * HEIGHT * 4];

    let mut paint = |x: usize, y: usize| {
        if x >= WIDTH || y >= HEIGHT {
            return;
        }
        let idx = (y * WIDTH + x) * 4;
        rgba[idx] = 0;
        rgba[idx + 1] = 0;
        rgba[idx + 2] = 0;
        rgba[idx + 3] = 255;
    };

    for y in 3..10 {
        for x in 7..11 {
            paint(x, y);
        }
    }
    for y in 2..4 {
        for x in 8..10 {
            paint(x, y);
        }
    }
    for y in 10..13 {
        for x in 8..10 {
            paint(x, y);
        }
    }
    for x in 5..13 {
        paint(x, 13);
    }
    for y in 14..16 {
        paint(8, y);
        paint(9, y);
    }
    for x in 6..12 {
        paint(x, 16);
    }

    tauri::image::Image::new_owned(rgba, WIDTH as u32, HEIGHT as u32)
}
